use crate::models::ModelDomain;
use crate::network::{
    Network, NetworkFinaliseError, NetworkRecorderSaveError, NetworkRecorderSetupError, NetworkResult,
    NetworkSetupError, NetworkSolverSetupError, NetworkState, NetworkStepError, NetworkTimings, RunDuration,
};
use crate::recorders::RecorderInternalState;
#[cfg(all(feature = "cbc", feature = "pyo3"))]
use crate::solvers::{CbcSolver, build_cbc_settings_py};
#[cfg(all(feature = "ipm-ocl", feature = "pyo3"))]
use crate::solvers::{ClIpmF32Solver, ClIpmF64Solver, ClIpmSolverSettings};
#[cfg(all(feature = "clp", feature = "pyo3"))]
use crate::solvers::{ClpSolver, build_clp_settings_py};
#[cfg(all(feature = "highs", feature = "pyo3"))]
use crate::solvers::{HighsSolver, build_highs_settings_py};
use crate::solvers::{MultiStateSolver, Solver, SolverFeatures, SolverSettings};
#[cfg(all(feature = "ipm-simd", feature = "pyo3"))]
use crate::solvers::{SimdIpmF64Solver, build_ipm_simd_settings_py};
use crate::timestep::Timestep;
#[cfg(feature = "pyo3")]
use pyo3::{Bound, PyErr, PyResult, Python, exceptions::PyRuntimeError, pyclass, pymethods, types::PyDict};
use rayon::ThreadPool;
use std::collections::HashSet;
use thiserror::Error;
use tracing::{debug, info};

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
}

/// Errors that can occur when setting up a multi-network model.
#[derive(Debug, Error)]
pub enum ModelSetupError {
    #[error("Failed to setup network: {0}")]
    NetworkSetupError(#[from] Box<NetworkSetupError>),
    #[error("Error setting up recorder for network: {0}")]
    RecorderSetupError(#[from] Box<NetworkRecorderSetupError>),
    #[error("Failed to setup solver for network: {0}")]
    SolverSetupError(#[from] Box<NetworkSolverSetupError>),
}

/// Errors that can occur when stepping through (simulating) a multi-network model.
#[derive(Debug, Error)]
pub enum ModelStepError {
    #[error("No more timesteps")]
    EndOfTimesteps,
    #[error("Error stepping through network at timestep {timestep:#?}: {source}")]
    NetworkStepError {
        timestep: Timestep,
        #[source]
        source: Box<NetworkStepError>,
    },
    #[error("Error saving recorder for network at timestep {timestep:#?}: {source}")]
    RecorderSaveError {
        timestep: Timestep,
        #[source]
        source: Box<NetworkRecorderSaveError>,
    },
}

/// Errors that can occur when finalising a multi-network model.
#[derive(Debug, Error)]
pub enum ModelFinaliseError {
    #[error("Error finalising network: {0}")]
    NetworkFinaliseError(#[from] NetworkFinaliseError),
}

#[derive(Debug, Error)]
pub enum ModelRunError {
    #[error("Error setting up model: {0}")]
    SetupError(#[from] ModelSetupError),
    #[error("Error stepping through model: {0}")]
    StepError(#[from] ModelStepError),
    #[error("Error finalising model: {0}")]
    FinaliseError(#[from] ModelFinaliseError),
}

#[cfg(feature = "pyo3")]
impl From<ModelRunError> for PyErr {
    fn from(err: ModelRunError) -> PyErr {
        PyRuntimeError::new_err(err.to_string())
    }
}

/// Internal struct for tracking model timings.
#[cfg_attr(feature = "pyo3", pyclass)]
#[derive(Clone)]
pub struct ModelTimings {
    run_duration: RunDuration,
    network_timings: NetworkTimings,
}

impl ModelTimings {
    pub fn new_with_component_timings(network: &Network) -> Self {
        Self {
            run_duration: RunDuration::start(),
            network_timings: NetworkTimings::new_with_component_timings(network),
        }
    }

    fn finish(&mut self) {
        self.run_duration = self.run_duration.finish();
    }

    /// Print summary statistics of the model run.
    pub fn print_summary_statistics(&self, network: &Network) {
        info!("Run timing statistics:");
        let total_duration = self.run_duration.total_duration().as_secs_f64();
        info!("{: <24} | {: <10}", "Metric", "Value");
        self.run_duration.print_table();
        self.network_timings.print_table(total_duration, network);
    }
}

#[cfg(feature = "pyo3")]
#[pymethods]
impl ModelTimings {
    /// Total duration of the model run in seconds.
    #[getter]
    pub fn total_duration(&self) -> f64 {
        self.run_duration.total_duration().as_secs_f64()
    }

    #[getter]
    pub fn speed(&self) -> f64 {
        self.run_duration.speed()
    }

    fn __repr__(&self) -> String {
        format!(
            "<ModelTimings completed in {:.2} seconds with speed {:.2} time-steps/second>",
            self.total_duration(),
            self.speed()
        )
    }
}

/// The results of a model run.
///
/// Only recorders which produced a result will be present.
#[cfg_attr(feature = "pyo3", pyclass)]
#[derive(Clone)]
pub struct ModelResult {
    pub domain: ModelDomain,
    pub timings: ModelTimings,
    pub network_result: NetworkResult,
}

#[cfg(feature = "pyo3")]
#[pymethods]
impl ModelResult {
    #[getter]
    #[pyo3(name = "timings")]
    fn timings_py(&self) -> ModelTimings {
        self.timings.clone()
    }
    #[getter]
    #[pyo3(name = "network_result")]
    fn network_result_py(&self) -> NetworkResult {
        self.network_result.clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "<ModelResult with {} recorder results; {} scenarios completed in {:.2} seconds with speed {:.2} time-steps/second>",
            self.network_result.len(),
            self.domain.scenarios.len(),
            self.timings.total_duration(),
            self.timings.speed()
        )
    }
}

/// A standard Pywr model containing a single network.
#[cfg_attr(feature = "pyo3", pyclass)]
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

    pub fn required_features(&self) -> HashSet<SolverFeatures> {
        self.network.required_features()
    }

    pub fn network_mut(&mut self) -> &mut Network {
        &mut self.network
    }

    /// Check whether a solver `S` has the required features to run this model.
    pub fn check_solver_features<S>(&self) -> bool
    where
        S: Solver,
    {
        self.network.check_solver_features::<S>()
    }

    /// Check whether a solver `S` has the required features to run this model.
    pub fn check_multi_scenario_solver_features<S>(&self) -> bool
    where
        S: MultiStateSolver,
    {
        self.network.check_multi_scenario_solver_features::<S>()
    }

    pub fn setup<S>(&self, settings: &S::Settings) -> Result<ModelState<Vec<Box<S>>>, ModelSetupError>
    where
        S: Solver,
        <S as Solver>::Settings: SolverSettings,
    {
        let timesteps = self.domain.time.timesteps();
        let scenario_indices = self.domain.scenarios.indices();

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
            .setup_solver::<S>(scenario_indices, &state, settings)
            .map_err(|source| ModelSetupError::SolverSetupError(Box::new(source)))?;

        Ok(ModelState {
            current_time_step_idx: 0,
            state,
            recorder_state,
            solvers,
        })
    }

    pub fn setup_multi_scenario<S>(&self, settings: &S::Settings) -> Result<ModelState<Box<S>>, ModelSetupError>
    where
        S: MultiStateSolver,
        <S as MultiStateSolver>::Settings: SolverSettings,
    {
        let timesteps = self.domain.time.timesteps();
        let scenario_indices = self.domain.scenarios.indices();

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
            .setup_multi_scenario_solver::<S>(scenario_indices, settings)
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
        let timestep = self
            .domain
            .time
            .timesteps()
            .get(state.current_time_step_idx)
            .ok_or(ModelStepError::EndOfTimesteps)?;

        let scenario_indices = self.domain.scenarios.indices();
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
        let timestep = self
            .domain
            .time
            .timesteps()
            .get(state.current_time_step_idx)
            .ok_or(ModelStepError::EndOfTimesteps)?;

        let scenario_indices = self.domain.scenarios.indices();
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

        Ok(())
    }

    pub fn finalise<S>(
        &self,
        mut state: ModelState<Vec<Box<S>>>,
        mut timings: ModelTimings,
    ) -> Result<ModelResult, ModelFinaliseError>
    where
        S: Solver,
        <S as Solver>::Settings: SolverSettings,
    {
        let network_result = self
            .network
            .finalise(
                self.domain.scenarios.indices(),
                state.state.all_metric_set_internal_states_mut(),
                state.recorder_state,
            )
            .map_err(ModelFinaliseError::NetworkFinaliseError)?;

        // End the global timer and print the run statistics
        timings.finish();

        timings.print_summary_statistics(&self.network);

        Ok(ModelResult {
            network_result,
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
        <S as MultiStateSolver>::Settings: SolverSettings,
    {
        let network_result = self
            .network
            .finalise(
                self.domain.scenarios.indices(),
                state.state.all_metric_set_internal_states_mut(),
                state.recorder_state,
            )
            .map_err(ModelFinaliseError::NetworkFinaliseError)?;

        // End the global timer and print the run statistics
        timings.finish();

        timings.print_summary_statistics(&self.network);

        Ok(ModelResult {
            network_result,
            timings,
            domain: self.domain.clone(),
        })
    }

    /// Run a model through the given time-steps.
    ///
    /// This method will setup state and solvers, and then run the model through the time-steps.
    pub fn run<S>(&self, settings: &S::Settings) -> Result<ModelResult, ModelRunError>
    where
        S: Solver,
        <S as Solver>::Settings: SolverSettings,
    {
        let mut state = self.setup::<S>(settings)?;

        let mut timings = ModelTimings::new_with_component_timings(&self.network);

        self.run_with_state::<S>(&mut state, settings, &mut timings)?;

        let result = self.finalise(state, timings)?;

        Ok(result)
    }

    /// Run the model with the provided states and solvers.
    pub fn run_with_state<S>(
        &self,
        state: &mut ModelState<Vec<Box<S>>>,
        settings: &S::Settings,
        timings: &mut ModelTimings,
    ) -> Result<(), ModelRunError>
    where
        S: Solver,
        <S as Solver>::Settings: SolverSettings,
    {
        // Setup thread pool if running in parallel
        let pool = if settings.parallel() {
            Some(
                rayon::ThreadPoolBuilder::new()
                    .num_threads(settings.threads())
                    .build()
                    .unwrap(),
            )
        } else {
            None
        };

        loop {
            match self.step::<S>(state, pool.as_ref(), &mut timings.network_timings) {
                Ok(_) => {}
                Err(ModelStepError::EndOfTimesteps) => break,
                Err(e) => return Err(ModelRunError::StepError(e)),
            }

            timings
                .run_duration
                .complete_scenarios(self.domain.scenarios.indices().len());
        }

        Ok(())
    }

    /// Run a network through the given time-steps with [`MultiStateSolver`].
    ///
    /// This method will setup state and the solver, and then run the network through the time-steps.
    pub fn run_multi_scenario<S>(&self, settings: &S::Settings) -> Result<ModelResult, ModelRunError>
    where
        S: MultiStateSolver,
        <S as MultiStateSolver>::Settings: SolverSettings,
    {
        // Setup the network and create the initial state
        let mut state = self.setup_multi_scenario(settings)?;
        let mut timings = ModelTimings::new_with_component_timings(&self.network);
        self.run_multi_scenario_with_state::<S>(&mut state, settings, &mut timings)?;

        let result = self.finalise_multi_scenario(state, timings)?;

        Ok(result)
    }

    /// Run the network with the provided states and [`MultiStateSolver`] solver.
    pub fn run_multi_scenario_with_state<S>(
        &self,
        state: &mut ModelState<Box<S>>,
        settings: &S::Settings,
        timings: &mut ModelTimings,
    ) -> Result<(), ModelRunError>
    where
        S: MultiStateSolver,
        <S as MultiStateSolver>::Settings: SolverSettings,
    {
        let num_threads = if settings.parallel() { settings.threads() } else { 1 };

        // Setup thread pool
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build()
            .unwrap();

        loop {
            match self.step_multi_scenario::<S>(state, &pool, &mut timings.network_timings) {
                Ok(_) => {}
                Err(ModelStepError::EndOfTimesteps) => break,
                Err(e) => return Err(ModelRunError::StepError(e)),
            }

            timings
                .run_duration
                .complete_scenarios(self.domain.scenarios.indices().len());
        }

        Ok(())
    }

    /// Run a model using the specified solver unlocking the GIL
    #[cfg(any(feature = "clp", feature = "highs"))]
    #[cfg(feature = "pyo3")]
    fn run_allowing_threads_py<S>(&self, py: Python<'_>, settings: &S::Settings) -> Result<ModelResult, PyErr>
    where
        S: Solver,
        <S as Solver>::Settings: SolverSettings + Sync,
    {
        let result = py.detach(|| self.run::<S>(settings))?;
        Ok(result)
    }

    /// Run a model using the specified multi solver unlocking the GIL
    #[cfg(any(feature = "ipm-simd", feature = "ipm-ocl"))]
    #[cfg(feature = "pyo3")]
    fn run_multi_allowing_threads_py<S>(&self, py: Python<'_>, settings: &S::Settings) -> Result<ModelResult, PyErr>
    where
        S: MultiStateSolver,
        <S as MultiStateSolver>::Settings: SolverSettings + Sync,
    {
        let result = py.detach(|| self.run_multi_scenario::<S>(settings))?;
        Ok(result)
    }
}

/// Run a model using the specified multi solver unlocking the GIL
#[cfg(feature = "pyo3")]
#[pymethods]
impl Model {
    #[pyo3(name = "run", signature = (solver_name, solver_kwargs=None))]
    fn run_py(
        &self,
        #[cfg_attr(
            not(any(feature = "clp", feature = "highs", feature = "ipm-simd", feature = "ipm-ocl")),
            allow(unused_variables)
        )]
        py: Python<'_>,
        #[cfg_attr(
            not(any(feature = "clp", feature = "highs", feature = "ipm-simd", feature = "ipm-ocl")),
            allow(unused_variables)
        )]
        solver_name: &str,
        #[cfg_attr(
            not(any(feature = "clp", feature = "highs", feature = "ipm-simd", feature = "ipm-ocl")),
            allow(unused_variables)
        )]
        solver_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<ModelResult> {
        match solver_name {
            #[cfg(feature = "clp")]
            "clp" => {
                let settings = build_clp_settings_py(solver_kwargs)?;
                self.run_allowing_threads_py::<ClpSolver>(py, &settings)
            }
            #[cfg(feature = "cbc")]
            "cbc" => {
                let settings = build_cbc_settings_py(solver_kwargs)?;
                self.run_allowing_threads_py::<CbcSolver>(py, &settings)
            }
            #[cfg(feature = "highs")]
            "highs" => {
                let settings = build_highs_settings_py(solver_kwargs)?;
                self.run_allowing_threads_py::<HighsSolver>(py, &settings)
            }
            #[cfg(feature = "ipm-simd")]
            "ipm-simd" => {
                let settings = build_ipm_simd_settings_py(solver_kwargs)?;
                self.run_multi_allowing_threads_py::<SimdIpmF64Solver>(py, &settings)
            }
            #[cfg(feature = "ipm-ocl")]
            "clipm-f32" => self.run_multi_allowing_threads_py::<ClIpmF32Solver>(py, &ClIpmSolverSettings::default()),

            #[cfg(feature = "ipm-ocl")]
            "clipm-f64" => self.run_multi_allowing_threads_py::<ClIpmF64Solver>(py, &ClIpmSolverSettings::default()),
            _ => Err(PyRuntimeError::new_err(format!("Unknown solver: {solver_name}",))),
        }
    }
}
