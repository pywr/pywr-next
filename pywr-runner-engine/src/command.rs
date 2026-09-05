use jiff::civil::DateTime;
use pywr_runner_protocol::v1;
use std::convert::Infallible;
use std::path::PathBuf;

/// Represents a command to the engine.
#[derive(Debug, strum_macros::EnumDiscriminants)]
#[strum_discriminants(name(EngineCommandKind))]
pub enum EngineCommand {
    Initialize { request: InitialiseRequest },
    Step,
    RunUntil { datetime: DateTime },
    RunToEnd,
    Pause,
    Cancel,
    Ping { nonce: u64 },
    Shutdown,
}

impl EngineCommand {
    pub fn kind(&self) -> EngineCommandKind {
        self.into()
    }
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<v1::ClientCommand> for EngineCommand {
    type Error = Infallible;

    fn try_from(cmd: v1::ClientCommand) -> Result<Self, Self::Error> {
        let cmd = match cmd {
            v1::ClientCommand::Step => EngineCommand::Step,
            v1::ClientCommand::RunUntil { datetime } => EngineCommand::RunUntil { datetime },
            v1::ClientCommand::RunToEnd => EngineCommand::RunToEnd,
            v1::ClientCommand::Pause => EngineCommand::Pause,
            v1::ClientCommand::Initialise { request } => EngineCommand::Initialize {
                request: request.try_into()?,
            },
            v1::ClientCommand::Cancel => EngineCommand::Cancel,

            v1::ClientCommand::Ping { nonce } => EngineCommand::Ping { nonce },
            v1::ClientCommand::Shutdown => EngineCommand::Shutdown,
        };

        Ok(cmd)
    }
}

#[derive(Debug)]
pub struct InitialiseRequest {
    pub run_name: String,

    // Stable wire representation, not a ModelSchema Rust value.
    pub model: ModelDocument,

    pub data_path: Option<PathBuf>,
    pub output_path: Option<PathBuf>,

    pub solver: SolverConfiguration,
    pub result_options: ResultOptions,
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<v1::InitialiseRequest> for InitialiseRequest {
    type Error = Infallible;

    fn try_from(req: v1::InitialiseRequest) -> Result<Self, Self::Error> {
        Ok(InitialiseRequest {
            run_name: req.run_name,
            model: req.model.try_into()?,
            output_path: req.output_path,
            data_path: req.data_path,
            solver: req.solver.try_into()?,
            result_options: req.result_options.try_into()?,
        })
    }
}

#[derive(Debug)]
pub enum ModelDocument {
    Json(serde_json::Value),
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<v1::ModelDocument> for ModelDocument {
    type Error = Infallible;

    fn try_from(doc: v1::ModelDocument) -> Result<Self, Self::Error> {
        let doc = match doc {
            v1::ModelDocument::Json(json) => ModelDocument::Json(json),
        };

        Ok(doc)
    }
}
#[derive(Debug)]
pub struct SolverConfiguration {}

#[allow(clippy::infallible_try_from)]
impl TryFrom<v1::SolverConfiguration> for SolverConfiguration {
    type Error = Infallible;

    fn try_from(_config: v1::SolverConfiguration) -> Result<Self, Self::Error> {
        Ok(SolverConfiguration {})
    }
}
#[derive(Debug)]
pub struct ResultOptions {
    pub all_nodes_metric_set: Option<AddNodesMetricSet>,
    pub all_edges_metric_set: Option<AddEdgesMetricSet>,
    pub clear_existing_outputs: bool,
    pub snapshot: Option<SnapshotOptions>,
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<v1::ResultOptions> for ResultOptions {
    type Error = Infallible;

    fn try_from(options: v1::ResultOptions) -> Result<Self, Self::Error> {
        Ok(ResultOptions {
            all_nodes_metric_set: options.all_nodes_metric_set.map(|set| set.try_into()).transpose()?,
            all_edges_metric_set: options.all_edges_metric_set.map(|set| set.try_into()).transpose()?,
            clear_existing_outputs: options.clear_existing_outputs,
            snapshot: options.snapshot.map(|set| set.try_into()).transpose()?,
        })
    }
}

#[derive(Debug)]
pub struct AddNodesMetricSet {
    pub name: String,
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<v1::AddNodesMetricSet> for AddNodesMetricSet {
    type Error = Infallible;

    fn try_from(set: v1::AddNodesMetricSet) -> Result<Self, Self::Error> {
        Ok(AddNodesMetricSet { name: set.name })
    }
}

#[derive(Debug)]
pub struct AddEdgesMetricSet {
    pub name: String,
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<v1::AddEdgesMetricSet> for AddEdgesMetricSet {
    type Error = Infallible;

    fn try_from(set: v1::AddEdgesMetricSet) -> Result<Self, Self::Error> {
        Ok(AddEdgesMetricSet { name: set.name })
    }
}

#[derive(Debug)]
pub struct SnapshotOptions {
    pub name: String,
    pub metric_sets: Vec<String>,
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<v1::SnapshotOptions> for SnapshotOptions {
    type Error = Infallible;

    fn try_from(options: v1::SnapshotOptions) -> Result<Self, Self::Error> {
        Ok(SnapshotOptions {
            name: options.name,
            metric_sets: options.metric_sets,
        })
    }
}
