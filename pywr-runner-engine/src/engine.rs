use crate::backend::{BackendStepOutcome, RunnerBackend};
use crate::command::{EngineCommand, InitialiseRequest};
use crate::event::{EngineEvent, EngineStatus, LogLevel, LogRecord};
use crate::logging::capture_logs;
use crate::state::{ReadyReason, RunTarget, RunnerState};
use std::sync::mpsc::{self, Receiver};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OutputError {}

pub trait OutputSink {
    fn emit(&mut self, output: EngineEvent) -> Result<(), OutputError>;
}

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("Output sink error: {0}")]
    OutputSinkError(#[from] OutputError),
}

#[derive(Debug, Error)]
pub enum TickError {
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

    pub fn handle_command(&mut self, command: EngineCommand) -> Result<(), CommandError> {
        let state = std::mem::replace(
            &mut self.state,
            RunnerState::Failed {
                error: "command handling state placeholder".into(),
            },
        );

        self.state = match (state, command) {
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
                let command = command.to_string();
                self.state = state;
                self.output.emit(EngineEvent::CommandRejected {
                    command,
                    status: self.state.status(),
                })?;
                return Ok(());
            }
        };

        self.output.emit(EngineEvent::StateChanged {
            status: self.state.status(),
        })?;

        Ok(())
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

                emit_captured_logs!();

                match result {
                    Ok(initialised) => {
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
                    Err(error) => Self::failed_state(&mut self.output, error.to_string())?,
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
                    Err(error) => Self::failed_state(&mut self.output, error.to_string())?,
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
                    Err(error) => Self::failed_state(&mut self.output, error.to_string())?,
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
                    Err(error) => Self::failed_state(&mut self.output, error.to_string())?,
                }
            }
            RunnerState::Completed(summary) => RunnerState::Completed(summary),
            RunnerState::Cancelled(summary) => RunnerState::Cancelled(summary),
            RunnerState::Failed { error } => RunnerState::Failed { error },
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

    fn failed_state(output: &mut O, error: String) -> Result<RunnerState<B::Runtime>, OutputError> {
        output.emit(EngineEvent::Failed { error: error.clone() })?;
        Ok(RunnerState::Failed { error })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{BackendFinalisation, BackendStep, Initialised};
    use crate::command::{ModelDocument, ResultOptions, SolverConfiguration};
    use std::sync::mpsc::Receiver;

    #[derive(Default)]
    struct TestOutput(Vec<EngineEvent>);

    impl OutputSink for TestOutput {
        fn emit(&mut self, event: EngineEvent) -> Result<(), OutputError> {
            self.0.push(event);
            Ok(())
        }
    }

    #[derive(Clone, Copy)]
    enum FailurePoint {
        Initialise,
        Step,
        Finalise,
        Cancel,
    }

    struct FailingBackend(FailurePoint);

    impl RunnerBackend for FailingBackend {
        type Runtime = ();

        fn initialise(
            &mut self,
            _request: InitialiseRequest,
        ) -> Result<Initialised<Self::Runtime>, crate::backend::BackendError> {
            if matches!(self.0, FailurePoint::Initialise) {
                return Err(crate::backend::BackendError::AlreadyFinalised);
            }

            Ok(Initialised {
                runtime: (),
                progress: progress(),
                arrow_stream: None,
                arrow_stream_commits: None,
            })
        }

        fn step(
            &mut self,
            _runtime: &mut Self::Runtime,
            commits: Option<Receiver<pywr_core::recorders::ArrowStreamCommit>>,
            _target: &RunTarget,
        ) -> Result<BackendStep, crate::backend::BackendError> {
            if matches!(self.0, FailurePoint::Step) {
                return Err(crate::backend::BackendError::AlreadyFinalised);
            }

            Ok(BackendStep {
                outcome: BackendStepOutcome::EndOfTimesteps,
                progress: progress(),
                arrow_stream_commits: commits,
                target_reached: true,
            })
        }

        fn finalise(
            &mut self,
            _runtime: &mut Self::Runtime,
        ) -> Result<BackendFinalisation, crate::backend::BackendError> {
            Err(crate::backend::BackendError::AlreadyFinalised)
        }

        fn cancel(
            &mut self,
            _runtime: &mut Self::Runtime,
        ) -> Result<BackendFinalisation, crate::backend::BackendError> {
            Err(crate::backend::BackendError::AlreadyFinalised)
        }
    }

    fn request() -> InitialiseRequest {
        InitialiseRequest {
            run_name: "test".into(),
            model: ModelDocument::Json(serde_json::Value::Null),
            data_path: None,
            output_path: None,
            log_level: None,
            solver: SolverConfiguration {},
            result_options: ResultOptions {
                all_nodes_metric_set: None,
                all_edges_metric_set: None,
                clear_existing_outputs: false,
                arrow_stream: None,
            },
        }
    }

    fn progress() -> crate::event::RunProgress {
        crate::event::RunProgress {
            completed_timesteps: 0,
            total_timesteps: 1,
            last_completed_date: None,
            next_date: None,
        }
    }

    fn assert_failed(engine: RunnerEngine<FailingBackend, TestOutput>) {
        assert!(matches!(engine.status(), EngineStatus::Failed));
        assert!(engine.is_terminal());
        assert!(!engine.needs_tick());
        assert!(matches!(
            engine.output.0.last(),
            Some(EngineEvent::Failed { error }) if error == "Backend already finalised"
        ));
    }

    #[test]
    fn backend_errors_transition_to_failed_and_emit_an_event() {
        assert_failed(
            RunnerEngine::initialise(
                request(),
                FailingBackend(FailurePoint::Initialise),
                TestOutput::default(),
            )
            .tick()
            .unwrap(),
        );

        let mut engine = RunnerEngine::initialise(request(), FailingBackend(FailurePoint::Step), TestOutput::default())
            .tick()
            .unwrap();
        engine.handle_command(EngineCommand::Step).unwrap();
        let engine = engine.tick().unwrap();
        assert_failed(engine);

        let mut engine =
            RunnerEngine::initialise(request(), FailingBackend(FailurePoint::Finalise), TestOutput::default())
                .tick()
                .unwrap();
        engine.handle_command(EngineCommand::RunToEnd).unwrap();
        let engine = engine.tick().unwrap().tick().unwrap();
        assert_failed(engine);

        let mut engine =
            RunnerEngine::initialise(request(), FailingBackend(FailurePoint::Cancel), TestOutput::default())
                .tick()
                .unwrap();
        engine.handle_command(EngineCommand::Cancel).unwrap();
        let engine = engine.tick().unwrap();
        assert_failed(engine);
    }

    #[test]
    fn invalid_command_is_rejected_without_changing_the_engine_state() {
        let mut engine = RunnerEngine::initialise(request(), FailingBackend(FailurePoint::Step), TestOutput::default())
            .tick()
            .unwrap();

        engine.handle_command(EngineCommand::Pause).unwrap();

        assert!(matches!(engine.status(), EngineStatus::Ready));
        assert!(!engine.needs_tick());
        assert!(matches!(
            engine.output.0.last(),
            Some(EngineEvent::CommandRejected { command, status })
                if command == "pause" && matches!(status, EngineStatus::Ready)
        ));

        engine.handle_command(EngineCommand::Step).unwrap();
        assert!(matches!(engine.status(), EngineStatus::Running));
        assert_failed(engine.tick().unwrap());
    }
}
