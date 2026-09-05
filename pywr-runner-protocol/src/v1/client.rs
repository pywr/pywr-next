use jiff::civil::DateTime;
use serde::{Deserialize, Serialize};
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
    pub snapshot: Option<SnapshotOptions>,
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
pub struct SnapshotOptions {
    pub name: String,
    pub metric_sets: Vec<String>,
}
