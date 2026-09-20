use jiff::civil::DateTime;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ServerMessage {
    Initialising,
    StateChanged(ServerStatus),
    Initialised {
        progress: RunProgress,
        arrow_stream: Option<ArrowStreamDescriptor>,
    },
    Update {
        progress: RunProgress,
        arrow_stream_commits: Vec<ArrowStreamCommit>,
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
pub struct ArrowStreamDescriptor {
    /// Stable output name supplied in the initialise request.
    pub name: String,
    /// The resolved file path written by the runner.
    pub filename: PathBuf,
    /// Name of the sole metric set in this stream.
    pub metric_set: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArrowStreamCommit {
    pub batch_index: u64,
    pub row_count: u64,
    /// Exclusive byte offset after the flushed IPC record batch.
    pub byte_offset: u64,
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

#[cfg(test)]
mod tests {
    use super::{ArrowStreamCommit, RunProgress, ServerMessage};

    #[test]
    fn update_serializes_arrow_stream_commits() {
        let message = ServerMessage::Update {
            progress: RunProgress {
                completed_timesteps: 4,
                total_timesteps: 10,
                last_completed_date: None,
                next_date: None,
            },
            arrow_stream_commits: vec![ArrowStreamCommit {
                batch_index: 1,
                row_count: 8,
                byte_offset: 4096,
            }],
        };
        let value = serde_json::to_value(message).unwrap();
        assert_eq!(value["type"], "update");
        assert_eq!(value["payload"]["arrow_stream_commits"][0]["byte_offset"], 4096);
    }
}
