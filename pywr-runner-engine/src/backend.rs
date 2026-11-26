use crate::command::{ArrowStreamOptions, InitialiseRequest, ModelDocument, ResultOptions, Solver};
use crate::event::{ArrowStreamDescriptor, FinalOutcome, RunFailure, RunFailureStage, RunProgress, RunSummary};
use crate::state::RunTarget;
use pywr_core::models::{
    Model, ModelBuilder, ModelDomain, ModelFinaliseError, ModelState, ModelStepError, ModelTimings, MultiNetworkModel,
    MultiNetworkModelBuilder, MultiNetworkModelFinaliseError, MultiNetworkModelSetupError, MultiNetworkModelState,
    MultiNetworkModelStepError, MultiNetworkModelTimings,
};
use pywr_core::recorders::{ArrowStreamCommit, ArrowStreamOutputBuilder, ArrowStreamSink, RunMetadata};
#[cfg(feature = "cbc")]
use pywr_core::solvers::CbcSolverSettings;
#[cfg(feature = "clp")]
use pywr_core::solvers::ClpSolverSettings;
#[cfg(feature = "highs")]
use pywr_core::solvers::HighsSolverSettings;
#[cfg(feature = "microlp")]
use pywr_core::solvers::MicroLpSolverSettings;
use pywr_core::solvers::{BuiltInSolver, BuiltInSolverConfig};
use pywr_schema::{FileProvider, FileSystem, ModelSchema, MultiNetworkModelSchema, NetworkSchema, NetworkSchemaRef};
use std::any::Any;
use std::collections::HashSet;
use std::sync::mpsc::{self, Receiver};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("Failed to deserialise model schema.")]
    ModelSchemaDeserialisationError(#[from] serde_json::Error),
    #[error("Failed to create model builder from schema.")]
    ModelBuilderCreationError(#[from] pywr_schema::ModelSchemaBuildError),
    #[error("Failed to build model from builder.")]
    ModelBuildError(#[from] pywr_core::models::ModelBuilderError),
    #[error("Failed to setup model state.")]
    ModelSetupError(#[from] pywr_core::models::ModelSetupError),
    #[error("Failed to create multi-network model builder from schema.")]
    MultiNetworkModelBuilderCreationError(#[from] pywr_schema::MultiNetworkModelSchemaBuildError),
    #[error("Failed to read a network of the multi-network model.")]
    NetworkReadError(#[from] pywr_schema::NetworkSchemaReadError),
    #[error("Failed to build multi-network model from builder.")]
    MultiNetworkModelBuildError(#[from] pywr_core::models::MultiNetworkModelBuilderError),
    #[error("Failed to setup multi-network model state.")]
    MultiNetworkModelSetupError(#[from] MultiNetworkModelSetupError),
    #[error("Backend already finalised")]
    AlreadyFinalised,
    #[error("Model state not initialised")]
    ModelStateNotInitialised,
    #[error("Requested solver is not enabled in this runner: {0:?}")]
    SolverUnavailable(Solver),
    #[error("More than one Arrow stream is named `{name}`.")]
    DuplicateArrowStreamName { name: String },
    #[error("Arrow stream `{name}` has no filename, and this runner writes streams to files.")]
    ArrowStreamWithoutFilename { name: String },
    #[error("Arrow stream `{name}` names a file, but this runner keeps streams in memory.")]
    ArrowStreamWithFilename { name: String },
    #[error("Arrow stream `{name}` names no network, and the model has several.")]
    ArrowStreamWithoutNetwork { name: String },
    #[error("Arrow stream `{name}` names network `{network}`, but the model has a single network.")]
    ArrowStreamWithNetwork { name: String, network: String },
    #[error("Arrow stream `{name}` names network `{network}`, which is not in the model.")]
    ArrowStreamNetworkNotFound { name: String, network: String },
    #[error("Model step error.")]
    ModelStepError(#[from] ModelStepError),
    #[error("Model finalisation error.")]
    ModelFinalisationError(#[from] ModelFinaliseError),
    #[error("Multi-network model step error.")]
    MultiNetworkModelStepError(#[from] MultiNetworkModelStepError),
    #[error("Multi-network model finalisation error.")]
    MultiNetworkModelFinalisationError(#[from] MultiNetworkModelFinaliseError),
    #[error("Backend panicked: {0}")]
    Panic(String),
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum BackendOperation {
    Initialise,
    Step,
    FlushRecorders,
    Finalise,
    Cancel,
}

impl BackendError {
    pub(crate) fn from_panic_payload(payload: Box<dyn Any + Send>) -> Self {
        let message = if let Some(message) = payload.downcast_ref::<String>() {
            message.clone()
        } else if let Some(message) = payload.downcast_ref::<&'static str>() {
            (*message).to_owned()
        } else {
            "non-string panic payload".to_owned()
        };

        Self::Panic(message)
    }

    pub(crate) fn into_run_failure(self, operation: BackendOperation) -> RunFailure {
        let stage = match (&self, operation) {
            (Self::Panic(_), _) => RunFailureStage::Panic,
            (Self::ModelSchemaDeserialisationError(_), _) => RunFailureStage::SchemaConversion,
            (
                Self::ModelBuilderCreationError(_)
                | Self::ModelBuildError(_)
                | Self::MultiNetworkModelBuilderCreationError(_)
                | Self::NetworkReadError(_)
                | Self::MultiNetworkModelBuildError(_),
                _,
            ) => RunFailureStage::ModelBuild,
            (
                Self::ModelSetupError(pywr_core::models::ModelSetupError::SolverSetupError(_))
                | Self::MultiNetworkModelSetupError(MultiNetworkModelSetupError::SolverSetupError { .. }),
                _,
            ) => RunFailureStage::SolverSetup,
            (Self::MultiNetworkModelSetupError(MultiNetworkModelSetupError::RecorderSetupError { .. }), _) => {
                RunFailureStage::Recorder
            }
            (Self::ModelSetupError(pywr_core::models::ModelSetupError::RecorderSetupError(_)), _) => {
                RunFailureStage::Recorder
            }
            (
                Self::ModelStepError(
                    ModelStepError::RecorderSaveError { .. } | ModelStepError::RecorderFlushError { .. },
                )
                | Self::MultiNetworkModelStepError(
                    MultiNetworkModelStepError::RecorderSaveError { .. }
                    | MultiNetworkModelStepError::RecorderFlushError { .. },
                ),
                _,
            ) => RunFailureStage::Recorder,
            (_, BackendOperation::Initialise) => RunFailureStage::Initialisation,
            (_, BackendOperation::Step) => RunFailureStage::Timestep,
            (_, BackendOperation::FlushRecorders) => RunFailureStage::Recorder,
            (_, BackendOperation::Finalise | BackendOperation::Cancel) => RunFailureStage::Finalisation,
        };
        let timestep = match &self {
            Self::ModelStepError(
                ModelStepError::NetworkStepError { timestep, .. } | ModelStepError::RecorderSaveError { timestep, .. },
            )
            | Self::MultiNetworkModelStepError(
                MultiNetworkModelStepError::NetworkStepError { timestep, .. }
                | MultiNetworkModelStepError::RecorderSaveError { timestep, .. },
            ) => Some(timestep.date),
            _ => None,
        };
        let summary = self.to_string();
        let mut causes = Vec::new();
        let mut source = std::error::Error::source(&self);
        while let Some(error) = source {
            causes.push(error.to_string());
            source = error.source();
        }

        RunFailure {
            stage,
            summary,
            causes,
            timestep,
        }
    }
}

pub trait RunnerBackend {
    type Runtime;

    fn initialise(&mut self, request: InitialiseRequest) -> Result<Initialised<Self::Runtime>, BackendError>;

    fn step(&mut self, runtime: &mut Self::Runtime, target: &RunTarget) -> Result<BackendStep, BackendError>;

    /// Flush recorder output without finalising the model so execution can resume.
    fn flush_recorders(&mut self, runtime: &mut Self::Runtime) -> Result<(), BackendError>;

    fn finalise(&mut self, runtime: &mut Self::Runtime) -> Result<BackendFinalisation, BackendError>;

    fn cancel(&mut self, runtime: &mut Self::Runtime) -> Result<BackendFinalisation, BackendError>;
}

pub struct Initialised<R> {
    pub runtime: R,
    pub progress: RunProgress,
    pub arrow_streams: Vec<ArrowStreamDescriptor>,
    pub arrow_stream_commits: Option<Receiver<ArrowStreamCommit>>,
}

pub struct BackendStep {
    pub outcome: BackendStepOutcome,
    pub progress: RunProgress,
    pub target_reached: bool,
}

pub enum BackendStepOutcome {
    Advanced,
    EndOfTimesteps,
}

pub struct BackendFinalisation {
    pub summary: RunSummary,
}

/// Applies the result options to the network schema, modifying it in place.
fn apply_result_options_to_schema(
    schema: &mut NetworkSchema,
    result_options: &ResultOptions,
) -> Result<(), BackendError> {
    // Add a nodes metric set if requested
    if let Some(nodes_metric_set) = &result_options.all_nodes_metric_set {
        let metric_set = pywr_schema::metric_sets::MetricSet {
            meta: pywr_schema::meta::NamedMeta {
                name: nodes_metric_set.name.clone(),
                ..Default::default()
            },
            metrics: None,
            aggregator: None,
            filters: pywr_schema::metric_sets::MetricSetFilters {
                all_nodes: true,
                all_virtual_nodes: false,
                all_parameters: false,
                all_edges: false,
            },
        };

        if let Some(metric_sets) = &mut schema.metric_sets {
            metric_sets.push(metric_set);
        } else {
            schema.metric_sets = Some(vec![metric_set]);
        }
    }

    // Add an edges metric set if requested
    if let Some(edges_metric_set) = &result_options.all_edges_metric_set {
        let metric_set = pywr_schema::metric_sets::MetricSet {
            meta: pywr_schema::meta::NamedMeta {
                name: edges_metric_set.name.clone(),
                ..Default::default()
            },
            metrics: None,
            aggregator: None,
            filters: pywr_schema::metric_sets::MetricSetFilters {
                all_nodes: false,
                all_virtual_nodes: false,
                all_parameters: false,
                all_edges: true,
            },
        };

        if let Some(metric_sets) = &mut schema.metric_sets {
            metric_sets.push(metric_set);
        } else {
            schema.metric_sets = Some(vec![metric_set]);
        }
    }

    // Clear existing outputs if requested
    if result_options.clear_existing_outputs {
        schema.outputs = None;
    }

    Ok(())
}

/// Applies the result options to every network of a multi-network model, first reading each
/// network given by path so it can be changed.
fn apply_result_options_to_multi_network_schema(
    schema: &mut MultiNetworkModelSchema,
    result_options: &ResultOptions,
    files: &dyn FileProvider,
    data_path: Option<&std::path::Path>,
) -> Result<(), BackendError> {
    schema.inline_network_files(files, data_path)?;
    for entry in &mut schema.networks {
        if let NetworkSchemaRef::Inline(network) = &mut entry.network {
            apply_result_options_to_schema(network, result_options)?;
        }
    }
    Ok(())
}

/// Where each requested Arrow stream is written: its file, resolved against `output_path`, or
/// memory. Each stream must have its own name, since commits are routed by it, a filename
/// exactly when the runner writes streams to files, and a network exactly when the model has
/// several.
fn arrow_stream_sinks(
    result_options: &ResultOptions,
    sink: ArrowStreamSinkKind,
    output_path: Option<&std::path::Path>,
    multi_network: bool,
) -> Result<Vec<ArrowStreamSink>, BackendError> {
    let mut names = HashSet::new();
    let mut sinks = Vec::with_capacity(result_options.arrow_streams.len());
    for options in &result_options.arrow_streams {
        if !names.insert(options.name.as_str()) {
            return Err(BackendError::DuplicateArrowStreamName {
                name: options.name.clone(),
            });
        }
        check_arrow_stream_network(options, multi_network)?;
        sinks.push(match (sink, &options.filename) {
            (ArrowStreamSinkKind::File, Some(filename)) => {
                ArrowStreamSink::File(match (output_path, filename.is_relative()) {
                    (Some(output_directory), true) => output_directory.join(filename),
                    _ => filename.clone(),
                })
            }
            (ArrowStreamSinkKind::File, None) => {
                return Err(BackendError::ArrowStreamWithoutFilename {
                    name: options.name.clone(),
                });
            }
            (ArrowStreamSinkKind::Memory, Some(_)) => {
                return Err(BackendError::ArrowStreamWithFilename {
                    name: options.name.clone(),
                });
            }
            (ArrowStreamSinkKind::Memory, None) => ArrowStreamSink::Memory,
        });
    }
    Ok(sinks)
}

/// Checks that a stream names a network exactly when the model has several.
fn check_arrow_stream_network(options: &ArrowStreamOptions, multi_network: bool) -> Result<(), BackendError> {
    match (multi_network, &options.network) {
        (true, None) => Err(BackendError::ArrowStreamWithoutNetwork {
            name: options.name.clone(),
        }),
        (false, Some(network)) => Err(BackendError::ArrowStreamWithNetwork {
            name: options.name.clone(),
            network: network.clone(),
        }),
        _ => Ok(()),
    }
}

/// The builder of a single-network or multi-network model.
enum PywrModelBuilder {
    Single(ModelBuilder),
    Multi(MultiNetworkModelBuilder),
}

impl PywrModelBuilder {
    /// The builder of the network an Arrow stream records.
    fn network_builder(
        &mut self,
        options: &ArrowStreamOptions,
    ) -> Result<&mut pywr_core::network::NetworkBuilder, BackendError> {
        match (self, &options.network) {
            (Self::Single(builder), None) => Ok(builder.network_builder()),
            (Self::Multi(builder), Some(network)) => builder
                .entry_builder(network)
                .map(|entry| entry.network_builder())
                .ok_or_else(|| BackendError::ArrowStreamNetworkNotFound {
                    name: options.name.clone(),
                    network: network.clone(),
                }),
            (builder, _) => {
                check_arrow_stream_network(options, matches!(builder, Self::Multi(_)))?;
                unreachable!("a stream names a network exactly when the model has several")
            }
        }
    }

    /// Set the metadata describing the run, which the model's Arrow and Parquet outputs record.
    fn run_metadata(&mut self, run_metadata: RunMetadata) {
        match self {
            Self::Single(builder) => {
                builder.run_metadata(run_metadata);
            }
            Self::Multi(builder) => {
                builder.run_metadata(run_metadata);
            }
        }
    }

    fn build(self) -> Result<PywrModel, BackendError> {
        Ok(match self {
            Self::Single(builder) => PywrModel::Single {
                model: Box::new(builder.build()?),
                state: None,
            },
            Self::Multi(builder) => PywrModel::Multi {
                model: builder.build()?,
                state: None,
            },
        })
    }
}

/// Adds a recorder for each requested Arrow stream to the network it records, writing to its
/// sink from [`arrow_stream_sinks`]. Their commits share one channel, each naming its stream.
fn apply_arrow_stream_recorders_to_model_builder(
    model_builder: &mut PywrModelBuilder,
    result_options: &ResultOptions,
    sinks: Vec<ArrowStreamSink>,
) -> Result<(Vec<ArrowStreamDescriptor>, Option<Receiver<ArrowStreamCommit>>), BackendError> {
    if sinks.is_empty() {
        return Ok((Vec::new(), None));
    }

    let (commit_sender, commit_receiver) = mpsc::channel();
    let mut descriptors = Vec::new();
    for (options, sink) in result_options.arrow_streams.iter().zip(sinks) {
        let filename = match &sink {
            ArrowStreamSink::File(filename) => Some(filename.clone()),
            ArrowStreamSink::Memory => None,
        };
        let mut recorder_builder =
            ArrowStreamOutputBuilder::new(&options.name, sink, &options.metric_set, options.batch_size);
        recorder_builder.commit_sender(commit_sender.clone());
        model_builder
            .network_builder(options)?
            .recorder(Box::new(recorder_builder));

        descriptors.push(ArrowStreamDescriptor {
            name: options.name.clone(),
            filename,
            metric_set: options.metric_set.clone(),
            network: options.network.clone(),
        });
    }

    Ok((descriptors, Some(commit_receiver)))
}

#[allow(clippy::vec_box)] // TODO there's some refinement here with the solver traits that could be improved.
type Solvers = Vec<Box<BuiltInSolver>>;

/// A built model, with its state from setup until it is finalised.
enum PywrModel {
    Single {
        model: Box<Model>,
        state: Option<(ModelState<Solvers>, ModelTimings)>,
    },
    Multi {
        model: MultiNetworkModel,
        state: Option<(MultiNetworkModelState<Solvers>, MultiNetworkModelTimings)>,
    },
}

impl PywrModel {
    fn setup(&mut self, solver_config: &BuiltInSolverConfig) -> Result<(), BackendError> {
        match self {
            Self::Single { model, state } => {
                *state = Some((
                    model.setup(solver_config)?,
                    ModelTimings::new_without_component_timings(),
                ));
            }
            Self::Multi { model, state } => {
                let timings = MultiNetworkModelTimings::new_without_component_timings(model);
                *state = Some((model.setup(solver_config)?, timings));
            }
        }
        Ok(())
    }

    fn domain(&self) -> &ModelDomain {
        match self {
            Self::Single { model, .. } => model.domain(),
            Self::Multi { model, .. } => model.domain(),
        }
    }

    /// The index of the next timestep, or `None` once the model is finalised.
    fn current_time_step_idx(&self) -> Option<usize> {
        match self {
            Self::Single { state, .. } => state.as_ref().map(|(state, _)| state.current_time_step_idx()),
            Self::Multi { state, .. } => state.as_ref().map(|(state, _)| state.current_time_step_idx()),
        }
    }

    /// Steps the model one timestep, returning whether there was one left to step.
    fn step(&mut self) -> Result<bool, BackendError> {
        match self {
            Self::Single { model, state } => {
                let (state, timings) = state.as_mut().ok_or(BackendError::ModelStateNotInitialised)?;
                match model.step(state, None, timings.network_timings_mut()) {
                    Ok(()) => Ok(true),
                    Err(ModelStepError::EndOfTimesteps) => Ok(false),
                    Err(error) => Err(error.into()),
                }
            }
            Self::Multi { model, state } => {
                let (state, timings) = state.as_mut().ok_or(BackendError::ModelStateNotInitialised)?;
                match model.step(state, None, timings) {
                    Ok(()) => Ok(true),
                    Err(MultiNetworkModelStepError::EndOfTimesteps) => Ok(false),
                    Err(error) => Err(error.into()),
                }
            }
        }
    }

    fn flush_recorders(&mut self) -> Result<(), BackendError> {
        match self {
            Self::Single { model, state } => {
                let (state, _) = state.as_mut().ok_or(BackendError::ModelStateNotInitialised)?;
                model.flush_recorders(state)?;
            }
            Self::Multi { model, state } => {
                let (state, _) = state.as_mut().ok_or(BackendError::ModelStateNotInitialised)?;
                model.flush_recorders(state)?;
            }
        }
        Ok(())
    }

    /// Finalises the model, discarding its state.
    fn finalise(&mut self) -> Result<(), BackendError> {
        match self {
            Self::Single { model, state } => {
                let (state, timings) = state.take().ok_or(BackendError::AlreadyFinalised)?;
                model.finalise(state, timings)?;
            }
            Self::Multi { model, state } => {
                let (state, timings) = state.take().ok_or(BackendError::AlreadyFinalised)?;
                model.finalise(state, timings)?;
            }
        }
        Ok(())
    }
}

pub struct PywrRuntime {
    model: PywrModel,
}

impl PywrRuntime {
    fn current_progress(&self) -> RunProgress {
        let timesteps = self.model.domain().time().timesteps();
        let completed = self.model.current_time_step_idx().unwrap_or(0);
        RunProgress {
            completed_timesteps: completed as u64,
            total_timesteps: timesteps.len() as u64,
            last_completed_date: completed
                .checked_sub(1)
                .and_then(|idx| timesteps.get(idx))
                .map(|t| t.date),
            next_date: timesteps.get(completed).map(|t| t.date),
        }
    }

    /// Finalises the model, keeping the progress it had reached.
    fn finalise(&mut self) -> Result<RunProgress, BackendError> {
        let progress = self.current_progress();
        self.model.finalise()?;
        Ok(progress)
    }
}

/// The solver settings for a requested built-in solver, if this runner has it.
fn solver_config(solver: Solver) -> Result<BuiltInSolverConfig, BackendError> {
    match solver {
        Solver::Clp => {
            #[cfg(feature = "clp")]
            {
                Ok(BuiltInSolverConfig::Clp(ClpSolverSettings::default()))
            }
            #[cfg(not(feature = "clp"))]
            {
                Err(BackendError::SolverUnavailable(Solver::Clp))
            }
        }
        Solver::Cbc => {
            #[cfg(feature = "cbc")]
            {
                Ok(BuiltInSolverConfig::Cbc(CbcSolverSettings::default()))
            }
            #[cfg(not(feature = "cbc"))]
            {
                Err(BackendError::SolverUnavailable(Solver::Cbc))
            }
        }
        Solver::Highs => {
            #[cfg(feature = "highs")]
            {
                Ok(BuiltInSolverConfig::Highs(HighsSolverSettings::default()))
            }
            #[cfg(not(feature = "highs"))]
            {
                Err(BackendError::SolverUnavailable(Solver::Highs))
            }
        }
        Solver::Microlp => {
            #[cfg(feature = "microlp")]
            {
                Ok(BuiltInSolverConfig::MicroLp(MicroLpSolverSettings::default()))
            }
            #[cfg(not(feature = "microlp"))]
            {
                Err(BackendError::SolverUnavailable(Solver::Microlp))
            }
        }
    }
}

/// Where a [`PywrBackend`] writes a run's Arrow streams.
#[derive(Debug, Clone, Copy)]
pub enum ArrowStreamSinkKind {
    /// Each to the file its request names.
    File,
    /// Each to memory, handed on in its commits' bytes.
    Memory,
}

/// Runs models with pywr, opening their input files through a [`FileProvider`] and writing their
/// Arrow streams to files or memory: from disk and to files by default.
pub struct PywrBackend {
    files: Box<dyn FileProvider + Send>,
    arrow_stream_sink: ArrowStreamSinkKind,
}

impl PywrBackend {
    pub fn new(files: impl FileProvider + Send + 'static, arrow_stream_sink: ArrowStreamSinkKind) -> Self {
        Self {
            files: Box::new(files),
            arrow_stream_sink,
        }
    }
}

impl Default for PywrBackend {
    fn default() -> Self {
        Self::new(FileSystem, ArrowStreamSinkKind::File)
    }
}

impl RunnerBackend for PywrBackend {
    type Runtime = PywrRuntime;

    fn initialise(&mut self, request: InitialiseRequest) -> Result<Initialised<Self::Runtime>, BackendError> {
        let multi_network = matches!(request.model, ModelDocument::MultiNetworkJson(_));
        let arrow_stream_sinks = arrow_stream_sinks(
            &request.result_options,
            self.arrow_stream_sink,
            request.output_path.as_deref(),
            multi_network,
        )?;
        let solver_config = solver_config(request.solver.solver)?;
        let files = self.files.as_ref();
        let data_path = request.data_path.as_deref();
        let output_path = request.output_path.as_deref();

        // Read the model document, apply the result options, and start building the model.
        let mut model_builder = match request.model {
            ModelDocument::Json(value) => {
                let mut schema: ModelSchema = serde_json::from_value(value)?;
                apply_result_options_to_schema(&mut schema.network, &request.result_options)?;
                PywrModelBuilder::Single(schema.create_model_builder(files, data_path, output_path)?)
            }
            ModelDocument::MultiNetworkJson(value) => {
                let mut schema: MultiNetworkModelSchema = serde_json::from_value(value)?;
                apply_result_options_to_multi_network_schema(&mut schema, &request.result_options, files, data_path)?;
                PywrModelBuilder::Multi(schema.create_model_builder(files, data_path, output_path)?)
            }
        };

        // Record the run in the model's outputs, always under the request's run name.
        let mut run_metadata = request.run_metadata.unwrap_or_default();
        run_metadata.run_name = Some(request.run_name);
        model_builder.run_metadata(run_metadata);

        let (arrow_streams, arrow_stream_commits) = apply_arrow_stream_recorders_to_model_builder(
            &mut model_builder,
            &request.result_options,
            arrow_stream_sinks,
        )?;

        let mut model = model_builder.build()?;
        model.setup(&solver_config)?;

        let runtime = PywrRuntime { model };
        let progress = runtime.current_progress();

        Ok(Initialised {
            runtime,
            progress,
            arrow_streams,
            arrow_stream_commits,
        })
    }

    fn step(&mut self, runtime: &mut Self::Runtime, target: &RunTarget) -> Result<BackendStep, BackendError> {
        if !runtime.model.step()? {
            return Ok(BackendStep {
                outcome: BackendStepOutcome::EndOfTimesteps,
                progress: runtime.current_progress(),
                target_reached: true,
            });
        }

        let current_progress = runtime.current_progress();
        let target_reached = target_reached(&current_progress, target);

        // Flush before every Ready transition so all completed output,
        // including a partial Arrow batch, has sent its commit.
        if target_reached {
            self.flush_recorders(runtime)?;
        }

        Ok(BackendStep {
            outcome: BackendStepOutcome::Advanced,
            progress: current_progress,
            target_reached,
        })
    }

    fn flush_recorders(&mut self, runtime: &mut Self::Runtime) -> Result<(), BackendError> {
        runtime.model.flush_recorders()
    }

    fn finalise(&mut self, runtime: &mut Self::Runtime) -> Result<BackendFinalisation, BackendError> {
        let progress = runtime.finalise()?;
        Ok(BackendFinalisation {
            summary: RunSummary {
                outcome: FinalOutcome::Completed,
                progress,
            },
        })
    }

    fn cancel(&mut self, runtime: &mut Self::Runtime) -> Result<BackendFinalisation, BackendError> {
        let progress = runtime.finalise()?;
        Ok(BackendFinalisation {
            summary: RunSummary {
                outcome: FinalOutcome::Cancelled,
                progress,
            },
        })
    }
}

fn target_reached(progress: &RunProgress, target: &RunTarget) -> bool {
    match target {
        RunTarget::Step => true,
        RunTarget::ToEnd => false, // The backend will return EndOfTimesteps when the end is reached, so we don't need to check here.
        RunTarget::ToDatetime(datetime) => {
            if let Some(last_date) = progress.last_completed_date {
                last_date >= *datetime
            } else {
                false
            }
        }
    }
}

#[cfg(all(test, feature = "clp"))]
mod tests {
    use super::*;
    use crate::command::{AddEdgesMetricSet, AddNodesMetricSet, ArrowStreamOptions, SolverConfiguration};
    use pywr_core::recorders::MetricColumnMetadata;
    use pywr_schema::MemoryFiles;
    use std::num::NonZeroUsize;
    use std::path::{Path, PathBuf};

    /// Initialise a model whose time series is at `../data/inflow.csv` from the data path
    /// `site/models`, so it is read from `site/data/inflow.csv`.
    fn initialise(files: MemoryFiles) -> Result<Initialised<PywrRuntime>, BackendError> {
        let model = serde_json::json!({
            "metadata": { "title": "Input files in memory" },
            "time": { "start": "2021-01-01", "end": "2021-01-02", "timestep": { "type": "Days", "days": 1 } },
            "network": {
                "nodes": [
                    {
                        "meta": { "name": "input" },
                        "type": "Input",
                        "max_flow": { "type": "TimeSeries", "name": "inflow", "columns": { "type": "Column", "name": "inflow" } }
                    },
                    { "meta": { "name": "output" }, "type": "Output", "cost": { "type": "Literal", "value": -10.0 } }
                ],
                "edges": [{ "from_node": "input", "to_node": "output" }],
                "time_series": [
                    { "meta": { "name": "inflow" }, "type": "Arrow", "time_col": "date", "path": "../data/inflow.csv" }
                ]
            }
        });

        PywrBackend::new(files, ArrowStreamSinkKind::File).initialise(InitialiseRequest {
            run_name: "test".into(),
            model: ModelDocument::Json(model),
            data_path: Some(PathBuf::from("site/models")),
            output_path: None,
            log_level: None,
            solver: SolverConfiguration { solver: Solver::Clp },
            result_options: ResultOptions {
                all_nodes_metric_set: None,
                all_edges_metric_set: None,
                clear_existing_outputs: false,
                arrow_streams: Vec::new(),
            },
            run_metadata: None,
        })
    }

    #[test]
    fn initialise_reads_input_files_through_the_provider() {
        let mut files = MemoryFiles::default();
        files.insert(
            "site/data/inflow.csv",
            "date,inflow\n2021-01-01,1.0\n2021-01-02,2.0\n".as_bytes(),
        );
        initialise(files).unwrap();
    }

    #[test]
    fn a_missing_input_file_fails_the_model_build() {
        let Err(error) = initialise(MemoryFiles::default()) else {
            panic!("the model built without its input file");
        };
        let failure = error.into_run_failure(BackendOperation::Initialise);
        assert!(matches!(failure.stage, RunFailureStage::ModelBuild));
        assert!(
            failure.causes.iter().any(|cause| cause.contains("inflow.csv")),
            "{:?}",
            failure.causes
        );
    }

    fn stream(name: &str, metric_set: &str) -> ArrowStreamOptions {
        ArrowStreamOptions {
            name: name.into(),
            filename: Some(format!("{name}.arrow").into()),
            metric_set: metric_set.into(),
            batch_size: NonZeroUsize::new(10).unwrap(),
            network: None,
        }
    }

    fn network_stream(name: &str, metric_set: &str, network: &str) -> ArrowStreamOptions {
        ArrowStreamOptions {
            network: Some(network.into()),
            ..stream(name, metric_set)
        }
    }

    /// A two-day model with all-nodes and all-edges metric sets, writing its streams under
    /// `output_path`.
    fn request(output_path: &Path, arrow_streams: Vec<ArrowStreamOptions>) -> InitialiseRequest {
        let model = serde_json::json!({
            "metadata": { "title": "Arrow streams" },
            "time": { "start": "2021-01-01", "end": "2021-01-02", "timestep": { "type": "Days", "days": 1 } },
            "network": {
                "nodes": [
                    { "meta": { "name": "input" }, "type": "Input", "max_flow": { "type": "Literal", "value": 5.0 } },
                    { "meta": { "name": "output" }, "type": "Output", "cost": { "type": "Literal", "value": -10.0 } }
                ],
                "edges": [{ "from_node": "input", "to_node": "output" }]
            }
        });

        InitialiseRequest {
            run_name: "test".into(),
            model: ModelDocument::Json(model),
            data_path: None,
            output_path: Some(output_path.to_path_buf()),
            log_level: None,
            solver: SolverConfiguration { solver: Solver::Clp },
            result_options: ResultOptions {
                all_nodes_metric_set: Some(AddNodesMetricSet { name: "nodes".into() }),
                all_edges_metric_set: Some(AddEdgesMetricSet { name: "edges".into() }),
                clear_existing_outputs: false,
                arrow_streams,
            },
            run_metadata: None,
        }
    }

    #[test]
    fn initialise_adds_a_recorder_for_each_arrow_stream() {
        let output = tempfile::tempdir().unwrap();
        let streams = vec![stream("node-values", "nodes"), stream("edge-values", "edges")];
        let mut backend = PywrBackend::default();
        let mut initialised = backend.initialise(request(output.path(), streams)).unwrap();

        let descriptors: Vec<_> = initialised
            .arrow_streams
            .iter()
            .map(|descriptor| (descriptor.name.as_str(), descriptor.metric_set.as_str()))
            .collect();
        assert_eq!(descriptors, [("node-values", "nodes"), ("edge-values", "edges")]);

        backend.step(&mut initialised.runtime, &RunTarget::Step).unwrap();
        let receiver = initialised.arrow_stream_commits.unwrap();
        let mut committed: Vec<_> = receiver.try_iter().map(|commit| commit.name).collect();
        committed.sort();
        assert_eq!(committed, ["edge-values", "node-values"]);

        let finalisation = backend.finalise(&mut initialised.runtime).unwrap();
        assert_eq!(finalisation.summary.progress.completed_timesteps, 1);
        for descriptor in &initialised.arrow_streams {
            let filename = output.path().join(format!("{}.arrow", descriptor.name));
            assert_eq!(descriptor.filename.as_deref(), Some(filename.as_path()));
            assert!(filename.is_file());
        }
    }

    #[test]
    fn arrow_streams_record_the_run_metadata_under_the_request_run_name() {
        let output = tempfile::tempdir().unwrap();
        let mut request = request(output.path(), vec![stream("node-values", "nodes")]);
        let mut extra = std::collections::BTreeMap::new();
        extra.insert("analyst".to_string(), serde_json::json!("someone"));
        request.run_metadata = Some(pywr_runner_protocol::v1::RunMetadata {
            run_name: Some("ignored".to_string()),
            extra: extra.clone(),
            ..Default::default()
        });
        let mut backend = PywrBackend::default();
        let mut initialised = backend.initialise(request).unwrap();
        backend.step(&mut initialised.runtime, &RunTarget::Step).unwrap();
        backend.finalise(&mut initialised.runtime).unwrap();

        let file = std::fs::File::open(output.path().join("node-values.arrow")).unwrap();
        let reader = arrow::ipc::reader::StreamReader::try_new(file, None).unwrap();
        let schema = reader.schema();
        let metadata = &schema.metadata()[pywr_core::recorders::RUN_METADATA_KEY];
        let recorded: pywr_runner_protocol::v1::RunMetadata = serde_json::from_str(metadata).unwrap();
        assert_eq!(recorded.run_name.as_deref(), Some("test"));
        assert_eq!(recorded.extra, extra);
    }

    #[test]
    fn a_memory_sink_hands_its_bytes_on_in_the_commits() {
        let output = tempfile::tempdir().unwrap();
        let mut backend = PywrBackend::new(FileSystem, ArrowStreamSinkKind::Memory);
        let mut options = stream("node-values", "nodes");
        options.filename = None;
        let mut initialised = backend.initialise(request(output.path(), vec![options])).unwrap();
        assert!(initialised.arrow_streams[0].filename.is_none());

        backend.step(&mut initialised.runtime, &RunTarget::Step).unwrap();
        let receiver = initialised.arrow_stream_commits.unwrap();
        let commit = receiver.try_recv().unwrap();
        assert!(commit.bytes.is_some_and(|bytes| !bytes.is_empty()));

        backend.finalise(&mut initialised.runtime).unwrap();
        let last = receiver.try_iter().last().unwrap();
        assert_eq!(last.row_count, 0);
        assert!(output.path().read_dir().unwrap().next().is_none());
    }

    /// Initialises `streams` with a document that is not a model, so only a check made before the
    /// model is read can refuse them.
    fn refusal(mut backend: PywrBackend, streams: Vec<ArrowStreamOptions>) -> BackendError {
        let mut request = request(Path::new("outputs"), streams);
        request.model = ModelDocument::Json(serde_json::Value::Null);
        backend.initialise(request).err().expect("the streams were accepted")
    }

    #[test]
    fn a_repeated_arrow_stream_name_is_refused_before_the_model_is_read() {
        let error = refusal(
            PywrBackend::default(),
            vec![stream("values", "nodes"), stream("values", "edges")],
        );
        assert!(matches!(error, BackendError::DuplicateArrowStreamName { name } if name == "values"));
    }

    #[test]
    fn a_filename_must_match_where_the_runner_writes_streams() {
        let mut unnamed = stream("values", "nodes");
        unnamed.filename = None;
        let error = refusal(PywrBackend::default(), vec![unnamed]);
        assert!(matches!(error, BackendError::ArrowStreamWithoutFilename { name } if name == "values"));

        let memory = PywrBackend::new(FileSystem, ArrowStreamSinkKind::Memory);
        let error = refusal(memory, vec![stream("values", "nodes")]);
        assert!(matches!(error, BackendError::ArrowStreamWithFilename { name } if name == "values"));
    }

    /// A two-day model of two networks: `network1`, inline, supplies 10 a day to its demand,
    /// which is transferred to `network2` as its inflow. `network2` is read from
    /// `site/network2.json` through the backend's files. Both have an all-nodes metric set.
    fn multi_network_request(output_path: &Path, arrow_streams: Vec<ArrowStreamOptions>) -> InitialiseRequest {
        let model = serde_json::json!({
            "metadata": { "title": "Two networks" },
            "time": { "start": "2021-01-01", "end": "2021-01-02", "timestep": { "type": "Days", "days": 1 } },
            "networks": [
                {
                    "name": "network1",
                    "network": {
                        "nodes": [
                            { "meta": { "name": "supply1" }, "type": "Input", "max_flow": { "type": "Literal", "value": 15.0 } },
                            {
                                "meta": { "name": "demand1" },
                                "type": "Output",
                                "max_flow": { "type": "Literal", "value": 10.0 },
                                "cost": { "type": "Literal", "value": -10.0 }
                            }
                        ],
                        "edges": [{ "from_node": "supply1", "to_node": "demand1" }]
                    },
                    "transfers": []
                },
                {
                    "name": "network2",
                    "network": "network2.json",
                    "transfers": [
                        {
                            "from_network": "network1",
                            "metric": { "type": "Node", "name": "demand1", "attribute": "Inflow" },
                            "name": "inflow"
                        }
                    ]
                }
            ]
        });

        let mut request = request(output_path, arrow_streams);
        request.model = ModelDocument::MultiNetworkJson(model);
        request.data_path = Some(PathBuf::from("site"));
        request.result_options.all_edges_metric_set = None;
        request
    }

    fn multi_network_files() -> MemoryFiles {
        let network2 = serde_json::json!({
            "nodes": [
                { "meta": { "name": "supply2" }, "type": "Input", "max_flow": { "type": "InterNetworkTransfer", "name": "inflow" } },
                {
                    "meta": { "name": "demand2" },
                    "type": "Output",
                    "max_flow": { "type": "Literal", "value": 20.0 },
                    "cost": { "type": "Literal", "value": -10.0 }
                }
            ],
            "edges": [{ "from_node": "supply2", "to_node": "demand2" }]
        });
        let mut files = MemoryFiles::default();
        files.insert("site/network2.json", network2.to_string().into_bytes());
        files
    }

    #[test]
    fn a_missing_network_file_fails_the_model_build() {
        let output = tempfile::tempdir().unwrap();
        let request = multi_network_request(output.path(), Vec::new());
        let Err(error) = PywrBackend::new(MemoryFiles::default(), ArrowStreamSinkKind::File).initialise(request) else {
            panic!("the model built without its network file");
        };
        let failure = error.into_run_failure(BackendOperation::Initialise);
        assert!(matches!(failure.stage, RunFailureStage::ModelBuild));
        assert!(
            failure.causes.iter().any(|cause| cause.contains("network2.json")),
            "{:?}",
            failure.causes
        );
    }

    /// The metadata of each metric column in an Arrow stream file, with its values in each row.
    fn read_metric_columns(filename: &Path) -> Vec<(MetricColumnMetadata, Vec<f64>)> {
        use arrow::array::{Array, AsArray};
        use arrow::datatypes::Float64Type;

        let file = std::fs::File::open(filename).unwrap();
        let batches: Vec<_> = arrow::ipc::reader::StreamReader::try_new(file, None)
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let schema = batches[0].schema();
        schema
            .fields()
            .iter()
            .enumerate()
            .skip(2)
            .map(|(idx, field)| {
                let metadata = serde_json::from_str(&field.metadata()["ARROW:extension:metadata"]).unwrap();
                let values = batches
                    .iter()
                    .flat_map(|batch| {
                        let column = batch.column(idx).as_fixed_size_list();
                        (0..column.len())
                            .map(|row| column.value(row).as_primitive::<Float64Type>().value(0))
                            .collect::<Vec<_>>()
                    })
                    .collect();
                (metadata, values)
            })
            .collect()
    }

    #[test]
    fn a_multi_network_model_writes_a_stream_for_each_named_network() {
        let output = tempfile::tempdir().unwrap();
        let streams = vec![
            network_stream("supplier", "nodes", "network1"),
            network_stream("receiver", "nodes", "network2"),
        ];
        let mut backend = PywrBackend::new(multi_network_files(), ArrowStreamSinkKind::File);
        let mut initialised = backend
            .initialise(multi_network_request(output.path(), streams))
            .unwrap();

        let descriptors: Vec<_> = initialised
            .arrow_streams
            .iter()
            .map(|descriptor| (descriptor.name.as_str(), descriptor.network.as_deref()))
            .collect();
        assert_eq!(
            descriptors,
            [("supplier", Some("network1")), ("receiver", Some("network2"))]
        );
        assert_eq!(initialised.progress.total_timesteps, 2);

        let step = backend.step(&mut initialised.runtime, &RunTarget::Step).unwrap();
        assert_eq!(step.progress.completed_timesteps, 1);
        let receiver = initialised.arrow_stream_commits.unwrap();
        let mut committed: Vec<_> = receiver.try_iter().map(|commit| commit.name).collect();
        committed.sort();
        assert_eq!(committed, ["receiver", "supplier"]);

        loop {
            let step = backend.step(&mut initialised.runtime, &RunTarget::ToEnd).unwrap();
            if matches!(step.outcome, BackendStepOutcome::EndOfTimesteps) {
                break;
            }
        }
        let finalisation = backend.finalise(&mut initialised.runtime).unwrap();
        assert_eq!(finalisation.summary.progress.completed_timesteps, 2);

        for (stream, network, node, expected) in [
            ("supplier", "network1", "demand1", 10.0),
            ("receiver", "network2", "demand2", 10.0),
        ] {
            let columns = read_metric_columns(&output.path().join(format!("{stream}.arrow")));
            assert!(
                columns
                    .iter()
                    .all(|(metadata, _)| metadata.network.as_deref() == Some(network)),
                "{columns:?}"
            );
            let (_, values) = columns
                .iter()
                .find(|(metadata, _)| metadata.name == node)
                .unwrap_or_else(|| panic!("no column for `{node}` in {columns:?}"));
            assert_eq!(values, &[expected, expected]);
        }
    }

    #[test]
    fn a_single_network_model_records_no_network() {
        let output = tempfile::tempdir().unwrap();
        let mut backend = PywrBackend::default();
        let mut initialised = backend
            .initialise(request(output.path(), vec![stream("node-values", "nodes")]))
            .unwrap();
        assert!(initialised.arrow_streams[0].network.is_none());
        backend.step(&mut initialised.runtime, &RunTarget::Step).unwrap();
        backend.finalise(&mut initialised.runtime).unwrap();

        let columns = read_metric_columns(&output.path().join("node-values.arrow"));
        assert!(!columns.is_empty());
        assert!(columns.iter().all(|(metadata, _)| metadata.network.is_none()));
    }

    #[test]
    fn a_stream_names_a_network_exactly_when_the_model_has_several() {
        let error = refusal(
            PywrBackend::default(),
            vec![network_stream("values", "nodes", "network1")],
        );
        assert!(matches!(
            error,
            BackendError::ArrowStreamWithNetwork { name, network } if name == "values" && network == "network1"
        ));

        let mut request = multi_network_request(Path::new("outputs"), vec![stream("values", "nodes")]);
        request.model = ModelDocument::MultiNetworkJson(serde_json::Value::Null);
        let error = PywrBackend::default().initialise(request).err().unwrap();
        assert!(matches!(error, BackendError::ArrowStreamWithoutNetwork { name } if name == "values"));
    }

    #[test]
    fn a_stream_naming_a_network_not_in_the_model_is_refused() {
        let output = tempfile::tempdir().unwrap();
        let streams = vec![network_stream("values", "nodes", "network3")];
        let mut backend = PywrBackend::new(multi_network_files(), ArrowStreamSinkKind::File);
        let error = backend
            .initialise(multi_network_request(output.path(), streams))
            .err()
            .unwrap();
        assert!(matches!(
            error,
            BackendError::ArrowStreamNetworkNotFound { name, network } if name == "values" && network == "network3"
        ));
    }
}
