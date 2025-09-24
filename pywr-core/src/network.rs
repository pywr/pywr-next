use crate::aggregated_node::{AggregatedNode, AggregatedNodeIndex, AggregatedNodeVec, Relationship};
use crate::aggregated_storage_node::{AggregatedStorageNode, AggregatedStorageNodeIndex, AggregatedStorageNodeVec};
use crate::derived_metric::{DerivedMetric, DerivedMetricError, DerivedMetricIndex};
use crate::edge::{Edge, EdgeIndex, EdgeVec};
use crate::metric::{MetricF64, SimpleMetricF64};
use crate::models::ModelDomain;
use crate::node::{Node, NodeError, NodeVec, StorageInitialVolume};
use crate::parameters::{
    GeneralParameterIndex, GeneralParameterType, ParameterCalculationError, ParameterCollection,
    ParameterCollectionConstCalculationError, ParameterCollectionError, ParameterCollectionSetupError,
    ParameterCollectionSimpleCalculationError, ParameterIndex, ParameterName, ParameterStates, VariableConfig,
};
use crate::recorders::{
    MetricSet, MetricSetIndex, MetricSetSaveError, MetricSetState, RecorderAggregationError, RecorderFinalResult,
    RecorderFinaliseError, RecorderInternalState, RecorderSaveError, RecorderSetupError,
};
use crate::scenario::ScenarioIndex;
use crate::solvers::{
    MultiStateSolver, Solver, SolverFeatures, SolverSettings, SolverSetupError, SolverSolveError, SolverTimings,
};
use crate::state::{MultiValue, SetStateError, State, StateBuilder};
use crate::timestep::Timestep;
use crate::virtual_storage::{
    VirtualStorage, VirtualStorageBuilder, VirtualStorageError, VirtualStorageIndex, VirtualStorageVec,
};
use crate::{NodeIndex, RecorderIndex, parameters, recorders};
#[cfg(feature = "pyo3")]
use pyo3::{PyResult, exceptions::PyKeyError, pyclass, pymethods};
#[cfg(feature = "pyo3")]
use pyo3_polars::PyDataFrame;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
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

#[derive(Default, Copy, Clone)]
pub struct ComponentTiming {
    calculation: Duration,
    after: Duration,
}

impl ComponentTiming {
    /// Time spent in the calculation method of the component.
    pub fn calculation(&self) -> Duration {
        self.calculation
    }

    /// Time spent in the "after" method of the component.
    pub fn after(&self) -> Duration {
        self.after
    }

    /// Total time spent in calculation and after methods.
    pub fn total(&self) -> Duration {
        self.calculation + self.after
    }
}

/// Collect timing information for component of a network.
#[derive(Clone)]
pub struct ComponentTimings {
    /// Timing information for calculation of each component.
    calculation: Option<Vec<ComponentTiming>>,
    /// Total time spent in component calculations.
    total: Duration,
}

impl ComponentTimings {
    pub fn new_with_components(num_components: usize) -> Self {
        Self {
            calculation: Some(vec![ComponentTiming::default(); num_components]),
            total: Duration::ZERO,
        }
    }

    pub fn new_without_components() -> Self {
        Self {
            calculation: None,
            total: Duration::ZERO,
        }
    }

    /// Returns the slowest `n` components and their duration, if timing information is available.
    ///
    /// This includes both "calculation" and "after" duration.
    pub fn slowest_components(
        &self,
        n: usize,
        component_types: &[ComponentType],
    ) -> Option<Vec<(ComponentType, ComponentTiming)>> {
        self.calculation.as_ref().map(|calculation| {
            let mut components: Vec<_> = calculation
                .iter()
                .zip(component_types)
                .map(|(d, ct)| (*ct, *d))
                .collect();
            components.sort_by_key(|(_, duration)| duration.total());
            components.iter().rev().take(n).map(|(ct, d)| (*ct, *d)).collect()
        })
    }

    /// Add timing information for a component calculation.
    pub fn add_component_calculation_timing(&mut self, idx: usize, duration: Duration) {
        if let Some(calculation) = &mut self.calculation {
            if let Some(c) = calculation.get_mut(idx) {
                c.calculation += duration;
            }
        }
    }

    /// Add timing information for a component "after" calculation.
    pub fn add_component_after_timing(&mut self, idx: usize, duration: Duration) {
        if let Some(calculation) = &mut self.calculation {
            if let Some(c) = calculation.get_mut(idx) {
                c.after += duration;
            }
        }
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
        Self {
            component_timings: ComponentTimings::new_with_components(network.resolve_order.len()),
            recorder_saving: Duration::ZERO,
            solve: SolverTimings::default(),
        }
    }

    pub fn new_without_component_timings() -> Self {
        Self {
            component_timings: ComponentTimings::new_without_components(),
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

        if let Some(slowest) = self.component_timings.slowest_components(10, &network.resolve_order) {
            info!("Slowest components:");
            info!(
                "  {: <24} | {: <10}  | {: <10}  | {: <10}  | {:5}",
                "Component", "Calc", "After", "Total", "% of total"
            );
            for (ct, duration) in slowest {
                info!(
                    "  {: <24} | {: <10.5}s | {: <10.5}s | {: <10.5}s | {:5.2}%",
                    ct.name(network),
                    duration.calculation.as_secs_f64(),
                    duration.after.as_secs_f64(),
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
    DerivedMetric(DerivedMetricIndex),
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
            ComponentType::DerivedMetric(idx) => network
                .get_derived_metric(idx)
                .unwrap()
                .name(network)
                .unwrap()
                .to_string(),
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
    #[error("Error calculating value for parameter `{name}`: {source}")]
    ParameterCalculationError {
        name: ParameterName,
        #[source]
        source: Box<ParameterCalculationError>,
    },
    #[error("Error performing `after` method on parameter `{name}`: {source}")]
    ParameterAfterError {
        name: ParameterName,
        #[source]
        source: Box<ParameterCalculationError>,
    },
    #[error("Error setting state for general F64 parameter `{name}`: {source}")]
    ParameterF64SetStateError {
        name: ParameterName,
        #[source]
        source: SetStateError<GeneralParameterIndex<f64>>,
    },
    #[error("Error setting state for general U64 parameter `{name}`: {source}")]
    ParameterU64SetStateError {
        name: ParameterName,
        #[source]
        source: SetStateError<GeneralParameterIndex<u64>>,
    },
    #[error("Error setting state for general Multi parameter `{name}`: {source}")]
    ParameterMultiSetStateError {
        name: ParameterName,
        #[source]
        source: SetStateError<GeneralParameterIndex<MultiValue>>,
    },
    #[error("Derived metric index not found: {0}")]
    DerivedMetricIndexNotFound(DerivedMetricIndex),
    #[error("Error performing `before` method on derived metric `{name}`: `{source}`")]
    DerivedMetricBeforeError {
        name: String,
        #[source]
        source: DerivedMetricError,
    },
    #[error("Error setting state for derived metric `{name}`: {source}")]
    DerivedMetricSetStateError {
        name: String,
        #[source]
        source: SetStateError<DerivedMetricIndex>,
    },
    #[error("Error calculating derived metric `{name}`: `{source}`")]
    DerivedMetricCalculationError {
        name: String,
        #[source]
        source: DerivedMetricError,
    },
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
    #[error("Cannot connect a node to itself: `{name}`")]
    NodeConnectToSelf { name: String, sub_name: Option<String> },
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
#[cfg_attr(feature = "pyo3", pyclass)]
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
#[derive(Default)]
pub struct Network {
    nodes: NodeVec,
    edges: EdgeVec,
    aggregated_nodes: AggregatedNodeVec,
    aggregated_storage_nodes: AggregatedStorageNodeVec,
    virtual_storage_nodes: VirtualStorageVec,
    parameters: ParameterCollection,
    derived_metrics: Vec<DerivedMetric>,
    metric_sets: Vec<MetricSet>,
    resolve_order: Vec<ComponentType>,
    recorders: Vec<Box<dyn recorders::Recorder>>,
}

impl Network {
    pub fn nodes(&self) -> &NodeVec {
        &self.nodes
    }

    pub fn edges(&self) -> &EdgeVec {
        &self.edges
    }

    pub fn recorders(&self) -> &Vec<Box<dyn recorders::Recorder>> {
        &self.recorders
    }

    pub fn aggregated_nodes(&self) -> &AggregatedNodeVec {
        &self.aggregated_nodes
    }

    pub fn aggregated_storage_nodes(&self) -> &AggregatedStorageNodeVec {
        &self.aggregated_storage_nodes
    }

    pub fn virtual_storage_nodes(&self) -> &VirtualStorageVec {
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
                .with_derived_metrics(self.derived_metrics.len())
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
            .zip(recorder_internal_states.into_iter())
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
        mut timings: Option<&mut ComponentTimings>,
    ) -> Result<(), NetworkStepError> {
        // TODO reset parameter state to zero

        // First we update the simple parameters
        self.parameters
            .compute_simple(timestep, scenario_index, state, internal_states)?;

        for (c_idx, c_type) in self.resolve_order.iter().enumerate() {
            let start = Instant::now();

            match c_type {
                ComponentType::Node(idx) => {
                    let n = self
                        .nodes
                        .get(idx)
                        .ok_or_else(|| NetworkStepError::NodeIndexNotFound(*idx))?;

                    n.before(timestep, state)
                        .map_err(|source| NetworkStepError::NodeBeforeError {
                            name: n.name().to_string(),
                            source,
                        })?;
                }
                ComponentType::VirtualStorageNode(idx) => {
                    let n = self
                        .virtual_storage_nodes
                        .get(idx)
                        .ok_or_else(|| NetworkStepError::VirtualStorageIndexNotFound(*idx))?;

                    n.before(timestep, state)
                        .map_err(|source| NetworkStepError::VirtualStorageBeforeError {
                            name: n.name().to_string(),
                            source,
                        })?;
                }
                ComponentType::Parameter(p_type) => {
                    match p_type {
                        GeneralParameterType::Parameter(idx) => {
                            // Find the parameter itself
                            let p = self
                                .parameters
                                .get_general_f64(*idx)
                                .ok_or_else(|| NetworkStepError::ParameterF64IndexNotFound(*idx))?;

                            // ... and its internal state
                            let internal_state = internal_states
                                .get_general_mut_f64_state(*idx)
                                .ok_or_else(|| NetworkStepError::ParameterF64IndexNotFound(*idx))?;

                            let value = p
                                .compute(timestep, scenario_index, self, state, internal_state)
                                .map_err(|source| NetworkStepError::ParameterCalculationError {
                                    name: p.name().clone(),
                                    source: Box::new(source),
                                })?;

                            state.set_parameter_value(*idx, value).map_err(|source| {
                                NetworkStepError::ParameterF64SetStateError {
                                    name: p.name().clone(),
                                    source,
                                }
                            })?;
                        }
                        GeneralParameterType::Index(idx) => {
                            let p = self
                                .parameters
                                .get_general_u64(*idx)
                                .ok_or_else(|| NetworkStepError::ParameterU64IndexNotFound(*idx))?;

                            // ... and its internal state
                            let internal_state = internal_states
                                .get_general_mut_u64_state(*idx)
                                .ok_or_else(|| NetworkStepError::ParameterU64IndexNotFound(*idx))?;

                            let value = p
                                .compute(timestep, scenario_index, self, state, internal_state)
                                .map_err(|source| NetworkStepError::ParameterCalculationError {
                                    name: p.name().clone(),
                                    source: Box::new(source),
                                })?;

                            state.set_parameter_index(*idx, value).map_err(|source| {
                                NetworkStepError::ParameterU64SetStateError {
                                    name: p.name().clone(),
                                    source,
                                }
                            })?;
                        }
                        GeneralParameterType::Multi(idx) => {
                            let p = self
                                .parameters
                                .get_general_multi(idx)
                                .ok_or_else(|| NetworkStepError::ParameterMultiIndexNotFound(*idx))?;

                            // ... and its internal state
                            let internal_state = internal_states
                                .get_general_mut_multi_state(*idx)
                                .ok_or_else(|| NetworkStepError::ParameterMultiIndexNotFound(*idx))?;

                            let value = p
                                .compute(timestep, scenario_index, self, state, internal_state)
                                .map_err(|source| NetworkStepError::ParameterCalculationError {
                                    name: p.name().clone(),
                                    source: Box::new(source),
                                })?;
                            // debug!("Current value of index parameter {}: {}", p.name(), value);
                            state.set_multi_parameter_value(*idx, value).map_err(|source| {
                                NetworkStepError::ParameterMultiSetStateError {
                                    name: p.name().clone(),
                                    source,
                                }
                            })?;
                        }
                    }
                }
                ComponentType::DerivedMetric(idx) => {
                    // Compute derived metrics in before
                    let m = self
                        .derived_metrics
                        .get(*idx.deref())
                        .ok_or(NetworkStepError::DerivedMetricIndexNotFound(*idx))?;

                    let maybe_new_value = m.before(timestep, self, state).map_err(|source| {
                        // There could be an error determining the name of the metric!
                        let name = m.name(self).unwrap_or("unknown").to_string();
                        NetworkStepError::DerivedMetricBeforeError { name, source }
                    })?;

                    if let Some(value) = maybe_new_value {
                        state.set_derived_metric_value(*idx, value).map_err(|source| {
                            let name = m.name(self).unwrap_or("unknown").to_string();
                            NetworkStepError::DerivedMetricSetStateError { name, source }
                        })?;
                    }
                }
            }

            if let Some(timings) = timings.as_deref_mut() {
                // Update the component timings
                timings.add_component_calculation_timing(c_idx, start.elapsed());
            }
        }

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
        mut timings: Option<&mut ComponentTimings>,
    ) -> Result<(), NetworkStepError> {
        // TODO reset parameter state to zero

        self.parameters
            .after_simple(timestep, scenario_index, state, internal_states)?;

        for (c_idx, c_type) in self.resolve_order.iter().enumerate() {
            let start = Instant::now();

            match c_type {
                ComponentType::Node(_) => {
                    // Nodes do not have an "after" method.
                }
                ComponentType::VirtualStorageNode(_) => {
                    // Nodes do not have an "after" method.;
                }
                ComponentType::Parameter(p_type) => {
                    match p_type {
                        GeneralParameterType::Parameter(idx) => {
                            // Find the parameter itself
                            let p = self
                                .parameters
                                .get_general_f64(*idx)
                                .ok_or_else(|| NetworkStepError::ParameterF64IndexNotFound(*idx))?;

                            // ... and its internal state
                            let internal_state = internal_states
                                .get_general_mut_f64_state(*idx)
                                .ok_or_else(|| NetworkStepError::ParameterF64IndexNotFound(*idx))?;

                            p.after(timestep, scenario_index, self, state, internal_state)
                                .map_err(|source| NetworkStepError::ParameterAfterError {
                                    name: p.name().clone(),
                                    source: Box::new(source),
                                })?;
                        }
                        GeneralParameterType::Index(idx) => {
                            let p = self
                                .parameters
                                .get_general_u64(*idx)
                                .ok_or_else(|| NetworkStepError::ParameterU64IndexNotFound(*idx))?;

                            // .. and its internal state
                            let internal_state = internal_states
                                .get_general_mut_u64_state(*idx)
                                .ok_or_else(|| NetworkStepError::ParameterU64IndexNotFound(*idx))?;

                            p.after(timestep, scenario_index, self, state, internal_state)
                                .map_err(|source| NetworkStepError::ParameterAfterError {
                                    name: p.name().clone(),
                                    source: Box::new(source),
                                })?;
                        }
                        GeneralParameterType::Multi(idx) => {
                            let p = self
                                .parameters
                                .get_general_multi(idx)
                                .ok_or_else(|| NetworkStepError::ParameterMultiIndexNotFound(*idx))?;

                            // .. and its internal state
                            let internal_state = internal_states
                                .get_general_mut_multi_state(*idx)
                                .ok_or_else(|| NetworkStepError::ParameterMultiIndexNotFound(*idx))?;

                            p.after(timestep, scenario_index, self, state, internal_state)
                                .map_err(|source| NetworkStepError::ParameterAfterError {
                                    name: p.name().clone(),
                                    source: Box::new(source),
                                })?;
                        }
                    }
                }
                ComponentType::DerivedMetric(idx) => {
                    // Compute derived metrics in "after"
                    let m = self
                        .derived_metrics
                        .get(*idx.deref())
                        .ok_or(NetworkStepError::DerivedMetricIndexNotFound(*idx))?;

                    let value = m.compute(self, state).map_err(|source| {
                        // There could be an error determining the name of the metric!
                        let name = m.name(self).unwrap_or("unknown").to_string();
                        NetworkStepError::DerivedMetricCalculationError { name, source }
                    })?;

                    state.set_derived_metric_value(*idx, value).map_err(|source| {
                        let name = m.name(self).unwrap_or("unknown").to_string();
                        NetworkStepError::DerivedMetricSetStateError { name, source }
                    })?;
                }
            }

            if let Some(timings) = timings.as_deref_mut() {
                // Update the component timings
                timings.add_component_after_timing(c_idx, start.elapsed());
            }
        }

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
        self.edges.get(index)
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
        self.nodes.get(index)
    }

    /// Get a Node from a node's index
    pub fn get_node_mut(&mut self, index: &NodeIndex) -> Option<&mut Node> {
        self.nodes.get_mut(index)
    }

    /// Get a Node from a node's name
    pub fn get_node_by_name(&self, name: &str, sub_name: Option<&str>) -> Option<&Node> {
        self.nodes.iter().find(|&n| n.full_name() == (name, sub_name))
    }

    /// Get a NodeIndex from a node's name
    pub fn get_mut_node_by_name(&mut self, name: &str, sub_name: Option<&str>) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.full_name() == (name, sub_name))
    }

    pub fn set_node_cost(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        value: Option<MetricF64>,
    ) -> Result<(), NetworkError> {
        let node = self
            .get_mut_node_by_name(name, sub_name)
            .ok_or(NetworkError::NodeNotFound {
                name: name.to_string(),
                sub_name: sub_name.map(|s| s.to_string()),
            })?;

        node.set_cost(value);
        Ok(())
    }

    pub fn set_node_max_flow(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        value: Option<MetricF64>,
    ) -> Result<(), NetworkError> {
        let node = self
            .get_mut_node_by_name(name, sub_name)
            .ok_or(NetworkError::NodeNotFound {
                name: name.to_string(),
                sub_name: sub_name.map(|s| s.to_string()),
            })?;

        node.set_max_flow_constraint(value)
            .map_err(|source| NetworkError::NodeSetAttributeError {
                name: node.name().to_string(),
                sub_name: node.sub_name().map(|s| s.to_string()),
                attribute: "max_flow".to_string(),
                source: Box::new(source),
            })
    }

    pub fn set_node_min_flow(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        value: Option<MetricF64>,
    ) -> Result<(), NetworkError> {
        let node = self
            .get_mut_node_by_name(name, sub_name)
            .ok_or(NetworkError::NodeNotFound {
                name: name.to_string(),
                sub_name: sub_name.map(|s| s.to_string()),
            })?;

        node.set_min_flow_constraint(value)
            .map_err(|source| NetworkError::NodeSetAttributeError {
                name: node.name().to_string(),
                sub_name: node.sub_name().map(|s| s.to_string()),
                attribute: "min_flow".to_string(),
                source: Box::new(source),
            })
    }
    pub fn set_node_initial_volume(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        initial_volume: StorageInitialVolume,
    ) -> Result<(), NetworkError> {
        let node = self
            .get_mut_node_by_name(name, sub_name)
            .ok_or(NetworkError::NodeNotFound {
                name: name.to_string(),
                sub_name: sub_name.map(|s| s.to_string()),
            })?;

        node.set_initial_volume(initial_volume)
            .map_err(|source| NetworkError::NodeSetAttributeError {
                name: node.name().to_string(),
                sub_name: node.sub_name().map(|s| s.to_string()),
                attribute: "initial_volume".to_string(),
                source: Box::new(source),
            })
    }

    pub fn set_node_max_volume(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        value: Option<SimpleMetricF64>,
    ) -> Result<(), NetworkError> {
        let node = self
            .get_mut_node_by_name(name, sub_name)
            .ok_or(NetworkError::NodeNotFound {
                name: name.to_string(),
                sub_name: sub_name.map(|s| s.to_string()),
            })?;

        node.set_max_volume_constraint(value)
            .map_err(|source| NetworkError::NodeSetAttributeError {
                name: node.name().to_string(),
                sub_name: node.sub_name().map(|s| s.to_string()),
                attribute: "max_volume".to_string(),
                source: Box::new(source),
            })
    }

    pub fn set_node_min_volume(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        value: Option<SimpleMetricF64>,
    ) -> Result<(), NetworkError> {
        let node = self
            .get_mut_node_by_name(name, sub_name)
            .ok_or(NetworkError::NodeNotFound {
                name: name.to_string(),
                sub_name: sub_name.map(|s| s.to_string()),
            })?;

        node.set_min_volume_constraint(value)
            .map_err(|source| NetworkError::NodeSetAttributeError {
                name: node.name().to_string(),
                sub_name: node.sub_name().map(|s| s.to_string()),
                attribute: "min_volume".to_string(),
                source: Box::new(source),
            })
    }

    /// Get an [`AggregatedNode`] from its index.
    pub fn get_aggregated_node(&self, index: &AggregatedNodeIndex) -> Option<&AggregatedNode> {
        self.aggregated_nodes.get(index)
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

    pub fn set_aggregated_node_max_flow(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        value: Option<MetricF64>,
    ) -> Result<(), NetworkError> {
        let node = self
            .get_mut_aggregated_node_by_name(name, sub_name)
            .ok_or(NetworkError::NodeNotFound {
                name: name.to_string(),
                sub_name: sub_name.map(|s| s.to_string()),
            })?;

        node.set_max_flow_constraint(value);
        Ok(())
    }

    pub fn set_aggregated_node_min_flow(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        value: Option<MetricF64>,
    ) -> Result<(), NetworkError> {
        let node = self
            .get_mut_aggregated_node_by_name(name, sub_name)
            .ok_or(NetworkError::NodeNotFound {
                name: name.to_string(),
                sub_name: sub_name.map(|s| s.to_string()),
            })?;

        node.set_min_flow_constraint(value);
        Ok(())
    }

    pub fn set_aggregated_node_relationship(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        relationship: Option<Relationship>,
    ) -> Result<(), NetworkError> {
        let node = self
            .get_mut_aggregated_node_by_name(name, sub_name)
            .ok_or(NetworkError::NodeNotFound {
                name: name.to_string(),
                sub_name: sub_name.map(|s| s.to_string()),
            })?;

        node.set_relationship(relationship);
        Ok(())
    }

    /// Get a `&AggregatedStorageNode` from a node's name
    pub fn get_aggregated_storage_node(&self, index: &AggregatedStorageNodeIndex) -> Option<&AggregatedStorageNode> {
        self.aggregated_storage_nodes.get(index)
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
    pub fn get_virtual_storage_node(&self, index: &VirtualStorageIndex) -> Option<&VirtualStorage> {
        self.virtual_storage_nodes.get(index)
    }

    /// Get a `VirtualStorageNode` from a node's name
    pub fn get_virtual_storage_node_by_name(&self, name: &str, sub_name: Option<&str>) -> Option<&VirtualStorage> {
        self.virtual_storage_nodes
            .iter()
            .find(|&n| n.full_name() == (name, sub_name))
    }

    /// Get a `VirtualStorageNode` from a node's name
    pub fn get_mut_virtual_storage_node_by_name(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
    ) -> Option<&mut VirtualStorage> {
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

    pub fn set_virtual_storage_cost(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        value: Option<MetricF64>,
    ) -> Result<(), NetworkError> {
        let node = self
            .get_mut_virtual_storage_node_by_name(name, sub_name)
            .ok_or(NetworkError::NodeNotFound {
                name: name.to_string(),
                sub_name: sub_name.map(|s| s.to_string()),
            })?;

        node.set_cost(value);
        Ok(())
    }

    pub fn set_virtual_storage_max_volume(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        value: Option<SimpleMetricF64>,
    ) -> Result<(), NetworkError> {
        let node = self
            .get_mut_virtual_storage_node_by_name(name, sub_name)
            .ok_or(NetworkError::NodeNotFound {
                name: name.to_string(),
                sub_name: sub_name.map(|s| s.to_string()),
            })?;

        node.set_max_volume_constraint(value);
        Ok(())
    }

    pub fn set_virtual_storage_min_volume(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        value: Option<SimpleMetricF64>,
    ) -> Result<(), NetworkError> {
        let node = self
            .get_mut_virtual_storage_node_by_name(name, sub_name)
            .ok_or(NetworkError::NodeNotFound {
                name: name.to_string(),
                sub_name: sub_name.map(|s| s.to_string()),
            })?;

        node.set_min_volume_constraint(value);
        Ok(())
    }

    pub fn get_storage_node_metric(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        proportional: bool,
    ) -> Result<MetricF64, NetworkError> {
        if let Some(idx) = self.get_node_index_by_name(name, sub_name) {
            // A regular node
            if proportional {
                // Proportional is a derived metric
                let dm_idx = self.add_derived_metric(DerivedMetric::NodeProportionalVolume(idx));
                Ok(MetricF64::DerivedMetric(dm_idx))
            } else {
                Ok(MetricF64::NodeVolume(idx))
            }
        } else if let Some(idx) = self.get_aggregated_storage_node_index_by_name(name, sub_name) {
            if proportional {
                // Proportional is a derived metric
                let dm_idx = self.add_derived_metric(DerivedMetric::AggregatedNodeProportionalVolume(idx));
                Ok(MetricF64::DerivedMetric(dm_idx))
            } else {
                Ok(MetricF64::AggregatedNodeVolume(idx))
            }
        } else if let Some(node) = self.get_virtual_storage_node_by_name(name, sub_name) {
            if proportional {
                // Proportional is a derived metric
                let dm_idx = self.add_derived_metric(DerivedMetric::VirtualStorageProportionalVolume(node.index()));
                Ok(MetricF64::DerivedMetric(dm_idx))
            } else {
                Ok(MetricF64::VirtualStorageVolume(node.index()))
            }
        } else {
            Err(NetworkError::NodeNotFound {
                name: name.to_string(),
                sub_name: sub_name.map(|s| s.to_string()),
            })
        }
    }

    /// Get a [`DerivedMetricIndex`] for the given derived metric
    pub fn get_derived_metric_index(&self, derived_metric: &DerivedMetric) -> Option<DerivedMetricIndex> {
        self.derived_metrics
            .iter()
            .position(|dm| dm == derived_metric)
            .map(DerivedMetricIndex::new)
    }

    /// Get a [`DerivedMetricIndex`] for the given derived metric
    pub fn get_derived_metric(&self, index: &DerivedMetricIndex) -> Option<&DerivedMetric> {
        self.derived_metrics.get(*index.deref())
    }

    pub fn add_derived_metric(&mut self, derived_metric: DerivedMetric) -> DerivedMetricIndex {
        match self.get_derived_metric_index(&derived_metric) {
            Some(idx) => idx,
            None => {
                self.derived_metrics.push(derived_metric);
                let idx = DerivedMetricIndex::new(self.derived_metrics.len() - 1);
                self.resolve_order.push(ComponentType::DerivedMetric(idx));
                idx
            }
        }
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

    /// Add a new Node::Input to the network.
    pub fn add_input_node(&mut self, name: &str, sub_name: Option<&str>) -> Result<NodeIndex, NetworkError> {
        // Check for name.
        // TODO move this check to `NodeVec`
        if self.get_node_by_name(name, sub_name).is_some() {
            return Err(NetworkError::NodeAlreadyExists {
                name: name.to_string(),
                sub_name: sub_name.map(|s| s.to_string()),
            });
        }

        // Now add the node to the network.
        let node_index = self.nodes.push_new_input(name, sub_name);
        // ... and add it to the resolve order.
        self.resolve_order.push(ComponentType::Node(node_index));
        Ok(node_index)
    }

    /// Add a new Node::Link to the network.
    pub fn add_link_node(&mut self, name: &str, sub_name: Option<&str>) -> Result<NodeIndex, NetworkError> {
        // Check for name.
        // TODO move this check to `NodeVec`
        if self.get_node_by_name(name, sub_name).is_some() {
            return Err(NetworkError::NodeAlreadyExists {
                name: name.to_string(),
                sub_name: sub_name.map(|s| s.to_string()),
            });
        }

        // Now add the node to the network.
        let node_index = self.nodes.push_new_link(name, sub_name);
        // ... and add it to the resolve order.
        self.resolve_order.push(ComponentType::Node(node_index));
        Ok(node_index)
    }

    /// Add a new Node::Link to the network.
    pub fn add_output_node(&mut self, name: &str, sub_name: Option<&str>) -> Result<NodeIndex, NetworkError> {
        // Check for name.
        // TODO move this check to `NodeVec`
        if self.get_node_by_name(name, sub_name).is_some() {
            return Err(NetworkError::NodeAlreadyExists {
                name: name.to_string(),
                sub_name: sub_name.map(|s| s.to_string()),
            });
        }

        // Now add the node to the network.
        let node_index = self.nodes.push_new_output(name, sub_name);
        // ... and add it to the resolve order.
        self.resolve_order.push(ComponentType::Node(node_index));
        Ok(node_index)
    }

    /// Add a new Node::Link to the network.
    pub fn add_storage_node(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        initial_volume: StorageInitialVolume,
        min_volume: Option<SimpleMetricF64>,
        max_volume: Option<SimpleMetricF64>,
    ) -> Result<NodeIndex, NetworkError> {
        // Check for name.
        // TODO move this check to `NodeVec`
        if self.get_node_by_name(name, sub_name).is_some() {
            return Err(NetworkError::NodeAlreadyExists {
                name: name.to_string(),
                sub_name: sub_name.map(|s| s.to_string()),
            });
        }

        // Now add the node to the network.
        let node_index = self
            .nodes
            .push_new_storage(name, sub_name, initial_volume, min_volume, max_volume);
        // ... and add it to the resolve order.
        self.resolve_order.push(ComponentType::Node(node_index));
        Ok(node_index)
    }

    /// Add a new `aggregated_node::AggregatedNode` to the network.
    pub fn add_aggregated_node(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        nodes: &[Vec<NodeIndex>],
        relationship: Option<Relationship>,
    ) -> Result<AggregatedNodeIndex, NetworkError> {
        if self.get_aggregated_node_by_name(name, sub_name).is_some() {
            return Err(NetworkError::NodeAlreadyExists {
                name: name.to_string(),
                sub_name: sub_name.map(|s| s.to_string()),
            });
        }

        let node_index = self.aggregated_nodes.push_new(name, sub_name, nodes, relationship);
        Ok(node_index)
    }

    /// Add a new `aggregated_storage_node::AggregatedStorageNode` to the network.
    pub fn add_aggregated_storage_node(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        nodes: Vec<NodeIndex>,
    ) -> Result<AggregatedStorageNodeIndex, NetworkError> {
        if self.get_aggregated_storage_node_by_name(name, sub_name).is_some() {
            return Err(NetworkError::NodeAlreadyExists {
                name: name.to_string(),
                sub_name: sub_name.map(|s| s.to_string()),
            });
        }

        let node_index = self.aggregated_storage_nodes.push_new(name, sub_name, nodes);
        Ok(node_index)
    }

    /// Add a new `VirtualStorage` to the network.
    pub fn add_virtual_storage_node(
        &mut self,
        builder: VirtualStorageBuilder,
    ) -> Result<VirtualStorageIndex, NetworkError> {
        let vs_node_index = self.virtual_storage_nodes.push_new(builder)?;

        let vs_node = self
            .virtual_storage_nodes
            .get(&vs_node_index)
            .expect("VirtualStorageNode not found; this is a bug and should not be possible.");

        // Link the virtual storage node to the nodes it is including
        for node_idx in vs_node.nodes() {
            let node = self
                .nodes
                .get_mut(node_idx)
                .ok_or(NetworkError::NodeIndexNotFound { index: *node_idx })?;

            node.add_virtual_storage(vs_node_index)
                .map_err(|source| NetworkError::NodeError {
                    name: node.name().to_string(),
                    sub_name: node.sub_name().map(|s| s.to_string()),
                    source: Box::new(source),
                })?;
        }

        // Add to the resolve order.
        self.resolve_order
            .push(ComponentType::VirtualStorageNode(vs_node_index));

        Ok(vs_node_index)
    }

    pub fn set_virtual_storage_node_cost(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        value: Option<MetricF64>,
    ) -> Result<(), NetworkError> {
        let node = self
            .get_mut_virtual_storage_node_by_name(name, sub_name)
            .ok_or_else(|| NetworkError::NodeNotFound {
                name: name.to_string(),
                sub_name: sub_name.map(|s| s.to_string()),
            })?;
        node.set_cost(value);
        Ok(())
    }

    /// Add a [`parameters::GeneralParameter`] to the network
    pub fn add_parameter(
        &mut self,
        parameter: Box<dyn parameters::GeneralParameter<f64>>,
    ) -> Result<ParameterIndex<f64>, NetworkError> {
        let parameter_index = self.parameters.add_general_f64(parameter)?;

        // add it to the general resolve order (simple and constant parameters are resolved separately)
        if let ParameterIndex::General(idx) = parameter_index {
            self.resolve_order.push(ComponentType::Parameter(idx.into()));
        }

        Ok(parameter_index)
    }

    /// Add a [`parameters::SimpleParameter`] to the network
    pub fn add_simple_parameter(
        &mut self,
        parameter: Box<dyn parameters::SimpleParameter<f64>>,
    ) -> Result<ParameterIndex<f64>, NetworkError> {
        Ok(self.parameters.add_simple_f64(parameter)?)
    }

    /// Add a [`parameters::SimpleParameter`] to the network
    pub fn add_simple_index_parameter(
        &mut self,
        parameter: Box<dyn parameters::SimpleParameter<u64>>,
    ) -> Result<ParameterIndex<u64>, NetworkError> {
        Ok(self.parameters.add_simple_u64(parameter)?)
    }

    /// Add a [`parameters::ConstParameter`] to the network
    pub fn add_const_parameter(
        &mut self,
        parameter: Box<dyn parameters::ConstParameter<f64>>,
    ) -> Result<ParameterIndex<f64>, NetworkError> {
        Ok(self.parameters.add_const_f64(parameter)?)
    }

    /// Add a `parameters::IndexParameter` to the network
    pub fn add_index_parameter(
        &mut self,
        parameter: Box<dyn parameters::GeneralParameter<u64>>,
    ) -> Result<ParameterIndex<u64>, NetworkError> {
        let parameter_index = self.parameters.add_general_u64(parameter)?;
        // add it to the general resolve order (simple and constant parameters are resolved separately)
        if let ParameterIndex::General(idx) = parameter_index {
            self.resolve_order.push(ComponentType::Parameter(idx.into()));
        }

        Ok(parameter_index)
    }

    /// Add a `parameters::MultiValueParameter` to the network
    pub fn add_multi_value_parameter(
        &mut self,
        parameter: Box<dyn parameters::GeneralParameter<MultiValue>>,
    ) -> Result<ParameterIndex<MultiValue>, NetworkError> {
        let parameter_index = self.parameters.add_general_multi(parameter)?;
        // add it to the general resolve order (simple and constant parameters are resolved separately)
        if let ParameterIndex::General(idx) = parameter_index {
            self.resolve_order.push(ComponentType::Parameter(idx.into()));
        }

        Ok(parameter_index)
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

    /// Add a `recorders::Recorder` to the network
    pub fn add_recorder(&mut self, recorder: Box<dyn recorders::Recorder>) -> Result<RecorderIndex, NetworkError> {
        // TODO reinstate this check
        // if let Ok(idx) = self.get_recorder_by_name(&recorder.meta().name) {
        //     return Err(PywrError::RecorderNameAlreadyExists(
        //         recorder.meta().name.to_string(),
        //         idx,
        //     ));
        // }

        let recorder_index = RecorderIndex::new(self.recorders.len());
        self.recorders.push(recorder);
        Ok(recorder_index)
    }

    /// Connect two nodes together
    pub fn connect_nodes(
        &mut self,
        from_node_index: NodeIndex,
        to_node_index: NodeIndex,
    ) -> Result<EdgeIndex, NetworkError> {
        // The network can get in a bad state here if the edge is added to the `from_node`
        // successfully, but fails on the `to_node`.
        // Suggest to do a check before attempting to add.
        let from_node = self
            .nodes
            .get_mut(&from_node_index)
            .ok_or(NetworkError::NodeIndexNotFound { index: from_node_index })?;

        // Self connections are not allowed.
        if from_node_index == to_node_index {
            return Err(NetworkError::NodeConnectToSelf {
                name: from_node.name().to_string(),
                sub_name: from_node.sub_name().map(|s| s.to_string()),
            });
        }

        // Next edge index
        let edge_index = self.edges.push(from_node_index, to_node_index);

        from_node
            .add_outgoing_edge(edge_index)
            .map_err(|source| NetworkError::NodeError {
                name: from_node.name().to_string(),
                sub_name: from_node.sub_name().map(|s| s.to_string()),
                source: Box::new(source),
            })?;

        let to_node = self
            .nodes
            .get_mut(&to_node_index)
            .ok_or(NetworkError::NodeIndexNotFound { index: from_node_index })?;

        to_node
            .add_incoming_edge(edge_index)
            .map_err(|source| NetworkError::NodeError {
                name: to_node.name().to_string(),
                sub_name: to_node.sub_name().map(|s| s.to_string()),
                source: Box::new(source),
            })?;

        Ok(edge_index)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metric::MetricF64;
    use crate::network::Network;
    use crate::parameters::{ActivationFunction, ControlCurveInterpolatedParameter, Parameter};
    use crate::recorders::AssertionF64Recorder;
    use crate::solvers::{ClpSolver, ClpSolverSettings};
    use crate::test_utils::{run_all_solvers, simple_model, simple_storage_model};
    use float_cmp::assert_approx_eq;
    use ndarray::{Array, Array2};
    use std::default::Default;
    use std::ops::Deref;

    #[test]
    fn test_simple_network() {
        let mut network = Network::default();

        let input_node = network.add_input_node("input", None).unwrap();
        let link_node = network.add_link_node("link", None).unwrap();
        let output_node = network.add_output_node("output", None).unwrap();

        assert_eq!(*input_node.deref(), 0);
        assert_eq!(*link_node.deref(), 1);
        assert_eq!(*output_node.deref(), 2);

        let edge = network.connect_nodes(input_node, link_node).unwrap();
        assert_eq!(*edge.deref(), 0);
        let edge = network.connect_nodes(link_node, output_node).unwrap();
        assert_eq!(*edge.deref(), 1);

        // Now assert the internal structure is as expected.
        let input_node = network.get_node_by_name("input", None).unwrap();
        let link_node = network.get_node_by_name("link", None).unwrap();
        let output_node = network.get_node_by_name("output", None).unwrap();
        assert_eq!(input_node.get_outgoing_edges().unwrap().len(), 1);
        assert_eq!(link_node.get_incoming_edges().unwrap().len(), 1);
        assert_eq!(link_node.get_outgoing_edges().unwrap().len(), 1);
        assert_eq!(output_node.get_incoming_edges().unwrap().len(), 1);
    }

    #[test]
    /// Test the duplicate node names are not permitted.
    fn test_duplicate_node_name() {
        let mut network = Network::default();

        network.add_input_node("my-node", None).unwrap();
        // Second add with the same name
        assert!(matches!(
            network.add_input_node("my-node", None),
            Err(NetworkError::NodeAlreadyExists { name, .. }) if name == "my-node"));

        network.add_input_node("my-node", Some("a")).unwrap();
        // Second add with the same name
        assert!(matches!(
            network.add_input_node("my-node", Some("a")),
            Err(NetworkError::NodeAlreadyExists { name, .. }) if name == "my-node"));

        assert!(matches!(
            network.add_link_node("my-node", None),
            Err(NetworkError::NodeAlreadyExists { name, .. }) if name == "my-node"));

        assert!(matches!(
            network.add_output_node("my-node", None),
            Err(NetworkError::NodeAlreadyExists { name, .. }) if name == "my-node"));

        assert!(matches!(
            network.add_storage_node(
                "my-node",
                None,
                StorageInitialVolume::Absolute(10.0),
                None,
                Some(10.0.into())
            ),
            Err(NetworkError::NodeAlreadyExists { name, .. }) if name == "my-node"));
    }

    #[test]
    /// Test adding a constant parameter to a network.
    fn test_constant_parameter() {
        let mut network = Network::default();
        let _node_index = network.add_input_node("input", None).unwrap();

        let input_max_flow = parameters::ConstantParameter::new("my-constant".into(), 10.0);
        let parameter = network.add_const_parameter(Box::new(input_max_flow)).unwrap();

        // assign the new parameter to one of the nodes.
        let node = network.get_mut_node_by_name("input", None).unwrap();
        node.set_max_flow_constraint(Some(parameter.into())).unwrap();

        // Try to assign a constraint not defined for particular node type
        assert!(matches!(
            node.set_max_volume_constraint(Some(10.0.into())),
            Err(NodeError::StorageConstraintsUndefined)
        ));
    }

    #[test]
    fn test_step() {
        const NUM_SCENARIOS: usize = 2;
        let model = simple_model(NUM_SCENARIOS, None);

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
        let mut model = simple_model(10, None);

        // Set-up assertion for "input" node
        let idx = model.network().get_node_by_name("input", None).unwrap().index();
        let expected = Array::from_shape_fn((366, 10), |(i, j)| (1.0 + i as f64 + j as f64).min(12.0));

        let recorder =
            AssertionF64Recorder::new("input-flow", MetricF64::NodeOutFlow(idx), expected.clone(), None, None);
        model.network_mut().add_recorder(Box::new(recorder)).unwrap();

        let idx = model.network().get_node_by_name("link", None).unwrap().index();
        let recorder =
            AssertionF64Recorder::new("link-flow", MetricF64::NodeOutFlow(idx), expected.clone(), None, None);
        model.network_mut().add_recorder(Box::new(recorder)).unwrap();

        let idx = model.network().get_node_by_name("output", None).unwrap().index();
        let recorder = AssertionF64Recorder::new("output-flow", MetricF64::NodeInFlow(idx), expected, None, None);
        model.network_mut().add_recorder(Box::new(recorder)).unwrap();

        let idx = model
            .network()
            .get_parameter_index_by_name(&"total-demand".into())
            .unwrap();
        let expected = Array2::from_elem((366, 10), 12.0);
        let recorder = AssertionF64Recorder::new("total-demand", idx.into(), expected, None, None);
        model.network_mut().add_recorder(Box::new(recorder)).unwrap();

        // Test all solvers
        run_all_solvers(&model, &[], &[], &[]);
    }

    #[test]
    fn test_run_storage() {
        let mut model = simple_storage_model();

        let network = model.network_mut();

        let idx = network.get_node_by_name("output", None).unwrap().index();

        let expected = Array2::from_shape_fn((15, 10), |(i, _j)| if i < 10 { 10.0 } else { 0.0 });

        let recorder = AssertionF64Recorder::new("output-flow", MetricF64::NodeInFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        let idx = network.get_node_by_name("reservoir", None).unwrap().index();

        let expected = Array2::from_shape_fn((15, 10), |(i, _j)| (90.0 - 10.0 * i as f64).max(0.0));

        let recorder = AssertionF64Recorder::new("reservoir-volume", MetricF64::NodeVolume(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        // Test all solvers
        run_all_solvers(&model, &[], &[], &[]);
    }

    /// Test proportional storage derived metric.
    ///
    /// Proportional storage is a derived metric that is updated after each solve. However, a
    /// parameter may required a value for the initial time-step based on the initial volume.
    #[test]
    fn test_storage_proportional_volume() {
        let mut model = simple_storage_model();
        let network = model.network_mut();
        let idx = network.get_node_by_name("reservoir", None).unwrap().index();
        let dm_idx = network.add_derived_metric(DerivedMetric::NodeProportionalVolume(idx));

        // These are the expected values for the proportional volume at the end of the time-step
        let expected = Array2::from_shape_fn((15, 10), |(i, _j)| (90.0 - 10.0 * i as f64).max(0.0) / 100.0);
        let recorder = AssertionF64Recorder::new(
            "reservoir-proportion-volume",
            MetricF64::DerivedMetric(dm_idx),
            expected,
            None,
            None,
        );
        network.add_recorder(Box::new(recorder)).unwrap();

        // Set-up a control curve that uses the proportional volume
        // This should be use the initial proportion (100%) on the first time-step, and then the previous day's end value
        let cc = ControlCurveInterpolatedParameter::new(
            "interp".into(),
            MetricF64::DerivedMetric(dm_idx),
            vec![],
            vec![100.0.into(), 0.0.into()],
        );
        let p_idx = network.add_parameter(Box::new(cc)).unwrap();
        let expected = Array2::from_shape_fn((15, 10), |(i, _j)| (100.0 - 10.0 * i as f64).max(0.0));

        let recorder = AssertionF64Recorder::new("reservoir-cc", p_idx.into(), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        // Test all solvers
        run_all_solvers(&model, &[], &[], &[]);
    }

    #[test]
    /// Test the variable API
    fn test_variable_api() {
        let mut model = simple_model(1, None);

        let variable = ActivationFunction::Unit { min: 0.0, max: 10.0 };
        let input_max_flow = parameters::ConstantParameter::new("my-constant".into(), 10.0);

        assert!(input_max_flow.can_be_f64_variable());

        let input_max_flow_idx = model
            .network_mut()
            .add_const_parameter(Box::new(input_max_flow))
            .unwrap();

        // assign the new parameter to one of the nodes.
        let node = model.network_mut().get_mut_node_by_name("input", None).unwrap();
        node.set_max_flow_constraint(Some(input_max_flow_idx.into())).unwrap();

        let mut state = model.setup::<ClpSolver>(&ClpSolverSettings::default()).unwrap();

        // Initially the variable value should be unset
        let variable_values = model
            .network_mut()
            .get_f64_parameter_variable_values(input_max_flow_idx, state.network_state())
            .unwrap();
        assert_eq!(variable_values, vec![None]);

        // Update the variable values
        model
            .network_mut()
            .set_f64_parameter_variable_values(input_max_flow_idx, &[5.0], &variable, state.network_state_mut())
            .unwrap();

        // After update the variable value should match what was set
        let variable_values = model
            .network_mut()
            .get_f64_parameter_variable_values(input_max_flow_idx, state.network_state())
            .unwrap();

        assert_eq!(variable_values, vec![Some(vec![5.0])]);
    }
}
