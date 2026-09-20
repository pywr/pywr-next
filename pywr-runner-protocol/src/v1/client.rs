use super::server::LogLevel;
use jiff::civil::DateTime;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use strum_macros::EnumDiscriminants;

#[derive(Debug, Serialize, Deserialize, EnumDiscriminants)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
#[strum_discriminants(name(ClientCommandKind))]
pub enum ClientCommand {
    Initialise { request: InitialiseRequest },
    Step,
    RunUntil { datetime: DateTime },
    RunToEnd,
    Pause,
    Cancel,
    Ping { nonce: u64 },
    Shutdown,
}

impl ClientCommand {
    pub fn kind(&self) -> ClientCommandKind {
        self.into()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InitialiseRequest {
    pub run_name: String,

    // Stable wire representation, not a ModelSchema Rust value.
    pub model: ModelDocument,

    pub output_path: Option<PathBuf>,
    pub data_path: Option<PathBuf>,

    /// Minimum level of model and runner diagnostics to stream back as log events.
    #[serde(default)]
    pub log_level: Option<LogLevel>,

    pub solver: SolverConfiguration,
    pub result_options: ResultOptions,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SolverConfiguration {}
#[derive(Debug, Serialize, Deserialize)]
pub struct ResultOptions {
    pub all_nodes_metric_set: Option<AddNodesMetricSet>,
    pub all_edges_metric_set: Option<AddEdgesMetricSet>,
    pub clear_existing_outputs: bool,
    pub arrow_stream: Option<ArrowStreamOptions>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ModelDocument {
    Json(serde_json::Value),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddNodesMetricSet {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddEdgesMetricSet {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArrowStreamOptions {
    pub name: String,
    pub filename: PathBuf,
    pub metric_set: String,
    pub batch_size: NonZeroUsize,
}

#[cfg(test)]
mod tests {
    use super::ArrowStreamOptions;

    #[test]
    fn arrow_stream_options_reject_zero_batch_size() {
        let options = r#"{"name":"results","filename":"results.arrow","metric_set":"nodes","batch_size":0}"#;
        assert!(serde_json::from_str::<ArrowStreamOptions>(options).is_err());
    }
}
