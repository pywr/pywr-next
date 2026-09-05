use crate::backend::{BackendError, BackendStepOutcome, RunnerBackend};
use crate::command::{EngineCommand, EngineCommandKind, InitialiseRequest};
use crate::event::{EngineEvent, EngineStatus};
use crate::state::{ReadyReason, RunTarget, RunnerState, RunnerStateKind};
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
}

impl<B, O> RunnerEngine<B, O>
where
    B: RunnerBackend,
    O: OutputSink,
{
    pub fn initialise(request: InitialiseRequest, backend: B, output: O) -> Self {
        Self {
            backend,
            state: RunnerState::Initialising(request),
            output,
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
            (RunnerState::Ready { runtime, snapshots, .. }, EngineCommand::Step) => {
                // Ready → Running(Step)
                RunnerState::Running {
                    runtime,
                    snapshots,
                    target: RunTarget::Step,
                }
            }

            (RunnerState::Ready { runtime, .. }, EngineCommand::Cancel) => {
                // Ready → Cancelling
                RunnerState::Cancelling { runtime }
            }

            (RunnerState::Running { runtime, snapshots, .. }, EngineCommand::Pause) => {
                // Running → Pausing
                RunnerState::Pausing { runtime, snapshots }
            }

            (RunnerState::Ready { runtime, snapshots, .. }, EngineCommand::RunToEnd) => RunnerState::Running {
                runtime,
                snapshots,
                target: RunTarget::ToEnd,
            },

            (RunnerState::Ready { runtime, snapshots, .. }, EngineCommand::RunUntil { datetime }) => {
                RunnerState::Running {
                    runtime,
                    snapshots,
                    target: RunTarget::ToDatetime(datetime),
                }
            }

            (RunnerState::Running { runtime, .. }, EngineCommand::Cancel) => RunnerState::Cancelling { runtime },

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
        let current_state_kind = self.state.kind();

        let next_state = match self.state {
            RunnerState::Initialising(init_request) => {
                let initialised = self.backend.initialise(init_request)?;

                self.output
                    .emit(EngineEvent::Initialised {
                        progress: initialised.progress,
                        snapshot_metadata: initialised
                            .snapshots
                            .as_ref()
                            .and_then(|buffer| buffer.meta().map(|meta| meta.into())),
                    })
                    .map_err(TickError::OutputSinkError)?;

                RunnerState::Ready {
                    runtime: initialised.runtime,
                    snapshots: initialised.snapshots,
                    reason: ReadyReason::Initialised,
                }
            }
            RunnerState::Ready {
                runtime,
                snapshots,
                reason,
            } => {
                // Remain in ready state until a command is received
                RunnerState::Ready {
                    runtime,
                    snapshots,
                    reason,
                }
            }
            RunnerState::Running {
                mut runtime,
                snapshots,
                target,
            } => {
                let step_result = self.backend.step(&mut runtime, snapshots, &target);

                match step_result {
                    Ok(step) => {
                        // Emit progress and snapshots
                        self.output
                            .emit(EngineEvent::Progress {
                                progress: step.progress.clone(),
                            })
                            .map_err(TickError::OutputSinkError)?;

                        let snapshots = step.snapshots.as_ref().map(|buffer| buffer.drain()).unwrap_or_default();

                        for snapshot in snapshots {
                            self.output
                                .emit(EngineEvent::Snapshot {
                                    snapshot: snapshot.into(),
                                })
                                .map_err(TickError::OutputSinkError)?;
                        }

                        match step.outcome {
                            BackendStepOutcome::Advanced => {
                                if step.target_reached {
                                    // Transition to ready state
                                    RunnerState::Ready {
                                        runtime,
                                        snapshots: step.snapshots,
                                        reason: ReadyReason::TargetReached,
                                    }
                                } else {
                                    // Remain in running state
                                    RunnerState::Running {
                                        runtime,
                                        snapshots: step.snapshots,
                                        target,
                                    }
                                }
                            }
                            BackendStepOutcome::EndOfTimesteps => {
                                // Transition to finalising state
                                RunnerState::Finalising {
                                    runtime,
                                    snapshots: step.snapshots,
                                }
                            }
                        }
                    }
                    Err(error) => Err(TickError::BackendError(error))?,
                }
            }
            RunnerState::Finalising { mut runtime, snapshots } => {
                // Make sure to emit any remaining snapshots before finalising
                let snapshots = snapshots.as_ref().map(|buffer| buffer.drain()).unwrap_or_default();

                for snapshot in snapshots {
                    self.output
                        .emit(EngineEvent::Snapshot {
                            snapshot: snapshot.into(),
                        })
                        .map_err(TickError::OutputSinkError)?;
                }

                let finalisation_result = self.backend.finalise(&mut runtime);

                match finalisation_result {
                    Ok(finalisation) => {
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
            RunnerState::Pausing { runtime, snapshots } => {
                // Transition to ready state
                RunnerState::Ready {
                    runtime,
                    snapshots,
                    reason: ReadyReason::Paused,
                }
            }
            RunnerState::Cancelling { mut runtime } => {
                // Transition to cancelled state
                let cancel_result = self.backend.cancel(&mut runtime);

                match cancel_result {
                    Ok(finalisation) => {
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
}
