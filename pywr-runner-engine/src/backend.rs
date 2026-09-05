use crate::command::{InitialiseRequest, ModelDocument, ResultOptions};
use crate::event::{FinalOutcome, RunProgress, RunSummary};
use crate::state::RunTarget;
use pywr_core::models::{Model, ModelFinaliseError, ModelState, ModelStepError, ModelTimings};
use pywr_core::recorders::{SnapshotBuffer, SnapshotRecorderBuilder};
use pywr_core::solvers::{ClpSolver, ClpSolverSettings};
use pywr_schema::NetworkSchema;
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
}

pub trait RunnerBackend {
    type Runtime;

    fn initialise(&mut self, request: InitialiseRequest) -> Result<Initialised<Self::Runtime>, BackendError>;

    fn step(
        &mut self,
        runtime: &mut Self::Runtime,
        snapshots: Option<SnapshotBuffer>,
        target: &RunTarget,
    ) -> Result<BackendStep, BackendError>;

    fn finalise(&mut self, runtime: &mut Self::Runtime) -> Result<BackendFinalisation, BackendError>;

    fn cancel(&mut self, runtime: &mut Self::Runtime) -> Result<BackendFinalisation, BackendError>;
}

pub struct Initialised<R> {
    pub runtime: R,
    pub progress: RunProgress,
    pub snapshots: Option<SnapshotBuffer>,
}

pub struct BackendStep {
    pub outcome: BackendStepOutcome,
    pub progress: RunProgress,
    pub snapshots: Option<SnapshotBuffer>,
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

fn apply_snapshot_recorder_to_model_builder(
    network_builder: &mut pywr_core::network::NetworkBuilder,
    result_options: &ResultOptions,
) -> Result<Option<SnapshotBuffer>, BackendError> {
    let buffer = if let Some(snapshot_options) = &result_options.snapshot {
        let buffer = SnapshotBuffer::new();

        let recorder_builder = SnapshotRecorderBuilder::new(
            &snapshot_options.name,
            snapshot_options.metric_sets.clone(),
            buffer.clone(),
        );

        network_builder.recorder(Box::new(recorder_builder));

        Some(buffer)
    } else {
        None
    };

    Ok(buffer)
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

        // Apply snapshot recorder to the model builder if requested
        let snapshots =
            apply_snapshot_recorder_to_model_builder(model_builder.network_builder(), &request.result_options)?;

        let model = model_builder.build()?;

        // Initialise the model state
        let settings = ClpSolverSettings::default();
        let model_state = model.setup::<ClpSolver>(&settings)?;

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
            snapshots,
        })
    }

    fn step(
        &mut self,
        runtime: &mut Self::Runtime,
        snapshots: Option<SnapshotBuffer>,
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

                Ok(BackendStep {
                    outcome: BackendStepOutcome::Advanced,
                    progress: current_progress,
                    snapshots,
                    target_reached,
                })
            }

            Err(ModelStepError::EndOfTimesteps) => Ok(BackendStep {
                outcome: BackendStepOutcome::EndOfTimesteps,
                progress: runtime.current_progress(),
                snapshots,
                target_reached: true,
            }),

            Err(error) => Err(error.into()),
        }
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
