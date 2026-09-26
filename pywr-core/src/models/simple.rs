use crate::models::ModelDomain;
use crate::network::{
    Network, NetworkBuildError, NetworkBuilder, NetworkFinaliseError, NetworkRecorderSaveError,
    NetworkRecorderSetupError, NetworkResult, NetworkSetupError, NetworkSolverSetupError, NetworkState,
    NetworkStepError, NetworkTimings, RunDuration,
};
use crate::parameters::ParameterCollectionIdMismatchError;
use crate::recorders::RecorderInternalState;
use crate::solvers::{MultiStateSolver, MultiStateSolverConfig, Solver, SolverConfig, SolverFeatures};
use crate::timestep::Timestep;
use log::{debug, info};
use rayon::ThreadPool;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use thiserror::Error;

pub struct ModelState<S> {
    current_time_step_idx: usize,
    state: NetworkState,
    recorder_state: Vec<Option<Box<dyn RecorderInternalState>>>,
    solvers: S,
}

impl<S> ModelState<S> {
    pub fn network_state(&self) -> &NetworkState {
        &self.state
    }

    pub fn network_state_mut(&mut self) -> &mut NetworkState {
        &mut self.state
    }

    pub fn recorder_state(&self) -> &Vec<Option<Box<dyn RecorderInternalState>>> {
        &self.recorder_state
    }

    /// Get the current time-step index of the model state.
    pub fn current_time_step_idx(&self) -> usize {
        self.current_time_step_idx
    }
}

/// Errors that can occur when setting up a multi-network model.
#[derive(Debug, Error)]
pub enum ModelSetupError {
    #[error("Failed to setup network.")]
    NetworkSetupError(#[from] Box<NetworkSetupError>),
    #[error("Error setting up recorder for network.")]
    RecorderSetupError(#[from] Box<NetworkRecorderSetupError>),
    #[error("Failed to setup solver for network.")]
    SolverSetupError(#[from] Box<NetworkSolverSetupError>),
}

/// Errors that can occur when stepping through (simulating) a multi-network model.
#[derive(Debug, Error)]
pub enum ModelStepError {
    #[error("No more timesteps")]
    EndOfTimesteps,
    #[error("Error stepping through network at timestep {timestep:#?}.")]
    NetworkStepError {
        timestep: Timestep,
        #[source]
        source: Box<NetworkStepError>,
    },
    #[error("Error saving recorder for network at timestep {timestep:#?}.")]
    RecorderSaveError {
        timestep: Timestep,
        #[source]
        source: Box<NetworkRecorderSaveError>,
    },
    #[error("Error flushing recorder output.")]
    RecorderFlushError {
        #[source]
        source: Box<NetworkRecorderSaveError>,
    },
}

/// Errors that can occur when finalising a multi-network model.
#[derive(Debug, Error)]
pub enum ModelFinaliseError {
    #[error("Error finalising network.")]
    NetworkFinaliseError(#[from] NetworkFinaliseError),
    #[error("Timing data from a different network.")]
    TimingMismatchError {
        #[source]
        source: ParameterCollectionIdMismatchError,
    },
}

#[derive(Debug, Error)]
pub enum ModelRunError {
    #[error("Error setting up model.")]
    SetupError(#[from] ModelSetupError),
    #[error("Error stepping through model.")]
    StepError(#[from] ModelStepError),
    #[error("Error finalising model.")]
    FinaliseError(#[from] ModelFinaliseError),
}

/// Internal struct for tracking model timings.
#[derive(Clone)]
pub struct ModelTimings {
    run_duration: RunDuration,
    network_timings: NetworkTimings,
}

impl ModelTimings {
    pub fn new_with_component_timings(model: &Model) -> Self {
        Self {
            run_duration: RunDuration::start(),
            network_timings: NetworkTimings::new_with_component_timings(&model.network),
        }
    }

    pub fn new_without_component_timings() -> Self {
        Self {
            run_duration: RunDuration::start(),
            network_timings: NetworkTimings::new_without_component_timings(),
        }
    }

    fn finish(&mut self) {
        self.run_duration = self.run_duration.finish();
    }

    /// Print summary statistics of the model run.
    pub fn print_summary_statistics(&self, network: &Network) -> Result<(), ParameterCollectionIdMismatchError> {
        info!("Run timing statistics:");
        info!("{: <24} | {: <10}", "Metric", "Value");
        self.run_duration
            .print_table(self.network_timings.timesteps_completed());
        self.network_timings.print_table(network)?;

        Ok(())
    }

    /// Total duration of the model run in seconds.
    pub fn total_duration(&self) -> f64 {
        self.run_duration.total_duration().as_secs_f64()
    }

    pub fn timesteps_completed(&self) -> usize {
        self.network_timings.timesteps_completed()
    }

    /// Average speed of the model run in timesteps per second.
    pub fn speed(&self) -> f64 {
        self.timesteps_completed() as f64 / self.total_duration()
    }

    pub fn network_timings(&self) -> &NetworkTimings {
        &self.network_timings
    }

    pub fn network_timings_mut(&mut self) -> &mut NetworkTimings {
        &mut self.network_timings
    }
}

/// The results of a model run.
///
/// Only recorders which produced a result will be present.
#[derive(Clone)]
pub struct ModelResult {
    pub domain: ModelDomain,
    pub timings: ModelTimings,
    pub network_result: Arc<NetworkResult>,
}

/// A standard Pywr model containing a single network.
pub struct Model {
    domain: ModelDomain,
    network: Network,
}

impl Model {
    /// Construct a new model from a [`ModelDomain`] and [`Network`].
    pub fn new(domain: ModelDomain, network: Network) -> Self {
        Self { domain, network }
    }

    /// Get a reference to the [`ModelDomain`]
    pub fn domain(&self) -> &ModelDomain {
        &self.domain
    }

    pub fn network(&self) -> &Network {
        &self.network
    }

    /// Wait until asynchronous recorder output saved in previous timesteps has
    /// been flushed.
    pub fn flush_recorders<S>(&self, state: &mut ModelState<S>) -> Result<(), ModelStepError> {
        self.network
            .flush_recorders(&mut state.recorder_state)
            .map_err(|source| ModelStepError::RecorderFlushError {
                source: Box::new(source),
            })
    }

    pub fn required_features(&self) -> HashSet<SolverFeatures> {
        self.network.required_features()
    }

    pub fn network_mut(&mut self) -> &mut Network {
        &mut self.network
    }

    /// Check whether a solver `S` has the required features to run this model.
    pub fn check_solver_features<C>(&self, solver_config: &C) -> bool
    where
        C: SolverConfig,
    {
        self.network.check_solver_features(solver_config)
    }

    /// Check whether a solver `S` has the required features to run this model.
    pub fn check_multi_scenario_solver_features<C>(&self, solver_config: &C) -> bool
    where
        C: MultiStateSolverConfig,
    {
        self.network.check_multi_scenario_solver_features(solver_config)
    }

    pub fn setup<C>(&self, solver_config: &C) -> Result<ModelState<Vec<Box<C::Solver>>>, ModelSetupError>
    where
        C: SolverConfig,
    {
        let timesteps = self.domain.time.timesteps();
        let scenario_indices = self.domain.scenario.indices();

        let state = self
            .network
            .setup_network(timesteps, scenario_indices, 0)
            .map_err(|source| ModelSetupError::NetworkSetupError(Box::new(source)))?;

        let recorder_state = self
            .network
            .setup_recorders(&self.domain)
            .map_err(|source| ModelSetupError::RecorderSetupError(Box::new(source)))?;
        let solvers = self
            .network
            .setup_solver(scenario_indices, &state, solver_config)
            .map_err(|source| ModelSetupError::SolverSetupError(Box::new(source)))?;

        Ok(ModelState {
            current_time_step_idx: 0,
            state,
            recorder_state,
            solvers,
        })
    }

    pub fn setup_multi_scenario<C>(&self, solver_config: &C) -> Result<ModelState<Box<C::Solver>>, ModelSetupError>
    where
        C: MultiStateSolverConfig,
    {
        let timesteps = self.domain.time.timesteps();
        let scenario_indices = self.domain.scenario.indices();

        let state = self
            .network
            .setup_network(timesteps, scenario_indices, 0)
            .map_err(|source| ModelSetupError::NetworkSetupError(Box::new(source)))?;
        let recorder_state = self
            .network
            .setup_recorders(&self.domain)
            .map_err(|source| ModelSetupError::RecorderSetupError(Box::new(source)))?;
        let solvers = self
            .network
            .setup_multi_scenario_solver(scenario_indices, solver_config)
            .map_err(|source| ModelSetupError::SolverSetupError(Box::new(source)))?;

        Ok(ModelState {
            current_time_step_idx: 0,
            state,
            recorder_state,
            solvers,
        })
    }

    pub fn step<S>(
        &self,
        state: &mut ModelState<Vec<Box<S>>>,
        thread_pool: Option<&ThreadPool>,
        timings: &mut NetworkTimings,
    ) -> Result<(), ModelStepError>
    where
        S: Solver,
    {
        let step_start = std::time::Instant::now();

        let timestep = self
            .domain
            .time
            .timesteps()
            .get(state.current_time_step_idx)
            .ok_or(ModelStepError::EndOfTimesteps)?;

        let scenario_indices = self.domain.scenario.indices();
        debug!("Starting timestep {:?}", timestep);

        let solvers = &mut state.solvers;
        let network_state = &mut state.state;

        match thread_pool {
            Some(pool) => {
                // State is mutated in-place
                pool.install(|| {
                    self.network
                        .step_par(timestep, scenario_indices, solvers, network_state, timings)
                })
                .map_err(|source| ModelStepError::NetworkStepError {
                    timestep: *timestep,
                    source: Box::new(source),
                })?
            }
            None => self
                .network
                .step(timestep, scenario_indices, solvers, network_state, timings)
                .map_err(|source| ModelStepError::NetworkStepError {
                    timestep: *timestep,
                    source: Box::new(source),
                })?,
        }

        self.network
            .save_recorders(
                timestep,
                scenario_indices,
                &state.state,
                &mut state.recorder_state,
                timings,
            )
            .map_err(|source| ModelStepError::RecorderSaveError {
                timestep: *timestep,
                source: Box::new(source),
            })?;

        // Finally increment the time-step index
        state.current_time_step_idx += 1;

        timings.complete_step(step_start.elapsed(), scenario_indices.len());

        Ok(())
    }

    pub fn step_multi_scenario<S>(
        &self,
        state: &mut ModelState<Box<S>>,
        thread_pool: &ThreadPool,
        timings: &mut NetworkTimings,
    ) -> Result<(), ModelStepError>
    where
        S: MultiStateSolver,
    {
        let step_start = std::time::Instant::now();

        let timestep = self
            .domain
            .time
            .timesteps()
            .get(state.current_time_step_idx)
            .ok_or(ModelStepError::EndOfTimesteps)?;

        let scenario_indices = self.domain.scenario.indices();
        debug!("Starting timestep {:?}", timestep);

        let solvers = &mut state.solvers;
        let network_state = &mut state.state;

        // State is mutated in-place
        thread_pool
            .install(|| {
                self.network
                    .step_multi_scenario(timestep, scenario_indices, solvers, network_state, timings)
            })
            .map_err(|source| ModelStepError::NetworkStepError {
                timestep: *timestep,
                source: Box::new(source),
            })?;

        self.network
            .save_recorders(
                timestep,
                scenario_indices,
                &state.state,
                &mut state.recorder_state,
                timings,
            )
            .map_err(|source| ModelStepError::RecorderSaveError {
                timestep: *timestep,
                source: Box::new(source),
            })?;

        // Finally increment the time-step index
        state.current_time_step_idx += 1;

        timings.complete_step(step_start.elapsed(), scenario_indices.len());

        Ok(())
    }

    pub fn finalise<S>(
        &self,
        mut state: ModelState<Vec<Box<S>>>,
        mut timings: ModelTimings,
    ) -> Result<ModelResult, ModelFinaliseError>
    where
        S: Solver,
    {
        let network_result = self
            .network
            .finalise(
                self.domain.scenario.indices(),
                state.state.all_metric_set_internal_states_mut(),
                state.recorder_state,
            )
            .map_err(ModelFinaliseError::NetworkFinaliseError)?;

        // End the global timer and print the run statistics
        timings.finish();

        timings
            .print_summary_statistics(&self.network)
            .map_err(|source| ModelFinaliseError::TimingMismatchError { source })?;

        Ok(ModelResult {
            network_result: Arc::new(network_result),
            timings,
            domain: self.domain.clone(),
        })
    }

    pub fn finalise_multi_scenario<S>(
        &self,
        mut state: ModelState<Box<S>>,
        mut timings: ModelTimings,
    ) -> Result<ModelResult, ModelFinaliseError>
    where
        S: MultiStateSolver,
    {
        let network_result = self
            .network
            .finalise(
                self.domain.scenario.indices(),
                state.state.all_metric_set_internal_states_mut(),
                state.recorder_state,
            )
            .map_err(ModelFinaliseError::NetworkFinaliseError)?;

        // End the global timer and print the run statistics
        timings.finish();

        timings
            .print_summary_statistics(&self.network)
            .map_err(|source| ModelFinaliseError::TimingMismatchError { source })?;

        Ok(ModelResult {
            network_result: Arc::new(network_result),
            timings,
            domain: self.domain.clone(),
        })
    }

    /// Run a model through the given time-steps.
    ///
    /// This method will setup state and solvers, and then run the model through the time-steps.
    pub fn run<C>(&self, solver_config: &C) -> Result<ModelResult, ModelRunError>
    where
        C: SolverConfig,
    {
        let mut state = self.setup(solver_config)?;

        let mut timings = ModelTimings::new_with_component_timings(self);

        self.run_with_state(&mut state, solver_config, &mut timings)?;

        let result = self.finalise(state, timings)?;

        Ok(result)
    }

    /// Run the model with the provided states and solvers.
    pub fn run_with_state<C>(
        &self,
        state: &mut ModelState<Vec<Box<C::Solver>>>,
        solver_config: &C,
        timings: &mut ModelTimings,
    ) -> Result<(), ModelRunError>
    where
        C: SolverConfig,
    {
        // Setup thread pool if running in parallel
        let pool = if solver_config.parallel() {
            Some(
                rayon::ThreadPoolBuilder::new()
                    .num_threads(solver_config.threads())
                    .build()
                    .unwrap(),
            )
        } else {
            None
        };

        loop {
            match self.step(state, pool.as_ref(), &mut timings.network_timings) {
                Ok(_) => {}
                Err(ModelStepError::EndOfTimesteps) => break,
                Err(e) => return Err(ModelRunError::StepError(e)),
            }
        }

        Ok(())
    }

    /// Run a network through the given time-steps with [`MultiStateSolver`].
    ///
    /// This method will setup state and the solver, and then run the network through the time-steps.
    pub fn run_multi_scenario<C>(&self, solver_config: &C) -> Result<ModelResult, ModelRunError>
    where
        C: MultiStateSolverConfig,
    {
        // Setup the network and create the initial state
        let mut state = self.setup_multi_scenario(solver_config)?;
        let mut timings = ModelTimings::new_with_component_timings(self);
        self.run_multi_scenario_with_state(&mut state, solver_config, &mut timings)?;

        let result = self.finalise_multi_scenario(state, timings)?;

        Ok(result)
    }

    /// Run the network with the provided states and [`MultiStateSolver`] solver.
    pub fn run_multi_scenario_with_state<C>(
        &self,
        state: &mut ModelState<Box<C::Solver>>,
        solver_config: &C,
        timings: &mut ModelTimings,
    ) -> Result<(), ModelRunError>
    where
        C: MultiStateSolverConfig,
    {
        let num_threads = if solver_config.parallel() {
            solver_config.threads()
        } else {
            1
        };

        // Setup thread pool
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build()
            .unwrap();

        loop {
            match self.step_multi_scenario(state, &pool, &mut timings.network_timings) {
                Ok(_) => {}
                Err(ModelStepError::EndOfTimesteps) => break,
                Err(e) => return Err(ModelRunError::StepError(e)),
            }
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum ModelBuilderError {
    #[error("Error building network.")]
    NetworkBuildError(#[from] NetworkBuildError),
}

pub struct ModelBuilder {
    domain: ModelDomain,
    network: NetworkBuilder,
}

impl ModelBuilder {
    pub fn new(domain: ModelDomain, network: NetworkBuilder) -> Self {
        Self { domain, network }
    }

    /// Get a reference to the [`NetworkBuilder`]
    pub fn network_builder(&mut self) -> &mut NetworkBuilder {
        &mut self.network
    }

    pub fn build(self) -> Result<Model, ModelBuilderError> {
        let (network, _) = self.network.build(&self.domain, &HashMap::new())?;
        Ok(Model {
            domain: self.domain,
            network,
        })
    }
}
