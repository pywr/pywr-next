use jiff::civil::DateTime;
use pywr_core::recorders::{Snapshot, SnapshotMeta};
use pywr_runner_protocol::v1;
use std::collections::HashMap;
use std::convert::Infallible;

#[derive(Debug, strum_macros::EnumDiscriminants)]
#[strum_discriminants(name(EngineEventKind))]
pub enum EngineEvent {
    StateChanged {
        status: EngineStatus,
    },
    Initialised {
        progress: RunProgress,
        snapshot_metadata: Option<ServerSnapshotMeta>,
    },
    Progress {
        progress: RunProgress,
    },
    Snapshot {
        snapshot: ServerSnapshot,
    },
    Log {
        log_record: LogRecord,
    },
    Completed {
        summary: RunSummary,
    },
    Cancelled {
        summary: RunSummary,
    },
    Failed {
        error: String,
        summary: RunSummary,
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

            EngineEvent::Initialised {
                progress,
                snapshot_metadata,
            } => v1::ServerMessage::Initialised {
                progress: progress.try_into()?,
                dataset: snapshot_metadata.map(TryInto::try_into).transpose()?,
            },

            EngineEvent::Progress { progress } => v1::ServerMessage::Progress {
                progress: progress.try_into()?,
            },

            EngineEvent::Snapshot { snapshot } => v1::ServerMessage::Snapshot {
                snapshot: snapshot.try_into()?,
            },

            EngineEvent::Log { log_record } => v1::ServerMessage::Log {
                record: log_record.try_into()?,
            },

            EngineEvent::Completed { summary } => v1::ServerMessage::Completed {
                summary: summary.try_into()?,
            },

            EngineEvent::Cancelled { summary } => v1::ServerMessage::Cancelled {
                summary: summary.try_into()?,
            },

            EngineEvent::Failed { error, .. } => {
                v1::ServerMessage::Failed {
                    error: v1::RunnerError {
                        // This is sufficient for the prototype. Eventually the
                        // engine failure should carry a structured stage.
                        stage: v1::RunnerStage::Timestep,
                        summary: error,
                        causes: Vec::new(),
                        timestep: None,
                    },
                }
            }
        };

        Ok(message)
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
pub struct ServerSnapshotMeta {
    inner: SnapshotMeta,
}

impl From<SnapshotMeta> for ServerSnapshotMeta {
    fn from(value: SnapshotMeta) -> Self {
        Self { inner: value }
    }
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<ServerSnapshotMeta> for v1::SnapshotMeta {
    type Error = Infallible;

    fn try_from(value: ServerSnapshotMeta) -> Result<Self, Self::Error> {
        let contents = value
            .inner
            .metric_set_meta
            .contents
            .iter()
            .map(|(key, items)| {
                let items = items
                    .iter()
                    .map(|item| v1::SnapshotMetricSetItem {
                        name: item.name.clone(),
                        attribute: item.attribute.clone(),
                    })
                    .collect::<Vec<_>>();
                (key.clone(), items)
            })
            .collect::<HashMap<_, _>>();

        let metric_set_meta = v1::SnapshotMetricSetMeta {
            names: value.inner.metric_set_meta.names,
            indices: value.inner.metric_set_meta.indices,
            contents,
        };

        // The scenarios provided here is a single flattened list of those that are running.
        // This is different to the full list of available scenarios in the schema.
        let scenario_items = value
            .inner
            .scenarios
            .iter()
            .enumerate()
            .map(|(index, si)| v1::ScenarioItem {
                name: si.label().to_string(),
                index: index as u64,
            })
            .collect::<Vec<_>>();

        let scenarios = vec![v1::ScenarioInfo {
            name: "Running".to_string(),
            items: scenario_items,
        }];

        let time = v1::TimestepMeta {
            timesteps: value.inner.time.timesteps().iter().map(|t| t.date).collect(),
        };

        Ok(v1::SnapshotMeta {
            metric_set_meta,
            scenarios,
            time,
        })
    }
}

#[derive(Debug)]
pub struct ServerSnapshot {
    inner: Snapshot,
}

impl From<Snapshot> for ServerSnapshot {
    fn from(value: Snapshot) -> Self {
        Self { inner: value }
    }
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<ServerSnapshot> for v1::Snapshot {
    type Error = Infallible;

    fn try_from(value: ServerSnapshot) -> Result<Self, Self::Error> {
        let metric_sets = value
            .inner
            .metric_sets
            .into_iter()
            .map(|(name, data)| {
                let data = v1::SnapshotData { data: data.data };
                (name, data)
            })
            .collect::<HashMap<_, _>>();

        Ok(v1::Snapshot {
            timestep_index: value.inner.timestep_index,
            metric_sets,
        })
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
