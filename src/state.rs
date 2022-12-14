use crate::edge::{Edge, EdgeIndex};
use crate::node::NodeIndex;
use crate::parameters::{IndexParameterIndex, ParameterIndex};
use crate::timestep::Timestep;
use crate::PywrError;
use pyo3::prelude::*;
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

    pub(crate) fn new_storage_state(initial_volume: f64, max_volume: f64) -> Self {
        Self::Storage(StorageState::new(initial_volume, max_volume))
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

#[derive(Clone, Copy, Debug)]
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

#[derive(Clone, Copy, Debug)]
pub struct StorageState {
    pub volume: f64,
    pub max_volume: f64,
    pub flows: FlowState,
}

impl StorageState {
    fn new(initial_volume: f64, max_volume: f64) -> Self {
        Self {
            volume: initial_volume,
            max_volume,
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

    fn proportional_volume(&self) -> f64 {
        // TODO handle divide by zero (is it full or empty?)
        self.volume / self.max_volume
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct EdgeState {
    flow: f64,
}

impl EdgeState {
    fn add_flow(&mut self, flow: f64) {
        self.flow += flow;
    }
}

// State of the parameters
#[derive(Clone, Debug, Default)]
pub struct ParameterState {
    values: Vec<f64>,
    indices: Vec<usize>,
}

impl ParameterState {
    pub(crate) fn with_capacity(num_values: usize, num_indices: usize) -> Self {
        Self {
            values: Vec::with_capacity(num_values),
            indices: Vec::with_capacity(num_indices),
        }
    }

    pub(crate) fn push_value(&mut self, value: f64) {
        self.values.push(value)
    }

    pub(crate) fn push_index(&mut self, index: usize) {
        self.indices.push(index)
    }

    // TODO this argument could be a reference?
    pub(crate) fn get_value(&self, parameter_index: ParameterIndex) -> Result<f64, PywrError> {
        match self.values.get(*parameter_index.deref()) {
            Some(v) => Ok(*v),
            None => Err(PywrError::ParameterIndexNotFound(parameter_index)),
        }
    }

    // TODO this argument could be a reference?
    pub(crate) fn get_index(&self, parameter_index: IndexParameterIndex) -> Result<usize, PywrError> {
        match self.indices.get(*parameter_index.deref()) {
            Some(i) => Ok(*i),
            None => Err(PywrError::IndexParameterIndexNotFound(parameter_index)),
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
    pub(crate) fn new() -> Self {
        Self {
            node_states: Vec::new(),
            edge_states: Vec::new(),
        }
    }

    pub(crate) fn with_capacity(&self) -> Self {
        let mut node_states = self.node_states.clone();
        for node_state in node_states.iter_mut() {
            node_state.reset();
        }

        let mut edge_states = Vec::with_capacity(self.edge_states.len());
        for _ in 0..self.edge_states.len() {
            edge_states.push(EdgeState::default())
        }

        Self {
            node_states,
            edge_states,
        }
    }

    pub(crate) fn push_node_state(&mut self, node_state: NodeState) {
        self.node_states.push(node_state);
    }

    pub(crate) fn push_edge_state(&mut self, edge_state: EdgeState) {
        self.edge_states.push(edge_state);
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

    pub fn get_node_max_volume(&self, node_index: &NodeIndex) -> Result<f64, PywrError> {
        match self.node_states.get(*node_index.deref()) {
            Some(s) => match s {
                NodeState::Storage(ss) => Ok(ss.max_volume),
                NodeState::Flow(_) => Err(PywrError::MetricNotDefinedForNode),
            },
            None => Err(PywrError::MetricNotDefinedForNode),
        }
    }

    pub fn get_node_proportional_volume(&self, node_index: &NodeIndex) -> Result<f64, PywrError> {
        match self.node_states.get(*node_index.deref()) {
            Some(s) => match s {
                NodeState::Storage(ss) => Ok(ss.proportional_volume()),
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
}
