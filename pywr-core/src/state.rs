use crate::derived_metric::DerivedMetricIndex;
use crate::edge::{Edge, EdgeIndex};
use crate::models::MultiNetworkTransferIndex;
use crate::network::Network;
use crate::node::{Node, NodeIndex};
use crate::parameters::{
    ConstParameterIndex, GeneralParameterIndex, ParameterCollection, ParameterCollectionSize, SimpleParameterIndex,
};
use crate::timestep::Timestep;
use crate::virtual_storage::VirtualStorageIndex;
use crate::PywrError;
use std::collections::{HashMap, VecDeque};
use std::num::NonZeroUsize;
use std::ops::Deref;
use thiserror::Error;

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
#[derive(Clone, Debug)]
struct VirtualStorageHistory {
    /// The flows are stored in a queue. The oldest flow is popped from the front of the queue
    flows: VecDeque<f64>,
    /// The maximum size of the history.
    size: NonZeroUsize,
}

impl VirtualStorageHistory {
    fn new(size: NonZeroUsize, initial_flow: f64) -> Self {
        Self {
            flows: vec![initial_flow; size.get()].into(),
            size,
        }
    }

    /// Reset the history to the initial flow.
    fn reset(&mut self, initial_flow: f64) {
        self.flows = vec![initial_flow; self.size.get()].into();
    }

    /// Add new flow to the history.
    fn add_flow(&mut self, flow: f64) {
        self.flows.push_back(flow);
    }

    /// Pop the oldest flow from the history as long as the history is at least as long as the
    /// maximum size. If the history is shorter than the maximum size then return zero.
    fn pop_flow(&mut self) -> f64 {
        if self.flows.len() >= self.size.get() {
            self.flows
                .pop_front()
                .expect("Size is non-zero therefore pop_front should succeed.")
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
    pub fn new(initial_volume: f64, history_size: Option<NonZeroUsize>) -> Self {
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

#[derive(Debug, Default, Clone, PartialEq)]
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

#[derive(Error, Debug)]
pub enum ParameterValuesError {
    #[error("index not found: {0}")]
    IndexNotFound(usize),
    #[error("key not found: {0}")]
    KeyNotFound(String),
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

    fn get_value(&self, idx: usize) -> Result<f64, ParameterValuesError> {
        self.values
            .get(idx)
            .ok_or(ParameterValuesError::IndexNotFound(idx))
            .copied()
    }

    fn set_value(&mut self, idx: usize, value: f64) -> Result<(), ParameterValuesError> {
        match self.values.get_mut(idx) {
            Some(s) => {
                *s = value;
                Ok(())
            }
            None => Err(ParameterValuesError::IndexNotFound(idx)),
        }
    }

    fn get_index(&self, idx: usize) -> Result<usize, ParameterValuesError> {
        self.indices
            .get(idx)
            .ok_or(ParameterValuesError::IndexNotFound(idx))
            .copied()
    }

    fn set_index(&mut self, idx: usize, value: usize) -> Result<(), ParameterValuesError> {
        match self.indices.get_mut(idx) {
            Some(s) => {
                *s = value;
                Ok(())
            }
            None => Err(ParameterValuesError::IndexNotFound(idx)),
        }
    }

    fn get_multi_value(&self, idx: usize, key: &str) -> Result<f64, ParameterValuesError> {
        let value = self
            .multi_values
            .get(idx)
            .ok_or(ParameterValuesError::IndexNotFound(idx))?;

        value
            .get_value(key)
            .ok_or(ParameterValuesError::KeyNotFound(key.to_string()))
            .copied()
    }

    fn set_multi_value(&mut self, idx: usize, value: MultiValue) -> Result<(), ParameterValuesError> {
        match self.multi_values.get_mut(idx) {
            Some(s) => {
                *s = value;
                Ok(())
            }
            None => Err(ParameterValuesError::IndexNotFound(idx)),
        }
    }

    fn get_multi_index(&self, idx: usize, key: &str) -> Result<usize, ParameterValuesError> {
        let value = self
            .multi_values
            .get(idx)
            .ok_or(ParameterValuesError::IndexNotFound(idx))?;

        value
            .get_index(key)
            .ok_or(ParameterValuesError::KeyNotFound(key.to_string()))
            .copied()
    }
}

#[derive(Debug, Clone)]
pub struct ParameterValuesCollection {
    constant: ParameterValues,
    simple: ParameterValues,
    general: ParameterValues,
}

impl ParameterValuesCollection {
    fn get_simple_parameter_values(&self) -> SimpleParameterValues {
        SimpleParameterValues {
            constant: ConstParameterValues {
                constant: ParameterValuesRef {
                    values: &self.constant.values,
                    indices: &self.constant.indices,
                    multi_values: &self.constant.multi_values,
                },
            },
            simple: ParameterValuesRef {
                values: &self.simple.values,
                indices: &self.simple.indices,
                multi_values: &self.simple.multi_values,
            },
        }
    }

    fn get_const_parameter_values(&self) -> ConstParameterValues {
        ConstParameterValues {
            constant: ParameterValuesRef {
                values: &self.constant.values,
                indices: &self.constant.indices,
                multi_values: &self.constant.multi_values,
            },
        }
    }
}

pub struct ParameterValuesRef<'a> {
    values: &'a [f64],
    indices: &'a [usize],
    multi_values: &'a [MultiValue],
}

impl<'a> ParameterValuesRef<'a> {
    fn get_value(&self, idx: usize) -> Option<&f64> {
        self.values.get(idx)
    }

    fn get_index(&self, idx: usize) -> Option<&usize> {
        self.indices.get(idx)
    }

    fn get_multi_value(&self, idx: usize, key: &str) -> Option<&f64> {
        self.multi_values.get(idx).and_then(|s| s.get_value(key))
    }
}

pub struct SimpleParameterValues<'a> {
    constant: ConstParameterValues<'a>,
    simple: ParameterValuesRef<'a>,
}

impl<'a> SimpleParameterValues<'a> {
    pub fn get_simple_parameter_f64(&self, idx: SimpleParameterIndex<f64>) -> Result<f64, PywrError> {
        self.simple
            .get_value(*idx.deref())
            .ok_or(PywrError::SimpleParameterIndexNotFound(idx))
            .copied()
    }

    pub fn get_simple_parameter_usize(&self, idx: SimpleParameterIndex<usize>) -> Result<usize, PywrError> {
        self.simple
            .get_index(*idx.deref())
            .ok_or(PywrError::SimpleIndexParameterIndexNotFound(idx))
            .copied()
    }

    pub fn get_simple_multi_parameter_f64(
        &self,
        idx: SimpleParameterIndex<MultiValue>,
        key: &str,
    ) -> Result<f64, PywrError> {
        self.simple
            .get_multi_value(*idx.deref(), key)
            .ok_or(PywrError::SimpleMultiValueParameterIndexNotFound(idx))
            .copied()
    }

    pub fn get_constant_values(&self) -> &ConstParameterValues {
        &self.constant
    }
}

pub struct ConstParameterValues<'a> {
    constant: ParameterValuesRef<'a>,
}

impl<'a> ConstParameterValues<'a> {
    pub fn get_const_parameter_f64(&self, idx: ConstParameterIndex<f64>) -> Result<f64, PywrError> {
        self.constant
            .get_value(*idx.deref())
            .ok_or(PywrError::ConstParameterIndexNotFound(idx))
            .copied()
    }

    pub fn get_const_parameter_usize(&self, idx: ConstParameterIndex<usize>) -> Result<usize, PywrError> {
        self.constant
            .get_index(*idx.deref())
            .ok_or(PywrError::ConstIndexParameterIndexNotFound(idx))
            .copied()
    }

    pub fn get_const_multi_parameter_f64(
        &self,
        idx: ConstParameterIndex<MultiValue>,
        key: &str,
    ) -> Result<f64, PywrError> {
        self.constant
            .get_multi_value(*idx.deref(), key)
            .ok_or(PywrError::ConstMultiValueParameterIndexNotFound(idx))
            .copied()
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
        match self.virtual_storage_states.get_mut(*idx.deref()) {
            Some(s) => {
                s.reset_volume(volume, timestep);
                Ok(())
            }
            None => Err(PywrError::VirtualStorageIndexNotFound(idx)),
        }
    }

    pub fn reset_virtual_storage_history(
        &mut self,
        idx: VirtualStorageIndex,
        initial_volume: f64,
    ) -> Result<(), PywrError> {
        match self.virtual_storage_states.get_mut(*idx.deref()) {
            Some(s) => {
                s.reset_history(initial_volume);
                Ok(())
            }
            None => Err(PywrError::VirtualStorageIndexNotFound(idx)),
        }
    }

    pub fn recover_virtual_storage_last_historical_flow(
        &mut self,
        idx: VirtualStorageIndex,
        timestep: &Timestep,
    ) -> Result<(), PywrError> {
        match self.virtual_storage_states.get_mut(*idx.deref()) {
            Some(s) => {
                s.recover_last_historical_flow(timestep);
                Ok(())
            }
            None => Err(PywrError::VirtualStorageIndexNotFound(idx)),
        }
    }

    pub fn get_virtual_storage_volume(&self, idx: &VirtualStorageIndex) -> Result<f64, PywrError> {
        match self.virtual_storage_states.get(*idx.deref()) {
            Some(s) => Ok(s.storage.volume),
            None => Err(PywrError::VirtualStorageIndexNotFound(*idx)),
        }
    }

    pub fn get_virtual_storage_proportional_volume(
        &self,
        idx: VirtualStorageIndex,
        max_volume: f64,
    ) -> Result<f64, PywrError> {
        match self.virtual_storage_states.get(*idx.deref()) {
            Some(s) => Ok(s.proportional_volume(max_volume)),
            None => Err(PywrError::VirtualStorageIndexNotFound(idx)),
        }
    }

    pub fn get_virtual_storage_last_reset(&self, idx: VirtualStorageIndex) -> Result<&Option<Timestep>, PywrError> {
        match self.virtual_storage_states.get(*idx.deref()) {
            Some(s) => Ok(&s.last_reset),
            None => Err(PywrError::VirtualStorageIndexNotFound(idx)),
        }
    }
}

/// State of the model simulation.
///
/// This struct contains the state of the model simulation at a given point in time. The state
/// contains the current state of the network, the values of the parameters, the values of the
/// derived metrics, and the values of the inter-network transfers.
///
/// This struct can be constructed using the [`StateBuilder`] and then updated using the various
/// methods to set the values of the parameters, derived metrics, and inter-network transfers.
///
#[derive(Debug, Clone)]
pub struct State {
    network: NetworkState,
    parameters: ParameterValuesCollection,
    derived_metrics: Vec<f64>,
    inter_network_values: Vec<f64>,
}

impl State {
    pub fn get_network_state(&self) -> &NetworkState {
        &self.network
    }

    pub fn get_mut_network_state(&mut self) -> &mut NetworkState {
        &mut self.network
    }

    pub fn get_parameter_value(&self, idx: GeneralParameterIndex<f64>) -> Result<f64, PywrError> {
        self.parameters.general.get_value(*idx).map_err(|e| match e {
            ParameterValuesError::IndexNotFound(_) => PywrError::GeneralParameterIndexNotFound(idx),
            ParameterValuesError::KeyNotFound(key) => PywrError::MultiValueParameterKeyNotFound(key),
        })
    }

    pub fn set_parameter_value(&mut self, idx: GeneralParameterIndex<f64>, value: f64) -> Result<(), PywrError> {
        self.parameters.general.set_value(*idx, value).map_err(|e| match e {
            ParameterValuesError::IndexNotFound(_) => PywrError::GeneralParameterIndexNotFound(idx),
            ParameterValuesError::KeyNotFound(key) => PywrError::MultiValueParameterKeyNotFound(key),
        })
    }

    pub fn set_simple_parameter_value(&mut self, idx: SimpleParameterIndex<f64>, value: f64) -> Result<(), PywrError> {
        self.parameters.simple.set_value(*idx, value).map_err(|e| match e {
            ParameterValuesError::IndexNotFound(_) => PywrError::SimpleParameterIndexNotFound(idx),
            ParameterValuesError::KeyNotFound(key) => PywrError::MultiValueParameterKeyNotFound(key),
        })
    }

    pub fn set_const_parameter_value(&mut self, idx: ConstParameterIndex<f64>, value: f64) -> Result<(), PywrError> {
        self.parameters.constant.set_value(*idx, value).map_err(|e| match e {
            ParameterValuesError::IndexNotFound(_) => PywrError::ConstParameterIndexNotFound(idx),
            ParameterValuesError::KeyNotFound(key) => PywrError::MultiValueParameterKeyNotFound(key),
        })
    }

    pub fn get_parameter_index(&self, idx: GeneralParameterIndex<usize>) -> Result<usize, PywrError> {
        self.parameters.general.get_index(*idx).map_err(|e| match e {
            ParameterValuesError::IndexNotFound(_) => PywrError::GeneralIndexParameterIndexNotFound(idx),
            ParameterValuesError::KeyNotFound(key) => PywrError::MultiValueParameterKeyNotFound(key),
        })
    }

    pub fn set_parameter_index(&mut self, idx: GeneralParameterIndex<usize>, value: usize) -> Result<(), PywrError> {
        self.parameters.general.set_index(*idx, value).map_err(|e| match e {
            ParameterValuesError::IndexNotFound(_) => PywrError::GeneralIndexParameterIndexNotFound(idx),
            ParameterValuesError::KeyNotFound(key) => PywrError::MultiValueParameterKeyNotFound(key),
        })
    }

    pub fn set_simple_parameter_index(
        &mut self,
        idx: SimpleParameterIndex<usize>,
        value: usize,
    ) -> Result<(), PywrError> {
        self.parameters.simple.set_index(*idx, value).map_err(|e| match e {
            ParameterValuesError::IndexNotFound(_) => PywrError::SimpleIndexParameterIndexNotFound(idx),
            ParameterValuesError::KeyNotFound(key) => PywrError::MultiValueParameterKeyNotFound(key),
        })
    }

    pub fn set_const_parameter_index(
        &mut self,
        idx: ConstParameterIndex<usize>,
        value: usize,
    ) -> Result<(), PywrError> {
        self.parameters.constant.set_index(*idx, value).map_err(|e| match e {
            ParameterValuesError::IndexNotFound(_) => PywrError::ConstIndexParameterIndexNotFound(idx),
            ParameterValuesError::KeyNotFound(key) => PywrError::MultiValueParameterKeyNotFound(key),
        })
    }
    pub fn get_multi_parameter_value(
        &self,
        idx: GeneralParameterIndex<MultiValue>,
        key: &str,
    ) -> Result<f64, PywrError> {
        self.parameters.general.get_multi_value(*idx, key).map_err(|e| match e {
            ParameterValuesError::IndexNotFound(_) => PywrError::GeneralMultiValueParameterIndexNotFound(idx),
            ParameterValuesError::KeyNotFound(key) => PywrError::MultiValueParameterKeyNotFound(key),
        })
    }

    pub fn set_multi_parameter_value(
        &mut self,
        idx: GeneralParameterIndex<MultiValue>,
        value: MultiValue,
    ) -> Result<(), PywrError> {
        self.parameters
            .general
            .set_multi_value(*idx, value)
            .map_err(|e| match e {
                ParameterValuesError::IndexNotFound(_) => PywrError::GeneralMultiValueParameterIndexNotFound(idx),
                ParameterValuesError::KeyNotFound(key) => PywrError::MultiValueParameterKeyNotFound(key),
            })
    }

    pub fn set_simple_multi_parameter_value(
        &mut self,
        idx: SimpleParameterIndex<MultiValue>,
        value: MultiValue,
    ) -> Result<(), PywrError> {
        self.parameters
            .simple
            .set_multi_value(*idx, value)
            .map_err(|e| match e {
                ParameterValuesError::IndexNotFound(_) => PywrError::SimpleMultiValueParameterIndexNotFound(idx),
                ParameterValuesError::KeyNotFound(key) => PywrError::MultiValueParameterKeyNotFound(key),
            })
    }

    pub fn set_const_multi_parameter_value(
        &mut self,
        idx: ConstParameterIndex<MultiValue>,
        value: MultiValue,
    ) -> Result<(), PywrError> {
        self.parameters
            .constant
            .set_multi_value(*idx, value)
            .map_err(|e| match e {
                ParameterValuesError::IndexNotFound(_) => PywrError::ConstMultiValueParameterIndexNotFound(idx),
                ParameterValuesError::KeyNotFound(key) => PywrError::MultiValueParameterKeyNotFound(key),
            })
    }

    pub fn get_multi_parameter_index(
        &self,
        idx: GeneralParameterIndex<MultiValue>,
        key: &str,
    ) -> Result<usize, PywrError> {
        self.parameters.general.get_multi_index(*idx, key).map_err(|e| match e {
            ParameterValuesError::IndexNotFound(_) => PywrError::GeneralMultiValueParameterIndexNotFound(idx),
            ParameterValuesError::KeyNotFound(key) => PywrError::MultiValueParameterKeyNotFound(key),
        })
    }

    pub fn get_simple_parameter_values(&self) -> SimpleParameterValues {
        self.parameters.get_simple_parameter_values()
    }

    pub fn get_const_parameter_values(&self) -> ConstParameterValues {
        self.parameters.get_const_parameter_values()
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

/// Builder for the [`State`] struct.
///
/// This builder is used to create a new state with the desired initial values. The builder
/// allows for the creation of a state with a specific number of nodes and edges, and optionally
/// with initial virtual storage, parameter, derived metric, and inter-network transfer states.
pub struct StateBuilder {
    initial_node_states: Vec<NodeState>,
    num_edges: usize,
    initial_virtual_storage_states: Option<Vec<VirtualStorageState>>,
    num_parameters: Option<ParameterCollectionSize>,
    num_derived_metrics: Option<usize>,
    num_inter_network_values: Option<usize>,
}

impl StateBuilder {
    /// Create a new state builder with the desired initial node states and number of edges.
    ///
    /// # Arguments
    ///
    /// * `initial_node_states` - The initial states for the nodes in the network.
    /// * `num_edges` - The number of edges in the network.
    pub fn new(initial_node_states: Vec<NodeState>, num_edges: usize) -> Self {
        Self {
            initial_node_states,
            num_edges,
            initial_virtual_storage_states: None,
            num_parameters: None,
            num_derived_metrics: None,
            num_inter_network_values: None,
        }
    }

    /// Add initial virtual storage states to the builder.
    pub fn with_virtual_storage_states(mut self, initial_virtual_storage_states: Vec<VirtualStorageState>) -> Self {
        self.initial_virtual_storage_states = Some(initial_virtual_storage_states);
        self
    }

    /// Add the number of value parameters to the builder.
    pub fn with_parameters(mut self, collection: &ParameterCollection) -> Self {
        self.num_parameters = Some(collection.size());
        self
    }

    /// Add the number of derived metrics to the builder.
    pub fn with_derived_metrics(mut self, num_derived_metrics: usize) -> Self {
        self.num_derived_metrics = Some(num_derived_metrics);
        self
    }

    /// Add the number of inter-network transfer values to the builder.
    pub fn with_inter_network_transfers(mut self, num_inter_network_values: usize) -> Self {
        self.num_inter_network_values = Some(num_inter_network_values);
        self
    }

    /// Build the [`State`] from the builder.
    pub fn build(self) -> State {
        let constant = ParameterValues::new(
            self.num_parameters.map(|s| s.const_f64).unwrap_or(0),
            self.num_parameters.map(|s| s.const_usize).unwrap_or(0),
            self.num_parameters.map(|s| s.const_multi).unwrap_or(0),
        );

        let simple = ParameterValues::new(
            self.num_parameters.map(|s| s.simple_f64).unwrap_or(0),
            self.num_parameters.map(|s| s.simple_usize).unwrap_or(0),
            self.num_parameters.map(|s| s.simple_multi).unwrap_or(0),
        );
        let general = ParameterValues::new(
            self.num_parameters.map(|s| s.general_f64).unwrap_or(0),
            self.num_parameters.map(|s| s.general_usize).unwrap_or(0),
            self.num_parameters.map(|s| s.general_multi).unwrap_or(0),
        );

        let parameters = ParameterValuesCollection {
            constant,
            simple,
            general,
        };

        State {
            network: NetworkState::new(
                self.initial_node_states,
                self.num_edges,
                self.initial_virtual_storage_states.unwrap_or_default(),
            ),
            parameters,
            derived_metrics: vec![0.0; self.num_derived_metrics.unwrap_or(0)],
            inter_network_values: vec![0.0; self.num_inter_network_values.unwrap_or(0)],
        }
    }
}
