use jiff::civil::DateTime;
use pywr_core::recorders::ArrowStreamCommit as CoreArrowStreamCommit;
use pywr_runner_protocol::v1;
use std::convert::Infallible;
use std::path::PathBuf;

#[derive(Debug, strum_macros::EnumDiscriminants)]
#[strum_discriminants(name(EngineEventKind))]
pub enum EngineEvent {
    StateChanged {
        status: EngineStatus,
    },
    Initialised {
        progress: RunProgress,
        arrow_stream: Option<ArrowStreamDescriptor>,
    },
    Progress {
        progress: RunProgress,
    },
    ArrowStreamCommitted {
        commit: ArrowStreamCommit,
    },
    Log {
        log_record: LogRecord,
    },
    CommandRejected {
        command: String,
        status: EngineStatus,
    },
    Completed {
        summary: RunSummary,
    },
    Cancelled {
        summary: RunSummary,
    },
    Failed {
        error: RunFailure,
    },
}

impl EngineEvent {
    pub fn kind(&self) -> EngineEventKind {
        self.into()
    }
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<EngineEvent> for v1::ServerMessage {
    type Error = Infallible;

    fn try_from(value: EngineEvent) -> Result<Self, Self::Error> {
        let message = match value {
            EngineEvent::StateChanged { status } => v1::ServerMessage::StateChanged(status.try_into()?),

            EngineEvent::Initialised { progress, arrow_stream } => v1::ServerMessage::Initialised {
                progress: progress.try_into()?,
                arrow_stream: arrow_stream.map(Into::into),
            },

            EngineEvent::Progress { progress } => v1::ServerMessage::Update {
                progress: progress.try_into()?,
                arrow_stream_commits: Vec::new(),
            },

            EngineEvent::ArrowStreamCommitted { commit } => v1::ServerMessage::Update {
                progress: RunProgress {
                    completed_timesteps: 0,
                    total_timesteps: 0,
                    last_completed_date: None,
                    next_date: None,
                }
                .try_into()?,
                arrow_stream_commits: vec![commit.into()],
            },

            EngineEvent::Log { log_record } => v1::ServerMessage::Log {
                record: log_record.try_into()?,
            },

            EngineEvent::CommandRejected { command, status } => v1::ServerMessage::CommandRejected {
                command,
                status: status.try_into()?,
            },

            EngineEvent::Completed { summary } => v1::ServerMessage::Completed {
                summary: summary.try_into()?,
            },

            EngineEvent::Cancelled { summary } => v1::ServerMessage::Cancelled {
                summary: summary.try_into()?,
            },

            EngineEvent::Failed { error } => v1::ServerMessage::Failed { error: error.into() },
        };

        Ok(message)
    }
}

#[derive(Debug, Clone)]
pub struct RunFailure {
    pub stage: RunFailureStage,
    pub summary: String,
    pub causes: Vec<String>,
    pub timestep: Option<DateTime>,
}

#[derive(Debug, Clone, Copy)]
pub enum RunFailureStage {
    Initialisation,
    SchemaConversion,
    ModelBuild,
    SolverSetup,
    Timestep,
    Recorder,
    Finalisation,
}

impl From<RunFailure> for v1::RunnerError {
    fn from(value: RunFailure) -> Self {
        Self {
            stage: match value.stage {
                RunFailureStage::Initialisation => v1::RunnerStage::Initialisation,
                RunFailureStage::SchemaConversion => v1::RunnerStage::SchemaConversion,
                RunFailureStage::ModelBuild => v1::RunnerStage::ModelBuild,
                RunFailureStage::SolverSetup => v1::RunnerStage::SolverSetup,
                RunFailureStage::Timestep => v1::RunnerStage::Timestep,
                RunFailureStage::Recorder => v1::RunnerStage::Recorder,
                RunFailureStage::Finalisation => v1::RunnerStage::Finalisation,
            },
            summary: value.summary,
            causes: value.causes,
            timestep: value.timestep,
        }
    }
}

#[derive(Debug)]
pub enum EngineStatus {
    Initialising,
    Ready,
    Running,
    Pausing,
    Cancelling,
    Finalising,
    Completed,
    Cancelled,
    Failed,
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<EngineStatus> for v1::ServerStatus {
    type Error = Infallible;

    fn try_from(value: EngineStatus) -> Result<Self, Self::Error> {
        let status = match value {
            EngineStatus::Initialising => v1::ServerStatus::Initialising,
            EngineStatus::Ready => v1::ServerStatus::Ready,
            EngineStatus::Running => v1::ServerStatus::Running,
            EngineStatus::Pausing => v1::ServerStatus::Pausing,
            EngineStatus::Cancelling => v1::ServerStatus::Cancelling,
            EngineStatus::Finalising => v1::ServerStatus::Finalising,
            EngineStatus::Completed => v1::ServerStatus::Completed,
            EngineStatus::Cancelled => v1::ServerStatus::Cancelled,
            EngineStatus::Failed => v1::ServerStatus::Failed,
        };

        Ok(status)
    }
}

#[derive(Debug, Clone)]
pub struct RunProgress {
    pub completed_timesteps: u64,
    pub total_timesteps: u64,
    pub last_completed_date: Option<DateTime>,
    pub next_date: Option<DateTime>,
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<RunProgress> for v1::RunProgress {
    type Error = Infallible;

    fn try_from(value: RunProgress) -> Result<Self, Self::Error> {
        Ok(v1::RunProgress {
            completed_timesteps: value.completed_timesteps,
            total_timesteps: value.total_timesteps,
            last_completed_date: value.last_completed_date,
            next_date: value.next_date,
        })
    }
}

#[derive(Debug)]
pub struct ArrowStreamDescriptor {
    pub name: String,
    pub filename: PathBuf,
    pub metric_set: String,
}

impl From<ArrowStreamDescriptor> for v1::ArrowStreamDescriptor {
    fn from(value: ArrowStreamDescriptor) -> Self {
        Self {
            name: value.name,
            filename: value.filename,
            metric_set: value.metric_set,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ArrowStreamCommit {
    pub batch_index: u64,
    pub row_count: u64,
    pub byte_offset: u64,
}

impl From<CoreArrowStreamCommit> for ArrowStreamCommit {
    fn from(value: CoreArrowStreamCommit) -> Self {
        Self {
            batch_index: value.batch_index,
            row_count: value.row_count as u64,
            byte_offset: value.byte_offset,
        }
    }
}

impl From<ArrowStreamCommit> for v1::ArrowStreamCommit {
    fn from(value: ArrowStreamCommit) -> Self {
        Self {
            batch_index: value.batch_index,
            row_count: value.row_count,
            byte_offset: value.byte_offset,
        }
    }
}

#[derive(Debug)]
pub struct LogRecord {
    pub level: LogLevel,
    pub message: String,
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<LogRecord> for v1::LogRecord {
    type Error = Infallible;

    fn try_from(value: LogRecord) -> Result<Self, Self::Error> {
        Ok(v1::LogRecord {
            level: value.level.try_into()?,
            message: value.message,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<LogLevel> for v1::LogLevel {
    type Error = Infallible;

    fn try_from(value: LogLevel) -> Result<Self, <v1::LogLevel as TryFrom<LogLevel>>::Error> {
        let level = match value {
            LogLevel::Debug => v1::LogLevel::Debug,
            LogLevel::Info => v1::LogLevel::Info,
            LogLevel::Warn => v1::LogLevel::Warn,
            LogLevel::Error => v1::LogLevel::Error,
        };

        Ok(level)
    }
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<v1::LogLevel> for LogLevel {
    type Error = Infallible;

    fn try_from(value: v1::LogLevel) -> Result<Self, Infallible> {
        Ok(match value {
            v1::LogLevel::Debug => Self::Debug,
            v1::LogLevel::Info => Self::Info,
            v1::LogLevel::Warn => Self::Warn,
            v1::LogLevel::Error => Self::Error,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub enum FinalOutcome {
    Completed,
    Cancelled,
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<FinalOutcome> for v1::FinalOutcome {
    type Error = Infallible;

    fn try_from(value: FinalOutcome) -> Result<Self, Self::Error> {
        let outcome = match value {
            FinalOutcome::Completed => v1::FinalOutcome::Completed,
            FinalOutcome::Cancelled => v1::FinalOutcome::Cancelled,
        };

        Ok(outcome)
    }
}

#[derive(Debug, Clone)]
pub struct RunSummary {
    pub outcome: FinalOutcome,
    pub progress: RunProgress,
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<RunSummary> for v1::RunSummary {
    type Error = Infallible;

    fn try_from(value: RunSummary) -> Result<Self, Self::Error> {
        Ok(v1::RunSummary {
            outcome: value.outcome.try_into()?,
            progress: value.progress.try_into()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_event_preserves_structured_failure_details() {
        let message: v1::ServerMessage = EngineEvent::Failed {
            error: RunFailure {
                stage: RunFailureStage::Recorder,
                summary: "failed to flush recorder output".into(),
                causes: vec!["disk is full".into()],
                timestep: Some("2024-01-02T00:00".parse().unwrap()),
            },
        }
        .try_into()
        .unwrap();

        let v1::ServerMessage::Failed { error } = message else {
            panic!("expected failed server message");
        };
        assert!(matches!(error.stage, v1::RunnerStage::Recorder));
        assert_eq!(error.summary, "failed to flush recorder output");
        assert_eq!(error.causes, ["disk is full"]);
        assert_eq!(error.timestep, Some("2024-01-02T00:00".parse().unwrap()));
    }
}
