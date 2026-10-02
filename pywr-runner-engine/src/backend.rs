use crate::command::{InitialiseRequest, ModelDocument, ResultOptions, Solver};
use crate::event::{ArrowStreamDescriptor, FinalOutcome, RunFailure, RunFailureStage, RunProgress, RunSummary};
use crate::state::RunTarget;
use pywr_core::models::{Model, ModelFinaliseError, ModelState, ModelStepError, ModelTimings};
use pywr_core::recorders::{ArrowStreamCommit, ArrowStreamOutputBuilder};
#[cfg(feature = "cbc")]
use pywr_core::solvers::CbcSolverSettings;
#[cfg(feature = "clp")]
use pywr_core::solvers::ClpSolverSettings;
#[cfg(feature = "highs")]
use pywr_core::solvers::HighsSolverSettings;
#[cfg(feature = "microlp")]
use pywr_core::solvers::MicroLpSolverSettings;
use pywr_core::solvers::{BuiltInSolver, BuiltInSolverConfig};
use pywr_schema::{FileProvider, FileSystem, NetworkSchema};
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
    #[error("Backend already finalised")]
    AlreadyFinalised,
    #[error("Model state not initialised")]
    ModelStateNotInitialised,
    #[error("Requested solver is not enabled in this runner: {0:?}")]
    SolverUnavailable(Solver),
    #[error("More than one Arrow stream is named `{name}`.")]
    DuplicateArrowStreamName { name: String },
    #[error("Model step error.")]
    ModelStepError(#[from] ModelStepError),
    #[error("Model finalisation error.")]
    ModelFinalisationError(#[from] ModelFinaliseError),
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
            (Self::ModelBuilderCreationError(_) | Self::ModelBuildError(_), _) => RunFailureStage::ModelBuild,
            (Self::ModelSetupError(pywr_core::models::ModelSetupError::SolverSetupError(_)), _) => {
                RunFailureStage::SolverSetup
            }
            (Self::ModelSetupError(pywr_core::models::ModelSetupError::RecorderSetupError(_)), _) => {
                RunFailureStage::Recorder
            }
            (
                Self::ModelStepError(
                    ModelStepError::RecorderSaveError { .. } | ModelStepError::RecorderFlushError { .. },
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

    fn step(
        &mut self,
        runtime: &mut Self::Runtime,
        arrow_stream_commits: Option<Receiver<ArrowStreamCommit>>,
        target: &RunTarget,
    ) -> Result<BackendStep, BackendError>;

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
    pub arrow_stream_commits: Option<Receiver<ArrowStreamCommit>>,
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

/// Commits are routed by stream name, so each stream must have its own.
fn check_arrow_stream_names(result_options: &ResultOptions) -> Result<(), BackendError> {
    let mut names = HashSet::new();
    for options in &result_options.arrow_streams {
        if !names.insert(options.name.as_str()) {
            return Err(BackendError::DuplicateArrowStreamName {
                name: options.name.clone(),
            });
        }
    }
    Ok(())
}

/// Adds a recorder for each requested Arrow stream. Their commits share one channel, each
/// naming its stream.
fn apply_arrow_stream_recorders_to_model_builder(
    network_builder: &mut pywr_core::network::NetworkBuilder,
    output_path: Option<&std::path::Path>,
    result_options: &ResultOptions,
) -> (Vec<ArrowStreamDescriptor>, Option<Receiver<ArrowStreamCommit>>) {
    if result_options.arrow_streams.is_empty() {
        return (Vec::new(), None);
    }

    let (commit_sender, commit_receiver) = mpsc::channel();
    let mut descriptors = Vec::new();
    for options in &result_options.arrow_streams {
        let filename = match (output_path, options.filename.is_relative()) {
            (Some(output_directory), true) => output_directory.join(&options.filename),
            _ => options.filename.clone(),
        };
        let mut recorder_builder =
            ArrowStreamOutputBuilder::new(&options.name, &filename, &options.metric_set, options.batch_size);
        recorder_builder.commit_sender(commit_sender.clone());
        network_builder.recorder(Box::new(recorder_builder));

        descriptors.push(ArrowStreamDescriptor {
            name: options.name.clone(),
            filename,
            metric_set: options.metric_set.clone(),
        });
    }

    (descriptors, Some(commit_receiver))
}

struct PywrState {
    #[allow(clippy::vec_box)] // TODO there's some refinement here with the solver traits that could be improved.
    model_state: ModelState<Vec<Box<BuiltInSolver>>>,
    timings: ModelTimings,
}

pub struct PywrRuntime {
    model: Model,
    model_state: Option<PywrState>,
    finalised: bool,
}

impl PywrRuntime {
    fn current_progress(&self) -> RunProgress {
        let total_timesteps = self.model.domain().time().len() as u64;

        if let Some(model_state) = &self.model_state {
            let current_timestep_idx = model_state.model_state.current_time_step_idx();
            let last_completed_date = if current_timestep_idx > 0 {
                self.model
                    .domain()
                    .time()
                    .timesteps()
                    .get(current_timestep_idx - 1)
                    .map(|t| t.date)
            } else {
                None
            };

            let next_date = self
                .model
                .domain()
                .time()
                .timesteps()
                .get(current_timestep_idx)
                .map(|t| t.date);

            RunProgress {
                completed_timesteps: current_timestep_idx as u64,
                total_timesteps,
                last_completed_date,
                next_date,
            }
        } else {
            RunProgress {
                completed_timesteps: 0,
                total_timesteps,
                last_completed_date: None,
                next_date: self.model.domain().time().timesteps().first().map(|t| t.date),
            }
        }
    }
}

/// Runs models with pywr, opening their input files through a [`FileProvider`], from disk by
/// default.
pub struct PywrBackend {
    files: Box<dyn FileProvider + Send>,
}

impl PywrBackend {
    pub fn new(files: impl FileProvider + Send + 'static) -> Self {
        Self { files: Box::new(files) }
    }
}

impl Default for PywrBackend {
    fn default() -> Self {
        Self::new(FileSystem)
    }
}

impl RunnerBackend for PywrBackend {
    type Runtime = PywrRuntime;

    fn initialise(&mut self, request: InitialiseRequest) -> Result<Initialised<Self::Runtime>, BackendError> {
        check_arrow_stream_names(&request.result_options)?;

        // Try to make the schema from the model document. If it fails, return an error.
        let mut schema: pywr_schema::ModelSchema = match request.model {
            ModelDocument::Json(value) => {
                serde_json::from_value(value).map_err(BackendError::ModelSchemaDeserialisationError)?
            }
        };

        // Apply result options to the schema
        apply_result_options_to_schema(&mut schema.network, &request.result_options)?;

        // Construct the model using the two-stage process.
        let mut model_builder = schema.create_model_builder(
            self.files.as_ref(),
            request.data_path.as_deref(),
            request.output_path.as_deref(),
        )?;

        let (arrow_streams, arrow_stream_commits) = apply_arrow_stream_recorders_to_model_builder(
            model_builder.network_builder(),
            request.output_path.as_deref(),
            &request.result_options,
        );

        let model = model_builder.build()?;

        // Initialise the model state using the requested built-in solver.
        let solver_config = match request.solver.solver {
            Solver::Clp => {
                #[cfg(feature = "clp")]
                {
                    BuiltInSolverConfig::Clp(ClpSolverSettings::default())
                }
                #[cfg(not(feature = "clp"))]
                {
                    return Err(BackendError::SolverUnavailable(Solver::Clp));
                }
            }
            Solver::Cbc => {
                #[cfg(feature = "cbc")]
                {
                    BuiltInSolverConfig::Cbc(CbcSolverSettings::default())
                }
                #[cfg(not(feature = "cbc"))]
                {
                    return Err(BackendError::SolverUnavailable(Solver::Cbc));
                }
            }
            Solver::Highs => {
                #[cfg(feature = "highs")]
                {
                    BuiltInSolverConfig::Highs(HighsSolverSettings::default())
                }
                #[cfg(not(feature = "highs"))]
                {
                    return Err(BackendError::SolverUnavailable(Solver::Highs));
                }
            }
            Solver::Microlp => {
                #[cfg(feature = "microlp")]
                {
                    BuiltInSolverConfig::MicroLp(MicroLpSolverSettings::default())
                }
                #[cfg(not(feature = "microlp"))]
                {
                    return Err(BackendError::SolverUnavailable(Solver::Microlp));
                }
            }
        };
        let model_state = model.setup(&solver_config)?;

        let timings = ModelTimings::new_without_component_timings();

        let runtime = PywrRuntime {
            model,
            model_state: Some(PywrState { model_state, timings }),

            finalised: false,
        };

        let progress = runtime.current_progress();

        Ok(Initialised {
            runtime,
            progress,
            arrow_streams,
            arrow_stream_commits,
        })
    }

    fn step(
        &mut self,
        runtime: &mut Self::Runtime,
        arrow_stream_commits: Option<Receiver<ArrowStreamCommit>>,
        target: &RunTarget,
    ) -> Result<BackendStep, BackendError> {
        let result = if let Some(model_state) = &mut runtime.model_state {
            runtime.model.step(
                &mut model_state.model_state,
                None,
                model_state.timings.network_timings_mut(),
            )
        } else {
            return Err(BackendError::ModelStateNotInitialised);
        };

        match result {
            Ok(_) => {
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
                    arrow_stream_commits,
                    target_reached,
                })
            }

            Err(ModelStepError::EndOfTimesteps) => Ok(BackendStep {
                outcome: BackendStepOutcome::EndOfTimesteps,
                progress: runtime.current_progress(),
                arrow_stream_commits,
                target_reached: true,
            }),

            Err(error) => Err(error.into()),
        }
    }

    fn flush_recorders(&mut self, runtime: &mut Self::Runtime) -> Result<(), BackendError> {
        let model_state = runtime
            .model_state
            .as_mut()
            .ok_or(BackendError::ModelStateNotInitialised)?;
        runtime.model.flush_recorders(&mut model_state.model_state)?;
        Ok(())
    }

    fn finalise(&mut self, runtime: &mut Self::Runtime) -> Result<BackendFinalisation, BackendError> {
        if runtime.finalised && runtime.model_state.is_none() {
            return Err(BackendError::AlreadyFinalised);
        }

        let model_state = runtime.model_state.take().ok_or(BackendError::AlreadyFinalised)?;
        let result = runtime.model.finalise(model_state.model_state, model_state.timings);

        runtime.finalised = true;

        match result {
            Ok(_) => {
                let summary = RunSummary {
                    outcome: FinalOutcome::Completed,
                    progress: runtime.current_progress(),
                };

                Ok(BackendFinalisation { summary })
            }
            Err(error) => Err(error.into()),
        }
    }

    fn cancel(&mut self, runtime: &mut Self::Runtime) -> Result<BackendFinalisation, BackendError> {
        if runtime.finalised && runtime.model_state.is_none() {
            return Err(BackendError::AlreadyFinalised);
        }

        let model_state = runtime.model_state.take().ok_or(BackendError::AlreadyFinalised)?;
        runtime.model.finalise(model_state.model_state, model_state.timings)?;
        runtime.finalised = true;

        let summary = RunSummary {
            outcome: FinalOutcome::Cancelled,
            progress: runtime.current_progress(),
        };

        Ok(BackendFinalisation { summary })
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

        PywrBackend::new(files).initialise(InitialiseRequest {
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
            filename: format!("{name}.arrow").into(),
            metric_set: metric_set.into(),
            batch_size: NonZeroUsize::new(10).unwrap(),
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

        let commits = initialised.arrow_stream_commits;
        let step = backend
            .step(&mut initialised.runtime, commits, &RunTarget::Step)
            .unwrap();
        let receiver = step.arrow_stream_commits.unwrap();
        let mut committed: Vec<_> = receiver.try_iter().map(|commit| commit.name).collect();
        committed.sort();
        assert_eq!(committed, ["edge-values", "node-values"]);

        backend.finalise(&mut initialised.runtime).unwrap();
        for descriptor in &initialised.arrow_streams {
            let filename = output.path().join(format!("{}.arrow", descriptor.name));
            assert_eq!(descriptor.filename, filename);
            assert!(filename.is_file());
        }
    }

    #[test]
    fn a_repeated_arrow_stream_name_is_refused_before_the_model_is_read() {
        let streams = vec![stream("values", "nodes"), stream("values", "edges")];
        let mut request = request(Path::new("outputs"), streams);
        request.model = ModelDocument::Json(serde_json::Value::Null);
        let Err(error) = PywrBackend::default().initialise(request) else {
            panic!("two Arrow streams shared a name");
        };
        assert!(matches!(error, BackendError::DuplicateArrowStreamName { name } if name == "values"));
    }
}
