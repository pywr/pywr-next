use crate::command::{InitialiseRequest, ModelDocument, ResultOptions};
use crate::event::{ArrowStreamDescriptor, FinalOutcome, RunFailure, RunFailureStage, RunProgress, RunSummary};
use crate::state::RunTarget;
use pywr_core::models::{Model, ModelFinaliseError, ModelState, ModelStepError, ModelTimings};
use pywr_core::recorders::{ArrowStreamCommit, ArrowStreamOutputBuilder};
use pywr_core::solvers::{ClpSolver, ClpSolverSettings};
use pywr_schema::NetworkSchema;
use std::any::Any;
use std::sync::mpsc::{self, Receiver};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("Failed to deserialise model schema: {0}")]
    ModelSchemaDeserialisationError(#[from] serde_json::Error),
    #[error("Failed to create model builder from schema: {0}")]
    ModelBuilderCreationError(#[from] pywr_schema::ModelSchemaBuildError),
    #[error("Failed to build model from builder: {0}")]
    ModelBuildError(#[from] pywr_core::models::ModelBuilderError),
    #[error("Failed to setup model state: {0}")]
    ModelSetupError(#[from] pywr_core::models::ModelSetupError),
    #[error("Backend already finalised")]
    AlreadyFinalised,
    #[error("Model state not initialised")]
    ModelStateNotInitialised,
    #[error("Model step error: {0}")]
    ModelStepError(#[from] ModelStepError),
    #[error("Model finalisation error: {0}")]
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
    pub arrow_stream: Option<ArrowStreamDescriptor>,
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
            name: nodes_metric_set.name.clone(),
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
            name: edges_metric_set.name.clone(),
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

fn apply_arrow_stream_recorder_to_model_builder(
    network_builder: &mut pywr_core::network::NetworkBuilder,
    output_path: Option<&std::path::Path>,
    result_options: &ResultOptions,
) -> (Option<ArrowStreamDescriptor>, Option<Receiver<ArrowStreamCommit>>) {
    if let Some(options) = &result_options.arrow_stream {
        let filename = match (output_path, options.filename.is_relative()) {
            (Some(output_directory), true) => output_directory.join(&options.filename),
            _ => options.filename.clone(),
        };
        let (commit_sender, commit_receiver) = mpsc::channel();
        let mut recorder_builder =
            ArrowStreamOutputBuilder::new(&options.name, &filename, &options.metric_set, options.batch_size);
        recorder_builder.commit_sender(commit_sender);
        network_builder.recorder(Box::new(recorder_builder));

        (
            Some(ArrowStreamDescriptor {
                name: options.name.clone(),
                filename,
                metric_set: options.metric_set.clone(),
            }),
            Some(commit_receiver),
        )
    } else {
        (None, None)
    }
}

struct PywrState {
    #[allow(clippy::vec_box)] // TODO there's some refinement here with the solver traits that could be improved.
    model_state: ModelState<Vec<Box<ClpSolver>>>,
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

#[derive(Debug, Default)]
pub struct PywrBackend {}

impl RunnerBackend for PywrBackend {
    type Runtime = PywrRuntime;

    fn initialise(&mut self, request: InitialiseRequest) -> Result<Initialised<Self::Runtime>, BackendError> {
        // Try to make the schema from the model document. If it fails, return an error.
        let mut schema: pywr_schema::ModelSchema = match request.model {
            ModelDocument::Json(value) => {
                serde_json::from_value(value).map_err(BackendError::ModelSchemaDeserialisationError)?
            }
        };

        // Apply result options to the schema
        apply_result_options_to_schema(&mut schema.network, &request.result_options)?;

        // Construct the model using the two-stage process.
        let mut model_builder =
            schema.create_model_builder(request.data_path.as_deref(), request.output_path.as_deref())?;

        let (arrow_stream, arrow_stream_commits) = apply_arrow_stream_recorder_to_model_builder(
            model_builder.network_builder(),
            request.output_path.as_deref(),
            &request.result_options,
        );

        let model = model_builder.build()?;

        // Initialise the model state
        let settings = ClpSolverSettings::default();
        let model_state = model.setup(&settings)?;

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
            arrow_stream,
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
