use crate::backend::{BackendError, BackendStepOutcome, RunnerBackend};
use crate::command::{EngineCommand, EngineCommandKind, InitialiseRequest};
use crate::event::{EngineEvent, EngineStatus, LogLevel, LogRecord};
use crate::logging::capture_logs;
use crate::state::{ReadyReason, RunTarget, RunnerState, RunnerStateKind};
use std::sync::mpsc::{self, Receiver};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OutputError {}

pub trait OutputSink {
    fn emit(&mut self, output: EngineEvent) -> Result<(), OutputError>;
}

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("Invalid command {command:?} for state {state:?}")]
    InvalidState {
        command: EngineCommandKind,
        state: RunnerStateKind,
    },
    #[error("Output sink error: {0}")]
    OutputSinkError(#[from] OutputError),
}

#[derive(Debug, Error)]
pub enum TickError {
    #[error("Backend error: {0}")]
    BackendError(#[from] BackendError),
    #[error("Output sink error: {0}")]
    OutputSinkError(#[from] OutputError),
}

pub struct RunnerEngine<B, O>
where
    B: RunnerBackend,
{
    backend: B,
    state: RunnerState<B::Runtime>,
    output: O,
    log_level: LogLevel,
    log_receiver: Receiver<LogRecord>,
    log_sender: mpsc::Sender<LogRecord>,
}

impl<B, O> RunnerEngine<B, O>
where
    B: RunnerBackend,
    O: OutputSink,
{
    pub fn initialise(request: InitialiseRequest, backend: B, output: O) -> Self {
        let log_level = request.log_level.unwrap_or(LogLevel::Info);
        let (log_sender, log_receiver) = mpsc::channel();
        Self {
            backend,
            state: RunnerState::Initialising(request),
            output,
            log_level,
            log_receiver,
            log_sender,
        }
    }

    pub fn status(&self) -> EngineStatus {
        self.state.status()
    }

    pub fn is_terminal(&self) -> bool {
        self.state.is_terminal()
    }

    pub fn needs_tick(&self) -> bool {
        self.state.needs_tick()
    }

    pub fn handle_command(mut self, command: EngineCommand) -> Result<Self, CommandError> {
        self.state = match (self.state, command) {
            (
                RunnerState::Ready {
                    runtime,
                    arrow_stream_commits,
                    ..
                },
                EngineCommand::Step,
            ) => {
                // Ready → Running(Step)
                RunnerState::Running {
                    runtime,
                    arrow_stream_commits,
                    target: RunTarget::Step,
                }
            }

            (
                RunnerState::Ready {
                    runtime,
                    arrow_stream_commits,
                    ..
                },
                EngineCommand::Cancel,
            ) => {
                // Ready → Cancelling
                RunnerState::Cancelling {
                    runtime,
                    arrow_stream_commits,
                }
            }

            (
                RunnerState::Running {
                    runtime,
                    arrow_stream_commits,
                    ..
                },
                EngineCommand::Pause,
            ) => {
                // Running → Pausing
                RunnerState::Pausing {
                    runtime,
                    arrow_stream_commits,
                }
            }

            (
                RunnerState::Ready {
                    runtime,
                    arrow_stream_commits,
                    ..
                },
                EngineCommand::RunToEnd,
            ) => RunnerState::Running {
                runtime,
                arrow_stream_commits,
                target: RunTarget::ToEnd,
            },

            (
                RunnerState::Ready {
                    runtime,
                    arrow_stream_commits,
                    ..
                },
                EngineCommand::RunUntil { datetime },
            ) => RunnerState::Running {
                runtime,
                arrow_stream_commits,
                target: RunTarget::ToDatetime(datetime),
            },

            (
                RunnerState::Running {
                    runtime,
                    arrow_stream_commits,
                    ..
                },
                EngineCommand::Cancel,
            ) => RunnerState::Cancelling {
                runtime,
                arrow_stream_commits,
            },

            (state, command) => {
                return Err(CommandError::InvalidState {
                    command: command.kind(),
                    state: state.kind(),
                });
            }
        };

        self.output.emit(EngineEvent::StateChanged {
            status: self.state.status(),
        })?;

        Ok(self)
    }

    /// Performs at most one bounded unit of work.
    pub fn tick(mut self) -> Result<Self, TickError> {
        macro_rules! emit_captured_logs {
            () => {
                for log_record in self.log_receiver.try_iter() {
                    self.output.emit(EngineEvent::Log { log_record })?;
                }
            };
        }

        let current_state_kind = self.state.kind();

        let next_state = match self.state {
            RunnerState::Initialising(init_request) => {
                let result = capture_logs(self.log_level, self.log_sender.clone(), || {
                    self.backend.initialise(init_request)
                });
                let initialised = result?;

                emit_captured_logs!();

                self.output
                    .emit(EngineEvent::Initialised {
                        progress: initialised.progress,
                        arrow_stream: initialised.arrow_stream,
                    })
                    .map_err(TickError::OutputSinkError)?;

                RunnerState::Ready {
                    runtime: initialised.runtime,
                    arrow_stream_commits: initialised.arrow_stream_commits,
                    reason: ReadyReason::Initialised,
                }
            }
            RunnerState::Ready {
                runtime,
                arrow_stream_commits,
                reason,
            } => {
                // Remain in ready state until a command is received
                RunnerState::Ready {
                    runtime,
                    arrow_stream_commits,
                    reason,
                }
            }
            RunnerState::Running {
                mut runtime,
                arrow_stream_commits,
                target,
            } => {
                let step_result = capture_logs(self.log_level, self.log_sender.clone(), || {
                    self.backend.step(&mut runtime, arrow_stream_commits, &target)
                });

                emit_captured_logs!();

                match step_result {
                    Ok(step) => {
                        // Progress and writer commits are coalesced by the service.
                        self.output
                            .emit(EngineEvent::Progress {
                                progress: step.progress.clone(),
                            })
                            .map_err(TickError::OutputSinkError)?;

                        Self::emit_arrow_stream_commits(&mut self.output, &step.arrow_stream_commits)
                            .map_err(TickError::OutputSinkError)?;

                        match step.outcome {
                            BackendStepOutcome::Advanced => {
                                if step.target_reached {
                                    // Transition to ready state
                                    RunnerState::Ready {
                                        runtime,
                                        arrow_stream_commits: step.arrow_stream_commits,
                                        reason: ReadyReason::TargetReached,
                                    }
                                } else {
                                    // Remain in running state
                                    RunnerState::Running {
                                        runtime,
                                        arrow_stream_commits: step.arrow_stream_commits,
                                        target,
                                    }
                                }
                            }
                            BackendStepOutcome::EndOfTimesteps => {
                                // Transition to finalising state
                                RunnerState::Finalising {
                                    runtime,
                                    arrow_stream_commits: step.arrow_stream_commits,
                                }
                            }
                        }
                    }
                    Err(error) => Err(TickError::BackendError(error))?,
                }
            }
            RunnerState::Finalising {
                mut runtime,
                arrow_stream_commits,
            } => {
                let finalisation_result = capture_logs(self.log_level, self.log_sender.clone(), || {
                    self.backend.finalise(&mut runtime)
                });

                emit_captured_logs!();

                match finalisation_result {
                    Ok(finalisation) => {
                        Self::emit_arrow_stream_commits(&mut self.output, &arrow_stream_commits)
                            .map_err(TickError::OutputSinkError)?;
                        self.output
                            .emit(EngineEvent::Completed {
                                summary: finalisation.summary.clone(),
                            })
                            .map_err(TickError::OutputSinkError)?;

                        RunnerState::Completed(finalisation.summary)
                    }
                    Err(error) => Err(TickError::BackendError(error))?,
                }
            }
            RunnerState::Pausing {
                runtime,
                arrow_stream_commits,
            } => {
                // Transition to ready state
                RunnerState::Ready {
                    runtime,
                    arrow_stream_commits,
                    reason: ReadyReason::Paused,
                }
            }
            RunnerState::Cancelling {
                mut runtime,
                arrow_stream_commits,
            } => {
                // Transition to cancelled state
                let cancel_result = capture_logs(self.log_level, self.log_sender.clone(), || {
                    self.backend.cancel(&mut runtime)
                });

                emit_captured_logs!();

                match cancel_result {
                    Ok(finalisation) => {
                        Self::emit_arrow_stream_commits(&mut self.output, &arrow_stream_commits)
                            .map_err(TickError::OutputSinkError)?;
                        self.output
                            .emit(EngineEvent::Cancelled {
                                summary: finalisation.summary.clone(),
                            })
                            .map_err(TickError::OutputSinkError)?;

                        RunnerState::Cancelled(finalisation.summary)
                    }
                    Err(error) => Err(TickError::BackendError(error))?,
                }
            }
            RunnerState::Completed(summary) => RunnerState::Completed(summary),
            RunnerState::Cancelled(summary) => RunnerState::Cancelled(summary),
            RunnerState::Failed { summary, error } => RunnerState::Failed { summary, error },
        };

        // Send a state change event if the state has changed and is not terminal
        let state_change = next_state.kind() != current_state_kind && !next_state.is_terminal();

        self.state = next_state;

        if state_change {
            self.output
                .emit(EngineEvent::StateChanged {
                    status: self.state.status(),
                })
                .map_err(TickError::from)?;
        }

        Ok(self)
    }

    fn emit_arrow_stream_commits(
        output: &mut O,
        commits: &Option<Receiver<pywr_core::recorders::ArrowStreamCommit>>,
    ) -> Result<(), OutputError> {
        if let Some(commits) = commits {
            for commit in commits.try_iter() {
                output.emit(EngineEvent::ArrowStreamCommitted { commit: commit.into() })?;
            }
        }
        Ok(())
    }
}
