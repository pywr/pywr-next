use jiff::civil::DateTime;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ServerMessage {
    Initialising,
    StateChanged(ServerStatus),
    Initialised {
        progress: RunProgress,
        dataset: Option<SnapshotMeta>,
    },
    Progress {
        progress: RunProgress,
    },
    Snapshot {
        snapshot: Snapshot,
    },
    Log {
        record: LogRecord,
    },
    Failed {
        error: RunnerError,
    },
    Completed {
        summary: RunSummary,
    },
    Cancelled {
        summary: RunSummary,
    },
    Pong {
        nonce: u64,
    },
    Goodbye {
        reason: GoodbyeReason,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerStatus {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunProgress {
    pub completed_timesteps: u64,
    pub total_timesteps: u64,
    pub last_completed_date: Option<DateTime>,
    pub next_date: Option<DateTime>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SnapshotMeta {
    pub metric_set_meta: SnapshotMetricSetMeta,
    pub scenarios: Vec<ScenarioInfo>,
    pub time: TimestepMeta,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SnapshotMetricSetMeta {
    /// Name of the metric set
    pub names: Vec<String>,
    /// Mapping from metric set name to index in the snapshot data
    pub indices: HashMap<String, usize>,
    /// Mapping from metric set name to the list of metric names in the metric set
    pub contents: HashMap<String, Vec<SnapshotMetricSetItem>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SnapshotMetricSetItem {
    pub name: String,
    pub attribute: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TimestepMeta {
    pub timesteps: Vec<DateTime>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScenarioInfo {
    pub name: String,
    pub items: Vec<ScenarioItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScenarioItem {
    pub name: String,
    pub index: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub timestep_index: usize,
    pub metric_sets: HashMap<String, SnapshotData>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SnapshotData {
    pub data: Vec<Vec<f64>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LogRecord {
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RunnerError {
    pub stage: RunnerStage,
    pub summary: String,
    pub causes: Vec<String>,
    pub timestep: Option<DateTime>,
    // pub recoverability: Recoverability,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerStage {
    Initialisation,
    SchemaConversion,
    ModelBuild,
    SolverSetup,
    Timestep,
    Recorder,
    Finalisation,
    Dataset,
    Communication,
    Panic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinalOutcome {
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub outcome: FinalOutcome,
    pub progress: RunProgress,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum GoodbyeReason {
    Normal,
    Error(String),
}
