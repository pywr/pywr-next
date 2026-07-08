use crate::aggregated_node::{AggregatedNode, AggregatedNodeBuilder, AggregatedNodeBuilderError};
use crate::aggregated_storage_node::{
    AggregatedStorageNode, AggregatedStorageNodeBuilder, AggregatedStorageNodeBuilderError,
};
use crate::edge::Edge;
use crate::models::{ModelDomain, MultiNetworkTransferIndex};
use crate::node::{Node, NodeBuilder, NodeBuilderError, NodeError, UnresolvedNode};
use crate::parameters::{
    GeneralParameterIndex, GeneralParameterType, ParameterCollection, ParameterCollectionBuilder,
    ParameterCollectionBuilderError, ParameterCollectionConstCalculationError, ParameterCollectionError,
    ParameterCollectionGeneralCalculationError, ParameterCollectionSetupError,
    ParameterCollectionSimpleCalculationError, ParameterIndex, ParameterName, ParameterStates, ParameterTiming,
    ParameterTimings, VariableConfig,
};
use crate::recorders::{
    MetricSet, MetricSetBuilder, MetricSetBuilderError, MetricSetSaveError, MetricSetState, RecorderAggregationError,
    RecorderBuilder, RecorderBuilderError, RecorderFinalResult, RecorderFinaliseError, RecorderInternalState,
    RecorderSaveError, RecorderSetupError,
};
use crate::scenario::ScenarioIndex;
use crate::solvers::{
    MultiStateSolver, Solver, SolverFeatures, SolverSettings, SolverSetupError, SolverSolveError, SolverTimings,
};
use crate::state::{MultiValue, State, StateBuilder};
use crate::timestep::Timestep;
use crate::virtual_storage::{
    VirtualStorageError, VirtualStorageNode, VirtualStorageNodeBuilder, VirtualStorageNodeBuilderError,
};
use crate::{parameters, recorders};
#[cfg(feature = "pyo3")]
use pyo3::{PyResult, exceptions::PyKeyError, pyclass, pymethods};
#[cfg(feature = "pyo3")]
use pyo3_polars::PyDataFrame;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::slice::{Iter, IterMut};
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use thiserror::Error;
use tracing::info;

#[derive(Copy, Clone)]
pub enum RunDuration {
    Running {
        /// The instant the run was started.
        started: Instant,
        /// The number of time steps completed so far.
        timesteps_completed: usize,
    },
    Finished {
        /// The total duration of the run.
        duration: Duration,
        /// The total number of time steps completed.
        timesteps_completed: usize,
    },
}

impl RunDuration {
    /// Start the global timer for this timing instance.
    pub fn start() -> Self {
        RunDuration::Running {
            started: Instant::now(),
            timesteps_completed: 0,
        }
    }

    /// Increment the number of completed scenarios by `num`.
    ///
    /// This has no effect if the run has already finished.
    pub fn complete_scenarios(&mut self, num: usize) {
        if let RunDuration::Running {
            timesteps_completed, ..
        } = self
        {
            *timesteps_completed += num;
        }
    }

    /// End the global timer for this timing instance.
    ///
    /// If the timer has already finished this method has no effect.
    pub fn finish(self) -> Self {
        if let RunDuration::Running {
            started,
            timesteps_completed,
        } = self
        {
            RunDuration::Finished {
                duration: started.elapsed(),
                timesteps_completed,
            }
        } else {
            self
        }
    }

    /// Returns the total duration of the run, whether it is still running or has finished.
    pub fn total_duration(&self) -> Duration {
        match self {
            RunDuration::Running { started, .. } => started.elapsed(),
            RunDuration::Finished { duration, .. } => *duration,
        }
    }

    /// Returns the speed of the run in terms of time steps per second.
    pub fn speed(&self) -> f64 {
        match self {
            RunDuration::Running {
                started,
                timesteps_completed,
            } => *timesteps_completed as f64 / started.elapsed().as_secs_f64(),
            RunDuration::Finished {
                duration,
                timesteps_completed,
            } => *timesteps_completed as f64 / duration.as_secs_f64(),
        }
    }

    /// Prints a summary of the run duration and speed to the log.
    pub fn print_table(&self) {
        info!("{: <24} | {: <10.5} s", "Total", self.total_duration().as_secs_f64());
        info!("{: <24} | {: <10.5} ts/s", "Speed", self.speed());
    }
}

/// Collect timing information for component of a network.
#[derive(Clone)]
pub struct ComponentTimings {
    /// Timing information for parameters.
    parameters: Option<ParameterTimings>,
    /// Total time spent in component calculations.
    total: Duration,
}

impl ComponentTimings {
    pub fn new(parameters: Option<ParameterTimings>) -> Self {
        Self {
            parameters,
            total: Default::default(),
        }
    }

    /// Returns the slowest `n` components and their duration, if timing information is available.
    ///
    /// This includes both "calculation" and "after" duration.
    pub fn slowest_components(
        &self,
        n: usize,
        collection: &ParameterCollection,
    ) -> Option<Vec<(ParameterName, ParameterTiming)>> {
        self.parameters
            .as_ref()
            .map(|p| p.slowest_parameters_named(n, collection))
    }
}

/// Collects timing information for a network
#[derive(Clone)]
pub struct NetworkTimings {
    /// Timing information for component calculations.
    component_timings: ComponentTimings,
    recorder_saving: Duration,
    solve: SolverTimings,
}

impl NetworkTimings {
    pub fn new_with_component_timings(network: &Network) -> Self {
        let parameter_timings = ParameterTimings::from_collection(&network.parameters);
        Self {
            component_timings: ComponentTimings::new(Some(parameter_timings)),
            recorder_saving: Duration::ZERO,
            solve: SolverTimings::default(),
        }
    }

    pub fn new_without_component_timings() -> Self {
        Self {
            component_timings: ComponentTimings::new(None),
            recorder_saving: Duration::ZERO,
            solve: SolverTimings::default(),
        }
    }

    /// Print a summary of the timings to the log.
    pub fn print_table(&self, total_duration: f64, network: &Network) {
        info!(
            "{: <24} | {: <10.5}s ({:5.2}%)",
            "Components calcs",
            self.component_timings.total.as_secs_f64(),
            100.0 * self.component_timings.total.as_secs_f64() / total_duration,
        );

        info!(
            "{: <24} | {: <10.5}s ({:5.2}%)",
            "Recorder save",
            self.recorder_saving.as_secs_f64(),
            100.0 * self.recorder_saving.as_secs_f64() / total_duration,
        );

        info!(
            "{: <24} | {: <10.5}s ({:5.2}%)",
            "Solver::obj update",
            self.solve.update_objective.as_secs_f64(),
            100.0 * self.solve.update_objective.as_secs_f64() / total_duration,
        );

        info!(
            "{: <24} | {: <10.5}s ({:5.2}%)",
            "Solver::const update",
            self.solve.update_constraints.as_secs_f64(),
            100.0 * self.solve.update_constraints.as_secs_f64() / total_duration
        );

        info!(
            "{: <24} | {: <10.5}s ({:5.2}%)",
            "Solver::solve",
            self.solve.solve.as_secs_f64(),
            100.0 * self.solve.solve.as_secs_f64() / total_duration,
        );

        info!(
            "{: <24} | {: <10.5}s ({:5.2}%)",
            "Solver::result update",
            self.solve.save_solution.as_secs_f64(),
            100.0 * self.solve.save_solution.as_secs_f64() / total_duration,
        );

        // Difference between total and the parts counted in the timings
        let not_counted = total_duration
            - self.component_timings.total.as_secs_f64()
            - self.recorder_saving.as_secs_f64()
            - self.solve.total().as_secs_f64();

        info!(
            "{: <24} | {: <10.5}s ({:5.2}%)",
            "Residual",
            not_counted,
            100.0 * not_counted / total_duration,
        );

        if let Some(slowest) = self.component_timings.slowest_components(10, &network.parameters) {
            info!("Slowest components:");
            info!(
                "  {: <24} | {: <10}  | {: <10}  | {: <10}  | {:5}",
                "Component", "before", "after", "total", "% of total"
            );
            for (name, duration) in slowest {
                info!(
                    "  {: <24} | {: <10.5}s | {: <10.5}s | {: <10.5}s | {:5.2}%",
                    name.to_string(),
                    duration.before().as_secs_f64(),
                    duration.after().as_secs_f64(),
                    duration.total().as_secs_f64(),
                    100.0 * duration.total().as_secs_f64() / total_duration,
                );
            }
        }
    }
}

#[derive(Hash, PartialEq, Eq, Copy, Clone)]
pub enum ComponentType {
    Node(NodeIndex),
    VirtualStorageNode(VirtualStorageIndex),
    Parameter(GeneralParameterType),
}

impl ComponentType {
    pub fn name(&self, network: &Network) -> String {
        match self {
            ComponentType::Node(idx) => network.get_node(idx).unwrap().name().to_string(),
            ComponentType::VirtualStorageNode(idx) => network.get_virtual_storage_node(idx).unwrap().name().to_string(),
            ComponentType::Parameter(p_type) => match p_type {
                GeneralParameterType::Parameter(idx) => {
                    network.parameters.get_general_f64(*idx).unwrap().name().to_string()
                }
                GeneralParameterType::Index(idx) => {
                    network.parameters.get_general_u64(*idx).unwrap().name().to_string()
                }
                GeneralParameterType::Multi(idx) => {
                    network.parameters.get_general_multi(idx).unwrap().name().to_string()
                }
            },
        }
    }
}

/// Internal states for each scenario and recorder.
pub struct NetworkState {
    // State by scenario
    states: Vec<State>,
    // Parameter state by scenario
    parameter_internal_states: Vec<ParameterStates>,
    // Metric set states by scenario
    metric_set_internal_states: Vec<Vec<MetricSetState>>,
}

impl NetworkState {
    pub fn state(&self, scenario_index: &ScenarioIndex) -> &State {
        &self.states[scenario_index.simulation_id()]
    }

    pub fn state_mut(&mut self, scenario_index: &ScenarioIndex) -> &mut State {
        &mut self.states[scenario_index.simulation_id()]
    }

    pub fn parameter_states(&self, scenario_index: &ScenarioIndex) -> &ParameterStates {
        &self.parameter_internal_states[scenario_index.simulation_id()]
    }

    pub fn parameter_states_mut(&mut self, scenario_index: &ScenarioIndex) -> &mut ParameterStates {
        &mut self.parameter_internal_states[scenario_index.simulation_id()]
    }

    /// Returns an iterator of immutable parameter states for each scenario.
    pub fn iter_parameter_states(&self) -> Iter<'_, ParameterStates> {
        self.parameter_internal_states.iter()
    }

    /// Returns an iterator that allows modifying the parameter states for each scenario.
    pub fn iter_parameter_states_mut(&mut self) -> IterMut<'_, ParameterStates> {
        self.parameter_internal_states.iter_mut()
    }

    pub fn all_metric_set_internal_states_mut(&mut self) -> &mut [Vec<MetricSetState>] {
        &mut self.metric_set_internal_states
    }
}

#[derive(Debug, Error)]
pub enum NetworkSetupError {
    #[error("Error setting up recorder `{}`: `{}`", .0.name, .0.source)]
    RecorderSetupError(#[from] NetworkRecorderSetupError),
    #[error("Error setting up parameters: `{0}`")]
    ParameterSetupError(#[from] ParameterCollectionSetupError),
    #[error("Error computing constant parameters: `{0}`")]
    ConstantParameterCalculationError(#[from] ParameterCollectionConstCalculationError),
}

#[derive(Debug, Error)]
pub enum NetworkStepError {
    #[error("Aggregated node index not found: {0}")]
    AggregatedNodeIndexNotFound(AggregatedNodeIndex),
    #[error("Error saving recorder `{}`: `{}`", .0.name, .0.source)]
    RecorderSaveError(#[from] NetworkRecorderSaveError),
    #[error("Error solving time-step: `{0}`")]
    SolverError(#[from] SolverSolveError),
    #[error("Error computing simple parameters: `{0}`")]
    SimpleParameterCalculationError(#[from] ParameterCollectionSimpleCalculationError),
    #[error("Node index not found: {0}")]
    NodeIndexNotFound(NodeIndex),
    #[error("Error performing `before` method on node `{name}`: `{source}`")]
    NodeBeforeError {
        name: String,
        #[source]
        source: NodeError,
    },
    #[error("Virtual storage index not found: {0}")]
    VirtualStorageIndexNotFound(VirtualStorageIndex),
    #[error("Error performing `before` method on virtual storage `{name}`: `{source}`")]
    VirtualStorageBeforeError {
        name: String,
        #[source]
        source: VirtualStorageError,
    },
    #[error("General parameter F64 index '{0}' not found.")]
    ParameterF64IndexNotFound(GeneralParameterIndex<f64>),
    #[error("General parameter U64 index '{0}' not found.")]
    ParameterU64IndexNotFound(GeneralParameterIndex<u64>),
    #[error("General parameter Multi index '{0}' not found.")]
    ParameterMultiIndexNotFound(GeneralParameterIndex<MultiValue>),
    #[error("Error computing general parameters: `{0}`")]
    GeneralParameterCalculationError(#[from] Box<ParameterCollectionGeneralCalculationError>),
    #[error("Error saving metric set `{name}`: `{source}`")]
    MetricSetSaveError {
        name: String,
        #[source]
        source: MetricSetSaveError,
    },
}

#[derive(Debug, Error)]
pub enum NetworkFinaliseError {
    #[error("Error finalising recorder `{}`: `{}`", .0.name, .0.source)]
    RecorderFinaliseError(#[from] NetworkRecorderFinaliseError),
}

#[derive(Error, Debug)]
#[error("Error setting up recorder `{name}`: `{source}`")]
pub struct NetworkRecorderSetupError {
    name: String,
    #[source]
    source: RecorderSetupError,
}

#[derive(Error, Debug)]
#[error("Error saving recorder `{name}`: `{source}`")]
pub struct NetworkRecorderSaveError {
    name: String,
    #[source]
    source: RecorderSaveError,
}

#[derive(Error, Debug)]
#[error("Error finalising recorder `{name}`: `{source}`")]
pub struct NetworkRecorderFinaliseError {
    name: String,
    #[source]
    source: RecorderFinaliseError,
}

#[derive(Error, Debug)]
pub enum NetworkSolverSetupError {
    #[error("Missing solver features required to run this network")]
    MissingSolverFeatures,
    #[error("Error setting up solver: {0}")]
    SolverSetupError(#[from] SolverSetupError),
}

#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Node with name `{name}` and sub-name `{}` not found", .sub_name.as_deref().unwrap_or("None"))]
    NodeNotFound { name: String, sub_name: Option<String> },
    #[error("Node with index `{index}` not found")]
    NodeIndexNotFound { index: NodeIndex },
    #[error("Error setting attribute `{attribute}` for node `{name}` and sub-name `{}`: {source}", .sub_name.as_deref().unwrap_or("None"))]
    NodeSetAttributeError {
        name: String,
        sub_name: Option<String>,
        attribute: String,
        #[source]
        source: Box<NodeError>,
    },
    #[error("Node with name `{name}` and sub-name `{}` already exists", .sub_name.as_deref().unwrap_or("None"))]
    NodeAlreadyExists { name: String, sub_name: Option<String> },
    #[error("Error on node `{name}`: `{source}`")]
    NodeError {
        name: String,
        sub_name: Option<String>,
        #[source]
        source: Box<NodeError>,
    },
    #[error("Error in parameter collection: `{0}`")]
    ParameterCollectionError(#[from] ParameterCollectionError),
    #[error("Metric set `{0}` already exists")]
    MetricSetNameAlreadyExists(String),
    #[error("Metric set `{0}` not found")]
    MetricSetNotFound(String),
    #[error("Parameter state not found for parameter `{name}`.")]
    ParameterStateNotFound { name: ParameterName },
    #[error("Parameter `{name}` is not of a type that supports variable values.")]
    ParameterTypeNotVariable { name: ParameterName },
    #[error("F64 Parameter with index `{0}` not found")]
    ParameterF64IndexNotFound(ParameterIndex<f64>),
    #[error("U64 Parameter with index `{0}` not found")]
    ParameterU64IndexNotFound(ParameterIndex<u64>),
    #[error("Error with variable parameter `{name}`: {source}")]
    VariableParameterError {
        name: ParameterName,
        #[source]
        source: parameters::VariableParameterError,
    },
}

#[derive(Error, Debug)]
pub enum NetworkRecorderAggregationError {
    #[error("Recorder `{name}` not found in network")]
    NotFound { name: String },
    #[error("Error aggregating recorder `{name}`: {source}")]
    AggregationError {
        name: String,
        #[source]
        source: RecorderAggregationError,
    },
}

/// The results of a model run.
///
/// Only recorders which produced a result will be present.
#[cfg_attr(feature = "pyo3", pyclass(skip_from_py_object))]
#[derive(Clone)]
pub struct NetworkResult {
    results: Arc<HashMap<String, Box<dyn RecorderFinalResult>>>,
}

impl NetworkResult {
    pub fn len(&self) -> usize {
        self.results.len()
    }

    pub fn is_empty(&self) -> bool {
        self.results.is_empty()
    }

    /// Get the results of a recorder by name.
    pub fn get(&self, name: &str) -> Option<&dyn RecorderFinalResult> {
        self.results.get(name).map(|r| r.as_ref())
    }

    /// Get the aggregated value of a recorder by name, if it exists and can be aggregated.
    pub fn get_aggregated_value(&self, name: &str) -> Option<f64> {
        self.results.get(name).and_then(|r| r.aggregated_value().ok())
    }
}

#[cfg(feature = "pyo3")]
#[pymethods]
impl NetworkResult {
    /// Get the aggregated value of a recorder by name, if it exists and can be aggregated.
    #[pyo3(name = "aggregated_value")]
    pub fn get_aggregated_value_py(&self, name: &str) -> PyResult<f64> {
        self.results
            .get(name)
            .ok_or_else(|| PyKeyError::new_err(format!("Output `{}` not found in results", name)))
            .and_then(|r| r.aggregated_value().map_err(|e| e.into()))
    }

    /// An iterator over the names of all available outputs.
    pub fn output_names(&self) -> Vec<String> {
        self.results.keys().map(|k| k.to_string()).collect()
    }

    /// Return an output as a dataframe.
    pub fn to_dataframe(&self, name: &str) -> PyResult<PyDataFrame> {
        self.results
            .get(name)
            .ok_or_else(|| PyKeyError::new_err(format!("Output `{}` not found in results", name)))
            .and_then(|r| r.to_dataframe().map_err(|e| e.into()))
            .map(PyDataFrame)
    }
}

/// A Pywr network containing nodes, edges, parameters, metric sets, etc.
///
/// This struct is the main entry point for constructing a Pywr network and should be used
/// to represent a discrete system. A network can be simulated using a model and a solver. The
/// network is translated into a linear program using the [`Solver`] trait.
///
#[derive(Debug, Default)]
pub struct Network {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    aggregated_nodes: Vec<AggregatedNode>,
    aggregated_storage_nodes: Vec<AggregatedStorageNode>,
    virtual_storage_nodes: Vec<VirtualStorageNode>,
    parameters: ParameterCollection,
    metric_sets: Vec<MetricSet>,
    recorders: Vec<Box<dyn recorders::Recorder>>,
}

impl Network {
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    pub fn recorders(&self) -> &[Box<dyn recorders::Recorder>] {
        &self.recorders
    }

    pub fn aggregated_nodes(&self) -> &[AggregatedNode] {
        &self.aggregated_nodes
    }

    pub fn aggregated_storage_nodes(&self) -> &[AggregatedStorageNode] {
        &self.aggregated_storage_nodes
    }

    pub fn virtual_storage_nodes(&self) -> &[VirtualStorageNode] {
        &self.virtual_storage_nodes
    }

    /// Setup the network and create the initial state for each scenario.
    pub fn setup_network(
        &self,
        timesteps: &[Timestep],
        scenario_indices: &[ScenarioIndex],
        num_inter_network_transfers: usize,
    ) -> Result<NetworkState, NetworkSetupError> {
        let mut states: Vec<State> = Vec::with_capacity(scenario_indices.len());
        let mut parameter_internal_states: Vec<ParameterStates> = Vec::with_capacity(scenario_indices.len());
        let mut metric_set_internal_states: Vec<_> = Vec::with_capacity(scenario_indices.len());

        for scenario_index in scenario_indices {
            // Initialise node states. Note that storage nodes will have a zero volume at this point.
            let initial_node_states = self.nodes.iter().map(|n| n.default_state()).collect();

            let initial_virtual_storage_states = self.virtual_storage_nodes.iter().map(|n| n.default_state()).collect();

            let state_builder = StateBuilder::new(initial_node_states, self.edges.len())
                .with_virtual_storage_states(initial_virtual_storage_states)
                .with_parameters(&self.parameters)
                .with_inter_network_transfers(num_inter_network_transfers);

            let mut state = state_builder.build();

            let mut internal_states = ParameterStates::from_collection(&self.parameters, timesteps, scenario_index)?;

            metric_set_internal_states.push(self.metric_sets.iter().map(|p| p.setup()).collect::<Vec<_>>());

            // Calculate parameters that implement `ConstParameter`
            // First we update the simple parameters
            self.parameters
                .compute_const(scenario_index, &mut state, &mut internal_states)?;

            states.push(state);
            parameter_internal_states.push(internal_states);
        }

        Ok(NetworkState {
            states,
            parameter_internal_states,
            metric_set_internal_states,
        })
    }

    pub fn setup_recorders(
        &self,
        domain: &ModelDomain,
    ) -> Result<Vec<Option<Box<dyn RecorderInternalState>>>, NetworkRecorderSetupError> {
        // Setup recorders
        let mut recorder_internal_states = Vec::new();
        for recorder in &self.recorders {
            let initial_state = recorder
                .setup(domain, self)
                .map_err(|source| NetworkRecorderSetupError {
                    name: recorder.name().to_string(),
                    source,
                })?;
            recorder_internal_states.push(initial_state);
        }

        Ok(recorder_internal_states)
    }

    /// Check whether a solver `S` has the required features to run this network.
    pub fn check_solver_features<S>(&self) -> bool
    where
        S: Solver,
    {
        let required_features = self.required_features();

        required_features.iter().all(|f| S::features().contains(f))
    }

    /// Check whether a solver `S` has the required features to run this network.
    pub fn check_multi_scenario_solver_features<S>(&self) -> bool
    where
        S: MultiStateSolver,
    {
        let required_features = self.required_features();

        required_features.iter().all(|f| S::features().contains(f))
    }

    pub fn setup_solver<S>(
        &self,
        scenario_indices: &[ScenarioIndex],
        state: &NetworkState,
        settings: &S::Settings,
    ) -> Result<Vec<Box<S>>, NetworkSolverSetupError>
    where
        S: Solver,
        <S as Solver>::Settings: SolverSettings,
    {
        if !settings.ignore_feature_requirements() && !self.check_solver_features::<S>() {
            return Err(NetworkSolverSetupError::MissingSolverFeatures);
        }

        let mut solvers = Vec::with_capacity(scenario_indices.len());

        for scenario_index in scenario_indices {
            // Create a solver for each scenario
            let const_values = state.state(scenario_index).get_const_parameter_values();
            let solver = S::setup(self, &const_values, settings)?;
            solvers.push(solver);
        }

        Ok(solvers)
    }

    pub fn setup_multi_scenario_solver<S>(
        &self,
        scenario_indices: &[ScenarioIndex],
        settings: &S::Settings,
    ) -> Result<Box<S>, NetworkSolverSetupError>
    where
        S: MultiStateSolver,
        <S as MultiStateSolver>::Settings: SolverSettings,
    {
        if !settings.ignore_feature_requirements() && !self.check_multi_scenario_solver_features::<S>() {
            return Err(NetworkSolverSetupError::MissingSolverFeatures);
        }
        Ok(S::setup(self, scenario_indices.len(), settings)?)
    }

    /// Finalise the run of the network, performing any final calculations and returning
    /// the results from the recorders.
    ///
    /// This method consumes the recorder internal states as they are no longer needed.
    ///
    /// Only recorders which produce a final result will be included in the returned HashMap.
    pub fn finalise(
        &self,
        scenario_indices: &[ScenarioIndex],
        metric_set_states: &mut [Vec<MetricSetState>],
        recorder_internal_states: Vec<Option<Box<dyn RecorderInternalState>>>,
    ) -> Result<NetworkResult, NetworkFinaliseError> {
        // Finally, save new data to the metric set

        for ms_states in metric_set_states.iter_mut() {
            for (metric_set, ms_state) in self.metric_sets.iter().zip(ms_states.iter_mut()) {
                metric_set.finalise(ms_state);
            }
        }

        // Finalise recorders and return results
        let recorder_results = self
            .recorders
            .iter()
            .zip(recorder_internal_states)
            .filter_map(|(recorder, internal_state)| {
                let result = recorder
                    .finalise(self, scenario_indices, metric_set_states, internal_state)
                    .map_err(|source| NetworkRecorderFinaliseError {
                        name: recorder.name().to_string(),
                        source,
                    });

                match result {
                    Err(e) => Some(Err(e)),
                    Ok(None) => None, // No final result
                    Ok(Some(r)) => Some(Ok((recorder.name().to_string(), r))),
                }
            })
            .collect::<Result<HashMap<_, _>, _>>()?;

        Ok(NetworkResult {
            results: Arc::new(recorder_results),
        })
    }

    /// Perform a single timestep mutating the current state.
    pub fn step<S>(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        solvers: &mut [Box<S>],
        state: &mut NetworkState,
        timings: &mut NetworkTimings,
    ) -> Result<(), NetworkStepError>
    where
        S: Solver,
    {
        scenario_indices
            .iter()
            .zip(state.states.iter_mut())
            .zip(state.parameter_internal_states.iter_mut())
            .zip(state.metric_set_internal_states.iter_mut())
            .zip(solvers)
            .try_for_each(
                |((((scenario_index, current_state), p_internal_states), ms_internal_states), solver)| {
                    // TODO clear the current parameter values state (i.e. set them all to zero).

                    let start_p_calc = Instant::now();
                    self.compute_components(
                        timestep,
                        scenario_index,
                        current_state,
                        p_internal_states,
                        Some(&mut timings.component_timings),
                    )?;

                    // State now contains updated parameter values BUT original network state
                    timings.component_timings.total += start_p_calc.elapsed();

                    // Solve determines the new network state
                    let solve_timings = solver.solve(self, timestep, current_state)?;
                    // State now contains updated parameter values AND updated network state
                    timings.solve += solve_timings;

                    // Now run the "after" method on all components
                    let start_p_after = Instant::now();
                    self.after(
                        timestep,
                        scenario_index,
                        current_state,
                        p_internal_states,
                        ms_internal_states,
                        Some(&mut timings.component_timings),
                    )?;

                    timings.component_timings.total += start_p_after.elapsed();

                    Ok::<(), NetworkStepError>(())
                },
            )?;

        Ok(())
    }

    /// Perform a single timestep in parallel using Rayon mutating the current state.
    ///
    /// Note that the `timings` struct will be incremented with the timing information from
    /// each scenario and therefore contain the total time across all parallel threads (i.e.
    /// not overall wall-time).
    pub fn step_par<S>(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        solvers: &mut [Box<S>],
        state: &mut NetworkState,
        timings: &mut NetworkTimings,
    ) -> Result<(), NetworkStepError>
    where
        S: Solver,
    {
        // Collect all the timings from each parallel solve
        let step_times: Vec<_> = scenario_indices
            .par_iter()
            .zip(&mut state.states)
            .zip(&mut state.parameter_internal_states)
            .zip(&mut state.metric_set_internal_states)
            .zip(solvers)
            .map(
                |((((scenario_index, current_state), p_internal_state), ms_internal_state), solver)| {
                    // TODO clear the current parameter values state (i.e. set them all to zero).

                    let start_p_calc = Instant::now();
                    self.compute_components(timestep, scenario_index, current_state, p_internal_state, None)
                        .unwrap();

                    // State now contains updated parameter values BUT original network state
                    let mut parameter_calculation = start_p_calc.elapsed();

                    // Solve determines the new network state
                    let solve_timings = solver.solve(self, timestep, current_state).unwrap();
                    // State now contains updated parameter values AND updated network state

                    // Now run the "after" method on all components
                    let start_p_after = Instant::now();
                    self.after(
                        timestep,
                        scenario_index,
                        current_state,
                        p_internal_state,
                        ms_internal_state,
                        None,
                    )
                    .unwrap();

                    parameter_calculation += start_p_after.elapsed();

                    (parameter_calculation, solve_timings)
                },
            )
            .collect();

        // Add them all together
        for (parameter_calculation, solve_timings) in step_times.into_iter() {
            timings.component_timings.total += parameter_calculation;
            timings.solve += solve_timings;
        }

        Ok(())
    }

    /// Perform a single timestep with a multi1-state solver mutating the current state.
    pub(crate) fn step_multi_scenario<S>(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        solver: &mut Box<S>,
        state: &mut NetworkState,
        timings: &mut NetworkTimings,
    ) -> Result<(), NetworkStepError>
    where
        S: MultiStateSolver,
    {
        // First compute all the updated state

        let p_calc_timings: Vec<_> = scenario_indices
            .par_iter()
            .zip(&mut state.states)
            .zip(&mut state.parameter_internal_states)
            .map(|((scenario_index, current_state), p_internal_states)| {
                // TODO clear the current parameter values state (i.e. set them all to zero).

                let start_p_calc = Instant::now();
                self.compute_components(timestep, scenario_index, current_state, p_internal_states, None)
                    .unwrap();

                // State now contains updated parameter values BUT original network state
                start_p_calc.elapsed()
            })
            .collect();

        for t in p_calc_timings.into_iter() {
            timings.component_timings.total += t;
        }

        // Now solve all the LPs simultaneously

        let solve_timings = solver.solve(self, timestep, &mut state.states).unwrap();
        // State now contains updated parameter values AND updated network state
        timings.solve += solve_timings;

        // Now run the "after" method on all components
        let p_after_timings: Vec<_> = scenario_indices
            .par_iter()
            .zip(&mut state.states)
            .zip(&mut state.parameter_internal_states)
            .zip(&mut state.metric_set_internal_states)
            .map(
                |(((scenario_index, current_state), p_internal_states), ms_internal_states)| {
                    let start_p_after = Instant::now();
                    self.after(
                        timestep,
                        scenario_index,
                        current_state,
                        p_internal_states,
                        ms_internal_states,
                        None,
                    )
                    .unwrap();
                    start_p_after.elapsed()
                },
            )
            .collect();

        for t in p_after_timings.into_iter() {
            timings.component_timings.total += t;
        }

        Ok(())
    }

    /// Calculate the set of [`SolverFeatures`] required to correctly run this network.
    pub fn required_features(&self) -> HashSet<SolverFeatures> {
        let mut features = HashSet::new();

        // Aggregated node feature required if there are any aggregated nodes
        if !self.aggregated_nodes.is_empty() {
            features.insert(SolverFeatures::AggregatedNode);
        }

        // Aggregated node factors required if any aggregated node has factors defined.
        if self.aggregated_nodes.iter().any(|n| n.has_factors()) {
            features.insert(SolverFeatures::AggregatedNodeFactors);
        }

        // Aggregated node dynamic factors required if any aggregated node has dynamic factors defined.
        if self
            .aggregated_nodes
            .iter()
            .any(|n| n.has_factors() && !n.has_const_factors())
        {
            features.insert(SolverFeatures::AggregatedNodeDynamicFactors);
        }

        // Aggregated nodes with exclusivities require the MutualExclusivity feature.
        if self.aggregated_nodes.iter().any(|n| n.has_exclusivity()) {
            features.insert(SolverFeatures::MutualExclusivity);
        }

        // The presence of any virtual storage node requires the VirtualStorage feature.
        if !self.virtual_storage_nodes.is_empty() {
            features.insert(SolverFeatures::VirtualStorage);
        }

        features
    }

    /// Undertake calculations for network components before solve.
    ///
    /// This method iterates through the network components (nodes, parameters, etc) to perform
    /// pre-solve calculations. For nodes this can be adjustments to storage volume (e.g. to
    /// set initial volume). For parameters this involves computing the current value for the
    /// the timestep. The `state` object is progressively updated with these values during this
    /// method.
    fn compute_components(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        state: &mut State,
        internal_states: &mut ParameterStates,
        timings: Option<&mut ComponentTimings>,
    ) -> Result<(), NetworkStepError> {
        // TODO reset parameter state to zero

        // First we update the simple parameters
        self.parameters
            .compute_simple(timestep, scenario_index, state, internal_states)?;

        // Next run "before" on nodes and virtual nodes
        for n in &self.nodes {
            n.before(timestep, state)
                .map_err(|source| NetworkStepError::NodeBeforeError {
                    name: n.name().to_string(),
                    source,
                })?;
        }

        for vs in &self.virtual_storage_nodes {
            vs.before(timestep, state)
                .map_err(|source| NetworkStepError::VirtualStorageBeforeError {
                    name: vs.name().to_string(),
                    source,
                })?;
        }

        let p_timings = timings.and_then(|timings| timings.parameters.as_mut());

        // Now we can compute the general parameters that may depend on node state.
        self.parameters
            .compute_general(timestep, scenario_index, self, state, internal_states, p_timings)
            .map_err(|source| NetworkStepError::GeneralParameterCalculationError(Box::new(source)))?;

        Ok(())
    }

    /// Undertake "after" for network components after solve.
    ///
    /// This method iterates through the network components (nodes, parameters, etc) to perform
    /// pre-solve calculations. For nodes this can be adjustments to storage volume (e.g. to
    /// set initial volume). For parameters this involves computing the current value for the
    /// the timestep. The `state` object is progressively updated with these values during this
    /// method.
    fn after(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        state: &mut State,
        internal_states: &mut ParameterStates,
        metric_set_states: &mut [MetricSetState],
        timings: Option<&mut ComponentTimings>,
    ) -> Result<(), NetworkStepError> {
        // TODO reset parameter state to zero

        self.parameters
            .after_simple(timestep, scenario_index, state, internal_states)?;

        // No "after" on nodes and virtual nodes

        let p_timings = timings.and_then(|timings| timings.parameters.as_mut());

        // Now we can compute the general parameters that may depend on node state.
        self.parameters
            .after_general(timestep, scenario_index, self, state, internal_states, p_timings)
            .map_err(|source| NetworkStepError::GeneralParameterCalculationError(Box::new(source)))?;

        // Finally, save new data to the metric set
        for (metric_set, ms_state) in self.metric_sets.iter().zip(metric_set_states.iter_mut()) {
            metric_set
                .save(timestep, scenario_index, self, state, ms_state)
                .map_err(|source| NetworkStepError::MetricSetSaveError {
                    name: metric_set.name().to_string(),
                    source,
                })?;
        }

        Ok(())
    }

    pub fn save_recorders(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        state: &NetworkState,
        recorder_internal_states: &mut [Option<Box<dyn RecorderInternalState>>],
        timings: &mut NetworkTimings,
    ) -> Result<(), NetworkRecorderSaveError> {
        let start = Instant::now();
        for (recorder, internal_state) in self.recorders.iter().zip(recorder_internal_states) {
            recorder
                .save(
                    timestep,
                    scenario_indices,
                    self,
                    &state.states,
                    &state.metric_set_internal_states,
                    internal_state,
                )
                .map_err(|source| NetworkRecorderSaveError {
                    name: recorder.name().to_string(),
                    source,
                })?;
        }
        timings.recorder_saving += start.elapsed();
        Ok(())
    }

    /// Get an [`Edge`] from an edge's index
    pub fn get_edge(&self, index: &EdgeIndex) -> Option<&Edge> {
        self.edges.get(index.0)
    }

    /// Get an [`EdgeIndex`] from connecting node indices.
    pub fn get_edge_index(&self, from_node_index: NodeIndex, to_node_index: NodeIndex) -> Option<EdgeIndex> {
        self.edges
            .iter()
            .find(|edge| edge.from_node_index == from_node_index && edge.to_node_index == to_node_index)
            .map(|edge| edge.index())
    }
    /// Get a Node from a node's name
    pub fn get_node_index_by_name(&self, name: &str, sub_name: Option<&str>) -> Option<NodeIndex> {
        self.get_node_by_name(name, sub_name).map(|n| n.index())
    }

    /// Get a Node from a node's index
    pub fn get_node(&self, index: &NodeIndex) -> Option<&Node> {
        self.nodes.get(*index.deref())
    }

    /// Get a Node from a node's index
    pub fn get_node_mut(&mut self, index: &NodeIndex) -> Option<&mut Node> {
        self.nodes.get_mut(*index.deref())
    }

    /// Get a Node from a node's name
    pub fn get_node_by_name(&self, name: &str, sub_name: Option<&str>) -> Option<&Node> {
        self.nodes.iter().find(|&n| n.full_name() == (name, sub_name))
    }

    /// Get a NodeIndex from a node's name
    pub fn get_mut_node_by_name(&mut self, name: &str, sub_name: Option<&str>) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.full_name() == (name, sub_name))
    }

    /// Get an [`AggregatedNode`] from its index.
    pub fn get_aggregated_node(&self, index: &AggregatedNodeIndex) -> Option<&AggregatedNode> {
        self.aggregated_nodes.get(index.0)
    }

    /// Get a `AggregatedNode` from a node's name
    pub fn get_aggregated_node_by_name(&self, name: &str, sub_name: Option<&str>) -> Option<&AggregatedNode> {
        self.aggregated_nodes
            .iter()
            .find(|&n| n.full_name() == (name, sub_name))
    }

    pub fn get_mut_aggregated_node_by_name(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
    ) -> Option<&mut AggregatedNode> {
        self.aggregated_nodes
            .iter_mut()
            .find(|n| n.full_name() == (name, sub_name))
    }

    /// Get a `AggregatedNodeIndex` from a node's name
    pub fn get_aggregated_node_index_by_name(&self, name: &str, sub_name: Option<&str>) -> Option<AggregatedNodeIndex> {
        self.get_aggregated_node_by_name(name, sub_name).map(|n| n.index())
    }

    /// Get a `&AggregatedStorageNode` from a node's name
    pub fn get_aggregated_storage_node(&self, index: AggregatedStorageNodeIndex) -> Option<&AggregatedStorageNode> {
        self.aggregated_storage_nodes.get(index.0)
    }

    /// Get a `&AggregatedStorageNode` from a node's name
    pub fn get_aggregated_storage_node_by_name(
        &self,
        name: &str,
        sub_name: Option<&str>,
    ) -> Option<&AggregatedStorageNode> {
        self.aggregated_storage_nodes
            .iter()
            .find(|&n| n.full_name() == (name, sub_name))
    }

    /// Get a `AggregatedStorageNodeIndex` from a node's name
    pub fn get_aggregated_storage_node_index_by_name(
        &self,
        name: &str,
        sub_name: Option<&str>,
    ) -> Option<AggregatedStorageNodeIndex> {
        self.get_aggregated_storage_node_by_name(name, sub_name)
            .map(|n| n.index())
    }

    pub fn get_mut_aggregated_storage_node_by_name(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
    ) -> Option<&mut AggregatedStorageNode> {
        self.aggregated_storage_nodes
            .iter_mut()
            .find(|n| n.full_name() == (name, sub_name))
    }

    /// Get a `VirtualStorageNode` from a node's name
    pub fn get_virtual_storage_node(&self, index: &VirtualStorageIndex) -> Option<&VirtualStorageNode> {
        self.virtual_storage_nodes.get(index.0)
    }

    /// Get a `VirtualStorageNode` from a node's name
    pub fn get_virtual_storage_node_by_name(&self, name: &str, sub_name: Option<&str>) -> Option<&VirtualStorageNode> {
        self.virtual_storage_nodes
            .iter()
            .find(|&n| n.full_name() == (name, sub_name))
    }

    /// Get a `VirtualStorageNode` from a node's name
    pub fn get_mut_virtual_storage_node_by_name(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
    ) -> Option<&mut VirtualStorageNode> {
        self.virtual_storage_nodes
            .iter_mut()
            .find(|n| n.full_name() == (name, sub_name))
    }

    /// Get a `VirtualStorageNode` from a node's name
    pub fn get_virtual_storage_node_index_by_name(
        &self,
        name: &str,
        sub_name: Option<&str>,
    ) -> Option<VirtualStorageIndex> {
        self.get_virtual_storage_node_by_name(name, sub_name).map(|n| n.index())
    }

    /// Get a `Parameter` from a parameter's name
    pub fn get_parameter(&self, index: ParameterIndex<f64>) -> Option<&dyn parameters::Parameter> {
        self.parameters.get_f64(index)
    }

    /// Get a `Parameter` from a parameter's name
    pub fn get_parameter_by_name(&self, name: &ParameterName) -> Option<&dyn parameters::Parameter> {
        self.parameters.get_f64_by_name(name)
    }

    /// Get a `ParameterIndex` from a parameter's name
    pub fn get_parameter_index_by_name(&self, name: &ParameterName) -> Option<ParameterIndex<f64>> {
        self.parameters.get_f64_index_by_name(name)
    }

    /// Get a `Parameter<usize>` from its index.
    pub fn get_index_parameter(&self, index: ParameterIndex<u64>) -> Option<&dyn parameters::Parameter> {
        self.parameters.get_u64(index)
    }

    /// Get a `IndexParameter` from a parameter's name
    pub fn get_index_parameter_by_name(&self, name: &ParameterName) -> Option<&dyn parameters::Parameter> {
        self.parameters.get_u64_by_name(name)
    }

    /// Get a `IndexParameterIndex` from a parameter's name
    pub fn get_index_parameter_index_by_name(&self, name: &ParameterName) -> Option<ParameterIndex<u64>> {
        self.parameters.get_u64_index_by_name(name)
    }

    /// Get a `MultiValueParameterIndex` from a parameter's name
    pub fn get_multi_valued_parameter(&self, index: &ParameterIndex<MultiValue>) -> Option<&dyn parameters::Parameter> {
        self.parameters.get_multi(index)
    }

    /// Get a `MultiValueParameterIndex` from a parameter's name
    pub fn get_multi_valued_parameter_index_by_name(&self, name: &ParameterName) -> Option<ParameterIndex<MultiValue>> {
        self.parameters.get_multi_index_by_name(name)
    }

    /// Get a `RecorderIndex` from a recorder's name
    pub fn get_recorder_by_name(&self, name: &str) -> Option<&dyn recorders::Recorder> {
        self.recorders.iter().find(|r| r.name() == name).map(|r| r.as_ref())
    }

    pub fn get_recorder_index_by_name(&self, name: &str) -> Option<RecorderIndex> {
        self.recorders
            .iter()
            .position(|r| r.name() == name)
            .map(RecorderIndex::new)
    }

    /// Add a [`MetricSet`] to the network.
    pub fn add_metric_set(&mut self, metric_set: MetricSet) -> Result<MetricSetIndex, NetworkError> {
        if self.get_metric_set_by_name(metric_set.name()).is_ok() {
            return Err(NetworkError::MetricSetNameAlreadyExists(metric_set.name().to_string()));
        }

        let metric_set_idx = MetricSetIndex::new(self.metric_sets.len());
        self.metric_sets.push(metric_set);
        Ok(metric_set_idx)
    }

    /// Get a [`MetricSet'] from its index.
    pub fn get_metric_set(&self, index: MetricSetIndex) -> Option<&MetricSet> {
        self.metric_sets.get(*index)
    }

    /// Get a ['MetricSet'] by its name.
    pub fn get_metric_set_by_name(&self, name: &str) -> Result<&MetricSet, NetworkError> {
        self.metric_sets
            .iter()
            .find(|&m| m.name() == name)
            .ok_or(NetworkError::MetricSetNotFound(name.to_string()))
    }

    /// Get a ['MetricSetIndex'] by its name.
    pub fn get_metric_set_index_by_name(&self, name: &str) -> Result<MetricSetIndex, NetworkError> {
        match self.metric_sets.iter().position(|m| m.name() == name) {
            Some(idx) => Ok(MetricSetIndex::new(idx)),
            None => Err(NetworkError::MetricSetNotFound(name.to_string())),
        }
    }

    /// Set the variable values on the parameter `parameter_index`.
    ///
    /// This will update the internal state of the parameter with the new values for all scenarios.
    pub fn set_f64_parameter_variable_values(
        &self,
        parameter_index: ParameterIndex<f64>,
        values: &[f64],
        variable_config: &dyn VariableConfig,
        state: &mut NetworkState,
    ) -> Result<(), NetworkError> {
        match self.parameters.get_f64(parameter_index) {
            Some(parameter) => match parameter.as_f64_variable() {
                Some(variable) => {
                    // Iterate over all scenarios and set the variable values
                    for parameter_states in state.iter_parameter_states_mut() {
                        let internal_state = parameter_states.get_mut_f64_state(parameter_index).ok_or(
                            NetworkError::ParameterStateNotFound {
                                name: parameter.name().clone(),
                            },
                        )?;

                        variable
                            .set_variables(values, variable_config, internal_state)
                            .map_err(|source| NetworkError::VariableParameterError {
                                name: parameter.name().clone(),
                                source,
                            })?;
                    }

                    Ok(())
                }
                None => Err(NetworkError::ParameterTypeNotVariable {
                    name: parameter.name().clone(),
                }),
            },
            None => Err(NetworkError::ParameterF64IndexNotFound(parameter_index)),
        }
    }

    /// Set the variable values on the parameter `parameter_index` and scenario `scenario_index`.
    ///
    /// Only the internal state of the parameter for the given scenario will be updated.
    pub fn set_f64_parameter_variable_values_for_scenario(
        &self,
        parameter_index: ParameterIndex<f64>,
        scenario_index: ScenarioIndex,
        values: &[f64],
        variable_config: &dyn VariableConfig,
        state: &mut NetworkState,
    ) -> Result<(), NetworkError> {
        match self.parameters.get_f64(parameter_index) {
            Some(parameter) => match parameter.as_f64_variable() {
                Some(variable) => {
                    let internal_state = state
                        .parameter_states_mut(&scenario_index)
                        .get_mut_f64_state(parameter_index)
                        .ok_or(NetworkError::ParameterStateNotFound {
                            name: parameter.name().clone(),
                        })?;

                    variable
                        .set_variables(values, variable_config, internal_state)
                        .map_err(|source| NetworkError::VariableParameterError {
                            name: parameter.name().clone(),
                            source,
                        })
                }
                None => Err(NetworkError::ParameterTypeNotVariable {
                    name: parameter.name().clone(),
                }),
            },
            None => Err(NetworkError::ParameterF64IndexNotFound(parameter_index)),
        }
    }

    /// Return a vector of the current values of active variable parameters.
    pub fn get_f64_parameter_variable_values_for_scenario(
        &self,
        parameter_index: ParameterIndex<f64>,
        scenario_index: ScenarioIndex,
        state: &NetworkState,
    ) -> Result<Option<Vec<f64>>, NetworkError> {
        match self.parameters.get_f64(parameter_index) {
            Some(parameter) => match parameter.as_f64_variable() {
                Some(variable) => {
                    let internal_state = state
                        .parameter_states(&scenario_index)
                        .get_f64_state(parameter_index)
                        .ok_or(NetworkError::ParameterStateNotFound {
                            name: parameter.name().clone(),
                        })?;

                    Ok(variable.get_variables(internal_state))
                }
                None => Err(NetworkError::ParameterTypeNotVariable {
                    name: parameter.name().clone(),
                }),
            },
            None => Err(NetworkError::ParameterF64IndexNotFound(parameter_index)),
        }
    }

    pub fn get_f64_parameter_variable_values(
        &self,
        parameter_index: ParameterIndex<f64>,
        state: &NetworkState,
    ) -> Result<Vec<Option<Vec<f64>>>, NetworkError> {
        match self.parameters.get_f64(parameter_index) {
            Some(parameter) => match parameter.as_f64_variable() {
                Some(variable) => {
                    let values = state
                        .iter_parameter_states()
                        .map(|parameter_states| {
                            let internal_state = parameter_states.get_f64_state(parameter_index).ok_or(
                                NetworkError::ParameterStateNotFound {
                                    name: parameter.name().clone(),
                                },
                            )?;

                            Ok(variable.get_variables(internal_state))
                        })
                        .collect::<Result<_, NetworkError>>()?;

                    Ok(values)
                }
                None => Err(NetworkError::ParameterTypeNotVariable {
                    name: parameter.name().clone(),
                }),
            },
            None => Err(NetworkError::ParameterF64IndexNotFound(parameter_index)),
        }
    }

    /// Set the variable values on the parameter `parameter_index`.
    ///
    /// This will update the internal state of the parameter with the new values for scenarios.
    pub fn set_u32_parameter_variable_values(
        &self,
        parameter_index: ParameterIndex<f64>,
        values: &[u32],
        variable_config: &dyn VariableConfig,
        state: &mut NetworkState,
    ) -> Result<(), NetworkError> {
        match self.parameters.get_f64(parameter_index) {
            Some(parameter) => match parameter.as_u32_variable() {
                Some(variable) => {
                    // Iterate over all scenarios and set the variable values
                    for parameter_states in state.iter_parameter_states_mut() {
                        let internal_state = parameter_states.get_mut_f64_state(parameter_index).ok_or(
                            NetworkError::ParameterStateNotFound {
                                name: parameter.name().clone(),
                            },
                        )?;

                        variable
                            .set_variables(values, variable_config, internal_state)
                            .map_err(|source| NetworkError::VariableParameterError {
                                name: parameter.name().clone(),
                                source,
                            })?;
                    }

                    Ok(())
                }
                None => Err(NetworkError::ParameterTypeNotVariable {
                    name: parameter.name().clone(),
                }),
            },
            None => Err(NetworkError::ParameterF64IndexNotFound(parameter_index)),
        }
    }

    /// Set the variable values on the parameter `parameter_index` and scenario `scenario_index`.
    ///
    /// Only the internal state of the parameter for the given scenario will be updated.
    pub fn set_u32_parameter_variable_values_for_scenario(
        &self,
        parameter_index: ParameterIndex<f64>,
        scenario_index: ScenarioIndex,
        values: &[u32],
        variable_config: &dyn VariableConfig,
        state: &mut NetworkState,
    ) -> Result<(), NetworkError> {
        match self.parameters.get_f64(parameter_index) {
            Some(parameter) => match parameter.as_u32_variable() {
                Some(variable) => {
                    let internal_state = state
                        .parameter_states_mut(&scenario_index)
                        .get_mut_f64_state(parameter_index)
                        .ok_or(NetworkError::ParameterStateNotFound {
                            name: parameter.name().clone(),
                        })?;

                    variable
                        .set_variables(values, variable_config, internal_state)
                        .map_err(|source| NetworkError::VariableParameterError {
                            name: parameter.name().clone(),
                            source,
                        })
                }
                None => Err(NetworkError::ParameterTypeNotVariable {
                    name: parameter.name().clone(),
                }),
            },
            None => Err(NetworkError::ParameterF64IndexNotFound(parameter_index)),
        }
    }

    /// Return a vector of the current values of active variable parameters.
    pub fn get_u32_parameter_variable_values_for_scenario(
        &self,
        parameter_index: ParameterIndex<f64>,
        scenario_index: ScenarioIndex,
        state: &NetworkState,
    ) -> Result<Option<Vec<u32>>, NetworkError> {
        match self.parameters.get_f64(parameter_index) {
            Some(parameter) => match parameter.as_u32_variable() {
                Some(variable) => {
                    let internal_state = state
                        .parameter_states(&scenario_index)
                        .get_f64_state(parameter_index)
                        .ok_or(NetworkError::ParameterStateNotFound {
                            name: parameter.name().clone(),
                        })?;
                    Ok(variable.get_variables(internal_state))
                }
                None => Err(NetworkError::ParameterTypeNotVariable {
                    name: parameter.name().clone(),
                }),
            },
            None => Err(NetworkError::ParameterF64IndexNotFound(parameter_index)),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct UnresolvedEdge {
    from: UnresolvedNode,
    to: UnresolvedNode,
}

impl UnresolvedEdge {
    pub fn new(from: UnresolvedNode, to: UnresolvedNode) -> Self {
        Self { from, to }
    }
}

impl Display for UnresolvedEdge {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} -> {}", self.from, self.to)
    }
}

/// An index to a regular node type.
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug, Hash)]
pub struct NodeIndex(usize);

impl Deref for NodeIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for NodeIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug, Hash)]
pub struct EdgeIndex(usize);

impl Deref for EdgeIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for EdgeIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct AggregatedNodeIndex(usize);

impl Deref for AggregatedNodeIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for AggregatedNodeIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct AggregatedStorageNodeIndex(usize);

impl Deref for AggregatedStorageNodeIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for AggregatedStorageNodeIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug, Hash)]
pub struct VirtualStorageIndex(usize);

impl Deref for VirtualStorageIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for VirtualStorageIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct RecorderIndex(usize);

impl RecorderIndex {
    pub fn new(idx: usize) -> Self {
        Self(idx)
    }
}

impl Deref for RecorderIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for RecorderIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct MetricSetIndex(usize);

impl MetricSetIndex {
    pub fn new(idx: usize) -> Self {
        Self(idx)
    }
}

impl Deref for MetricSetIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for MetricSetIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A helper struct for building a network. This struct contains look-ups and references
/// for resolving names and other unresolved references during the build process.
#[derive(Debug)]
pub struct ResolutionMaps {
    pub nodes: HashMap<UnresolvedNode, NodeIndex>,
    /// The edges incoming to each node.
    pub incoming_edges: HashMap<NodeIndex, Vec<EdgeIndex>>,
    /// The edges outgoing from each node.
    pub outgoing_edges: HashMap<NodeIndex, Vec<EdgeIndex>>,
    pub parameters_f64: HashMap<ParameterName, ParameterIndex<f64>>,
    pub parameters_u64: HashMap<ParameterName, ParameterIndex<u64>>,
    pub parameters_multi: HashMap<ParameterName, ParameterIndex<MultiValue>>,
    /// The index associated with the unresolved virtual storage node names.
    pub virtual_storage_node: HashMap<UnresolvedNode, VirtualStorageIndex>,
    /// The virtual storage nodes associated with each node index. This is used to resolve the
    /// virtual storage nodes after the regular nodes have been resolved.
    pub virtual_storage_associated_nodes: HashMap<NodeIndex, Vec<VirtualStorageIndex>>,
    /// The index associated with the unresolved aggregated node names.
    pub aggregated_nodes: HashMap<UnresolvedNode, AggregatedNodeIndex>,
    /// The index associated with the unresolved aggregated storage node names.
    pub aggregated_storage_nodes: HashMap<UnresolvedNode, AggregatedStorageNodeIndex>,
    /// The index associated with each unresolved edge.
    pub edges: HashMap<UnresolvedEdge, EdgeIndex>,
    /// The model domain
    pub domain: ModelDomain,
    /// Inter-network transfer indices
    pub inter_network_transfers: HashMap<String, MultiNetworkTransferIndex>,
    /// The index associated wit heach metric set
    pub metric_sets: HashMap<String, MetricSetIndex>,
}

impl ResolutionMaps {
    pub fn new(domain: ModelDomain) -> Self {
        Self {
            nodes: Default::default(),
            incoming_edges: Default::default(),
            outgoing_edges: Default::default(),
            parameters_f64: Default::default(),
            parameters_u64: Default::default(),
            parameters_multi: Default::default(),
            virtual_storage_node: Default::default(),
            virtual_storage_associated_nodes: Default::default(),
            aggregated_nodes: Default::default(),
            aggregated_storage_nodes: Default::default(),
            edges: Default::default(),
            domain,
            inter_network_transfers: Default::default(),
            metric_sets: Default::default(),
        }
    }
}

type NodeEdgeMap = HashMap<NodeIndex, Vec<EdgeIndex>>;

/// Errors returned by [`NetworkBuilder`]
#[derive(Debug, Error)]
pub enum NetworkBuildError {
    #[error("Duplicate node names found: {name}")]
    DuplicateNodeName { name: UnresolvedNode },
    #[error("Duplicate parameter names found: {name}")]
    DuplicateParameterName { name: ParameterName },
    #[error("Duplicate aggregated node names found: {name}")]
    DuplicateAggregatedNodeName { name: UnresolvedNode },
    #[error("Duplicate metric set names found: {name}")]
    DuplicateMetricSetName { name: String },
    #[error("Duplicate edge found: {edge}")]
    DuplicateEdge { edge: Box<UnresolvedEdge> },
    #[error("Node `{name}` not found while resolving edge: {edge}")]
    NodeNotFoundForEdge {
        name: UnresolvedNode,
        edge: Box<UnresolvedEdge>,
    },
    #[error("Node `{name}` not found while resolving virtual storage: {virtual_storage}")]
    NodeNotFoundForVirtualStorage {
        name: UnresolvedNode,
        virtual_storage: UnresolvedNode,
    },
    #[error("Error building node `{name}`: {source}")]
    NodeBuilderError {
        name: UnresolvedNode,
        #[source]
        source: Box<NodeBuilderError>,
    },
    #[error("Cannot connect a node to itself: `{name}`")]
    NodeConnectToSelf { name: UnresolvedNode },
    #[error("Error building aggregated node `{name}`: {source}")]
    AggregatedNodeBuilderError {
        name: UnresolvedNode,
        #[source]
        source: Box<AggregatedNodeBuilderError>,
    },
    #[error("Error building aggregated storage node `{name}`: {source}")]
    AggregatedStorageNodeBuilderError {
        name: UnresolvedNode,
        #[source]
        source: Box<AggregatedStorageNodeBuilderError>,
    },
    #[error("Error building virtual storage node `{name}`: {source}")]
    VirtualStorageNodeBuilderError {
        name: UnresolvedNode,
        #[source]
        source: Box<VirtualStorageNodeBuilderError>,
    },
    #[error("Error building recorder `{name}`: {source}")]
    RecorderBuilderError {
        name: String,
        #[source]
        source: Box<RecorderBuilderError>,
    },
    #[error("Error building metric set `{name}`: {source}")]
    MetricSetBuilderError {
        name: String,
        #[source]
        source: Box<MetricSetBuilderError>,
    },
    #[error("Parameter collection build error: {0}")]
    ParameterCollectionBuildError(#[from] Box<ParameterCollectionBuilderError>),
}

/// A builder for [`Network`].
///
/// This is the only way to construct a [`Network`] instance.
#[derive(Default, Debug)]
pub struct NetworkBuilder {
    nodes: Vec<NodeBuilder>,
    virtual_storage_nodes: Vec<VirtualStorageNodeBuilder>,
    edges: Vec<UnresolvedEdge>,
    parameters: ParameterCollectionBuilder,
    aggregated_nodes: Vec<AggregatedNodeBuilder>,
    aggregated_storage_nodes: Vec<AggregatedStorageNodeBuilder>,
    recorders: Vec<Box<dyn RecorderBuilder>>,
    metric_sets: Vec<MetricSetBuilder>,
}

impl NetworkBuilder {
    /// Add an input node to the network builder.
    pub fn node(&mut self, node: NodeBuilder) -> &mut Self {
        self.nodes.push(node);
        self
    }

    /// Get a reference to an existing node builder by name and sub-name, if it exists.
    pub fn node_builder(&mut self, name: &UnresolvedNode) -> Option<&mut NodeBuilder> {
        self.nodes.iter_mut().find(|n| n.name() == name)
    }

    /// Add a virtual storage node to the network builder.
    pub fn virtual_storage_node(&mut self, vs_node: VirtualStorageNodeBuilder) -> &mut Self {
        self.virtual_storage_nodes.push(vs_node);
        self
    }

    /// Connect two nodes together
    pub fn connect<N1: Into<UnresolvedNode>, N2: Into<UnresolvedNode>>(&mut self, from: N1, to: N2) -> &mut Self {
        self.edges.push(UnresolvedEdge {
            from: from.into(),
            to: to.into(),
        });
        self
    }

    /// Add an aggregated node to the network builder.
    pub fn agg_node(&mut self, agg_node: AggregatedNodeBuilder) -> &mut Self {
        self.aggregated_nodes.push(agg_node);
        self
    }

    pub fn agg_storage_node(&mut self, agg_storage_node: AggregatedStorageNodeBuilder) -> &mut Self {
        self.aggregated_storage_nodes.push(agg_storage_node);
        self
    }

    pub fn parameters(&mut self) -> &mut ParameterCollectionBuilder {
        &mut self.parameters
    }

    pub fn recorder(&mut self, recorder: Box<dyn RecorderBuilder>) -> &mut Self {
        self.recorders.push(recorder);
        self
    }

    pub fn metric_set(&mut self, metric_set: MetricSetBuilder) -> &mut Self {
        self.metric_sets.push(metric_set);
        self
    }

    fn node_index_map(&self) -> Result<HashMap<UnresolvedNode, NodeIndex>, NetworkBuildError> {
        // Build the NodeIndex map checking for any duplicate node names.
        let mut node_index_map: HashMap<UnresolvedNode, NodeIndex> = HashMap::with_capacity(self.nodes.len());

        for (i, nb) in self.nodes.iter().enumerate() {
            let unresolved_node = nb.name();
            if node_index_map.contains_key(unresolved_node) {
                return Err(NetworkBuildError::DuplicateNodeName {
                    name: unresolved_node.clone(),
                });
            }
            node_index_map.insert(unresolved_node.clone(), NodeIndex(i));
        }

        Ok(node_index_map)
    }

    fn edge_index_map(&self) -> Result<HashMap<UnresolvedEdge, EdgeIndex>, NetworkBuildError> {
        let mut edge_index_map: HashMap<UnresolvedEdge, EdgeIndex> = HashMap::with_capacity(self.edges.len());

        for (i, edge) in self.edges.iter().enumerate() {
            let unresolved_edge = edge.clone();
            if edge_index_map.contains_key(&unresolved_edge) {
                return Err(NetworkBuildError::DuplicateEdge {
                    edge: Box::new(unresolved_edge),
                });
            }
            edge_index_map.insert(unresolved_edge, EdgeIndex(i));
        }

        Ok(edge_index_map)
    }

    fn node_edge_maps(
        &self,
        node_indices: &HashMap<UnresolvedNode, NodeIndex>,
    ) -> Result<(NodeEdgeMap, NodeEdgeMap), NetworkBuildError> {
        let mut incoming: NodeEdgeMap = HashMap::with_capacity(self.nodes.len());
        let mut outgoing: NodeEdgeMap = HashMap::with_capacity(self.nodes.len());

        for (i, edge) in self.edges.iter().enumerate() {
            let ei = EdgeIndex(i);

            let from_node_index =
                node_indices
                    .get(&edge.from)
                    .ok_or_else(|| NetworkBuildError::NodeNotFoundForEdge {
                        name: edge.from.clone(),
                        edge: Box::new(edge.clone()),
                    })?;

            let to_node_index = node_indices
                .get(&edge.to)
                .ok_or_else(|| NetworkBuildError::NodeNotFoundForEdge {
                    name: edge.to.clone(),
                    edge: Box::new(edge.clone()),
                })?;

            // Self connections are forbidden.
            if from_node_index == to_node_index {
                return Err(NetworkBuildError::NodeConnectToSelf { name: edge.to.clone() });
            }

            outgoing.entry(*from_node_index).or_default().push(ei);
            incoming.entry(*to_node_index).or_default().push(ei);
        }

        Ok((incoming, outgoing))
    }

    fn aggregated_node_map(&self) -> Result<HashMap<UnresolvedNode, AggregatedNodeIndex>, NetworkBuildError> {
        let mut agg_node_index_map: HashMap<UnresolvedNode, AggregatedNodeIndex> =
            HashMap::with_capacity(self.nodes.len());
        for (i, nb) in self.aggregated_nodes.iter().enumerate() {
            let unresolved_node = nb.name();
            if agg_node_index_map.contains_key(unresolved_node) {
                return Err(NetworkBuildError::DuplicateAggregatedNodeName {
                    name: unresolved_node.clone(),
                });
            }

            agg_node_index_map.insert(unresolved_node.clone(), AggregatedNodeIndex(i));
        }

        Ok(agg_node_index_map)
    }

    fn aggregated_storage_node_map(
        &self,
    ) -> Result<HashMap<UnresolvedNode, AggregatedStorageNodeIndex>, NetworkBuildError> {
        let mut agg_node_index_map: HashMap<UnresolvedNode, AggregatedStorageNodeIndex> =
            HashMap::with_capacity(self.nodes.len());
        for (i, nb) in self.aggregated_storage_nodes.iter().enumerate() {
            let unresolved_node = nb.name();
            if agg_node_index_map.contains_key(unresolved_node) {
                return Err(NetworkBuildError::DuplicateAggregatedNodeName {
                    name: unresolved_node.clone(),
                });
            }

            agg_node_index_map.insert(unresolved_node.clone(), AggregatedStorageNodeIndex(i));
        }

        Ok(agg_node_index_map)
    }

    fn virtual_storage_node_map(&self) -> Result<HashMap<UnresolvedNode, VirtualStorageIndex>, NetworkBuildError> {
        let mut vs_node_index_map: HashMap<UnresolvedNode, VirtualStorageIndex> =
            HashMap::with_capacity(self.nodes.len());
        for (i, nb) in self.virtual_storage_nodes.iter().enumerate() {
            let unresolved_node = nb.name();
            if vs_node_index_map.contains_key(unresolved_node) {
                return Err(NetworkBuildError::DuplicateAggregatedNodeName {
                    name: unresolved_node.clone(),
                });
            }

            vs_node_index_map.insert(unresolved_node.clone(), VirtualStorageIndex(i));
        }

        Ok(vs_node_index_map)
    }

    /// Compute a map of [`NodeIndex`] to the associated [`VirtualStorageNode`]s. This is used
    /// to build the backward reference from nodes to VS nodes.
    fn node_associated_vs_nodes(
        &self,
        node_indices: &HashMap<UnresolvedNode, NodeIndex>,
    ) -> Result<HashMap<NodeIndex, Vec<VirtualStorageIndex>>, NetworkBuildError> {
        let mut vs_associated_nodes: HashMap<NodeIndex, Vec<VirtualStorageIndex>> =
            HashMap::with_capacity(self.nodes.len());
        for (i, vs) in self.virtual_storage_nodes.iter().enumerate() {
            let vi = VirtualStorageIndex(i);
            for node in vs.nodes() {
                let index = node_indices
                    .get(node)
                    .ok_or_else(|| NetworkBuildError::NodeNotFoundForVirtualStorage {
                        name: node.clone(),
                        virtual_storage: vs.name().clone(),
                    })?;

                vs_associated_nodes.entry(*index).or_default().push(vi);
            }
        }

        Ok(vs_associated_nodes)
    }

    fn metric_set_map(&self) -> Result<HashMap<String, MetricSetIndex>, NetworkBuildError> {
        let mut metric_set_map: HashMap<String, MetricSetIndex> = HashMap::with_capacity(self.nodes.len());
        for (i, ms) in self.metric_sets.iter().enumerate() {
            let msi = MetricSetIndex(i);

            if metric_set_map.contains_key(ms.name()) {
                return Err(NetworkBuildError::DuplicateMetricSetName {
                    name: ms.name().to_string(),
                });
            }
            metric_set_map.insert(ms.name().to_string(), msi);
        }

        Ok(metric_set_map)
    }

    fn build_resolution_map(
        &self,
        domain: &ModelDomain,
        inter_network_transfer_map: &HashMap<String, MultiNetworkTransferIndex>,
    ) -> Result<ResolutionMaps, NetworkBuildError> {
        let nodes = self.node_index_map()?;
        let edges = self.edge_index_map()?;
        let (incoming_edges, outgoing_edges) = self.node_edge_maps(&nodes)?;
        let aggregated_nodes = self.aggregated_node_map()?;
        let aggregated_storage_nodes = self.aggregated_storage_node_map()?;
        let virtual_storage_node = self.virtual_storage_node_map()?;
        let virtual_storage_associated_nodes = self.node_associated_vs_nodes(&nodes)?;
        let metric_sets = self.metric_set_map()?;

        Ok(ResolutionMaps {
            nodes,
            incoming_edges,
            outgoing_edges,
            // Parameter maps start empty and are iteratively populated.
            parameters_f64: Default::default(),
            parameters_u64: Default::default(),
            parameters_multi: Default::default(),
            virtual_storage_node,
            virtual_storage_associated_nodes,
            aggregated_nodes,
            aggregated_storage_nodes,
            edges,
            domain: domain.clone(), // TODO can this clone be removed; needs tying to the lifetime of the reference in `build`.
            inter_network_transfers: inter_network_transfer_map.clone(),
            metric_sets,
        })
    }

    /// Build the network.
    pub fn build(
        self,
        domain: &ModelDomain,
        inter_network_transfer_map: &HashMap<String, MultiNetworkTransferIndex>,
    ) -> Result<(Network, ResolutionMaps), NetworkBuildError> {
        // Resolution map
        let mut resolution_map = self.build_resolution_map(domain, inter_network_transfer_map)?;

        // Iterative load the parameters
        let parameters = self
            .parameters
            .build(&mut resolution_map)
            .map_err(|source| NetworkBuildError::ParameterCollectionBuildError(Box::new(source)))?;

        // Construct all the nodes
        let mut nodes = Vec::with_capacity(self.nodes.len());
        for node_builder in self.nodes.iter() {
            let node = node_builder
                .build(&resolution_map)
                .map_err(|source| NetworkBuildError::NodeBuilderError {
                    name: node_builder.name().clone(),
                    source: Box::new(source),
                })?;

            nodes.push(node);
        }

        // Construct all the aggregated nodes
        let mut aggregated_nodes = Vec::with_capacity(self.aggregated_nodes.len());
        for agg_node_builder in self.aggregated_nodes.iter() {
            let agg_node = agg_node_builder.build(&resolution_map).map_err(|source| {
                NetworkBuildError::AggregatedNodeBuilderError {
                    name: agg_node_builder.name().clone(),
                    source: Box::new(source),
                }
            })?;

            aggregated_nodes.push(agg_node);
        }

        // Construct all the aggregated storage nodes
        let mut aggregated_storage_nodes = Vec::with_capacity(self.aggregated_storage_nodes.len());
        for agg_storage_node_builder in self.aggregated_storage_nodes.iter() {
            let agg_node = agg_storage_node_builder.build(&resolution_map).map_err(|source| {
                NetworkBuildError::AggregatedStorageNodeBuilderError {
                    name: agg_storage_node_builder.name().clone(),
                    source: Box::new(source),
                }
            })?;

            aggregated_storage_nodes.push(agg_node);
        }

        // Construct all virtual storage nodes
        let mut virtual_storage_nodes = Vec::with_capacity(self.virtual_storage_nodes.len());
        for vs_node_builder in self.virtual_storage_nodes.iter() {
            let vs_node = vs_node_builder.build(&resolution_map).map_err(|source| {
                NetworkBuildError::VirtualStorageNodeBuilderError {
                    name: vs_node_builder.name().clone(),
                    source: Box::new(source),
                }
            })?;

            virtual_storage_nodes.push(vs_node);
        }

        // Construct all the edges
        let mut edges = Vec::with_capacity(self.edges.len());
        for (i, unresolved_edge) in self.edges.iter().enumerate() {
            let index = EdgeIndex(i);
            let from_node_index = resolution_map
                .nodes
                .get(&unresolved_edge.from)
                .copied()
                .ok_or_else(|| NetworkBuildError::NodeNotFoundForEdge {
                    name: unresolved_edge.from.clone(),
                    edge: Box::new(unresolved_edge.clone()),
                })?;

            let to_node_index = resolution_map.nodes.get(&unresolved_edge.to).copied().ok_or_else(|| {
                NetworkBuildError::NodeNotFoundForEdge {
                    name: unresolved_edge.to.clone(),
                    edge: Box::new(unresolved_edge.clone()),
                }
            })?;

            let edge = Edge {
                index,
                from_node_index,
                to_node_index,
            };

            edges.push(edge);
        }

        let mut metric_sets = Vec::with_capacity(nodes.len());
        for metric_set_builder in self.metric_sets {
            let name = metric_set_builder.name().to_string();
            let metric_set = metric_set_builder.build(&resolution_map).map_err(|source| {
                NetworkBuildError::MetricSetBuilderError {
                    name,
                    source: Box::new(source),
                }
            })?;

            metric_sets.push(metric_set);
        }

        // Construct all recorders
        let mut recorders = Vec::with_capacity(self.recorders.len());
        for recorder_builder in self.recorders.into_iter() {
            let name = recorder_builder.name().to_string();
            let recorder =
                recorder_builder
                    .build(&resolution_map)
                    .map_err(|source| NetworkBuildError::RecorderBuilderError {
                        name,
                        source: Box::new(source),
                    })?;

            recorders.push(recorder);
        }

        let network = Network {
            nodes,
            edges,
            aggregated_nodes,
            aggregated_storage_nodes,
            virtual_storage_nodes,
            parameters,
            metric_sets,
            recorders,
        };

        Ok((network, resolution_map))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metric::{MetricF64ResolutionError, UnresolvedMetricF64};
    use crate::models::ModelBuilder;
    use crate::parameters::{ActivationFunction, ControlCurveInterpolatedParameterBuilder};
    use crate::recorders::AssertionF64RecorderBuilder;
    use crate::solvers::{ClpSolver, ClpSolverSettings};
    use crate::test_utils::{
        default_domain, default_domain_builder, run_all_solvers, simple_model, simple_storage_model,
        simple_storage_network,
    };
    use float_cmp::assert_approx_eq;
    use ndarray::{Array, Array2};
    use std::default::Default;
    use std::ops::Deref;

    #[test]
    fn test_simple_network() {
        let mut builder = NetworkBuilder::default();

        builder
            .node(NodeBuilder::input("input"))
            .node(NodeBuilder::link("link"))
            .node(NodeBuilder::output("output"));

        builder.connect("input", "link");
        builder.connect("link", "output");

        let domain = default_domain();
        let (network, _) = builder.build(&domain, &HashMap::new()).unwrap();

        // Now assert the internal structure is as expected.
        let input_node = network.get_node_by_name("input", None).unwrap();
        let link_node = network.get_node_by_name("link", None).unwrap();
        let output_node = network.get_node_by_name("output", None).unwrap();

        assert_eq!(*input_node.index().deref(), 0);
        assert_eq!(*link_node.index().deref(), 1);
        assert_eq!(*output_node.index().deref(), 2);

        assert_eq!(input_node.get_outgoing_edges().unwrap().len(), 1);
        assert_eq!(link_node.get_incoming_edges().unwrap().len(), 1);
        assert_eq!(link_node.get_outgoing_edges().unwrap().len(), 1);
        assert_eq!(output_node.get_incoming_edges().unwrap().len(), 1);
    }

    #[test]
    /// Test the duplicate node names are not permitted.
    fn test_duplicate_node_name() {
        let mut builder = NetworkBuilder::default();

        builder
            .node(NodeBuilder::input("my-node"))
            .node(NodeBuilder::link("my-node"));

        let domain = default_domain();
        let result = builder.build(&domain, &HashMap::new());
        assert!(
            matches!(result, Err(NetworkBuildError::DuplicateNodeName { name, .. }) if name.to_string() == "my-node"
            )
        );

        let mut builder = NetworkBuilder::default();
        builder
            .node(NodeBuilder::input("my-node"))
            .node(NodeBuilder::input("my-node"));

        assert!(
            matches!(builder.build(&domain, &HashMap::new()), Err(NetworkBuildError::DuplicateNodeName { name, .. }) if name.to_string() == "my-node"
            )
        );

        let mut builder = NetworkBuilder::default();
        builder
            .node(NodeBuilder::input("my-node"))
            .node(NodeBuilder::link("my-other-node"))
            .node(NodeBuilder::output("my-node"));
        assert!(
            matches!(builder.build(&domain, &HashMap::new()), Err(NetworkBuildError::DuplicateNodeName { name, .. }) if name.to_string() == "my-node"
            )
        );
        // Second add with the same name
        let mut n1 = NodeBuilder::input("my-node");
        n1.sub_name("sub1");
        let mut n2 = NodeBuilder::input("my-node");
        n2.sub_name("sub1");

        let mut builder = NetworkBuilder::default();
        builder.node(n1).node(n2);
        assert!(
            matches!(builder.build(&domain, &HashMap::new()), Err(NetworkBuildError::DuplicateNodeName { name, .. }) if name.to_string() == "my-node[sub1]"
            )
        );
    }

    #[test]
    /// Test connecting to yourself is forbidden.
    fn test_self_connection() {
        let mut builder = NetworkBuilder::default();

        builder
            .node(NodeBuilder::input("my-input"))
            .node(NodeBuilder::link("my-link"))
            .node(NodeBuilder::output("my-output"));

        builder.connect("my-input", "my-link");
        builder.connect("my-link", "my-link");
        builder.connect("my-link", "my-output");

        let domain = default_domain();
        let result = builder.build(&domain, &HashMap::new());
        assert!(
            matches!(result, Err(NetworkBuildError::NodeConnectToSelf { name}) if name.to_string() == "my-link"
            )
        );
    }

    #[test]
    /// Test adding a constant parameter to a network.
    fn test_constant_parameter() {
        let mut builder = NetworkBuilder::default();

        let mut input_node_builder = NodeBuilder::input("input");
        // Add the reference to the constant parameter we have not yet added.
        input_node_builder.max_flow(UnresolvedMetricF64::new_parameter_before("my-constant"));
        builder.node(input_node_builder);

        let output_node_builder = NodeBuilder::output("output");
        builder.node(output_node_builder);

        builder.connect("input", "output");

        let input_max_flow_builder = parameters::ConstantParameterBuilder::new("my-constant".into(), 10.0);
        builder.parameters().f64(Box::new(input_max_flow_builder));

        let domain = default_domain();
        builder.build(&domain, &HashMap::new()).unwrap();
    }

    #[test]
    /// Test the error response when a parameter is missing for a node's attribute.
    fn test_missing_parameter_for_node_attr() {
        let mut builder = NetworkBuilder::default();

        let mut input_node_builder = NodeBuilder::input("input");
        // Add the reference to the constant parameter we have not yet added.
        input_node_builder.max_flow(UnresolvedMetricF64::new_parameter_before("this-is-missing"));
        builder.node(input_node_builder);

        let output_node_builder = NodeBuilder::output("output");
        builder.node(output_node_builder);

        builder.connect("input", "output");

        let input_max_flow_builder = parameters::ConstantParameterBuilder::new("my-constant".into(), 10.0);
        builder.parameters().f64(Box::new(input_max_flow_builder));

        let domain = default_domain();

        let build_err = builder
            .build(&domain, &HashMap::new())
            .expect_err("Builder should error.");

        if let NetworkBuildError::NodeBuilderError {
            name, source: node_err, ..
        } = &build_err
            && let NodeBuilderError::ResolveMetricF64Error {
                attr,
                source: metric_err,
            } = node_err.deref()
            && let MetricF64ResolutionError::ParameterNotFound { parameter } = metric_err
        {
            assert_eq!(name.to_string(), "input");
            assert_eq!(attr, "max_flow");
            assert_eq!(parameter.name(), "this-is-missing");
        } else {
            panic!("Incorrect error returned, expect ParameterNotFound: {build_err:?}");
        }
    }

    #[test]
    /// Test a parameter with a reference to a missing parameter.
    fn test_missing_parameter() {
        let mut builder = NetworkBuilder::default();

        let input_node_builder = NodeBuilder::input("input");
        builder.node(input_node_builder);

        let output_node_builder = NodeBuilder::output("output");
        builder.node(output_node_builder);

        builder.connect("input", "output");

        let broken_parameter = parameters::MaxParameterBuilder::new(
            "my-max".into(),
            UnresolvedMetricF64::new_parameter_before("this-is-missing"),
            0.0,
        );
        builder.parameters().f64(Box::new(broken_parameter));

        let domain = default_domain();

        let build_err = builder
            .build(&domain, &HashMap::new())
            .expect_err("Builder should error.");

        if let NetworkBuildError::ParameterCollectionBuildError(coll_err) = &build_err
            && let ParameterCollectionBuilderError::ParameterNotFound { name } = coll_err.deref()
        {
            assert_eq!(name.to_string(), "this-is-missing");
        } else {
            panic!("Incorrect error returned, expected ParameterNotFound: {build_err:?}");
        }
    }

    #[test]
    /// Test the error return when there is a circular reference.
    fn test_circular_parameter() {
        let mut builder = NetworkBuilder::default();

        let input_node_builder = NodeBuilder::input("input");
        builder.node(input_node_builder);

        let output_node_builder = NodeBuilder::output("output");
        builder.node(output_node_builder);

        builder.connect("input", "output");

        let max_param = parameters::MaxParameterBuilder::new(
            "my-max".into(),
            UnresolvedMetricF64::new_parameter_before("my-other-max"),
            0.0,
        );
        builder.parameters().f64(Box::new(max_param));

        let max_param = parameters::MaxParameterBuilder::new(
            "my-other-max".into(),
            UnresolvedMetricF64::new_parameter_before("my-max"),
            0.0,
        );
        builder.parameters().f64(Box::new(max_param));

        let domain = default_domain();

        let build_err = builder
            .build(&domain, &HashMap::new())
            .expect_err("Builder should error.");

        if let NetworkBuildError::ParameterCollectionBuildError(coll_err) = &build_err
            && let ParameterCollectionBuilderError::CircularParameterReference { names } = coll_err.deref()
        {
            // This is fine!
            assert_eq!(names.len(), 2);
            assert_eq!(names[0].name(), "my-max");
            assert_eq!(names[1].name(), "my-other-max");
        } else {
            panic!("Incorrect error returned, expected CircularParameterReference: {build_err:?}");
        }
    }

    #[test]
    fn test_step() {
        const NUM_SCENARIOS: usize = 2;

        let model = simple_model(NUM_SCENARIOS, None).build().unwrap();

        let mut timings = NetworkTimings::new_without_component_timings();

        let mut state = model.setup::<ClpSolver>(&ClpSolverSettings::default()).unwrap();

        let output_node = model.network().get_node_by_name("output", None).unwrap();

        for i in 0..2 {
            model.step(&mut state, None, &mut timings).unwrap();

            for j in 0..NUM_SCENARIOS {
                let state_j = state.network_state().states.get(j).unwrap();

                let output_inflow = state_j
                    .get_network_state()
                    .get_node_in_flow(&output_node.index())
                    .unwrap();
                assert_approx_eq!(f64, output_inflow, (1.0 + i as f64 + j as f64).min(12.0));
            }
        }
    }

    #[test]
    /// Test running a simple model
    fn test_run() {
        let mut model_builder = simple_model(10, None);

        let network = model_builder.network_builder();

        // Set-up assertion for "input" node
        let expected = Array::from_shape_fn((366, 10), |(i, j)| (1.0 + i as f64 + j as f64).min(12.0));

        let recorder = AssertionF64RecorderBuilder::new(
            "input-flow",
            UnresolvedMetricF64::NodeOutFlow("input".into()),
            expected.clone(),
        );
        network.recorder(Box::new(recorder));

        let recorder = AssertionF64RecorderBuilder::new(
            "link-flow",
            UnresolvedMetricF64::NodeOutFlow("link".into()),
            expected.clone(),
        );
        network.recorder(Box::new(recorder));

        let recorder = AssertionF64RecorderBuilder::new(
            "output-flow",
            UnresolvedMetricF64::NodeInFlow("output".into()),
            expected,
        );
        network.recorder(Box::new(recorder));

        let expected = Array2::from_elem((366, 10), 12.0);
        let recorder = AssertionF64RecorderBuilder::new(
            "total-demand",
            UnresolvedMetricF64::new_parameter_before("total-demand"),
            expected,
        );
        network.recorder(Box::new(recorder));

        let model = model_builder.build().unwrap();

        // Test all solvers
        run_all_solvers(&model, &[], &[], &[]);
    }

    #[test]
    fn test_run_storage() {
        let mut network = simple_storage_network();

        let expected = Array2::from_shape_fn((15, 10), |(i, _j)| if i < 10 { 10.0 } else { 0.0 });

        let recorder = AssertionF64RecorderBuilder::new(
            "output-flow",
            UnresolvedMetricF64::NodeInFlow("output".into()),
            expected,
        );
        network.recorder(Box::new(recorder));

        let expected = Array2::from_shape_fn((15, 10), |(i, _j)| (90.0 - 10.0 * i as f64).max(0.0));

        let recorder = AssertionF64RecorderBuilder::new(
            "reservoir-volume",
            UnresolvedMetricF64::NodeVolume("reservoir".into()),
            expected,
        );
        network.recorder(Box::new(recorder));

        let model = ModelBuilder::new(default_domain_builder(), network).build().unwrap();

        // Test all solvers
        run_all_solvers(&model, &[], &[], &[]);
    }

    /// Test proportional storage derived metric.
    ///
    /// Proportional storage is a derived metric that is updated after each solve. However, a
    /// parameter may required a value for the initial time-step based on the initial volume.
    #[test]
    fn test_storage_proportional_volume() {
        let mut model_builder = simple_storage_model();
        let network = model_builder.network_builder();

        // These are the expected values for the proportional volume at the end of the time-step
        let expected = Array2::from_shape_fn((15, 10), |(i, _j)| (90.0 - 10.0 * i as f64).max(0.0) / 100.0);
        let recorder = AssertionF64RecorderBuilder::new(
            "reservoir-proportion-volume",
            UnresolvedMetricF64::NodeProportionalVolume("reservoir".into()),
            expected,
        );
        network.recorder(Box::new(recorder));

        // Set-up a control curve that uses the proportional volume
        // This should be use the initial proportion (100%) on the first time-step, and then the previous day's end value
        let mut cc = ControlCurveInterpolatedParameterBuilder::new(
            "interp".into(),
            UnresolvedMetricF64::NodeProportionalVolume("reservoir".into()),
        );
        cc.value(100.0.into()).value(0.0.into());
        network.parameters().f64(Box::new(cc));

        let expected = Array2::from_shape_fn((15, 10), |(i, _j)| (100.0 - 10.0 * i as f64).max(0.0));

        let recorder = AssertionF64RecorderBuilder::new(
            "reservoir-cc",
            UnresolvedMetricF64::new_parameter_before("interp"),
            expected,
        );
        network.recorder(Box::new(recorder));

        let model = model_builder.build().unwrap();

        // Test all solvers
        run_all_solvers(&model, &[], &[], &[]);
    }

    #[test]
    /// Test the variable API
    fn test_variable_api() {
        let mut model_builder = simple_model(1, None);

        let variable = ActivationFunction::Unit { min: 0.0, max: 10.0 };
        let my_constant: ParameterName = "my-constant".into();
        let input_max_flow = parameters::ConstantParameterBuilder::new(my_constant.clone(), 10.0);

        model_builder
            .network_builder()
            .parameters()
            .f64(Box::new(input_max_flow));

        // assign the new parameter to one of the nodes.
        let input_node_ref = "input".into();
        let node_builder = model_builder.network_builder().node_builder(&input_node_ref).unwrap();
        node_builder.max_flow(UnresolvedMetricF64::new_parameter_before("my-constant"));

        let model = model_builder.build().unwrap();

        let mut state = model.setup::<ClpSolver>(&ClpSolverSettings::default()).unwrap();

        let input_max_flow_idx = model.network().get_parameter_index_by_name(&my_constant).unwrap();

        // Initially the variable value should be unset
        let variable_values = model
            .network()
            .get_f64_parameter_variable_values(input_max_flow_idx, state.network_state())
            .unwrap();
        assert_eq!(variable_values, vec![None]);

        // Update the variable values
        model
            .network()
            .set_f64_parameter_variable_values(input_max_flow_idx, &[5.0], &variable, state.network_state_mut())
            .unwrap();

        // After update the variable value should match what was set
        let variable_values = model
            .network()
            .get_f64_parameter_variable_values(input_max_flow_idx, state.network_state())
            .unwrap();

        assert_eq!(variable_values, vec![Some(vec![5.0])]);
    }
}
