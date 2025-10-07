use crate::derived_metric::DerivedMetricIndex;
use crate::edge::{Edge, EdgeIndex};
use crate::metric::SimpleMetricF64Error;
use crate::models::MultiNetworkTransferIndex;
use crate::network::Network;
use crate::node::{Node, NodeIndex};
use crate::parameters::{
    ConstParameterIndex, GeneralParameterIndex, ParameterCollection, ParameterCollectionSize, SimpleParameterIndex,
};
use crate::timestep::Timestep;
use crate::virtual_storage::VirtualStorageIndex;
use num::Zero;
#[cfg(feature = "pyo3")]
use pyo3::{
    Bound, FromPyObject, PyAny, PyResult,
    exceptions::PyValueError,
    prelude::PyAnyMethods,
    types::{PyDict, PyFloat, PyInt},
};
use std::collections::{HashMap, VecDeque};
use std::fmt::Display;
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

    fn clamp_volume(&mut self, min_volume: f64, max_volume: f64) {
        if let Self::Storage(s) = self {
            s.clamp(min_volume, max_volume);
        }
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
        if max_volume.is_zero() {
            // If max volume is zero then we will consider the storage to befull. This matches V1 behaviour.
            return 1.0;
        }
        self.volume / max_volume
    }

    /// Ensure the volume is within the min and max volume range (inclusive). If the volume
    /// is more than 1E6 outside the min or max volume then this function will panic,
    /// reporting a mass-balance message.
    fn clamp(&mut self, min_volume: f64, max_volume: f64) {
        if (self.volume - min_volume) < -1e-6 {
            panic!(
                "Mass-balance error detected. Volume ({}) is smaller than minimum volume ({}).",
                self.volume, min_volume
            );
        }
        if (self.volume - max_volume) > 1e-6 {
            panic!(
                "Mass-balance error detected. Volume ({}) is greater than maximum volume ({}).",
                self.volume, max_volume,
            );
        }
        self.volume = self.volume.clamp(min_volume, max_volume);
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
            history: history_size.map(|size| VirtualStorageHistory::new(size, initial_volume / size.get() as f64)),
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

    fn clamp_volume(&mut self, min_volume: f64, max_volume: f64) {
        self.storage.clamp(min_volume, max_volume);
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
    indices: HashMap<String, u64>,
}

#[cfg(feature = "pyo3")]
impl FromPyObject<'_> for MultiValue {
    fn extract_bound(ob: &Bound<'_, PyAny>) -> PyResult<Self> {
        let dict = ob.downcast::<PyDict>()?;

        // Try to convert the floats
        let mut values: HashMap<String, f64> = HashMap::default();
        let mut indices: HashMap<String, u64> = HashMap::default();

        for (k, v) in dict {
            if let Ok(float_value) = v.downcast::<PyFloat>() {
                values.insert(k.to_string(), float_value.extract::<f64>()?);
            } else if let Ok(int_value) = v.downcast::<PyInt>() {
                // If it's an integer, we will treat it as an index
                indices.insert(k.to_string(), int_value.extract::<u64>()?);
            } else {
                return Err(PyValueError::new_err(
                    "Some returned values were not interpreted as floats or integers.",
                ));
            }
        }

        Ok(MultiValue::new(values, indices))
    }
}

impl MultiValue {
    pub fn new(values: HashMap<String, f64>, indices: HashMap<String, u64>) -> Self {
        Self { values, indices }
    }

    pub fn get_value(&self, key: &str) -> Option<&f64> {
        self.values.get(key)
    }

    pub fn get_index(&self, key: &str) -> Option<&u64> {
        self.indices.get(key)
    }

    /// Check if any of the values in the MultiValue are NaN
    pub fn has_nan(&self) -> bool {
        self.values.values().any(|&v| v.is_nan())
    }
}

// State of the parameters
#[derive(Debug, Clone)]
struct ParameterValues {
    values: Vec<f64>,
    indices: Vec<u64>,
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

    fn get_value(&self, idx: usize) -> Option<&f64> {
        self.values.get(idx)
    }

    fn get_value_mut(&mut self, idx: usize) -> Option<&mut f64> {
        self.values.get_mut(idx)
    }

    fn get_index(&self, idx: usize) -> Option<&u64> {
        self.indices.get(idx)
    }

    fn get_index_mut(&mut self, idx: usize) -> Option<&mut u64> {
        self.indices.get_mut(idx)
    }

    fn get_multi_value(&self, idx: usize) -> Option<&MultiValue> {
        self.multi_values.get(idx)
    }

    fn get_multi_value_mut(&mut self, idx: usize) -> Option<&mut MultiValue> {
        self.multi_values.get_mut(idx)
    }
}

#[derive(Debug, Clone)]
pub struct ParameterValuesCollection {
    constant: ParameterValues,
    simple: ParameterValues,
    general: ParameterValues,
}

impl ParameterValuesCollection {
    fn get_simple_parameter_values(&self) -> SimpleParameterValues<'_> {
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

    fn get_const_parameter_values(&self) -> ConstParameterValues<'_> {
        ConstParameterValues {
            constant: ParameterValuesRef {
                values: &self.constant.values,
                indices: &self.constant.indices,
                multi_values: &self.constant.multi_values,
            },
        }
    }
}

#[derive(Default)]
pub struct ParameterValuesRef<'a> {
    values: &'a [f64],
    indices: &'a [u64],
    multi_values: &'a [MultiValue],
}

impl ParameterValuesRef<'_> {
    fn get_value(&self, idx: usize) -> Option<&f64> {
        self.values.get(idx)
    }

    fn get_index(&self, idx: usize) -> Option<&u64> {
        self.indices.get(idx)
    }

    fn get_multi_value(&self, idx: usize, key: &str) -> Option<&f64> {
        self.multi_values.get(idx).and_then(|s| s.get_value(key))
    }

    fn get_multi_index(&self, idx: usize, key: &str) -> Option<&u64> {
        self.multi_values.get(idx).and_then(|s| s.get_index(key))
    }
}

pub struct SimpleParameterValues<'a> {
    constant: ConstParameterValues<'a>,
    simple: ParameterValuesRef<'a>,
}

impl SimpleParameterValues<'_> {
    pub fn get_simple_parameter_f64(&self, idx: SimpleParameterIndex<f64>) -> Option<&f64> {
        self.simple.get_value(*idx.deref())
    }

    pub fn get_simple_parameter_u64(&self, idx: SimpleParameterIndex<u64>) -> Option<&u64> {
        self.simple.get_index(*idx.deref())
    }

    pub fn get_simple_multi_parameter_f64(&self, idx: SimpleParameterIndex<MultiValue>, key: &str) -> Option<&f64> {
        self.simple.get_multi_value(*idx.deref(), key)
    }

    pub fn get_simple_multi_parameter_u64(&self, idx: SimpleParameterIndex<MultiValue>, key: &str) -> Option<&u64> {
        self.simple.get_multi_index(*idx.deref(), key)
    }

    pub fn get_constant_values(&self) -> &ConstParameterValues<'_> {
        &self.constant
    }
}

#[derive(Default)]
pub struct ConstParameterValues<'a> {
    constant: ParameterValuesRef<'a>,
}

impl ConstParameterValues<'_> {
    pub fn get_const_parameter_f64(&self, idx: ConstParameterIndex<f64>) -> Option<&f64> {
        self.constant.get_value(*idx.deref())
    }

    pub fn get_const_parameter_u64(&self, idx: ConstParameterIndex<u64>) -> Option<&u64> {
        self.constant.get_index(*idx.deref())
    }

    pub fn get_const_multi_parameter_f64(&self, idx: ConstParameterIndex<MultiValue>, key: &str) -> Option<&f64> {
        self.constant.get_multi_value(*idx.deref(), key)
    }

    pub fn get_const_multi_parameter_u64(&self, idx: ConstParameterIndex<MultiValue>, key: &str) -> Option<&u64> {
        self.constant.get_multi_index(*idx.deref(), key)
    }
}

#[derive(Debug, Error)]
pub enum NetworkStateError {
    #[error("Node index not found: {0}")]
    NodeIndexNotFound(NodeIndex),
    #[error("Edge index not found: {0}")]
    EdgeIndexNotFound(EdgeIndex),
    #[error("Virtual storage index not found: {0}")]
    VirtualStorageIndexNotFound(VirtualStorageIndex),
    #[error("Node has no volume: {0}")]
    NodeHasNoVolume(NodeIndex),
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

    pub fn add_flow(&mut self, edge: &Edge, timestep: &Timestep, flow: f64) -> Result<(), NetworkStateError> {
        let from_node_index = edge.from_node_index();
        match self.node_states.get_mut(*from_node_index) {
            Some(s) => s.add_out_flow(flow, timestep),
            None => return Err(NetworkStateError::NodeIndexNotFound(from_node_index)),
        };

        let to_node_index = edge.to_node_index();
        match self.node_states.get_mut(*to_node_index) {
            Some(s) => s.add_in_flow(flow, timestep),
            None => return Err(NetworkStateError::NodeIndexNotFound(to_node_index)),
        };

        let edge_index = edge.index();
        match self.edge_states.get_mut(*edge_index) {
            Some(s) => s.add_flow(flow),
            None => return Err(NetworkStateError::EdgeIndexNotFound(edge_index)),
        };

        Ok(())
    }

    /// Complete a timestep after all the flow has been added.
    ///
    /// This final step ensures that derived states (e.g. virtual storage volume) are updated
    /// once all the flows have been updated.
    fn update_derived_states(&mut self, model: &Network, timestep: &Timestep) -> Result<(), NetworkStateError> {
        // Update virtual storage node states
        for (state, node) in self
            .virtual_storage_states
            .iter_mut()
            .zip(model.virtual_storage_nodes().iter())
        {
            // Only update if the node is active
            if node.is_active(timestep) {
                let flow = node
                    .iter_nodes_with_factors()
                    .map(|(idx, factor)| match self.node_states.get(*idx.deref()) {
                        None => Err(NetworkStateError::NodeIndexNotFound(*idx)),
                        Some(s) => {
                            let node = model
                                .nodes()
                                .get(idx)
                                .ok_or(NetworkStateError::NodeIndexNotFound(*idx))?;
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

    /// Clamp the volume of `node_index` to be within the bounds provided.
    fn clamp_node_volume(
        &mut self,
        node_index: &NodeIndex,
        min_volume: f64,
        max_volume: f64,
    ) -> Result<(), NetworkStateError> {
        match self.node_states.get_mut(*node_index.deref()) {
            Some(s) => {
                s.clamp_volume(min_volume, max_volume);
                Ok(())
            }
            None => Err(NetworkStateError::NodeIndexNotFound(*node_index)),
        }
    }

    /// Clamp the volume of `node_index` to be within the bounds provided.
    fn clamp_virtual_storage_node_volume(
        &mut self,
        node_index: &VirtualStorageIndex,
        min_volume: f64,
        max_volume: f64,
    ) -> Result<(), NetworkStateError> {
        match self.virtual_storage_states.get_mut(*node_index.deref()) {
            Some(s) => {
                s.clamp_volume(min_volume, max_volume);
                Ok(())
            }
            None => Err(NetworkStateError::VirtualStorageIndexNotFound(*node_index)),
        }
    }
    pub fn get_node_in_flow(&self, node_index: &NodeIndex) -> Result<f64, NetworkStateError> {
        match self.node_states.get(*node_index.deref()) {
            Some(s) => Ok(s.get_in_flow()),
            None => Err(NetworkStateError::NodeIndexNotFound(*node_index)),
        }
    }

    pub fn get_node_out_flow(&self, node_index: &NodeIndex) -> Result<f64, NetworkStateError> {
        match self.node_states.get(*node_index.deref()) {
            Some(s) => Ok(s.get_out_flow()),
            None => Err(NetworkStateError::NodeIndexNotFound(*node_index)),
        }
    }

    pub fn get_node_volume(&self, node_index: &NodeIndex) -> Result<f64, NetworkStateError> {
        match self.node_states.get(*node_index.deref()) {
            Some(s) => match s {
                NodeState::Storage(ss) => Ok(ss.volume),
                NodeState::Flow(_) => Err(NetworkStateError::NodeHasNoVolume(*node_index)),
            },
            None => Err(NetworkStateError::NodeIndexNotFound(*node_index)),
        }
    }

    pub fn get_node_proportional_volume(
        &self,
        node_index: &NodeIndex,
        max_volume: f64,
    ) -> Result<f64, NetworkStateError> {
        match self.node_states.get(*node_index.deref()) {
            Some(s) => match s {
                NodeState::Storage(ss) => Ok(ss.proportional_volume(max_volume)),
                NodeState::Flow(_) => Err(NetworkStateError::NodeHasNoVolume(*node_index)),
            },
            None => Err(NetworkStateError::NodeIndexNotFound(*node_index)),
        }
    }

    pub fn get_edge_flow(&self, edge_index: &EdgeIndex) -> Result<f64, NetworkStateError> {
        match self.edge_states.get(*edge_index.deref()) {
            Some(s) => Ok(s.flow),
            None => Err(NetworkStateError::EdgeIndexNotFound(*edge_index)),
        }
    }

    pub fn set_volume(&mut self, node_index: &NodeIndex, volume: f64) -> Result<(), NetworkStateError> {
        match self.node_states.get_mut(*node_index.deref()) {
            Some(s) => match s {
                NodeState::Flow(_) => Err(NetworkStateError::NodeHasNoVolume(*node_index)),
                NodeState::Storage(s) => {
                    s.volume = volume;
                    Ok(())
                }
            },
            None => Err(NetworkStateError::NodeIndexNotFound(*node_index)),
        }
    }

    pub fn reset_virtual_storage_volume(
        &mut self,
        idx: &VirtualStorageIndex,
        volume: f64,
        timestep: &Timestep,
    ) -> Result<(), NetworkStateError> {
        match self.virtual_storage_states.get_mut(*idx.deref()) {
            Some(s) => {
                s.reset_volume(volume, timestep);
                Ok(())
            }
            None => Err(NetworkStateError::VirtualStorageIndexNotFound(*idx)),
        }
    }

    pub fn reset_virtual_storage_history(
        &mut self,
        idx: &VirtualStorageIndex,
        initial_volume: f64,
    ) -> Result<(), NetworkStateError> {
        match self.virtual_storage_states.get_mut(*idx.deref()) {
            Some(s) => {
                s.reset_history(initial_volume);
                Ok(())
            }
            None => Err(NetworkStateError::VirtualStorageIndexNotFound(*idx)),
        }
    }

    pub fn recover_virtual_storage_last_historical_flow(
        &mut self,
        idx: &VirtualStorageIndex,
        timestep: &Timestep,
    ) -> Result<(), NetworkStateError> {
        match self.virtual_storage_states.get_mut(*idx.deref()) {
            Some(s) => {
                s.recover_last_historical_flow(timestep);
                Ok(())
            }
            None => Err(NetworkStateError::VirtualStorageIndexNotFound(*idx)),
        }
    }

    pub fn get_virtual_storage_volume(&self, idx: &VirtualStorageIndex) -> Result<f64, NetworkStateError> {
        match self.virtual_storage_states.get(*idx.deref()) {
            Some(s) => Ok(s.storage.volume),
            None => Err(NetworkStateError::VirtualStorageIndexNotFound(*idx)),
        }
    }

    pub fn get_virtual_storage_proportional_volume(
        &self,
        idx: &VirtualStorageIndex,
        max_volume: f64,
    ) -> Result<f64, NetworkStateError> {
        match self.virtual_storage_states.get(*idx.deref()) {
            Some(s) => Ok(s.proportional_volume(max_volume)),
            None => Err(NetworkStateError::VirtualStorageIndexNotFound(*idx)),
        }
    }

    pub fn get_virtual_storage_last_reset(
        &self,
        idx: &VirtualStorageIndex,
    ) -> Result<&Option<Timestep>, NetworkStateError> {
        match self.virtual_storage_states.get(*idx.deref()) {
            Some(s) => Ok(&s.last_reset),
            None => Err(NetworkStateError::VirtualStorageIndexNotFound(*idx)),
        }
    }
}

#[derive(Error, Debug)]
pub enum StateError {
    #[error("General parameter index not found: {0}")]
    GeneralParameterIndexNotFound(GeneralParameterIndex<f64>),
    #[error("General index parameter index not found: {0}")]
    GeneralIndexParameterIndexNotFound(GeneralParameterIndex<u64>),
    #[error("General parameter index not found: {0}")]
    GeneralMultiValueParameterIndexNotFound(GeneralParameterIndex<MultiValue>),
    #[error("General parameter with index {index} has no key: {key}")]
    GeneralMultiValueParameterKeyNotFound {
        index: GeneralParameterIndex<MultiValue>,
        key: String,
    },
    #[error("Simple parameter index not found: {0}")]
    SimpleParameterIndexNotFound(SimpleParameterIndex<f64>),
    #[error("Simple index parameter index not found: {0}")]
    SimpleIndexParameterIndexNotFound(SimpleParameterIndex<u64>),
    #[error("Simple parameter index not found: {0}")]
    SimpleMultiValueParameterIndexNotFound(SimpleParameterIndex<MultiValue>),
    #[error("Simple parameter with index {index} has no key: {key}")]
    SimpleMultiValueParameterKeyNotFound {
        index: SimpleParameterIndex<MultiValue>,
        key: String,
    },
    #[error("Constant parameter index not found: {0}")]
    ConstParameterIndexNotFound(ConstParameterIndex<f64>),
    #[error("Constant index parameter index not found: {0}")]
    ConstIndexParameterIndexNotFound(ConstParameterIndex<u64>),
    #[error("Constant parameter index not found: {0}")]
    ConstMultiValueParameterIndexNotFound(ConstParameterIndex<MultiValue>),
    #[error("Constant parameter with index {index} has no key: {key}")]
    ConstMultiValueParameterKeyNotFound {
        index: ConstParameterIndex<MultiValue>,
        key: String,
    },
    #[error("Derived metric index not found: {0}")]
    DerivedMetricIndexNotFound(DerivedMetricIndex),
    #[error("Multi-network transfer index not found: {0}")]
    MultiNetworkTransferIndexNotFound(MultiNetworkTransferIndex),
    #[error("Network state error: {0}")]
    NetworkStateError(#[from] NetworkStateError),
    #[error("Simple metric f64 error: {0}")]
    SimpleMetricF64Error(#[from] SimpleMetricF64Error),
}

#[derive(Error, Debug)]
pub enum SetStateError<I: Display> {
    #[error("Unable to set state; index not found: {0}")]
    IndexNotFound(I),
    #[error("Unable to set state; NaN encountered: {0}")]
    NaNValue(I),
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

    pub fn get_parameter_value(&self, idx: GeneralParameterIndex<f64>) -> Result<f64, StateError> {
        self.parameters
            .general
            .get_value(*idx)
            .ok_or(StateError::GeneralParameterIndexNotFound(idx))
            .copied()
    }

    pub fn set_parameter_value(
        &mut self,
        idx: GeneralParameterIndex<f64>,
        value: f64,
    ) -> Result<(), SetStateError<GeneralParameterIndex<f64>>> {
        let v = self
            .parameters
            .general
            .get_value_mut(*idx)
            .ok_or(SetStateError::IndexNotFound(idx))?;

        if value.is_nan() {
            return Err(SetStateError::NaNValue(idx));
        }

        *v = value;

        Ok(())
    }

    pub fn set_simple_parameter_value(
        &mut self,
        idx: SimpleParameterIndex<f64>,
        value: f64,
    ) -> Result<(), SetStateError<SimpleParameterIndex<f64>>> {
        let v = self
            .parameters
            .simple
            .get_value_mut(*idx)
            .ok_or(SetStateError::IndexNotFound(idx))?;

        if value.is_nan() {
            return Err(SetStateError::NaNValue(idx));
        }

        *v = value;

        Ok(())
    }

    pub fn set_const_parameter_value(
        &mut self,
        idx: ConstParameterIndex<f64>,
        value: f64,
    ) -> Result<(), SetStateError<ConstParameterIndex<f64>>> {
        let v = self
            .parameters
            .constant
            .get_value_mut(*idx)
            .ok_or(SetStateError::IndexNotFound(idx))?;

        *v = value;

        Ok(())
    }

    pub fn get_parameter_index(&self, idx: GeneralParameterIndex<u64>) -> Result<u64, StateError> {
        self.parameters
            .general
            .get_index(*idx)
            .ok_or(StateError::GeneralIndexParameterIndexNotFound(idx))
            .copied()
    }

    pub fn set_parameter_index(
        &mut self,
        idx: GeneralParameterIndex<u64>,
        value: u64,
    ) -> Result<(), SetStateError<GeneralParameterIndex<u64>>> {
        let v = self
            .parameters
            .general
            .get_index_mut(*idx)
            .ok_or(SetStateError::IndexNotFound(idx))?;

        *v = value;

        Ok(())
    }

    pub fn set_simple_parameter_index(
        &mut self,
        idx: SimpleParameterIndex<u64>,
        value: u64,
    ) -> Result<(), SetStateError<SimpleParameterIndex<u64>>> {
        let v = self
            .parameters
            .simple
            .get_index_mut(*idx)
            .ok_or(SetStateError::IndexNotFound(idx))?;

        *v = value;

        Ok(())
    }

    pub fn set_const_parameter_index(
        &mut self,
        idx: ConstParameterIndex<u64>,
        value: u64,
    ) -> Result<(), SetStateError<ConstParameterIndex<u64>>> {
        let v = self
            .parameters
            .constant
            .get_index_mut(*idx)
            .ok_or(SetStateError::IndexNotFound(idx))?;

        *v = value;

        Ok(())
    }
    pub fn get_multi_parameter_value(
        &self,
        idx: GeneralParameterIndex<MultiValue>,
        key: &str,
    ) -> Result<f64, StateError> {
        let mv = self
            .parameters
            .general
            .get_multi_value(*idx)
            .ok_or(StateError::GeneralMultiValueParameterIndexNotFound(idx))?;

        mv.get_value(key)
            .ok_or_else(|| StateError::GeneralMultiValueParameterKeyNotFound {
                index: idx,
                key: key.to_string(),
            })
            .copied()
    }

    pub fn get_multi_parameter_index(
        &self,
        idx: GeneralParameterIndex<MultiValue>,
        key: &str,
    ) -> Result<u64, StateError> {
        let mv = self
            .parameters
            .general
            .get_multi_value(*idx)
            .ok_or(StateError::GeneralMultiValueParameterIndexNotFound(idx))?;

        mv.get_index(key)
            .ok_or_else(|| StateError::GeneralMultiValueParameterKeyNotFound {
                index: idx,
                key: key.to_string(),
            })
            .copied()
    }

    pub fn set_multi_parameter_value(
        &mut self,
        idx: GeneralParameterIndex<MultiValue>,
        value: MultiValue,
    ) -> Result<(), SetStateError<GeneralParameterIndex<MultiValue>>> {
        let mv = self
            .parameters
            .general
            .get_multi_value_mut(*idx)
            .ok_or(SetStateError::IndexNotFound(idx))?;

        if value.has_nan() {
            return Err(SetStateError::NaNValue(idx));
        }

        *mv = value;

        Ok(())
    }

    pub fn set_simple_multi_parameter_value(
        &mut self,
        idx: SimpleParameterIndex<MultiValue>,
        value: MultiValue,
    ) -> Result<(), SetStateError<SimpleParameterIndex<MultiValue>>> {
        let mv = self
            .parameters
            .simple
            .get_multi_value_mut(*idx)
            .ok_or(SetStateError::IndexNotFound(idx))?;

        if value.has_nan() {
            return Err(SetStateError::NaNValue(idx));
        }

        *mv = value;

        Ok(())
    }

    pub fn set_const_multi_parameter_value(
        &mut self,
        idx: ConstParameterIndex<MultiValue>,
        value: MultiValue,
    ) -> Result<(), SetStateError<ConstParameterIndex<MultiValue>>> {
        let mv = self
            .parameters
            .constant
            .get_multi_value_mut(*idx)
            .ok_or(SetStateError::IndexNotFound(idx))?;

        if value.has_nan() {
            return Err(SetStateError::NaNValue(idx));
        }

        *mv = value;

        Ok(())
    }

    pub fn get_simple_parameter_values(&self) -> SimpleParameterValues<'_> {
        self.parameters.get_simple_parameter_values()
    }

    pub fn get_const_parameter_values(&self) -> ConstParameterValues<'_> {
        self.parameters.get_const_parameter_values()
    }

    pub fn set_node_volume(&mut self, idx: &NodeIndex, volume: f64) -> Result<(), StateError> {
        Ok(self.network.set_volume(idx, volume)?)
    }

    pub fn reset_virtual_storage_node_volume(
        &mut self,
        idx: &VirtualStorageIndex,
        volume: f64,
        timestep: &Timestep,
    ) -> Result<(), StateError> {
        Ok(self.network.reset_virtual_storage_volume(idx, volume, timestep)?)
    }

    pub fn reset_virtual_storage_history(
        &mut self,
        idx: &VirtualStorageIndex,
        initial_volume: f64,
    ) -> Result<(), StateError> {
        Ok(self.network.reset_virtual_storage_history(idx, initial_volume)?)
    }

    pub fn recover_virtual_storage_last_historical_flow(
        &mut self,
        idx: &VirtualStorageIndex,
        timestep: &Timestep,
    ) -> Result<(), StateError> {
        Ok(self
            .network
            .recover_virtual_storage_last_historical_flow(idx, timestep)?)
    }

    pub fn get_derived_metric_value(&self, idx: DerivedMetricIndex) -> Result<f64, StateError> {
        match self.derived_metrics.get(*idx.deref()) {
            Some(s) => Ok(*s),
            None => Err(StateError::DerivedMetricIndexNotFound(idx)),
        }
    }

    pub fn set_derived_metric_value(
        &mut self,
        idx: DerivedMetricIndex,
        value: f64,
    ) -> Result<(), SetStateError<DerivedMetricIndex>> {
        match self.derived_metrics.get_mut(*idx.deref()) {
            Some(s) => {
                *s = value;
                Ok(())
            }
            None => Err(SetStateError::IndexNotFound(idx)),
        }
    }

    pub fn get_inter_network_transfer_value(&self, idx: MultiNetworkTransferIndex) -> Result<f64, StateError> {
        match self.inter_network_values.get(*idx.deref()) {
            Some(s) => Ok(*s),
            None => Err(StateError::MultiNetworkTransferIndexNotFound(idx)),
        }
    }

    pub fn set_inter_network_transfer_value(
        &mut self,
        idx: MultiNetworkTransferIndex,
        value: f64,
    ) -> Result<(), StateError> {
        match self.inter_network_values.get_mut(*idx.deref()) {
            Some(s) => {
                *s = value;
                Ok(())
            }
            None => Err(StateError::MultiNetworkTransferIndexNotFound(idx)),
        }
    }

    /// Complete a timestep after all the flow has been added.
    ///
    /// This final step ensures, once all the flows have been updated, that:
    ///   - Derived states (e.g. virtual storage volume) are updated
    ///   - Volumes are within bounds
    pub fn complete(&mut self, model: &Network, timestep: &Timestep) -> Result<(), StateError> {
        for node in model.nodes().iter() {
            if let Node::Storage(storage) = node {
                let node_index = node.index();
                let min_volume = storage.get_min_volume(self)?;
                let max_volume = storage.get_max_volume(self)?;
                self.network.clamp_node_volume(&node_index, min_volume, max_volume)?;
            }
        }

        for node in model.virtual_storage_nodes().iter() {
            let node_index = node.index();
            let min_volume = node.get_min_volume(self)?;
            let max_volume = node.get_max_volume(self)?;
            self.network
                .clamp_virtual_storage_node_volume(&node_index, min_volume, max_volume)?;
        }

        self.network.update_derived_states(model, timestep)?;

        Ok(())
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
