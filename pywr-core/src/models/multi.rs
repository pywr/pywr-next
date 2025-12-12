use crate::metric::{MetricF64, MetricF64Error};
use crate::models::ModelDomain;
use crate::network::{
    Network, NetworkFinaliseError, NetworkRecorderSaveError, NetworkRecorderSetupError, NetworkResult,
    NetworkSetupError, NetworkSolverSetupError, NetworkState, NetworkTimings, RunDuration,
};
use crate::recorders::RecorderInternalState;
use crate::scenario::ScenarioIndex;
#[cfg(all(feature = "cbc", feature = "pyo3"))]
use crate::solvers::{CbcSolver, build_cbc_settings_py};
#[cfg(all(feature = "ipm-ocl", feature = "pyo3"))]
use crate::solvers::{ClIpmF32Solver, ClIpmF64Solver, ClIpmSolverSettings};
#[cfg(all(feature = "clp", feature = "pyo3"))]
use crate::solvers::{ClpSolver, build_clp_settings_py};
#[cfg(all(feature = "highs", feature = "pyo3"))]
use crate::solvers::{HighsSolver, build_highs_settings_py};
use crate::solvers::{MultiStateSolver, Solver, SolverSettings};
#[cfg(all(feature = "ipm-simd", feature = "pyo3"))]
use crate::solvers::{SimdIpmF64Solver, build_ipm_simd_settings_py};
use crate::state::StateError;
use crate::timestep::Timestep;
#[cfg(feature = "pyo3")]
use pyo3::{
    Bound, PyErr, PyResult, Python,
    exceptions::{PyKeyError, PyRuntimeError},
    pyclass, pymethods,
    types::PyDict,
};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::num::NonZeroUsize;
use std::ops::Deref;
use thiserror::Error;
use tracing::info;

/// An index to another model
///
/// The index is to either a model evaluated before this model, or after this model.
#[derive(Debug, Copy, Clone)]
enum OtherNetworkIndex {
    Before(NonZeroUsize),
    After(NonZeroUsize),
}

impl OtherNetworkIndex {
    fn new(from_idx: usize, to_idx: usize) -> Self {
        match from_idx.cmp(&to_idx) {
            Ordering::Equal => panic!("Cannot create OtherNetworkIndex to self."),
            Ordering::Less => Self::Before(NonZeroUsize::new(to_idx - from_idx).unwrap()),
            Ordering::Greater => Self::After(NonZeroUsize::new(from_idx - to_idx).unwrap()),
        }
    }
}

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct MultiNetworkTransferIndex(pub usize);

impl Deref for MultiNetworkTransferIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for MultiNetworkTransferIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A special parameter that retrieves a value from a metric in another model.
struct MultiNetworkTransfer {
    /// The model to get the value from.
    from_model_idx: OtherNetworkIndex,
    /// The metric to get the value from.
    from_metric: MetricF64,
    /// Optional initial value to use on the first time-step
    initial_value: Option<f64>,
}

struct MultiNetworkEntry {
    name: String,
    network: Network,
    transfers: Vec<MultiNetworkTransfer>,
}

pub struct MultiNetworkModelState<S> {
    current_time_step_idx: usize,
    states: Vec<NetworkState>,
    recorder_states: Vec<Vec<Option<Box<dyn RecorderInternalState>>>>,
    solvers: Vec<S>,
}

/// Errors that can occur when setting up a multi-network model.
#[derive(Debug, Error)]
pub enum MultiNetworkModelSetupError {
    #[error("Failed to setup network `{network}`: {source}")]
    NetworkSetupError {
        network: String,
        #[source]
        source: Box<NetworkSetupError>,
    },
    #[error("Error setting up recorder for network `{network}`: {source}")]
    RecorderSetupError {
        network: String,
        #[source]
        source: Box<NetworkRecorderSetupError>,
    },
    #[error("Failed to setup solver for network `{network}`: {source}")]
    SolverSetupError {
        network: String,
        #[source]
        source: Box<NetworkSolverSetupError>,
    },
}

/// Errors that can occur when stepping through (simulating) a multi-network model.
#[derive(Debug, Error)]
pub enum MultiNetworkModelStepError {
    #[error("Failed to transfer value to `{to_network}`: {source}")]
    TransferError {
        to_network: String,
        #[source]
        source: Box<InterNetworkTransferError>,
    },
    #[error("No more timesteps")]
    EndOfTimesteps,
    #[error("Error saving recorder for network `{network}` at timestep {timestep:#?}: {source}")]
    RecorderSaveError {
        network: String,
        timestep: Timestep,
        #[source]
        source: Box<NetworkRecorderSaveError>,
    },
}

/// Errors that can occur when finalising a multi-network model.
#[derive(Debug, Error)]
pub enum MultiNetworkModelFinaliseError {
    #[error("Error finalising network `{network}`: {source}")]
    NetworkFinaliseError {
        network: String,
        #[source]
        source: Box<NetworkFinaliseError>,
    },
}

#[derive(Debug, Error)]
pub enum MultiNetworkModelRunError {
    #[error("Error setting up multi-network model: {0}")]
    SetupError(#[from] MultiNetworkModelSetupError),
    #[error("Error stepping through multi-network model: {0}")]
    StepError(#[from] Box<MultiNetworkModelStepError>),
    #[error("Error finalising multi-network model: {0}")]
    FinaliseError(#[from] MultiNetworkModelFinaliseError),
}

#[cfg(feature = "pyo3")]
impl From<MultiNetworkModelRunError> for PyErr {
    fn from(err: MultiNetworkModelRunError) -> PyErr {
        PyRuntimeError::new_err(err.to_string())
    }
}

/// Internal struct for tracking model timings.
#[cfg_attr(feature = "pyo3", pyclass)]
#[derive(Clone)]
pub struct MultiNetworkModelTimings {
    run_duration: RunDuration,
    network_timings: HashMap<String, NetworkTimings>,
}

impl MultiNetworkModelTimings {
    fn new_with_component_timings(entries: &[MultiNetworkEntry]) -> Self {
        let network_timings = entries
            .iter()
            .map(|e| (e.name.clone(), NetworkTimings::new_with_component_timings(&e.network)))
            .collect();

        Self {
            run_duration: RunDuration::start(),
            network_timings,
        }
    }

    fn finish(&mut self) {
        self.run_duration = self.run_duration.finish();
    }

    /// Print summary statistics of the model run.
    fn print_summary_statistics(&self, entries: &[MultiNetworkEntry]) {
        info!("Run timing statistics:");
        let total_duration = self.run_duration.total_duration().as_secs_f64();
        info!("{: <24} | {: <10}", "Metric", "Value");
        self.run_duration.print_table();
        for entry in entries {
            let timing = self
                .network_timings
                .get(&entry.name)
                .expect("Network timings not found for network.");
            info!("Network: {}", entry.name);
            timing.print_table(total_duration, &entry.network);
        }
    }
}

#[cfg(feature = "pyo3")]
#[pymethods]
impl MultiNetworkModelTimings {
    /// Total duration of the model run in seconds.
    #[getter]
    fn total_duration(&self) -> f64 {
        self.run_duration.total_duration().as_secs_f64()
    }

    #[getter]
    fn speed(&self) -> f64 {
        self.run_duration.speed()
    }

    fn __repr__(&self) -> String {
        format!(
            "<MultiNetworkModelTimings completed in {:.2} seconds with speed {:.2} time-steps/second>",
            self.total_duration(),
            self.speed()
        )
    }
}

#[derive(Debug, Error)]
pub enum MultiNetworkModelError {
    #[error("Network name `{0}` already exists")]
    NetworkNameAlreadyExists(String),
}

/// The results of a model run.
///
/// Only recorders which produced a result will be present.
#[cfg_attr(feature = "pyo3", pyclass)]
#[derive(Clone)]
pub struct MultiNetworkModelResult {
    pub timings: MultiNetworkModelTimings,
    pub network_results: HashMap<String, NetworkResult>,
}

#[cfg(feature = "pyo3")]
#[pymethods]
impl MultiNetworkModelResult {
    #[getter]
    #[pyo3(name = "timings")]
    fn timings_py(&self) -> MultiNetworkModelTimings {
        self.timings.clone()
    }
    /// Get a reference to the results map.
    #[pyo3(name = "network_results")]
    pub fn network_results_py(&self, name: &str) -> PyResult<NetworkResult> {
        self.network_results
            .get(name)
            .ok_or_else(|| PyKeyError::new_err(format!("Network result `{}` not found", name)))
            .cloned()
    }

    fn __rep__(&self) -> String {
        format!(
            "<MultiNetworkModelResult with {} network results; completed in {:.2} seconds with speed {:.2} time-steps/second>",
            self.network_results.len(),
            self.timings.total_duration(),
            self.timings.speed()
        )
    }
}

/// A MultiNetwork is a collection of models that can be run together.
#[cfg_attr(feature = "pyo3", pyclass)]
pub struct MultiNetworkModel {
    domain: ModelDomain,
    networks: Vec<MultiNetworkEntry>,
}

impl MultiNetworkModel {
    pub fn new(domain: ModelDomain) -> Self {
        Self {
            domain,
            networks: Vec::new(),
        }
    }

    /// Get a reference to the [`ModelDomain`]
    pub fn domain(&self) -> &ModelDomain {
        &self.domain
    }

    /// Get a reference to a network by index.
    pub fn network(&self, idx: usize) -> Option<&Network> {
        self.networks.get(idx).map(|n| &n.network)
    }

    /// Get a mutable reference to a network by index.
    pub fn network_mut(&mut self, idx: usize) -> Option<&mut Network> {
        self.networks.get_mut(idx).map(|n| &mut n.network)
    }

    /// Get the index of a network by name.
    pub fn get_network_index_by_name(&self, name: &str) -> Option<usize> {
        self.networks.iter().position(|n| n.name == name)
    }

    /// Add a [`Network`] to the model. The name must be unique.
    pub fn add_network(&mut self, name: &str, network: Network) -> Result<usize, MultiNetworkModelError> {
        if self.get_network_index_by_name(name).is_some() {
            return Err(MultiNetworkModelError::NetworkNameAlreadyExists(name.to_string()));
        }

        let idx = self.networks.len();
        self.networks.push(MultiNetworkEntry {
            name: name.to_string(),
            network,
            transfers: Vec::new(),
        });

        Ok(idx)
    }

    /// Add a transfer of data from one network to another.
    pub fn add_inter_network_transfer(
        &mut self,
        from_network_idx: usize,
        from_metric: MetricF64,
        to_network_idx: usize,
        initial_value: Option<f64>,
    ) {
        let parameter = MultiNetworkTransfer {
            from_model_idx: OtherNetworkIndex::new(from_network_idx, to_network_idx),
            from_metric,
            initial_value,
        };

        self.networks[to_network_idx].transfers.push(parameter);
    }

    pub fn setup<S>(
        &self,
        settings: &S::Settings,
    ) -> Result<MultiNetworkModelState<Vec<Box<S>>>, MultiNetworkModelSetupError>
    where
        S: Solver,
        <S as Solver>::Settings: SolverSettings,
    {
        let timesteps = self.domain.time.timesteps();
        let scenario_indices = self.domain.scenarios.indices();

        let mut states = Vec::with_capacity(self.networks.len());
        let mut recorder_states = Vec::with_capacity(self.networks.len());
        let mut solvers = Vec::with_capacity(self.networks.len());

        for entry in &self.networks {
            let state = entry
                .network
                .setup_network(timesteps, scenario_indices, entry.transfers.len())
                .map_err(|source| MultiNetworkModelSetupError::NetworkSetupError {
                    network: entry.name.clone(),
                    source: Box::new(source),
                })?;
            let recorder_state = entry.network.setup_recorders(&self.domain).map_err(|source| {
                MultiNetworkModelSetupError::RecorderSetupError {
                    network: entry.name.clone(),
                    source: Box::new(source),
                }
            })?;
            let solver = entry
                .network
                .setup_solver::<S>(scenario_indices, &state, settings)
                .map_err(|source| MultiNetworkModelSetupError::SolverSetupError {
                    network: entry.name.clone(),
                    source: Box::new(source),
                })?;

            states.push(state);
            recorder_states.push(recorder_state);
            solvers.push(solver);
        }

        Ok(MultiNetworkModelState {
            current_time_step_idx: 0,
            states,
            recorder_states,
            solvers,
        })
    }

    pub fn setup_multi_scenario<S>(
        &self,
        settings: &S::Settings,
    ) -> Result<MultiNetworkModelState<Box<S>>, MultiNetworkModelSetupError>
    where
        S: MultiStateSolver,
        <S as MultiStateSolver>::Settings: SolverSettings,
    {
        let timesteps = self.domain.time.timesteps();
        let scenario_indices = self.domain.scenarios.indices();

        let mut states = Vec::with_capacity(self.networks.len());
        let mut recorder_states = Vec::with_capacity(self.networks.len());
        let mut solvers = Vec::with_capacity(self.networks.len());

        for entry in &self.networks {
            let state = entry
                .network
                .setup_network(timesteps, scenario_indices, entry.transfers.len())
                .map_err(|source| MultiNetworkModelSetupError::NetworkSetupError {
                    network: entry.name.clone(),
                    source: Box::new(source),
                })?;

            let recorder_state = entry.network.setup_recorders(&self.domain).map_err(|source| {
                MultiNetworkModelSetupError::RecorderSetupError {
                    network: entry.name.clone(),
                    source: Box::new(source),
                }
            })?;

            let solver = entry
                .network
                .setup_multi_scenario_solver::<S>(scenario_indices, settings)
                .map_err(|source| MultiNetworkModelSetupError::SolverSetupError {
                    network: entry.name.clone(),
                    source: Box::new(source),
                })?;

            states.push(state);
            recorder_states.push(recorder_state);
            solvers.push(solver);
        }

        Ok(MultiNetworkModelState {
            current_time_step_idx: 0,
            states,
            recorder_states,
            solvers,
        })
    }

    /// Compute inter model transfers
    fn compute_inter_network_transfers(
        &self,
        model_idx: usize,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        states: &mut [NetworkState],
    ) -> Result<(), MultiNetworkModelStepError> {
        // Get references to the models before and after this model
        let (before_models, after_models) = self.networks.split_at(model_idx);
        let (this_model, after_models) = after_models.split_first().unwrap();
        // Get references to the states before and after this model
        let (before, after) = states.split_at_mut(model_idx);
        let (this_models_state, after) = after.split_first_mut().unwrap();

        // Compute inter-model transfers for all scenarios
        for scenario_index in scenario_indices.iter() {
            compute_inter_network_transfers(
                timestep,
                scenario_index,
                &this_model.transfers,
                this_models_state,
                before_models,
                before,
                after_models,
                after,
            )
            .map_err(|source| MultiNetworkModelStepError::TransferError {
                to_network: this_model.name.clone(),
                source: Box::new(source),
            })?;
        }

        Ok(())
    }

    /// Perform a single time-step of the multi1-model.
    pub fn step<S>(
        &self,
        state: &mut MultiNetworkModelState<Vec<Box<S>>>,
        timings: &mut MultiNetworkModelTimings,
    ) -> Result<(), MultiNetworkModelStepError>
    where
        S: Solver,
    {
        let timestep = self
            .domain
            .time
            .timesteps()
            .get(state.current_time_step_idx)
            .ok_or(MultiNetworkModelStepError::EndOfTimesteps)?;

        let scenario_indices = self.domain.scenarios.indices();

        for (idx, entry) in self.networks.iter().enumerate() {
            let timing = timings
                .network_timings
                .get_mut(&entry.name)
                .expect("Network timings not found for network.");

            // Perform inter-model state updates
            self.compute_inter_network_transfers(idx, timestep, scenario_indices, &mut state.states)?;

            let sub_model_solvers = state.solvers.get_mut(idx).unwrap();
            let sub_model_states = state.states.get_mut(idx).unwrap();

            // Perform sub-model step
            entry
                .network
                .step(timestep, scenario_indices, sub_model_solvers, sub_model_states, timing)
                .unwrap();

            let sub_model_recorder_states = state.recorder_states.get_mut(idx).unwrap();

            entry
                .network
                .save_recorders(
                    timestep,
                    scenario_indices,
                    sub_model_states,
                    sub_model_recorder_states,
                    timing,
                )
                .map_err(|source| MultiNetworkModelStepError::RecorderSaveError {
                    network: entry.name.clone(),
                    timestep: *timestep,
                    source: Box::new(source),
                })?;
        }

        // Finally increment the time-step index
        state.current_time_step_idx += 1;

        Ok(())
    }

    pub fn step_multi_scenario<S>(
        &self,
        state: &mut MultiNetworkModelState<Box<S>>,
        timings: &mut MultiNetworkModelTimings,
    ) -> Result<(), MultiNetworkModelStepError>
    where
        S: MultiStateSolver,
    {
        let timestep = self
            .domain
            .time
            .timesteps()
            .get(state.current_time_step_idx)
            .ok_or(MultiNetworkModelStepError::EndOfTimesteps)?;

        let scenario_indices = self.domain.scenarios.indices();

        for (idx, entry) in self.networks.iter().enumerate() {
            let timing = timings
                .network_timings
                .get_mut(&entry.name)
                .expect("Network timings not found for network.");

            // Perform inter-model state updates
            self.compute_inter_network_transfers(idx, timestep, scenario_indices, &mut state.states)?;

            let sub_model_solvers = state.solvers.get_mut(idx).unwrap();
            let sub_model_states = state.states.get_mut(idx).unwrap();

            // Perform sub-model step
            entry
                .network
                .step_multi_scenario(timestep, scenario_indices, sub_model_solvers, sub_model_states, timing)
                .unwrap();

            let sub_model_recorder_states = state.recorder_states.get_mut(idx).unwrap();

            entry
                .network
                .save_recorders(
                    timestep,
                    scenario_indices,
                    sub_model_states,
                    sub_model_recorder_states,
                    timing,
                )
                .map_err(|source| MultiNetworkModelStepError::RecorderSaveError {
                    network: entry.name.clone(),
                    timestep: *timestep,
                    source: Box::new(source),
                })?;
        }

        // Finally increment the time-step index
        state.current_time_step_idx += 1;

        Ok(())
    }

    pub fn finalise<S>(
        &self,
        state: MultiNetworkModelState<Vec<Box<S>>>,
        mut timings: MultiNetworkModelTimings,
    ) -> Result<MultiNetworkModelResult, MultiNetworkModelFinaliseError>
    where
        S: Solver,
    {
        let network_results = self
            .networks
            .iter()
            .zip(state.states)
            .zip(state.recorder_states)
            .map(|((entry, mut sub_model_state), sub_model_recorder_states)| {
                let sub_model_ms_states = sub_model_state.all_metric_set_internal_states_mut();

                let result = entry
                    .network
                    .finalise(
                        self.domain.scenarios.indices(),
                        sub_model_ms_states,
                        sub_model_recorder_states,
                    )
                    .map_err(|source| MultiNetworkModelFinaliseError::NetworkFinaliseError {
                        network: entry.name.clone(),
                        source: Box::new(source),
                    })?;

                Ok((entry.name.clone(), result))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;

        timings.finish();
        timings.print_summary_statistics(&self.networks);

        Ok(MultiNetworkModelResult {
            network_results,
            timings,
        })
    }

    pub fn finalise_multi_scenario<S>(
        &self,
        state: MultiNetworkModelState<Box<S>>,
        mut timings: MultiNetworkModelTimings,
    ) -> Result<MultiNetworkModelResult, MultiNetworkModelFinaliseError>
    where
        S: MultiStateSolver,
    {
        let network_results = self
            .networks
            .iter()
            .zip(state.states)
            .zip(state.recorder_states)
            .map(|((entry, mut sub_model_state), sub_model_recorder_states)| {
                let sub_model_ms_states = sub_model_state.all_metric_set_internal_states_mut();

                let result = entry
                    .network
                    .finalise(
                        self.domain.scenarios.indices(),
                        sub_model_ms_states,
                        sub_model_recorder_states,
                    )
                    .map_err(|source| MultiNetworkModelFinaliseError::NetworkFinaliseError {
                        network: entry.name.clone(),
                        source: Box::new(source),
                    })?;

                Ok((entry.name.clone(), result))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;

        timings.finish();
        timings.print_summary_statistics(&self.networks);

        Ok(MultiNetworkModelResult {
            network_results,
            timings,
        })
    }

    /// Run the model through the given time-steps.
    ///
    /// This method will setup state and solvers, and then run the model through the time-steps.
    pub fn run<S>(&self, settings: &S::Settings) -> Result<MultiNetworkModelResult, MultiNetworkModelRunError>
    where
        S: Solver,
        <S as Solver>::Settings: SolverSettings,
    {
        let mut state = self.setup::<S>(settings)?;
        let mut timings = MultiNetworkModelTimings::new_with_component_timings(&self.networks);

        self.run_with_state::<S>(&mut state, settings, &mut timings)?;

        let result = self.finalise(state, timings)?;

        Ok(result)
    }

    /// Run the model with the provided states and solvers.
    pub fn run_with_state<S>(
        &self,
        state: &mut MultiNetworkModelState<Vec<Box<S>>>,
        _settings: &S::Settings,
        timings: &mut MultiNetworkModelTimings,
    ) -> Result<(), MultiNetworkModelRunError>
    where
        S: Solver,
        <S as Solver>::Settings: SolverSettings,
    {
        // TODO: Setup thread pool if running in parallel

        loop {
            match self.step::<S>(state, timings) {
                Ok(_) => {}
                Err(MultiNetworkModelStepError::EndOfTimesteps) => break,
                Err(e) => return Err(MultiNetworkModelRunError::StepError(Box::new(e))),
            }

            timings
                .run_duration
                .complete_scenarios(self.domain.scenarios.indices().len());
        }

        Ok(())
    }

    /// Run the model through the given time-steps.
    ///
    /// This method will setup state and solvers, and then run the model through the time-steps.
    pub fn run_multi_scenario<S>(
        &self,
        settings: &S::Settings,
    ) -> Result<MultiNetworkModelResult, MultiNetworkModelRunError>
    where
        S: MultiStateSolver,
        <S as MultiStateSolver>::Settings: SolverSettings,
    {
        let mut state = self.setup_multi_scenario::<S>(settings)?;
        let mut timings = MultiNetworkModelTimings::new_with_component_timings(&self.networks);

        self.run_multi_scenario_with_state::<S>(&mut state, settings, &mut timings)?;

        let result = self.finalise_multi_scenario(state, timings)?;

        Ok(result)
    }

    /// Run the model with the provided states and solvers.
    pub fn run_multi_scenario_with_state<S>(
        &self,
        state: &mut MultiNetworkModelState<Box<S>>,
        _settings: &S::Settings,
        timings: &mut MultiNetworkModelTimings,
    ) -> Result<(), MultiNetworkModelRunError>
    where
        S: MultiStateSolver,
        <S as MultiStateSolver>::Settings: SolverSettings,
    {
        // TODO: Setup thread pool if running in parallel

        loop {
            match self.step_multi_scenario::<S>(state, timings) {
                Ok(_) => {}
                Err(MultiNetworkModelStepError::EndOfTimesteps) => break,
                Err(e) => return Err(MultiNetworkModelRunError::StepError(Box::new(e))),
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
    fn run_allowing_threads_py<S>(
        &self,
        py: Python<'_>,
        settings: &S::Settings,
    ) -> Result<MultiNetworkModelResult, PyErr>
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
    fn run_multi_allowing_threads_py<S>(
        &self,
        py: Python<'_>,
        settings: &S::Settings,
    ) -> Result<MultiNetworkModelResult, PyErr>
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
impl MultiNetworkModel {
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
    ) -> PyResult<MultiNetworkModelResult> {
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

#[derive(Debug, Error)]
pub enum InterNetworkTransferError {
    #[error("Error retrieving value to transfer to other network: {source}")]
    RetrievingTransferValue {
        other_network: String,
        #[source]
        source: MetricF64Error,
    },
    #[error("Error setting transfer in receiving network: {source}")]
    SettingTransferValue {
        other_network: String,
        #[source]
        source: StateError,
    },
}

/// Calculate inter-model parameters for the given scenario index.
///
///
#[allow(clippy::too_many_arguments)] // This function is not too unreadable with 8 arguments.
fn compute_inter_network_transfers(
    timestep: &Timestep,
    scenario_index: &ScenarioIndex,
    inter_network_transfers: &[MultiNetworkTransfer],
    state: &mut NetworkState,
    before_models: &[MultiNetworkEntry],
    before_states: &[NetworkState],
    after_models: &[MultiNetworkEntry],
    after_states: &[NetworkState],
) -> Result<(), InterNetworkTransferError> {
    // Iterate through all of the inter-model transfers
    for (idx, transfer) in inter_network_transfers.iter().enumerate() {
        // Determine which model and state we are getting the value from
        let (other_network, other_model_state) = match transfer.from_model_idx {
            OtherNetworkIndex::Before(i) => {
                let rev_i = before_states.len() - i.get();
                (&before_models[rev_i], &before_states[rev_i])
            }
            OtherNetworkIndex::After(i) => (&after_models[i.get() - 1], &after_states[i.get() - 1]),
        };

        let value = match timestep.is_first().then_some(transfer.initial_value).flatten() {
            // Use the initial value if it is given and it is the first time-step.
            Some(initial_value) => initial_value,
            // Otherwise, get the value from the other model's state/metric
            None => transfer
                .from_metric
                .get_value(&other_network.network, other_model_state.state(scenario_index))
                .map_err(|source| InterNetworkTransferError::RetrievingTransferValue {
                    other_network: other_network.name.clone(),
                    source,
                })?,
        };

        state
            .state_mut(scenario_index)
            .set_inter_network_transfer_value(MultiNetworkTransferIndex(idx), value)
            .map_err(|source| InterNetworkTransferError::SettingTransferValue {
                other_network: other_network.name.clone(),
                source,
            })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{MultiNetworkModel, MultiNetworkModelTimings};
    use crate::models::ModelDomain;
    use crate::network::Network;
    use crate::scenario::{ScenarioDomainBuilder, ScenarioGroupBuilder};
    use crate::solvers::ClpSolver;
    use crate::test_utils::{default_timestepper, simple_network};

    /// Test basic [`MultiNetworkModel`] functionality by running two independent models.
    #[test]
    fn test_multi_model_step() {
        // Create two simple models
        let timestepper = default_timestepper();

        let mut scenario_builder = ScenarioDomainBuilder::default();
        let scenario_group = ScenarioGroupBuilder::new("test-scenario", 2).build().unwrap();
        scenario_builder = scenario_builder.with_group(scenario_group).unwrap();

        let mut multi_model = MultiNetworkModel::new(ModelDomain::try_from(timestepper, scenario_builder).unwrap());

        let test_scenario_group_idx = multi_model
            .domain()
            .scenarios
            .group_index("test-scenario")
            .expect("Scenario group not found.");

        let mut network1 = Network::default();
        simple_network(&mut network1, test_scenario_group_idx, 2);

        let mut network2 = Network::default();
        simple_network(&mut network2, test_scenario_group_idx, 2);

        let _network1_idx = multi_model.add_network("network1", network1);
        let _network2_idx = multi_model.add_network("network2", network2);

        let mut state = multi_model
            .setup::<ClpSolver>(&Default::default())
            .expect("Failed to setup multi1-model.");

        let mut timings = MultiNetworkModelTimings::new_with_component_timings(&multi_model.networks);

        multi_model
            .step(&mut state, &mut timings)
            .expect("Failed to step multi1-model.")
    }

    #[test]
    fn test_duplicate_network_names() {
        let timestepper = default_timestepper();
        let scenario_collection = ScenarioDomainBuilder::default();

        let mut multi_model = MultiNetworkModel::new(ModelDomain::try_from(timestepper, scenario_collection).unwrap());

        let network = Network::default();
        let _network1_idx = multi_model.add_network("network1", network);
        let network = Network::default();
        let result = multi_model.add_network("network1", network);

        assert!(result.is_err());
    }
}
