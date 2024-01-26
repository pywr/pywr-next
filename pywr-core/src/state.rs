use crate::derived_metric::DerivedMetricIndex;
use crate::edge::{Edge, EdgeIndex};
use crate::models::MultiNetworkTransferIndex;
use crate::network::Network;
use crate::node::{Node, NodeIndex};
use crate::parameters::{IndexParameterIndex, MultiValueParameterIndex, ParameterIndex};
use crate::timestep::Timestep;
use crate::virtual_storage::VirtualStorageIndex;
use crate::PywrError;
use dyn_clone::DynClone;
use std::any::Any;
use std::collections::{HashMap, VecDeque};
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
    pub fn new(initial_volume: f64) -> Self {
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

/// Stores the history of virtual storage flows.
#[derive(Clone, Debug, Default)]
struct VirtualStorageHistory {
    /// The flows are stored in a queue. The oldest flow is popped from the front of the queue
    flows: VecDeque<f64>,
    /// The maximum size of the history.
    size: usize,
}

impl VirtualStorageHistory {
    fn new(size: usize, initial_flow: f64) -> Self {
        Self {
            flows: vec![initial_flow; size].into(),
            size,
        }
    }

    /// Reset the history to the initial flow.
    fn reset(&mut self, initial_flow: f64) {
        self.flows = vec![initial_flow; self.size].into();
    }

    /// Add new flow to the history.
    fn add_flow(&mut self, flow: f64) {
        self.flows.push_back(flow);
    }

    /// Pop the oldest flow from the history as long as the history is at least as long as the
    /// maximum size. If the history is shorter than the maximum size then return zero.
    fn pop_flow(&mut self) -> f64 {
        if self.flows.len() >= self.size {
            self.flows.pop_front().unwrap()
        } else {
            0.0
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct VirtualStorageState {
    last_reset: Option<Timestep>,
    storage: StorageState,
    history: Option<VirtualStorageHistory>,
}

impl VirtualStorageState {
    pub fn new(initial_volume: f64, history_size: Option<usize>) -> Self {
        Self {
            last_reset: None,
            storage: StorageState::new(initial_volume),
            history: history_size.map(|size| VirtualStorageHistory::new(size, initial_volume)),
        }
    }

    fn reset(&mut self) {
        self.storage.reset();
        // Volume remains unchanged
    }

    /// Reset the volume to a new value storing the `timestep`
    fn reset_volume(&mut self, volume: f64, timestep: &Timestep) {
        self.storage.volume = volume;
        self.last_reset = Some(*timestep);
    }

    fn reset_history(&mut self, initial_flow: f64) {
        if let Some(history) = self.history.as_mut() {
            history.reset(initial_flow);
        }
    }

    fn recover_last_historical_flow(&mut self, timestep: &Timestep) {
        if let Some(history) = self.history.as_mut() {
            self.storage.add_in_flow(history.pop_flow(), timestep);
        }
    }

    fn add_out_flow(&mut self, flow: f64, timestep: &Timestep) {
        self.storage.add_out_flow(flow, timestep);
        if let Some(history) = self.history.as_mut() {
            history.add_flow(flow);
        }
    }

    fn proportional_volume(&self, max_volume: f64) -> f64 {
        self.storage.proportional_volume(max_volume)
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

pub trait ParameterState: Any + Send + DynClone {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T> ParameterState for T
where
    T: Any + Send + Clone,
{
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
// impl ParameterState for f64 {}

dyn_clone::clone_trait_object!(ParameterState);

#[derive(Clone)]
pub struct ParameterStates {
    values: Vec<Option<Box<dyn ParameterState>>>,
    indices: Vec<Option<Box<dyn ParameterState>>>,
    multi: Vec<Option<Box<dyn ParameterState>>>,
}

impl ParameterStates {
    /// Create new default states for the desired number of parameters.
    pub fn new(
        initial_values_states: Vec<Option<Box<dyn ParameterState>>>,
        initial_indices_states: Vec<Option<Box<dyn ParameterState>>>,
        initial_multi_states: Vec<Option<Box<dyn ParameterState>>>,
    ) -> Self {
        Self {
            values: initial_values_states,
            indices: initial_indices_states,
            multi: initial_multi_states,
        }
    }

    pub fn get_mut_value_state(&mut self, index: ParameterIndex) -> Option<&mut Option<Box<dyn ParameterState>>> {
        self.values.get_mut(*index.deref())
    }

    pub fn get_mut_index_state(&mut self, index: IndexParameterIndex) -> Option<&mut Option<Box<dyn ParameterState>>> {
        self.indices.get_mut(*index.deref())
    }

    pub fn get_mut_multi_state(
        &mut self,
        index: MultiValueParameterIndex,
    ) -> Option<&mut Option<Box<dyn ParameterState>>> {
        self.multi.get_mut(*index.deref())
    }
}

#[derive(Debug, Default, Clone)]
pub struct MultiValue {
    values: HashMap<String, f64>,
    indices: HashMap<String, usize>,
}

impl MultiValue {
    pub fn new(values: HashMap<String, f64>, indices: HashMap<String, usize>) -> Self {
        Self { values, indices }
    }

    pub fn get_value(&self, key: &str) -> Option<&f64> {
        self.values.get(key)
    }

    pub fn get_index(&self, key: &str) -> Option<&usize> {
        self.indices.get(key)
    }
}

// State of the parameters
#[derive(Debug, Clone)]
struct ParameterValues {
    values: Vec<f64>,
    indices: Vec<usize>,
    multi_values: Vec<MultiValue>,
}

impl ParameterValues {
    fn new(num_values: usize, num_indices: usize, num_multi_values: usize) -> Self {
        Self {
            values: vec![0.0; num_values],
            indices: vec![0; num_indices],
            multi_values: vec![MultiValue::default(); num_multi_values],
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

    fn get_multi_value(&self, idx: MultiValueParameterIndex, key: &str) -> Result<f64, PywrError> {
        match self.multi_values.get(*idx.deref()) {
            Some(s) => match s.get_value(key) {
                Some(v) => Ok(*v),
                None => Err(PywrError::MultiValueParameterKeyNotFound(key.to_string())),
            },
            None => Err(PywrError::MultiValueParameterIndexNotFound(idx)),
        }
    }

    fn set_multi_value(&mut self, idx: MultiValueParameterIndex, value: MultiValue) -> Result<(), PywrError> {
        match self.multi_values.get_mut(*idx.deref()) {
            Some(s) => {
                *s = value;
                Ok(())
            }
            None => Err(PywrError::MultiValueParameterIndexNotFound(idx)),
        }
    }

    fn get_multi_index(&self, idx: MultiValueParameterIndex, key: &str) -> Result<usize, PywrError> {
        match self.multi_values.get(*idx.deref()) {
            Some(s) => match s.get_index(key) {
                Some(v) => Ok(*v),
                None => Err(PywrError::MultiValueParameterKeyNotFound(key.to_string())),
            },
            None => Err(PywrError::MultiValueParameterIndexNotFound(idx)),
        }
    }
}

// State of the nodes and edges
#[derive(Clone, Debug)]
pub struct NetworkState {
    node_states: Vec<NodeState>,
    edge_states: Vec<EdgeState>,
    virtual_storage_states: Vec<VirtualStorageState>,
}

impl NetworkState {
    pub fn new(
        initial_node_states: Vec<NodeState>,
        num_edges: usize,
        initial_virtual_storage_states: Vec<VirtualStorageState>,
    ) -> Self {
        Self {
            node_states: initial_node_states,
            edge_states: (0..num_edges).map(|_| EdgeState::default()).collect(),
            virtual_storage_states: initial_virtual_storage_states,
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

        for vs in self.virtual_storage_states.iter_mut() {
            vs.reset()
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

    /// Complete a timestep after all the flow has been added.
    ///
    /// This final step ensures that derived states (e.g. virtual storage volume) are updated
    /// once all of the flows have been updated.
    pub fn complete(&mut self, model: &Network, timestep: &Timestep) -> Result<(), PywrError> {
        // Update virtual storage node states
        for (state, node) in self
            .virtual_storage_states
            .iter_mut()
            .zip(model.virtual_storage_nodes().iter())
        {
            if let Some(node_factors) = node.get_nodes_with_factors() {
                let flow = node_factors
                    .iter()
                    .map(|(idx, factor)| match self.node_states.get(*idx.deref()) {
                        None => Err(PywrError::NodeIndexNotFound),
                        Some(s) => {
                            let node = model.nodes().get(idx)?;
                            match node {
                                Node::Input(_) => Ok(factor * s.get_out_flow()),
                                Node::Output(_) => Ok(factor * s.get_in_flow()),
                                Node::Link(_) => Ok(factor * s.get_in_flow()),
                                Node::Storage(_) => panic!("Storage node not supported on virtual storage."),
                            }
                        }
                    })
                    .sum::<Result<f64, _>>()?;

                state.add_out_flow(flow, timestep);
            }
        }

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

    pub fn reset_virtual_storage_volume(
        &mut self,
        idx: VirtualStorageIndex,
        volume: f64,
        timestep: &Timestep,
    ) -> Result<(), PywrError> {
        // TODO handle these errors properly
        if let Some(s) = self.virtual_storage_states.get_mut(*idx.deref()) {
            s.reset_volume(volume, timestep)
        } else {
            panic!("Virtual storage node state not found.")
        }

        Ok(())
    }

    pub fn reset_virtual_storage_history(
        &mut self,
        idx: VirtualStorageIndex,
        initial_volume: f64,
    ) -> Result<(), PywrError> {
        // TODO handle these errors properly
        if let Some(s) = self.virtual_storage_states.get_mut(*idx.deref()) {
            s.reset_history(initial_volume)
        } else {
            panic!("Virtual storage node state not found.")
        }

        Ok(())
    }

    pub fn recover_virtual_storage_last_historical_flow(
        &mut self,
        idx: VirtualStorageIndex,
        timestep: &Timestep,
    ) -> Result<(), PywrError> {
        // TODO handle these errors properly
        if let Some(s) = self.virtual_storage_states.get_mut(*idx.deref()) {
            s.recover_last_historical_flow(timestep)
        } else {
            panic!("Virtual storage node state not found.")
        }

        Ok(())
    }

    pub fn get_virtual_storage_volume(&self, node_index: &VirtualStorageIndex) -> Result<f64, PywrError> {
        match self.virtual_storage_states.get(*node_index.deref()) {
            Some(s) => Ok(s.storage.volume),
            None => Err(PywrError::NodeIndexNotFound), // TODO should be a specific VS error
        }
    }

    pub fn get_virtual_storage_proportional_volume(
        &self,
        node_index: &VirtualStorageIndex,
        max_volume: f64,
    ) -> Result<f64, PywrError> {
        match self.virtual_storage_states.get(*node_index.deref()) {
            Some(s) => Ok(s.proportional_volume(max_volume)),
            None => Err(PywrError::NodeIndexNotFound), // TODO should be a specific VS error
        }
    }

    pub fn get_virtual_storage_last_reset(
        &self,
        node_index: &VirtualStorageIndex,
    ) -> Result<&Option<Timestep>, PywrError> {
        match self.virtual_storage_states.get(*node_index.deref()) {
            Some(s) => Ok(&s.last_reset),
            None => Err(PywrError::NodeIndexNotFound), // TODO should be a specific VS error
        }
    }
}

/// State of the model simulation
#[derive(Debug, Clone)]
pub struct State {
    network: NetworkState,
    parameters: ParameterValues,
    derived_metrics: Vec<f64>,
    inter_network_values: Vec<f64>,
}

impl State {
    pub fn new(
        initial_node_states: Vec<NodeState>,
        num_edges: usize,
        initial_virtual_storage_states: Vec<VirtualStorageState>,
        num_parameter_values: usize,
        num_parameter_indices: usize,
        num_multi_parameters: usize,
        num_derived_metrics: usize,
        num_inter_network_values: usize,
    ) -> Self {
        Self {
            network: NetworkState::new(initial_node_states, num_edges, initial_virtual_storage_states),
            parameters: ParameterValues::new(num_parameter_values, num_parameter_indices, num_multi_parameters),
            derived_metrics: vec![0.0; num_derived_metrics],
            inter_network_values: vec![0.0; num_inter_network_values],
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

    pub fn get_multi_parameter_value(&self, idx: MultiValueParameterIndex, key: &str) -> Result<f64, PywrError> {
        self.parameters.get_multi_value(idx, key)
    }

    pub fn set_multi_parameter_value(
        &mut self,
        idx: MultiValueParameterIndex,
        value: MultiValue,
    ) -> Result<(), PywrError> {
        self.parameters.set_multi_value(idx, value)
    }

    pub fn get_multi_parameter_index(&self, idx: MultiValueParameterIndex, key: &str) -> Result<usize, PywrError> {
        self.parameters.get_multi_index(idx, key)
    }

    pub fn set_node_volume(&mut self, idx: NodeIndex, volume: f64) -> Result<(), PywrError> {
        self.network.set_volume(idx, volume)
    }

    pub fn reset_virtual_storage_node_volume(
        &mut self,
        idx: VirtualStorageIndex,
        volume: f64,
        timestep: &Timestep,
    ) -> Result<(), PywrError> {
        self.network.reset_virtual_storage_volume(idx, volume, timestep)
    }

    pub fn reset_virtual_storage_history(
        &mut self,
        idx: VirtualStorageIndex,
        initial_volume: f64,
    ) -> Result<(), PywrError> {
        self.network.reset_virtual_storage_history(idx, initial_volume)
    }

    pub fn recover_virtual_storage_last_historical_flow(
        &mut self,
        idx: VirtualStorageIndex,
        timestep: &Timestep,
    ) -> Result<(), PywrError> {
        self.network.recover_virtual_storage_last_historical_flow(idx, timestep)
    }

    pub fn get_derived_metric_value(&self, idx: DerivedMetricIndex) -> Result<f64, PywrError> {
        match self.derived_metrics.get(*idx.deref()) {
            Some(s) => Ok(*s),
            None => Err(PywrError::DerivedMetricIndexNotFound(idx)),
        }
    }

    pub fn set_derived_metric_value(&mut self, idx: DerivedMetricIndex, value: f64) -> Result<(), PywrError> {
        match self.derived_metrics.get_mut(*idx.deref()) {
            Some(s) => {
                *s = value;
                Ok(())
            }
            None => Err(PywrError::DerivedMetricIndexNotFound(idx)),
        }
    }

    pub fn get_inter_network_transfer_value(&self, idx: MultiNetworkTransferIndex) -> Result<f64, PywrError> {
        match self.inter_network_values.get(*idx.deref()) {
            Some(s) => Ok(*s),
            None => Err(PywrError::MultiNetworkTransferIndexNotFound(idx)),
        }
    }

    pub fn set_inter_network_transfer_value(
        &mut self,
        idx: MultiNetworkTransferIndex,
        value: f64,
    ) -> Result<(), PywrError> {
        match self.inter_network_values.get_mut(*idx.deref()) {
            Some(s) => {
                *s = value;
                Ok(())
            }
            None => Err(PywrError::MultiNetworkTransferIndexNotFound(idx)),
        }
    }
}
