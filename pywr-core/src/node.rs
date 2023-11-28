use crate::edge::EdgeIndex;
use crate::metric::Metric;
use crate::network::Network;
use crate::state::{NodeState, State};
use crate::timestep::Timestep;
use crate::virtual_storage::VirtualStorageIndex;
use crate::PywrError;
use std::ops::{Deref, DerefMut};

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct NodeIndex(usize);

impl Deref for NodeIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
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
    pub fn get(&self, index: &NodeIndex) -> Result<&Node, PywrError> {
        self.nodes.get(index.0).ok_or(PywrError::NodeIndexNotFound)
    }

    pub fn get_mut(&mut self, index: &NodeIndex) -> Result<&mut Node, PywrError> {
        self.nodes.get_mut(index.0).ok_or(PywrError::NodeIndexNotFound)
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
        min_volume: ConstraintValue,
        max_volume: ConstraintValue,
    ) -> NodeIndex {
        let node_index = NodeIndex(self.nodes.len());
        let node = Node::new_storage(&node_index, name, sub_name, initial_volume, min_volume, max_volume);
        self.nodes.push(node);
        node_index
    }
}

#[derive(Debug, Clone)]
pub enum Constraint {
    MinFlow,
    MaxFlow,
    MinAndMaxFlow,
    MinVolume,
    MaxVolume,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConstraintValue {
    None,
    Scalar(f64),
    Metric(Metric),
}

impl From<f64> for ConstraintValue {
    fn from(v: f64) -> Self {
        ConstraintValue::Scalar(v)
    }
}

impl From<Metric> for ConstraintValue {
    fn from(metric: Metric) -> Self {
        Self::Metric(metric)
    }
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
        min_volume: ConstraintValue,
        max_volume: ConstraintValue,
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

    pub fn default_metric(&self) -> Metric {
        match self {
            Self::Input(_n) => Metric::NodeOutFlow(self.index()),
            Self::Output(_n) => Metric::NodeInFlow(self.index()),
            Self::Link(_n) => Metric::NodeOutFlow(self.index()),
            Self::Storage(_n) => Metric::NodeVolume(self.index()),
        }
    }

    pub fn add_incoming_edge(&mut self, edge: EdgeIndex) -> Result<(), PywrError> {
        match self {
            Self::Input(n) => Err(PywrError::InvalidNodeConnectionToInput(n.meta.name.clone())),
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

    pub fn add_outgoing_edge(&mut self, edge: EdgeIndex) -> Result<(), PywrError> {
        match self {
            Self::Input(n) => {
                n.add_outgoing_edge(edge);
                Ok(())
            }
            Self::Output(n) => Err(PywrError::InvalidNodeConnectionFromOutput(n.meta.name.clone())),
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

    pub fn get_incoming_edges(&self) -> Result<&Vec<EdgeIndex>, PywrError> {
        match self {
            Self::Input(n) => Err(PywrError::InvalidNodeConnectionToInput(n.meta.name.clone())), // TODO better error
            Self::Output(n) => Ok(&n.incoming_edges),
            Self::Link(n) => Ok(&n.incoming_edges),
            Self::Storage(n) => Ok(&n.incoming_edges),
        }
    }

    pub fn get_outgoing_edges(&self) -> Result<&Vec<EdgeIndex>, PywrError> {
        match self {
            Self::Input(n) => Ok(&n.outgoing_edges),
            Self::Output(n) => Err(PywrError::InvalidNodeConnectionFromOutput(n.meta.name.clone())), // TODO better error
            Self::Link(n) => Ok(&n.outgoing_edges),
            Self::Storage(n) => Ok(&n.outgoing_edges),
        }
    }

    pub fn add_virtual_storage(&mut self, virtual_storage_index: VirtualStorageIndex) -> Result<(), PywrError> {
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
            Self::Storage(_) => Err(PywrError::NoVirtualStorageOnStorageNode),
        }
    }

    // /// Return a reference to a node's flow constraints if they exist.
    // fn flow_constraints(&self) -> Option<&FlowConstraints> {
    //     match self {
    //         Node::Input(n) => Some(&n.flow_constraints),
    //         Node::Link(n) => Some(&n.flow_constraints),
    //         Node::Output(n) => Some(&n.flow_constraints),
    //         Node::Storage(n) => None,
    //     }
    // }

    // /// Return a mutable reference to a node's flow constraints if they exist.
    // fn flow_constraints_mut(&mut self) -> Result<&mut FlowConstraints, PywrError> {
    //     match self {
    //         Node::Input(n) => Ok(&mut n.flow_constraints),
    //         Node::Link(n) => Ok(&mut n.flow_constraints),
    //         Node::Output(n) => Ok(&mut n.flow_constraints),
    //         Node::Storage(_) => Err(PywrError::FlowConstraintsUndefined),
    //     }
    // }

    // /// Return a reference to a node's storage constraints if they exist.
    // fn storage_constraints(&self) -> Result<&StorageConstraints, PywrError> {
    //     match self {
    //         Node::Input(_) => Err(PywrError::StorageConstraintsUndefined),
    //         Node::Link(_) => Err(PywrError::StorageConstraintsUndefined),
    //         Node::Output(_) => Err(PywrError::StorageConstraintsUndefined),
    //         Node::Storage(n) => Ok(&n.storage_constraints),
    //     }
    // }

    // /// Return a mutable reference to a node's storage constraints if they exist.
    // fn storage_constraints_mut(&self) -> Result<&mut StorageConstraints, PywrError> {
    //     match self.0.borrow_mut().deref_mut() {
    //         _Node::Input(_) => Err(PywrError::StorageConstraintsUndefined),
    //         _Node::Link(_) => Err(PywrError::StorageConstraintsUndefined),
    //         _Node::Output(_) => Err(PywrError::StorageConstraintsUndefined),
    //         _Node::Storage(n) => Ok(&mut n.storage_constraints),
    //     }
    // }

    pub fn before(&self, timestep: &Timestep, network: &Network, state: &mut State) -> Result<(), PywrError> {
        // Currently only storage nodes do something during before
        match self {
            Node::Input(_) => Ok(()),
            Node::Output(_) => Ok(()),
            Node::Link(_) => Ok(()),
            Node::Storage(n) => n.before(timestep, network, state),
        }
    }

    /// Set a constraint on a node.
    pub fn set_constraint(&mut self, value: ConstraintValue, constraint: Constraint) -> Result<(), PywrError> {
        match constraint {
            Constraint::MinFlow => self.set_min_flow_constraint(value)?,
            Constraint::MaxFlow => self.set_max_flow_constraint(value)?,
            Constraint::MinAndMaxFlow => {
                self.set_min_flow_constraint(value.clone())?;
                self.set_max_flow_constraint(value)?;
            }
            Constraint::MinVolume => self.set_min_volume_constraint(value)?,
            Constraint::MaxVolume => self.set_max_volume_constraint(value)?,
        }
        Ok(())
    }

    pub fn set_min_flow_constraint(&mut self, value: ConstraintValue) -> Result<(), PywrError> {
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
            Self::Storage(_) => Err(PywrError::FlowConstraintsUndefined),
        }
    }

    pub fn get_current_min_flow(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        match self {
            Self::Input(n) => n.get_min_flow(network, state),
            Self::Link(n) => n.get_min_flow(network, state),
            Self::Output(n) => n.get_min_flow(network, state),
            Self::Storage(_) => Err(PywrError::FlowConstraintsUndefined),
        }
    }

    pub fn set_max_flow_constraint(&mut self, value: ConstraintValue) -> Result<(), PywrError> {
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
            Self::Storage(_) => Err(PywrError::FlowConstraintsUndefined),
        }
    }

    pub fn get_current_max_flow(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        match self {
            Self::Input(n) => n.get_max_flow(network, state),
            Self::Link(n) => n.get_max_flow(network, state),
            Self::Output(n) => n.get_max_flow(network, state),
            Self::Storage(_) => Err(PywrError::FlowConstraintsUndefined),
        }
    }

    pub fn is_max_flow_unconstrained(&self) -> Result<bool, PywrError> {
        match self {
            Self::Input(n) => Ok(n.is_max_flow_unconstrained()),
            Self::Link(n) => Ok(n.is_max_flow_unconstrained()),
            Self::Output(n) => Ok(n.is_max_flow_unconstrained()),
            Self::Storage(_) => Err(PywrError::FlowConstraintsUndefined),
        }
    }

    pub fn get_current_flow_bounds(&self, network: &Network, state: &State) -> Result<(f64, f64), PywrError> {
        match (
            self.get_current_min_flow(network, state),
            self.get_current_max_flow(network, state),
        ) {
            (Ok(min_flow), Ok(max_flow)) => Ok((min_flow, max_flow)),
            _ => Err(PywrError::FlowConstraintsUndefined),
        }
    }

    pub fn set_min_volume_constraint(&mut self, value: ConstraintValue) -> Result<(), PywrError> {
        match self {
            Self::Input(_) => Err(PywrError::StorageConstraintsUndefined),
            Self::Link(_) => Err(PywrError::StorageConstraintsUndefined),
            Self::Output(_) => Err(PywrError::StorageConstraintsUndefined),
            Self::Storage(n) => {
                n.set_min_volume(value);
                Ok(())
            }
        }
    }

    pub fn get_current_min_volume(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        match self {
            Self::Input(_) => Err(PywrError::StorageConstraintsUndefined),
            Self::Link(_) => Err(PywrError::StorageConstraintsUndefined),
            Self::Output(_) => Err(PywrError::StorageConstraintsUndefined),
            Self::Storage(n) => n.get_min_volume(network, state),
        }
    }

    pub fn set_max_volume_constraint(&mut self, value: ConstraintValue) -> Result<(), PywrError> {
        match self {
            Self::Input(_) => Err(PywrError::StorageConstraintsUndefined),
            Self::Link(_) => Err(PywrError::StorageConstraintsUndefined),
            Self::Output(_) => Err(PywrError::StorageConstraintsUndefined),
            Self::Storage(n) => {
                n.set_max_volume(value);
                Ok(())
            }
        }
    }

    pub fn get_current_max_volume(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        match self {
            Self::Input(_) => Err(PywrError::StorageConstraintsUndefined),
            Self::Link(_) => Err(PywrError::StorageConstraintsUndefined),
            Self::Output(_) => Err(PywrError::StorageConstraintsUndefined),
            Self::Storage(n) => n.get_max_volume(network, state),
        }
    }

    pub fn get_current_volume_bounds(&self, network: &Network, state: &State) -> Result<(f64, f64), PywrError> {
        match (
            self.get_current_min_volume(network, state),
            self.get_current_max_volume(network, state),
        ) {
            (Ok(min_vol), Ok(max_vol)) => Ok((min_vol, max_vol)),
            _ => Err(PywrError::FlowConstraintsUndefined),
        }
    }

    pub fn get_current_available_volume_bounds(
        &self,
        network: &Network,
        state: &State,
    ) -> Result<(f64, f64), PywrError> {
        match (
            self.get_current_min_volume(network, state),
            self.get_current_max_volume(network, state),
        ) {
            (Ok(min_vol), Ok(max_vol)) => {
                let current_volume = state.get_network_state().get_node_volume(&self.index())?;

                let available = current_volume - min_vol;
                let missing = max_vol - current_volume;

                Ok((available, missing))
            }
            _ => Err(PywrError::FlowConstraintsUndefined),
        }
    }

    pub fn set_cost(&mut self, value: ConstraintValue) {
        match self {
            Self::Input(n) => n.set_cost(value),
            Self::Link(n) => n.set_cost(value),
            Self::Output(n) => n.set_cost(value),
            Self::Storage(n) => n.set_cost(value),
        }
    }

    pub fn set_cost_agg_func(&mut self, agg_func: CostAggFunc) -> Result<(), PywrError> {
        match self {
            Self::Input(n) => n.set_cost_agg_func(agg_func),
            Self::Link(n) => n.set_cost_agg_func(agg_func),
            Self::Output(n) => n.set_cost_agg_func(agg_func),
            Self::Storage(_) => return Err(PywrError::NoVirtualStorageOnStorageNode),
        };

        Ok(())
    }

    pub fn get_outgoing_cost(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        match self {
            Self::Input(n) => n.get_cost(network, state),
            Self::Link(n) => Ok(n.get_cost(network, state)? / 2.0),
            Self::Output(n) => n.get_cost(network, state),
            Self::Storage(n) => Ok(-n.get_cost(network, state)?),
        }
    }

    pub fn get_incoming_cost(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        match self {
            Self::Input(n) => n.get_cost(network, state),
            Self::Link(n) => Ok(n.get_cost(network, state)? / 2.0),
            Self::Output(n) => n.get_cost(network, state),
            Self::Storage(n) => n.get_cost(network, state),
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

#[derive(Debug, PartialEq)]
pub struct FlowConstraints {
    pub(crate) min_flow: ConstraintValue,
    pub(crate) max_flow: ConstraintValue,
}

impl FlowConstraints {
    pub(crate) fn new() -> Self {
        Self {
            min_flow: ConstraintValue::None,
            max_flow: ConstraintValue::None,
        }
    }
    /// Return the current minimum flow from the parameter state
    ///
    /// Defaults to zero if no parameter is defined.
    pub(crate) fn get_min_flow(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        match &self.min_flow {
            ConstraintValue::None => Ok(0.0),
            ConstraintValue::Scalar(v) => Ok(*v),
            ConstraintValue::Metric(m) => m.get_value(network, state),
        }
    }
    /// Return the current maximum flow from the parameter state
    ///
    /// Defaults to f64::MAX if no parameter is defined.
    pub(crate) fn get_max_flow(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        match &self.max_flow {
            ConstraintValue::None => Ok(f64::MAX), // TODO should this return infinity?
            ConstraintValue::Scalar(v) => Ok(*v),
            ConstraintValue::Metric(m) => m.get_value(network, state),
        }
    }

    pub(crate) fn is_max_flow_unconstrained(&self) -> bool {
        self.max_flow == ConstraintValue::None
    }
}

#[derive(Debug, PartialEq)]
pub struct StorageConstraints {
    pub(crate) min_volume: ConstraintValue,
    pub(crate) max_volume: ConstraintValue,
}

impl StorageConstraints {
    pub fn new(min_volume: ConstraintValue, max_volume: ConstraintValue) -> Self {
        Self { min_volume, max_volume }
    }
    /// Return the current minimum volume from the parameter state
    ///
    /// Defaults to zero if no parameter is defined.
    pub fn get_min_volume(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        match &self.min_volume {
            ConstraintValue::None => Ok(f64::MAX),
            ConstraintValue::Scalar(v) => Ok(*v),
            ConstraintValue::Metric(m) => m.get_value(network, state),
        }
    }
    /// Return the current maximum volume from the metric state
    ///
    /// Defaults to f64::MAX if no parameter is defined.
    pub fn get_max_volume(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        match &self.max_volume {
            ConstraintValue::None => Ok(f64::MAX),
            ConstraintValue::Scalar(v) => Ok(*v),
            ConstraintValue::Metric(m) => m.get_value(network, state),
        }
    }
}

/// Generic cost data for a node.
#[derive(Debug, PartialEq)]
struct NodeCost {
    local: ConstraintValue,
    virtual_storage_nodes: Vec<VirtualStorageIndex>,
    agg_func: CostAggFunc,
}

impl Default for NodeCost {
    fn default() -> Self {
        Self {
            local: ConstraintValue::None,
            virtual_storage_nodes: Vec::new(),
            agg_func: CostAggFunc::Max,
        }
    }
}

impl NodeCost {
    fn get_cost(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        let local_cost = match &self.local {
            ConstraintValue::None => Ok(0.0),
            ConstraintValue::Scalar(v) => Ok(*v),
            ConstraintValue::Metric(m) => m.get_value(network, state),
        }?;

        let vs_costs: Vec<f64> = self
            .virtual_storage_nodes
            .iter()
            .map(|idx| {
                let vs = network.get_virtual_storage_node(idx)?;
                vs.get_cost(network, state)
            })
            .collect::<Result<_, _>>()?;

        let cost = match self.agg_func {
            CostAggFunc::Sum => local_cost + vs_costs.iter().sum::<f64>(),
            CostAggFunc::Max => local_cost.max(
                vs_costs
                    .into_iter()
                    .max_by(|a, b| a.total_cmp(b))
                    .unwrap_or(f64::NEG_INFINITY),
            ),
            CostAggFunc::Min => local_cost.min(
                vs_costs
                    .into_iter()
                    .min_by(|a, b| a.total_cmp(b))
                    .unwrap_or(f64::INFINITY),
            ),
        };

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
            flow_constraints: FlowConstraints::new(),
            outgoing_edges: Vec::new(),
        }
    }
    fn set_cost(&mut self, value: ConstraintValue) {
        self.cost.local = value
    }
    fn set_cost_agg_func(&mut self, agg_func: CostAggFunc) {
        self.cost.agg_func = agg_func
    }
    fn get_cost(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        self.cost.get_cost(network, state)
    }
    fn set_min_flow(&mut self, value: ConstraintValue) {
        self.flow_constraints.min_flow = value;
    }
    fn get_min_flow(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        self.flow_constraints.get_min_flow(network, state)
    }
    fn set_max_flow(&mut self, value: ConstraintValue) {
        self.flow_constraints.max_flow = value;
    }
    fn get_max_flow(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        self.flow_constraints.get_max_flow(network, state)
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
            flow_constraints: FlowConstraints::new(),
            incoming_edges: Vec::new(),
        }
    }
    fn set_cost(&mut self, value: ConstraintValue) {
        self.cost.local = value
    }
    fn get_cost(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        self.cost.get_cost(network, state)
    }
    fn set_cost_agg_func(&mut self, agg_func: CostAggFunc) {
        self.cost.agg_func = agg_func
    }
    fn set_min_flow(&mut self, value: ConstraintValue) {
        self.flow_constraints.min_flow = value;
    }
    fn get_min_flow(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        self.flow_constraints.get_min_flow(network, state)
    }
    fn set_max_flow(&mut self, value: ConstraintValue) {
        self.flow_constraints.max_flow = value;
    }
    fn get_max_flow(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        self.flow_constraints.get_max_flow(network, state)
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
            flow_constraints: FlowConstraints::new(),
            incoming_edges: Vec::new(),
            outgoing_edges: Vec::new(),
        }
    }
    fn set_cost(&mut self, value: ConstraintValue) {
        self.cost.local = value
    }
    fn set_cost_agg_func(&mut self, agg_func: CostAggFunc) {
        self.cost.agg_func = agg_func
    }
    fn get_cost(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        self.cost.get_cost(network, state)
    }
    fn set_min_flow(&mut self, value: ConstraintValue) {
        self.flow_constraints.min_flow = value;
    }
    fn get_min_flow(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        self.flow_constraints.get_min_flow(network, state)
    }
    fn set_max_flow(&mut self, value: ConstraintValue) {
        self.flow_constraints.max_flow = value;
    }
    fn get_max_flow(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        self.flow_constraints.get_max_flow(network, state)
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

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum StorageInitialVolume {
    Absolute(f64),
    Proportional(f64),
}

#[derive(Debug, PartialEq)]
pub struct StorageNode {
    pub meta: NodeMeta<NodeIndex>,
    pub cost: ConstraintValue,
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
        min_volume: ConstraintValue,
        max_volume: ConstraintValue,
    ) -> Self {
        Self {
            meta: NodeMeta::new(index, name, sub_name),
            cost: ConstraintValue::None,
            initial_volume,
            storage_constraints: StorageConstraints::new(min_volume, max_volume),
            incoming_edges: Vec::new(),
            outgoing_edges: Vec::new(),
        }
    }

    pub fn before(&self, timestep: &Timestep, network: &Network, state: &mut State) -> Result<(), PywrError> {
        // Set the initial volume if it is the first timestep.
        if timestep.is_first() {
            let volume = match &self.initial_volume {
                StorageInitialVolume::Absolute(iv) => *iv,
                StorageInitialVolume::Proportional(ipc) => {
                    let max_volume = self.get_max_volume(network, state)?;
                    max_volume * ipc
                }
            };

            state.set_node_volume(self.meta.index, volume)?;
        }
        Ok(())
    }

    fn set_cost(&mut self, value: ConstraintValue) {
        self.cost = value
    }
    fn get_cost(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        match &self.cost {
            ConstraintValue::None => Ok(0.0),
            ConstraintValue::Scalar(v) => Ok(*v),
            ConstraintValue::Metric(m) => m.get_value(network, state),
        }
    }
    fn set_min_volume(&mut self, value: ConstraintValue) {
        // TODO use a set_min_volume method
        self.storage_constraints.min_volume = value;
    }
    fn get_min_volume(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        self.storage_constraints.get_min_volume(network, state)
    }
    fn set_max_volume(&mut self, value: ConstraintValue) {
        // TODO use a set_min_volume method
        self.storage_constraints.max_volume = value;
    }
    fn get_max_volume(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        self.storage_constraints.get_max_volume(network, state)
    }
    fn add_incoming_edge(&mut self, edge: EdgeIndex) {
        self.incoming_edges.push(edge);
    }
    fn add_outgoing_edge(&mut self, edge: EdgeIndex) {
        self.outgoing_edges.push(edge);
    }
}
