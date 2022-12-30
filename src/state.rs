use crate::edge::{Edge, EdgeIndex};
use crate::node::NodeIndex;
use crate::parameters::{IndexParameterIndex, ParameterIndex};
use crate::timestep::Timestep;
use crate::PywrError;
use pyo3::prelude::*;
use std::any::Any;
use std::ops::Deref;

#[derive(Clone, Copy, Debug)]
pub enum NodeState {
    Flow(FlowState),
    Storage(StorageState),
}

impl NodeState {
    pub(crate) fn new_flow_state() -> Self {
        Self::Flow(FlowState::new())
    }

    pub(crate) fn new_storage_state(initial_volume: f64) -> Self {
        Self::Storage(StorageState::new(initial_volume))
    }

    fn reset(&mut self) {
        match self {
            Self::Flow(s) => s.reset(),
            Self::Storage(s) => s.reset(),
        }
    }

    fn add_in_flow(&mut self, flow: f64, timestep: &Timestep) {
        match self {
            Self::Flow(s) => s.add_in_flow(flow),
            Self::Storage(s) => s.add_in_flow(flow, timestep),
        };
    }

    pub fn get_in_flow(&self) -> f64 {
        match self {
            Self::Flow(s) => s.in_flow,
            Self::Storage(s) => s.flows.in_flow,
        }
    }

    pub fn get_out_flow(&self) -> f64 {
        match self {
            Self::Flow(s) => s.out_flow,
            Self::Storage(s) => s.flows.out_flow,
        }
    }

    fn add_out_flow(&mut self, flow: f64, timestep: &Timestep) {
        match self {
            Self::Flow(s) => s.add_out_flow(flow),
            Self::Storage(s) => s.add_out_flow(flow, timestep),
        };
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FlowState {
    pub in_flow: f64,
    pub out_flow: f64,
}

impl FlowState {
    fn new() -> Self {
        Self {
            in_flow: 0.0,
            out_flow: 0.0,
        }
    }

    fn reset(&mut self) {
        self.in_flow = 0.0;
        self.out_flow = 0.0;
    }

    fn add_in_flow(&mut self, flow: f64) {
        self.in_flow += flow;
    }
    fn add_out_flow(&mut self, flow: f64) {
        self.out_flow += flow;
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StorageState {
    pub volume: f64,
    pub flows: FlowState,
}

impl StorageState {
    fn new(initial_volume: f64) -> Self {
        Self {
            volume: initial_volume,
            flows: FlowState::new(),
        }
    }

    fn reset(&mut self) {
        self.flows.reset();
        // Volume remains unchanged
    }

    fn add_in_flow(&mut self, flow: f64, timestep: &Timestep) {
        self.flows.add_in_flow(flow);
        self.volume += flow * timestep.days();
    }
    fn add_out_flow(&mut self, flow: f64, timestep: &Timestep) {
        self.flows.add_out_flow(flow);
        self.volume -= flow * timestep.days();
    }

    fn proportional_volume(&self, max_volume: f64) -> f64 {
        // TODO handle divide by zero (is it full or empty?)
        self.volume / max_volume
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct EdgeState {
    flow: f64,
}

impl EdgeState {
    fn reset(&mut self) {
        self.flow = 0.0;
    }
    fn add_flow(&mut self, flow: f64) {
        self.flow += flow;
    }
}

#[derive(Debug)]
pub struct ParameterStates {
    values: Vec<Option<Box<dyn Any + Send>>>,
    indices: Vec<Option<Box<dyn Any + Send>>>,
}

impl ParameterStates {
    /// Create new default states for the desired number of parameters.
    pub fn new(
        initial_values_states: Vec<Option<Box<dyn Any + Send>>>,
        initial_indices_states: Vec<Option<Box<dyn Any + Send>>>,
    ) -> Self {
        Self {
            values: initial_values_states,
            indices: initial_indices_states,
        }
    }

    pub fn get_mut_value_state(&mut self, index: ParameterIndex) -> Option<&mut Option<Box<dyn Any + Send>>> {
        self.values.get_mut(*index.deref())
    }

    pub fn get_mut_index_state(&mut self, index: IndexParameterIndex) -> Option<&mut Option<Box<dyn Any + Send>>> {
        self.indices.get_mut(*index.deref())
    }
}

// State of the parameters
#[derive(Debug)]
struct ParameterValues {
    values: Vec<f64>,
    indices: Vec<usize>,
}

impl ParameterValues {
    fn new(num_values: usize, num_indices: usize) -> Self {
        Self {
            values: vec![0.0; num_values],
            indices: vec![0; num_indices],
        }
    }

    fn get_value(&self, idx: ParameterIndex) -> Result<f64, PywrError> {
        match self.values.get(*idx.deref()) {
            Some(s) => Ok(*s),
            None => Err(PywrError::ParameterIndexNotFound(idx)),
        }
    }

    fn set_value(&mut self, idx: ParameterIndex, value: f64) -> Result<(), PywrError> {
        match self.values.get_mut(*idx.deref()) {
            Some(s) => {
                *s = value;
                Ok(())
            }
            None => Err(PywrError::ParameterIndexNotFound(idx)),
        }
    }

    fn get_index(&self, idx: IndexParameterIndex) -> Result<usize, PywrError> {
        match self.indices.get(*idx.deref()) {
            Some(s) => Ok(*s),
            None => Err(PywrError::IndexParameterIndexNotFound(idx)),
        }
    }

    fn set_index(&mut self, idx: IndexParameterIndex, value: usize) -> Result<(), PywrError> {
        match self.indices.get_mut(*idx.deref()) {
            Some(s) => {
                *s = value;
                Ok(())
            }
            None => Err(PywrError::IndexParameterIndexNotFound(idx)),
        }
    }
}

// State of the nodes and edges
#[pyclass]
#[derive(Clone, Debug)]
pub struct NetworkState {
    node_states: Vec<NodeState>,
    edge_states: Vec<EdgeState>,
}

impl NetworkState {
    pub fn new(initial_node_states: Vec<NodeState>, num_edges: usize) -> Self {
        Self {
            node_states: initial_node_states,
            edge_states: (0..num_edges).map(|_| EdgeState::default()).collect(),
        }
    }

    /// Reset the current flow information
    ///
    /// This method should be called between each time-step to set all the flow states to zero.
    /// Non-flow state (i.e. volume) is retained. After this flow can be added back to the state
    /// using the `.add_flow` method.
    pub fn reset(&mut self) {
        for ns in self.node_states.iter_mut() {
            ns.reset()
        }

        for es in self.edge_states.iter_mut() {
            es.reset()
        }
    }

    pub(crate) fn add_flow(&mut self, edge: &Edge, timestep: &Timestep, flow: f64) -> Result<(), PywrError> {
        match self.node_states.get_mut(*edge.from_node_index()) {
            Some(s) => s.add_out_flow(flow, timestep),
            None => return Err(PywrError::NodeIndexNotFound),
        };

        match self.node_states.get_mut(*edge.to_node_index()) {
            Some(s) => s.add_in_flow(flow, timestep),
            None => return Err(PywrError::NodeIndexNotFound),
        };

        match self.edge_states.get_mut(*edge.index()) {
            Some(s) => s.add_flow(flow),
            None => return Err(PywrError::EdgeIndexNotFound),
        };

        Ok(())
    }

    pub fn get_node_in_flow(&self, node_index: &NodeIndex) -> Result<f64, PywrError> {
        match self.node_states.get(*node_index.deref()) {
            Some(s) => Ok(s.get_in_flow()),
            None => Err(PywrError::NodeIndexNotFound),
        }
    }

    pub fn get_node_out_flow(&self, node_index: &NodeIndex) -> Result<f64, PywrError> {
        match self.node_states.get(*node_index.deref()) {
            Some(s) => Ok(s.get_out_flow()),
            None => Err(PywrError::NodeIndexNotFound),
        }
    }

    pub fn get_node_volume(&self, node_index: &NodeIndex) -> Result<f64, PywrError> {
        match self.node_states.get(*node_index.deref()) {
            Some(s) => match s {
                NodeState::Storage(ss) => Ok(ss.volume),
                NodeState::Flow(_) => Err(PywrError::MetricNotDefinedForNode),
            },
            None => Err(PywrError::MetricNotDefinedForNode),
        }
    }

    pub fn get_node_proportional_volume(&self, node_index: &NodeIndex, max_volume: f64) -> Result<f64, PywrError> {
        match self.node_states.get(*node_index.deref()) {
            Some(s) => match s {
                NodeState::Storage(ss) => Ok(ss.proportional_volume(max_volume)),
                NodeState::Flow(_) => Err(PywrError::MetricNotDefinedForNode),
            },
            None => Err(PywrError::MetricNotDefinedForNode),
        }
    }

    pub fn get_edge_flow(&self, edge_index: &EdgeIndex) -> Result<f64, PywrError> {
        match self.edge_states.get(*edge_index.deref()) {
            Some(s) => Ok(s.flow),
            None => Err(PywrError::EdgeIndexNotFound),
        }
    }

    pub fn set_volume(&mut self, idx: NodeIndex, volume: f64) -> Result<(), PywrError> {
        // TODO handle these errors properly
        if let Some(node_state) = self.node_states.get_mut(*idx.deref()) {
            match node_state {
                NodeState::Flow(_) => panic!("Cannot set volume for a non-storage state :("),
                NodeState::Storage(s) => s.volume = volume,
            }
        } else {
            panic!("Node state not found.")
        }

        Ok(())
    }
}

/// State of the model simulation
pub struct State {
    network: NetworkState,
    parameters: ParameterValues,
}

impl State {
    pub fn new(
        initial_node_states: Vec<NodeState>,
        num_edges: usize,
        num_parameter_values: usize,
        num_parameter_indices: usize,
    ) -> Self {
        Self {
            network: NetworkState::new(initial_node_states, num_edges),
            parameters: ParameterValues::new(num_parameter_values, num_parameter_indices),
        }
    }

    pub fn get_network_state(&self) -> &NetworkState {
        &self.network
    }

    pub fn get_mut_network_state(&mut self) -> &mut NetworkState {
        &mut self.network
    }

    pub fn get_parameter_value(&self, idx: ParameterIndex) -> Result<f64, PywrError> {
        self.parameters.get_value(idx)
    }

    pub fn set_parameter_value(&mut self, idx: ParameterIndex, value: f64) -> Result<(), PywrError> {
        self.parameters.set_value(idx, value)
    }

    pub fn get_parameter_index(&self, idx: IndexParameterIndex) -> Result<usize, PywrError> {
        self.parameters.get_index(idx)
    }

    pub fn set_parameter_index(&mut self, idx: IndexParameterIndex, value: usize) -> Result<(), PywrError> {
        self.parameters.set_index(idx, value)
    }

    pub fn set_node_volume(&mut self, idx: NodeIndex, volume: f64) -> Result<(), PywrError> {
        self.network.set_volume(idx, volume)
    }
}
