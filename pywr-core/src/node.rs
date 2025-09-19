use crate::edge::EdgeIndex;
use crate::metric::{ConstantMetricF64Error, MetricF64, MetricF64Error, SimpleMetricF64, SimpleMetricF64Error};
use crate::network::Network;
use crate::state::{ConstParameterValues, NetworkStateError, NodeState, SimpleParameterValues, State, StateError};
use crate::timestep::Timestep;
use crate::virtual_storage::VirtualStorageIndex;
use std::fmt::{Display, Formatter};
use std::ops::{Deref, DerefMut};
use thiserror::Error;

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

#[derive(Debug, Error)]
pub enum NodeError {
    #[error("Flow constraints are undefined for this node type")]
    FlowConstraintsUndefined,
    #[error("Storage constraints are undefined for this node type")]
    StorageConstraintsUndefined,
    #[error("F64 metric error: {0}")]
    MetricF64Error(#[from] MetricF64Error),
    #[error("F64 simple metric error: {0}")]
    SimpleMetricF64Error(#[from] SimpleMetricF64Error),
    #[error("F64 constant metric error: {0}")]
    ConstantMetricF64Error(#[from] ConstantMetricF64Error),
    #[error("Invalid node connection to input node.")]
    InvalidNodeConnectionToInput,
    #[error("Input node has no incoming edges.")]
    InputNodeHasNoIncomingEdges,
    #[error("Invalid node connection from output node.")]
    InvalidNodeConnectionFromOutput,
    #[error("Output node has no outgoing edges.")]
    OutputNodeHasNoOutgoingEdges,
    #[error("No virtual storage on storage node")]
    NoVirtualStorageOnStorageNode,
    #[error("Network state error: {0}")]
    NetworkStateError(#[from] NetworkStateError),
    #[error("State error: {0}")]
    StateError(#[from] StateError),
    #[error("Virtual storage index not found: {0}")]
    VirtualStorageIndexNotFound(VirtualStorageIndex),
    #[error("Node index not found: {0}")]
    NodeIndexNotFound(NodeIndex),
}

#[derive(Debug, PartialEq)]
pub enum Node {
    Input(InputNode),
    Output(OutputNode),
    Link(LinkNode),
    Storage(StorageNode),
}

#[derive(Eq, PartialEq)]
pub enum NodeType {
    Input,
    Output,
    Link,
    Storage,
}

#[derive(Default)]
pub struct NodeVec {
    nodes: Vec<Node>,
}

impl Deref for NodeVec {
    type Target = Vec<Node>;

    fn deref(&self) -> &Self::Target {
        &self.nodes
    }
}

impl DerefMut for NodeVec {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.nodes
    }
}

impl NodeVec {
    pub fn get(&self, index: &NodeIndex) -> Option<&Node> {
        self.nodes.get(index.0)
    }

    pub fn get_mut(&mut self, index: &NodeIndex) -> Option<&mut Node> {
        self.nodes.get_mut(index.0)
    }

    pub fn push_new_input(&mut self, name: &str, sub_name: Option<&str>) -> NodeIndex {
        let node_index = NodeIndex(self.nodes.len());
        let node = Node::new_input(&node_index, name, sub_name);
        self.nodes.push(node);
        node_index
    }
    pub fn push_new_link(&mut self, name: &str, sub_name: Option<&str>) -> NodeIndex {
        let node_index = NodeIndex(self.nodes.len());
        let node = Node::new_link(&node_index, name, sub_name);
        self.nodes.push(node);
        node_index
    }
    pub fn push_new_output(&mut self, name: &str, sub_name: Option<&str>) -> NodeIndex {
        let node_index = NodeIndex(self.nodes.len());
        let node = Node::new_output(&node_index, name, sub_name);
        self.nodes.push(node);
        node_index
    }

    pub fn push_new_storage(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        initial_volume: StorageInitialVolume,
        min_volume: Option<SimpleMetricF64>,
        max_volume: Option<SimpleMetricF64>,
    ) -> NodeIndex {
        let node_index = NodeIndex(self.nodes.len());
        let node = Node::new_storage(&node_index, name, sub_name, initial_volume, min_volume, max_volume);
        self.nodes.push(node);
        node_index
    }
}

/// Bounds for the flow of a node
#[derive(Debug, Clone, Copy)]
pub struct FlowBounds {
    /// Minimum flow
    pub min_flow: f64,
    /// Maximum flow
    pub max_flow: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct VolumeBounds {
    /// Available volume (i.e. max that can be removed)
    pub available: f64,
    /// Missing volume (i.e. max that can be added)
    pub missing: f64,
}

pub enum NodeBounds {
    Flow(FlowBounds),
    Volume(VolumeBounds),
}

#[derive(Debug, Clone)]
pub enum Constraint {
    MinFlow,
    MaxFlow,
    MinAndMaxFlow,
    MinVolume,
    MaxVolume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostAggFunc {
    Sum,
    Max,
    Min,
}

impl Node {
    /// Create a new input node
    pub fn new_input(node_index: &NodeIndex, name: &str, sub_name: Option<&str>) -> Self {
        Self::Input(InputNode::new(node_index, name, sub_name))
    }

    /// Create a new output node
    pub fn new_output(node_index: &NodeIndex, name: &str, sub_name: Option<&str>) -> Self {
        Self::Output(OutputNode::new(node_index, name, sub_name))
    }

    /// Create a new link node
    pub fn new_link(node_index: &NodeIndex, name: &str, sub_name: Option<&str>) -> Self {
        Self::Link(LinkNode::new(node_index, name, sub_name))
    }

    /// Create a new storage node
    pub fn new_storage(
        node_index: &NodeIndex,
        name: &str,
        sub_name: Option<&str>,
        initial_volume: StorageInitialVolume,
        min_volume: Option<SimpleMetricF64>,
        max_volume: Option<SimpleMetricF64>,
    ) -> Self {
        Self::Storage(StorageNode::new(
            node_index,
            name,
            sub_name,
            initial_volume,
            min_volume,
            max_volume,
        ))
    }

    /// Get a node's name
    pub fn name(&self) -> &str {
        match self {
            Self::Input(n) => n.meta.name(),
            Self::Output(n) => n.meta.name(),
            Self::Link(n) => n.meta.name(),
            Self::Storage(n) => n.meta.name(),
        }
    }

    /// Get a node's sub_name
    pub fn sub_name(&self) -> Option<&str> {
        match self {
            Self::Input(n) => n.meta.sub_name(),
            Self::Output(n) => n.meta.sub_name(),
            Self::Link(n) => n.meta.sub_name(),
            Self::Storage(n) => n.meta.sub_name(),
        }
    }

    /// Get a node's full name
    pub fn full_name(&self) -> (&str, Option<&str>) {
        match self {
            Self::Input(n) => n.meta.full_name(),
            Self::Output(n) => n.meta.full_name(),
            Self::Link(n) => n.meta.full_name(),
            Self::Storage(n) => n.meta.full_name(),
        }
    }

    /// Get a node's index
    pub fn index(&self) -> NodeIndex {
        match self {
            Self::Input(n) => n.meta.index,
            Self::Output(n) => n.meta.index,
            Self::Link(n) => n.meta.index,
            Self::Storage(n) => n.meta.index,
        }
    }

    pub fn node_type(&self) -> NodeType {
        match self {
            Self::Input(_) => NodeType::Input,
            Self::Output(_) => NodeType::Output,
            Self::Link(_) => NodeType::Link,
            Self::Storage(_) => NodeType::Storage,
        }
    }

    pub fn apply<F>(&self, f: F)
    where
        F: Fn(&Node),
    {
        f(self);
    }

    pub fn default_state(&self) -> NodeState {
        match self {
            Self::Input(_n) => NodeState::new_flow_state(),
            Self::Output(_n) => NodeState::new_flow_state(),
            Self::Link(_n) => NodeState::new_flow_state(),
            Self::Storage(_n) => NodeState::new_storage_state(0.0),
        }
    }

    pub fn default_metric(&self) -> MetricF64 {
        match self {
            Self::Input(_n) => MetricF64::NodeOutFlow(self.index()),
            Self::Output(_n) => MetricF64::NodeInFlow(self.index()),
            Self::Link(_n) => MetricF64::NodeOutFlow(self.index()),
            Self::Storage(_n) => MetricF64::NodeVolume(self.index()),
        }
    }

    pub fn add_incoming_edge(&mut self, edge: EdgeIndex) -> Result<(), NodeError> {
        match self {
            Self::Input(_) => Err(NodeError::InvalidNodeConnectionToInput),
            Self::Output(n) => {
                n.add_incoming_edge(edge);
                Ok(())
            }
            Self::Link(n) => {
                n.add_incoming_edge(edge);
                Ok(())
            }
            Self::Storage(n) => {
                n.add_incoming_edge(edge);
                Ok(())
            }
        }
    }

    pub fn add_outgoing_edge(&mut self, edge: EdgeIndex) -> Result<(), NodeError> {
        match self {
            Self::Input(n) => {
                n.add_outgoing_edge(edge);
                Ok(())
            }
            Self::Output(_) => Err(NodeError::InvalidNodeConnectionFromOutput),
            Self::Link(n) => {
                n.add_outgoing_edge(edge);
                Ok(())
            }
            Self::Storage(n) => {
                n.add_outgoing_edge(edge);
                Ok(())
            }
        }
    }

    pub fn get_incoming_edges(&self) -> Result<&Vec<EdgeIndex>, NodeError> {
        match self {
            Self::Input(_) => Err(NodeError::InputNodeHasNoIncomingEdges),
            Self::Output(n) => Ok(&n.incoming_edges),
            Self::Link(n) => Ok(&n.incoming_edges),
            Self::Storage(n) => Ok(&n.incoming_edges),
        }
    }

    pub fn get_outgoing_edges(&self) -> Result<&Vec<EdgeIndex>, NodeError> {
        match self {
            Self::Input(n) => Ok(&n.outgoing_edges),
            Self::Output(_) => Err(NodeError::OutputNodeHasNoOutgoingEdges),
            Self::Link(n) => Ok(&n.outgoing_edges),
            Self::Storage(n) => Ok(&n.outgoing_edges),
        }
    }

    pub fn add_virtual_storage(&mut self, virtual_storage_index: VirtualStorageIndex) -> Result<(), NodeError> {
        match self {
            Self::Input(n) => {
                n.cost.virtual_storage_nodes.push(virtual_storage_index);
                Ok(())
            }
            Self::Output(n) => {
                n.cost.virtual_storage_nodes.push(virtual_storage_index);
                Ok(())
            }
            Self::Link(n) => {
                n.cost.virtual_storage_nodes.push(virtual_storage_index);
                Ok(())
            }
            Self::Storage(_) => Err(NodeError::NoVirtualStorageOnStorageNode),
        }
    }

    pub fn before(&self, timestep: &Timestep, state: &mut State) -> Result<(), NodeError> {
        // Currently only storage nodes do something during before
        match self {
            Node::Input(_) => Ok(()),
            Node::Output(_) => Ok(()),
            Node::Link(_) => Ok(()),
            Node::Storage(n) => n.before(timestep, state),
        }
    }

    pub fn set_min_flow_constraint(&mut self, value: Option<MetricF64>) -> Result<(), NodeError> {
        match self {
            Self::Input(n) => {
                n.set_min_flow(value);
                Ok(())
            }
            Self::Link(n) => {
                n.set_min_flow(value);
                Ok(())
            }
            Self::Output(n) => {
                n.set_min_flow(value);
                Ok(())
            }
            Self::Storage(_) => Err(NodeError::FlowConstraintsUndefined),
        }
    }

    pub fn get_min_flow(&self, network: &Network, state: &State) -> Result<f64, NodeError> {
        match self {
            Self::Input(n) => Ok(n.get_min_flow(network, state)?),
            Self::Link(n) => Ok(n.get_min_flow(network, state)?),
            Self::Output(n) => Ok(n.get_min_flow(network, state)?),
            Self::Storage(_) => Err(NodeError::FlowConstraintsUndefined),
        }
    }

    pub fn get_const_min_flow(&self, values: &ConstParameterValues) -> Result<Option<f64>, NodeError> {
        match self {
            Self::Input(n) => Ok(n.get_const_min_flow(values)?),
            Self::Link(n) => Ok(n.get_const_min_flow(values)?),
            Self::Output(n) => Ok(n.get_const_min_flow(values)?),
            Self::Storage(_) => Err(NodeError::FlowConstraintsUndefined),
        }
    }

    pub fn set_max_flow_constraint(&mut self, value: Option<MetricF64>) -> Result<(), NodeError> {
        match self {
            Self::Input(n) => {
                n.set_max_flow(value);
                Ok(())
            }
            Self::Link(n) => {
                n.set_max_flow(value);
                Ok(())
            }
            Self::Output(n) => {
                n.set_max_flow(value);
                Ok(())
            }
            Self::Storage(_) => Err(NodeError::FlowConstraintsUndefined),
        }
    }

    pub fn get_max_flow(&self, network: &Network, state: &State) -> Result<f64, NodeError> {
        match self {
            Self::Input(n) => Ok(n.get_max_flow(network, state)?),
            Self::Link(n) => Ok(n.get_max_flow(network, state)?),
            Self::Output(n) => Ok(n.get_max_flow(network, state)?),
            Self::Storage(_) => Err(NodeError::FlowConstraintsUndefined),
        }
    }

    pub fn get_const_max_flow(&self, values: &ConstParameterValues) -> Result<Option<f64>, NodeError> {
        match self {
            Self::Input(n) => Ok(n.get_const_max_flow(values)?),
            Self::Link(n) => Ok(n.get_const_max_flow(values)?),
            Self::Output(n) => Ok(n.get_const_max_flow(values)?),
            Self::Storage(_) => Err(NodeError::FlowConstraintsUndefined),
        }
    }

    pub fn is_max_flow_unconstrained(&self) -> Result<bool, NodeError> {
        match self {
            Self::Input(n) => Ok(n.is_max_flow_unconstrained()),
            Self::Link(n) => Ok(n.is_max_flow_unconstrained()),
            Self::Output(n) => Ok(n.is_max_flow_unconstrained()),
            Self::Storage(_) => Err(NodeError::FlowConstraintsUndefined),
        }
    }

    pub fn set_initial_volume(&mut self, initial_volume: StorageInitialVolume) -> Result<(), NodeError> {
        match self {
            Self::Input(_) => Err(NodeError::StorageConstraintsUndefined),
            Self::Link(_) => Err(NodeError::StorageConstraintsUndefined),
            Self::Output(_) => Err(NodeError::StorageConstraintsUndefined),
            Self::Storage(n) => {
                n.set_initial_volume(initial_volume);
                Ok(())
            }
        }
    }

    pub fn set_min_volume_constraint(&mut self, value: Option<SimpleMetricF64>) -> Result<(), NodeError> {
        match self {
            Self::Input(_) => Err(NodeError::StorageConstraintsUndefined),
            Self::Link(_) => Err(NodeError::StorageConstraintsUndefined),
            Self::Output(_) => Err(NodeError::StorageConstraintsUndefined),
            Self::Storage(n) => {
                n.set_min_volume(value);
                Ok(())
            }
        }
    }

    pub fn get_min_volume(&self, state: &State) -> Result<f64, NodeError> {
        match self {
            Self::Input(_) => Err(NodeError::StorageConstraintsUndefined),
            Self::Link(_) => Err(NodeError::StorageConstraintsUndefined),
            Self::Output(_) => Err(NodeError::StorageConstraintsUndefined),
            Self::Storage(n) => Ok(n.get_min_volume(state)?),
        }
    }

    pub fn set_max_volume_constraint(&mut self, value: Option<SimpleMetricF64>) -> Result<(), NodeError> {
        match self {
            Self::Input(_) => Err(NodeError::StorageConstraintsUndefined),
            Self::Link(_) => Err(NodeError::StorageConstraintsUndefined),
            Self::Output(_) => Err(NodeError::StorageConstraintsUndefined),
            Self::Storage(n) => {
                n.set_max_volume(value);
                Ok(())
            }
        }
    }

    pub fn get_max_volume(&self, state: &State) -> Result<f64, NodeError> {
        match self {
            Self::Input(_) => Err(NodeError::StorageConstraintsUndefined),
            Self::Link(_) => Err(NodeError::StorageConstraintsUndefined),
            Self::Output(_) => Err(NodeError::StorageConstraintsUndefined),
            Self::Storage(n) => Ok(n.get_max_volume(state)?),
        }
    }

    /// Return the current min and max volumes as a tuple.
    pub fn get_volume_bounds(&self, state: &State) -> Result<(f64, f64), NodeError> {
        match (self.get_min_volume(state), self.get_max_volume(state)) {
            (Ok(min_vol), Ok(max_vol)) => Ok((min_vol, max_vol)),
            _ => Err(NodeError::StorageConstraintsUndefined),
        }
    }

    /// Get constant bounds for the node, if they exist, depending on its type.
    ///
    /// Note that [`Node::Storage`] nodes can never have constant bounds.
    pub fn get_const_bounds(&self, values: &ConstParameterValues) -> Result<Option<NodeBounds>, NodeError> {
        match self {
            Self::Input(n) => {
                let min_flow = n.get_const_min_flow(values)?;
                let max_flow = n.get_const_max_flow(values)?;

                match (min_flow, max_flow) {
                    (Some(min_flow), Some(max_flow)) => Ok(Some(NodeBounds::Flow(FlowBounds { min_flow, max_flow }))),
                    _ => Ok(None),
                }
            }
            Self::Output(n) => {
                let min_flow = n.get_const_min_flow(values)?;
                let max_flow = n.get_const_max_flow(values)?;

                match (min_flow, max_flow) {
                    (Some(min_flow), Some(max_flow)) => Ok(Some(NodeBounds::Flow(FlowBounds { min_flow, max_flow }))),
                    _ => Ok(None),
                }
            }
            Self::Link(n) => {
                let min_flow = n.get_const_min_flow(values)?;
                let max_flow = n.get_const_max_flow(values)?;

                match (min_flow, max_flow) {
                    (Some(min_flow), Some(max_flow)) => Ok(Some(NodeBounds::Flow(FlowBounds { min_flow, max_flow }))),
                    _ => Ok(None),
                }
            }
            Self::Storage(_) => Ok(None),
        }
    }

    /// Get bounds for the node depending on its type.
    pub fn get_bounds(&self, network: &Network, state: &State) -> Result<NodeBounds, NodeError> {
        match self {
            Self::Input(n) => Ok(NodeBounds::Flow(FlowBounds {
                min_flow: n.flow_constraints.get_min_flow(network, state)?,
                max_flow: n.flow_constraints.get_max_flow(network, state)?,
            })),
            Self::Output(n) => Ok(NodeBounds::Flow(FlowBounds {
                min_flow: n.flow_constraints.get_min_flow(network, state)?,
                max_flow: n.flow_constraints.get_max_flow(network, state)?,
            })),
            Self::Link(n) => Ok(NodeBounds::Flow(FlowBounds {
                min_flow: n.flow_constraints.get_min_flow(network, state)?,
                max_flow: n.flow_constraints.get_max_flow(network, state)?,
            })),
            Self::Storage(n) => {
                let current_volume = state.get_network_state().get_node_volume(&n.meta.index)?;

                let available = current_volume - n.get_min_volume(state)?;
                let missing = n.get_max_volume(state)? - current_volume;

                Ok(NodeBounds::Volume(VolumeBounds { available, missing }))
            }
        }
    }

    pub fn set_cost(&mut self, value: Option<MetricF64>) {
        match self {
            Self::Input(n) => n.set_cost(value),
            Self::Link(n) => n.set_cost(value),
            Self::Output(n) => n.set_cost(value),
            Self::Storage(n) => n.set_cost(value),
        }
    }

    pub fn set_cost_agg_func(&mut self, agg_func: Option<CostAggFunc>) -> Result<(), NodeError> {
        match self {
            Self::Input(n) => n.set_cost_agg_func(agg_func),
            Self::Link(n) => n.set_cost_agg_func(agg_func),
            Self::Output(n) => n.set_cost_agg_func(agg_func),
            Self::Storage(_) => return Err(NodeError::NoVirtualStorageOnStorageNode),
        };

        Ok(())
    }

    pub fn get_outgoing_cost(&self, network: &Network, state: &State) -> Result<f64, NodeError> {
        match self {
            Self::Input(n) => n.get_cost(network, state),
            Self::Link(n) => Ok(n.get_cost(network, state)? / 2.0),
            Self::Output(n) => n.get_cost(network, state),
            Self::Storage(n) => Ok(-n.get_cost(network, state)?),
        }
    }

    pub fn get_incoming_cost(&self, network: &Network, state: &State) -> Result<f64, NodeError> {
        match self {
            Self::Input(n) => n.get_cost(network, state),
            Self::Link(n) => Ok(n.get_cost(network, state)? / 2.0),
            Self::Output(n) => n.get_cost(network, state),
            Self::Storage(n) => Ok(n.get_cost(network, state)?),
        }
    }
}

/// Meta data common to all nodes.
#[derive(Debug, PartialEq, Eq)]
pub struct NodeMeta<T> {
    index: T,
    name: String,
    sub_name: Option<String>,
    comment: String,
}

impl<T> NodeMeta<T>
where
    T: Copy,
{
    pub(crate) fn new(index: &T, name: &str, sub_name: Option<&str>) -> Self {
        Self {
            index: *index,
            name: name.to_string(),
            sub_name: sub_name.map(|s| s.to_string()),
            comment: "".to_string(),
        }
    }

    pub(crate) fn index(&self) -> &T {
        &self.index
    }
    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }
    pub(crate) fn sub_name(&self) -> Option<&str> {
        self.sub_name.as_deref()
    }
    pub(crate) fn full_name(&self) -> (&str, Option<&str>) {
        (self.name(), self.sub_name())
    }
}

#[derive(Debug, PartialEq, Default)]
pub struct FlowConstraints {
    pub min_flow: Option<MetricF64>,
    pub max_flow: Option<MetricF64>,
}

impl FlowConstraints {
    /// Return the current minimum flow from the parameter state
    ///
    /// Defaults to zero if no parameter is defined.
    pub fn get_min_flow(&self, network: &Network, state: &State) -> Result<f64, MetricF64Error> {
        match &self.min_flow {
            None => Ok(0.0),
            Some(m) => m.get_value(network, state),
        }
    }

    /// Return the constant minimum flow if it exists.
    ///
    /// Defaults to zero if no parameter is defined.
    pub fn get_const_min_flow(&self, values: &ConstParameterValues) -> Result<Option<f64>, ConstantMetricF64Error> {
        match &self.min_flow {
            None => Ok(Some(0.0)),
            Some(m) => m.try_get_constant_value(values),
        }
    }
    /// Return the current maximum flow from the parameter state
    ///
    /// Defaults to [`f64::MAX`] if no parameter is defined.
    pub fn get_max_flow(&self, network: &Network, state: &State) -> Result<f64, MetricF64Error> {
        match &self.max_flow {
            None => Ok(f64::MAX),
            Some(m) => m.get_value(network, state),
        }
    }

    /// Return the constant maximum flow if it exists.
    ///
    /// Defaults to [`f64::MAX`] if no parameter is defined.
    pub fn get_const_max_flow(&self, values: &ConstParameterValues) -> Result<Option<f64>, ConstantMetricF64Error> {
        match &self.max_flow {
            None => Ok(Some(f64::MAX)),
            Some(m) => m.try_get_constant_value(values),
        }
    }

    pub fn is_max_flow_unconstrained(&self) -> bool {
        self.max_flow.is_none()
    }
}

#[derive(Debug, PartialEq)]
pub struct StorageConstraints {
    pub(crate) min_volume: Option<SimpleMetricF64>,
    pub(crate) max_volume: Option<SimpleMetricF64>,
}

impl StorageConstraints {
    pub fn new(min_volume: Option<SimpleMetricF64>, max_volume: Option<SimpleMetricF64>) -> Self {
        Self { min_volume, max_volume }
    }
    /// Return the current minimum volume from the parameter state
    ///
    /// Defaults to zero if no parameter is defined.
    pub fn get_min_volume(&self, values: &SimpleParameterValues) -> Result<f64, SimpleMetricF64Error> {
        match &self.min_volume {
            None => Ok(0.0),
            Some(m) => m.get_value(values),
        }
    }
    /// Return the current maximum volume from the metric state
    ///
    /// Defaults to f64::MAX if no parameter is defined.
    pub fn get_max_volume(&self, values: &SimpleParameterValues) -> Result<f64, SimpleMetricF64Error> {
        match &self.max_volume {
            None => Ok(f64::MAX),
            Some(m) => m.get_value(values),
        }
    }
}

/// Generic cost data for a node.
#[derive(Debug, PartialEq, Default)]
struct NodeCost {
    local: Option<MetricF64>,
    virtual_storage_nodes: Vec<VirtualStorageIndex>,
    agg_func: Option<CostAggFunc>,
}

impl NodeCost {
    fn get_cost(&self, network: &Network, state: &State) -> Result<f64, NodeError> {
        // Initial local cost that has any virtual storage cost applied
        let mut cost = match &self.local {
            None => Ok(0.0),
            Some(m) => m.get_value(network, state),
        }?;

        if let Some(agg_func) = &self.agg_func {
            let vs_costs = self.virtual_storage_nodes.iter().map(|idx| {
                let vs = network
                    .get_virtual_storage_node(idx)
                    .ok_or(NodeError::VirtualStorageIndexNotFound(*idx))?;
                Ok::<_, NodeError>(vs.get_cost(network, state)?)
            });

            match agg_func {
                CostAggFunc::Sum => {
                    for vs_cost in vs_costs {
                        cost += vs_cost?;
                    }
                }
                CostAggFunc::Max => {
                    for vs_cost in vs_costs {
                        cost = cost.max(vs_cost?);
                    }
                }
                CostAggFunc::Min => {
                    for vs_cost in vs_costs {
                        cost = cost.min(vs_cost?);
                    }
                }
            };
        }

        Ok(cost)
    }
}

#[derive(Debug, PartialEq)]
pub struct InputNode {
    pub meta: NodeMeta<NodeIndex>,
    cost: NodeCost,
    pub flow_constraints: FlowConstraints,
    pub outgoing_edges: Vec<EdgeIndex>,
}

impl InputNode {
    fn new(index: &NodeIndex, name: &str, sub_name: Option<&str>) -> Self {
        Self {
            meta: NodeMeta::new(index, name, sub_name),
            cost: NodeCost::default(),
            flow_constraints: FlowConstraints::default(),
            outgoing_edges: Vec::new(),
        }
    }
    fn set_cost(&mut self, value: Option<MetricF64>) {
        self.cost.local = value
    }
    fn set_cost_agg_func(&mut self, agg_func: Option<CostAggFunc>) {
        self.cost.agg_func = agg_func
    }
    fn get_cost(&self, network: &Network, state: &State) -> Result<f64, NodeError> {
        self.cost.get_cost(network, state)
    }
    fn set_min_flow(&mut self, value: Option<MetricF64>) {
        self.flow_constraints.min_flow = value;
    }
    fn get_min_flow(&self, network: &Network, state: &State) -> Result<f64, MetricF64Error> {
        self.flow_constraints.get_min_flow(network, state)
    }
    fn get_const_min_flow(&self, values: &ConstParameterValues) -> Result<Option<f64>, ConstantMetricF64Error> {
        self.flow_constraints.get_const_min_flow(values)
    }
    fn set_max_flow(&mut self, value: Option<MetricF64>) {
        self.flow_constraints.max_flow = value;
    }
    fn get_max_flow(&self, network: &Network, state: &State) -> Result<f64, MetricF64Error> {
        self.flow_constraints.get_max_flow(network, state)
    }
    fn get_const_max_flow(&self, values: &ConstParameterValues) -> Result<Option<f64>, ConstantMetricF64Error> {
        self.flow_constraints.get_const_max_flow(values)
    }
    fn is_max_flow_unconstrained(&self) -> bool {
        self.flow_constraints.is_max_flow_unconstrained()
    }
    fn add_outgoing_edge(&mut self, edge: EdgeIndex) {
        self.outgoing_edges.push(edge);
    }
}

#[derive(Debug, PartialEq)]
pub struct OutputNode {
    pub meta: NodeMeta<NodeIndex>,
    cost: NodeCost,
    pub flow_constraints: FlowConstraints,
    pub incoming_edges: Vec<EdgeIndex>,
}

impl OutputNode {
    fn new(index: &NodeIndex, name: &str, sub_name: Option<&str>) -> Self {
        Self {
            meta: NodeMeta::new(index, name, sub_name),
            cost: NodeCost::default(),
            flow_constraints: FlowConstraints::default(),
            incoming_edges: Vec::new(),
        }
    }
    fn set_cost(&mut self, value: Option<MetricF64>) {
        self.cost.local = value
    }
    fn get_cost(&self, network: &Network, state: &State) -> Result<f64, NodeError> {
        self.cost.get_cost(network, state)
    }
    fn set_cost_agg_func(&mut self, agg_func: Option<CostAggFunc>) {
        self.cost.agg_func = agg_func
    }
    fn set_min_flow(&mut self, value: Option<MetricF64>) {
        self.flow_constraints.min_flow = value;
    }
    fn get_min_flow(&self, network: &Network, state: &State) -> Result<f64, MetricF64Error> {
        self.flow_constraints.get_min_flow(network, state)
    }
    fn get_const_min_flow(&self, values: &ConstParameterValues) -> Result<Option<f64>, ConstantMetricF64Error> {
        self.flow_constraints.get_const_min_flow(values)
    }
    fn set_max_flow(&mut self, value: Option<MetricF64>) {
        self.flow_constraints.max_flow = value;
    }
    fn get_max_flow(&self, network: &Network, state: &State) -> Result<f64, MetricF64Error> {
        self.flow_constraints.get_max_flow(network, state)
    }
    fn get_const_max_flow(&self, values: &ConstParameterValues) -> Result<Option<f64>, ConstantMetricF64Error> {
        self.flow_constraints.get_const_max_flow(values)
    }
    fn is_max_flow_unconstrained(&self) -> bool {
        self.flow_constraints.is_max_flow_unconstrained()
    }
    fn add_incoming_edge(&mut self, edge: EdgeIndex) {
        self.incoming_edges.push(edge);
    }
}

#[derive(Debug, PartialEq)]
pub struct LinkNode {
    pub meta: NodeMeta<NodeIndex>,
    cost: NodeCost,
    pub flow_constraints: FlowConstraints,
    pub incoming_edges: Vec<EdgeIndex>,
    pub outgoing_edges: Vec<EdgeIndex>,
}

impl LinkNode {
    fn new(index: &NodeIndex, name: &str, sub_name: Option<&str>) -> Self {
        Self {
            meta: NodeMeta::new(index, name, sub_name),
            cost: NodeCost::default(),
            flow_constraints: FlowConstraints::default(),
            incoming_edges: Vec::new(),
            outgoing_edges: Vec::new(),
        }
    }
    fn set_cost(&mut self, value: Option<MetricF64>) {
        self.cost.local = value
    }
    fn set_cost_agg_func(&mut self, agg_func: Option<CostAggFunc>) {
        self.cost.agg_func = agg_func
    }
    fn get_cost(&self, network: &Network, state: &State) -> Result<f64, NodeError> {
        self.cost.get_cost(network, state)
    }
    fn set_min_flow(&mut self, value: Option<MetricF64>) {
        self.flow_constraints.min_flow = value;
    }
    fn get_min_flow(&self, network: &Network, state: &State) -> Result<f64, MetricF64Error> {
        self.flow_constraints.get_min_flow(network, state)
    }
    fn get_const_min_flow(&self, values: &ConstParameterValues) -> Result<Option<f64>, ConstantMetricF64Error> {
        self.flow_constraints.get_const_min_flow(values)
    }
    fn set_max_flow(&mut self, value: Option<MetricF64>) {
        self.flow_constraints.max_flow = value;
    }
    fn get_max_flow(&self, network: &Network, state: &State) -> Result<f64, MetricF64Error> {
        self.flow_constraints.get_max_flow(network, state)
    }
    fn get_const_max_flow(&self, values: &ConstParameterValues) -> Result<Option<f64>, ConstantMetricF64Error> {
        self.flow_constraints.get_const_max_flow(values)
    }
    fn is_max_flow_unconstrained(&self) -> bool {
        self.flow_constraints.is_max_flow_unconstrained()
    }
    fn add_incoming_edge(&mut self, edge: EdgeIndex) {
        self.incoming_edges.push(edge);
    }
    fn add_outgoing_edge(&mut self, edge: EdgeIndex) {
        self.outgoing_edges.push(edge);
    }
}

/// Initial volume for a storage node.
#[derive(Debug, PartialEq, Clone)]
pub enum StorageInitialVolume {
    /// Absolute initial volume.
    Absolute(f64),
    /// Proportional initial volume, relative to the maximum volume.
    Proportional(f64),
    /// Absolute initial volume, but distributed progressively over other storage nodes.
    /// This is used for piecewise storage node configurations that comprise multiple
    /// nodes. The `absolute` field is the initial volume, and `prior_max_volume` contains
    /// the metrics that this volume is distributed over before this node.
    /// Only if there is any remaining volume after distributing
    /// `absolute` over `prior_max_volume`, this node will have a non-zero initial volume.
    DistributedAbsolute {
        /// The absolute initial volume.
        absolute: f64,
        /// The sum of the max volumes distributed prior to this node.
        prior_max_volume: SimpleMetricF64,
    },
    /// Similar to `DistributedAbsolute`, but the initial volume is proportional.
    DistributedProportional {
        /// The total max volume of the group of storage nodes.
        total_volume: SimpleMetricF64,
        /// The absolute initial volume.
        proportion: f64,
        /// The sum of the max volumes distributed prior to this node.
        prior_max_volume: SimpleMetricF64,
    },
}

impl StorageInitialVolume {
    /// Get the initial volume as an absolute value.
    pub fn get_absolute_initial_volume(&self, max_volume: f64, state: &State) -> Result<f64, SimpleMetricF64Error> {
        match self {
            StorageInitialVolume::Absolute(iv) => Ok(*iv),
            StorageInitialVolume::Proportional(ipc) => Ok(max_volume * ipc),
            StorageInitialVolume::DistributedAbsolute {
                absolute,
                prior_max_volume,
            } => {
                let prior_max_volume = prior_max_volume.get_value(&state.get_simple_parameter_values())?;

                // The initial volume is the absolute value minus the prior volumes,
                // but it cannot exceed the maximum volume.
                Ok((*absolute - prior_max_volume).max(0.0).min(max_volume))
            }
            StorageInitialVolume::DistributedProportional {
                total_volume,
                proportion,
                prior_max_volume,
            } => {
                let prior_max_volume = prior_max_volume.get_value(&state.get_simple_parameter_values())?;

                // Calculate the absolute initial volume based on the total volume and proportion.
                let absolute = total_volume.get_value(&state.get_simple_parameter_values())? * proportion;

                // The initial volume is the absolute value minus the prior volumes,
                // but it cannot exceed the maximum volume.
                Ok((absolute - prior_max_volume).max(0.0).min(max_volume))
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct StorageNode {
    pub meta: NodeMeta<NodeIndex>,
    pub cost: Option<MetricF64>,
    pub initial_volume: StorageInitialVolume,
    pub storage_constraints: StorageConstraints,
    pub incoming_edges: Vec<EdgeIndex>,
    pub outgoing_edges: Vec<EdgeIndex>,
}

impl StorageNode {
    fn new(
        index: &NodeIndex,
        name: &str,
        sub_name: Option<&str>,
        initial_volume: StorageInitialVolume,
        min_volume: Option<SimpleMetricF64>,
        max_volume: Option<SimpleMetricF64>,
    ) -> Self {
        Self {
            meta: NodeMeta::new(index, name, sub_name),
            cost: None,
            initial_volume,
            storage_constraints: StorageConstraints::new(min_volume, max_volume),
            incoming_edges: Vec::new(),
            outgoing_edges: Vec::new(),
        }
    }

    pub fn before(&self, timestep: &Timestep, state: &mut State) -> Result<(), NodeError> {
        // Set the initial volume if it is the first timestep.
        if timestep.is_first() {
            let volume = self
                .initial_volume
                .get_absolute_initial_volume(self.get_max_volume(state)?, state)?;

            state.set_node_volume(&self.meta.index, volume)?;
        }
        Ok(())
    }

    fn set_cost(&mut self, value: Option<MetricF64>) {
        self.cost = value
    }
    fn get_cost(&self, network: &Network, state: &State) -> Result<f64, MetricF64Error> {
        match &self.cost {
            None => Ok(0.0),
            Some(m) => m.get_value(network, state),
        }
    }
    fn set_initial_volume(&mut self, initial_volume: StorageInitialVolume) {
        self.initial_volume = initial_volume;
    }
    fn set_min_volume(&mut self, value: Option<SimpleMetricF64>) {
        // TODO use a set_min_volume method
        self.storage_constraints.min_volume = value;
    }
    pub fn get_min_volume(&self, state: &State) -> Result<f64, SimpleMetricF64Error> {
        self.storage_constraints
            .get_min_volume(&state.get_simple_parameter_values())
    }
    fn set_max_volume(&mut self, value: Option<SimpleMetricF64>) {
        // TODO use a set_min_volume method
        self.storage_constraints.max_volume = value;
    }
    pub fn get_max_volume(&self, state: &State) -> Result<f64, SimpleMetricF64Error> {
        self.storage_constraints
            .get_max_volume(&state.get_simple_parameter_values())
    }
    fn add_incoming_edge(&mut self, edge: EdgeIndex) {
        self.incoming_edges.push(edge);
    }
    fn add_outgoing_edge(&mut self, edge: EdgeIndex) {
        self.outgoing_edges.push(edge);
    }
}
