use crate::metric::Metric;
use crate::parameters::Parameter;
use crate::state::{NetworkState, NodeState, ParameterState};
use crate::{Edge, PywrError};
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

pub enum NodeType {
    Input,
    Output,
    Link,
    Storage,
}

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
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }
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

    pub fn push_new_storage(&mut self, name: &str, sub_name: Option<&str>, initial_volume: f64) -> NodeIndex {
        let node_index = NodeIndex(self.nodes.len());
        let node = Node::new_storage(&node_index, name, sub_name, initial_volume);
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
    Parameter(Parameter),
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
    pub fn new_storage(node_index: &NodeIndex, name: &str, sub_name: Option<&str>, initial_volume: f64) -> Self {
        Self::Storage(StorageNode::new(node_index, name, sub_name, initial_volume))
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

    pub fn new_state(&self) -> NodeState {
        // TODO add a reference to the node in the state objects?

        match self {
            Self::Input(_n) => NodeState::new_flow_state(),
            Self::Output(_n) => NodeState::new_flow_state(),
            Self::Link(_n) => NodeState::new_flow_state(),
            // TODO fix initial proportional volume!!!
            Self::Storage(n) => NodeState::new_storage_state(n.initial_volume),
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

    pub fn add_incoming_edge(&mut self, edge: Edge) -> Result<(), PywrError> {
        match self {
            Self::Input(_n) => Err(PywrError::InvalidNodeConnectionToInput),
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

    pub fn add_outgoing_edge(&mut self, edge: Edge) -> Result<(), PywrError> {
        match self {
            Self::Input(n) => {
                n.add_outgoing_edge(edge);
                Ok(())
            }
            Self::Output(_n) => Err(PywrError::InvalidNodeConnectionFromOutput),
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

    pub fn get_incoming_edges(&self) -> Result<Vec<Edge>, PywrError> {
        match self {
            Self::Input(_n) => Err(PywrError::InvalidNodeConnectionToInput), // TODO better error
            Self::Output(n) => Ok(n.incoming_edges.clone()),
            Self::Link(n) => Ok(n.incoming_edges.clone()),
            Self::Storage(n) => Ok(n.incoming_edges.clone()),
        }
    }

    pub fn get_outgoing_edges(&self) -> Result<Vec<Edge>, PywrError> {
        match self {
            Self::Input(n) => Ok(n.outgoing_edges.clone()),
            Self::Output(_n) => Err(PywrError::InvalidNodeConnectionFromOutput), // TODO better error
            Self::Link(n) => Ok(n.outgoing_edges.clone()),
            Self::Storage(n) => Ok(n.outgoing_edges.clone()),
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

    /// Set a constraint on a node.
    pub fn set_constraint(&mut self, value: ConstraintValue, constraint: Constraint) -> Result<(), PywrError> {
        match constraint {
            Constraint::MinFlow => self.set_min_flow_constraint(value)?,
            Constraint::MaxFlow => self.set_max_flow_constraint(value)?,
            Constraint::MinAndMaxFlow => {
                self.set_min_flow_constraint(value.clone())?;
                self.set_max_flow_constraint(value)?;
            }
            Constraint::MinVolume => match value {
                ConstraintValue::Scalar(v) => self.set_min_volume_constraint(v)?,
                _ => {
                    return Err(PywrError::InvalidConstraintValue(
                        "min_volume must be a scalar!".to_string(),
                    ))
                }
            },
            Constraint::MaxVolume => match value {
                ConstraintValue::Scalar(v) => self.set_max_volume_constraint(v)?,
                _ => {
                    return Err(PywrError::InvalidConstraintValue(
                        "max_volume must be a scalar!".to_string(),
                    ))
                }
            },
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

    pub fn get_current_min_flow(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        match self {
            Self::Input(n) => n.get_min_flow(parameter_states),
            Self::Link(n) => n.get_min_flow(parameter_states),
            Self::Output(n) => n.get_min_flow(parameter_states),
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

    pub fn get_current_max_flow(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        match self {
            Self::Input(n) => n.get_max_flow(parameter_states),
            Self::Link(n) => n.get_max_flow(parameter_states),
            Self::Output(n) => n.get_max_flow(parameter_states),
            Self::Storage(_) => Err(PywrError::FlowConstraintsUndefined),
        }
    }

    pub fn get_current_flow_bounds(&self, parameter_states: &ParameterState) -> Result<(f64, f64), PywrError> {
        match (
            self.get_current_min_flow(parameter_states),
            self.get_current_max_flow(parameter_states),
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

    pub fn get_current_min_volume(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        match self {
            Self::Input(_) => Err(PywrError::StorageConstraintsUndefined),
            Self::Link(_) => Err(PywrError::StorageConstraintsUndefined),
            Self::Output(_) => Err(PywrError::StorageConstraintsUndefined),
            Self::Storage(n) => n.get_min_volume(parameter_states),
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

    pub fn get_current_max_volume(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        match self {
            Self::Input(_) => Err(PywrError::StorageConstraintsUndefined),
            Self::Link(_) => Err(PywrError::StorageConstraintsUndefined),
            Self::Output(_) => Err(PywrError::StorageConstraintsUndefined),
            Self::Storage(n) => n.get_max_volume(parameter_states),
        }
    }

    pub fn get_current_volume_bounds(&self) -> Result<(f64, f64), PywrError> {
        match (self.get_current_min_volume(), self.get_current_max_volume()) {
            (Ok(min_vol), Ok(max_vol)) => Ok((min_vol, max_vol)),
            _ => Err(PywrError::FlowConstraintsUndefined),
        }
    }

    pub fn get_current_available_volume_bounds(&self, network_state: &NetworkState) -> Result<(f64, f64), PywrError> {
        match (self.get_current_min_volume(), self.get_current_max_volume()) {
            (Ok(min_vol), Ok(max_vol)) => {
                let current_volume = network_state.get_node_volume(&self.index())?;

                let available = (current_volume - min_vol).max(0.0);
                let missing = (max_vol - current_volume).max(0.0);

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

    pub fn get_outgoing_cost(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        match self {
            Self::Input(n) => n.get_cost(parameter_states),
            Self::Link(n) => Ok(n.get_cost(parameter_states)? / 2.0),
            Self::Output(n) => n.get_cost(parameter_states),
            Self::Storage(n) => Ok(-n.get_cost(parameter_states)?),
        }
    }

    pub fn get_incoming_cost(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        match self {
            Self::Input(n) => n.get_cost(parameter_states),
            Self::Link(n) => Ok(n.get_cost(parameter_states)? / 2.0),
            Self::Output(n) => n.get_cost(parameter_states),
            Self::Storage(n) => n.get_cost(parameter_states),
        }
    }
}

/// Meta data common to all nodes.
#[derive(Debug, PartialEq)]
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
    pub(crate) fn get_min_flow(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        match &self.min_flow {
            ConstraintValue::None => Ok(0.0),
            ConstraintValue::Scalar(v) => Ok(*v),
            ConstraintValue::Parameter(p) => parameter_states.get_value(p.index()),
        }
    }
    /// Return the current maximum flow from the parameter state
    ///
    /// Defaults to f64::MAX if no parameter is defined.
    pub(crate) fn get_max_flow(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        match &self.max_flow {
            ConstraintValue::None => Ok(f64::MAX), // TODO should this return infinity?
            ConstraintValue::Scalar(v) => Ok(*v),
            ConstraintValue::Parameter(p) => parameter_states.get_value(p.index()),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct StorageConstraints {
    pub(crate) min_volume: f64,
    pub(crate) max_volume: f64, // TODO Should this be required (i.e. not an Option)
}

impl StorageConstraints {
    fn new() -> Self {
        Self {
            min_volume: 0.0,
            max_volume: f64::MAX,
        }
    }
    /// Return the current minimum volume from the parameter state
    ///
    /// Defaults to zero if no parameter is defined.
    fn get_min_volume(&self) -> f64 {
        self.min_volume
    }
    /// Return the current maximum volume from the parameter state
    ///
    /// Defaults to f64::MAX if no parameter is defined.
    fn get_max_volume(&self) -> f64 {
        self.max_volume
    }
}

#[derive(Debug, PartialEq)]
pub struct InputNode {
    pub meta: NodeMeta<NodeIndex>,
    pub cost: ConstraintValue,
    pub flow_constraints: FlowConstraints,
    pub outgoing_edges: Vec<Edge>,
}

impl InputNode {
    fn new(index: &NodeIndex, name: &str, sub_name: Option<&str>) -> Self {
        Self {
            meta: NodeMeta::new(index, name, sub_name),
            cost: ConstraintValue::None,
            flow_constraints: FlowConstraints::new(),
            outgoing_edges: Vec::new(),
        }
    }
    fn set_cost(&mut self, value: ConstraintValue) {
        self.cost = value
    }
    fn get_cost(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        match &self.cost {
            ConstraintValue::None => Ok(0.0),
            ConstraintValue::Scalar(v) => Ok(*v),
            ConstraintValue::Parameter(p) => parameter_states.get_value(p.index()),
        }
    }
    fn set_min_flow(&mut self, value: ConstraintValue) {
        self.flow_constraints.min_flow = value;
    }
    fn get_min_flow(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        self.flow_constraints.get_min_flow(parameter_states)
    }
    fn set_max_flow(&mut self, value: ConstraintValue) {
        self.flow_constraints.max_flow = value;
    }
    fn get_max_flow(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        self.flow_constraints.get_max_flow(parameter_states)
    }
    fn add_outgoing_edge(&mut self, edge: Edge) {
        self.outgoing_edges.push(edge);
    }
}

#[derive(Debug, PartialEq)]
pub struct OutputNode {
    pub meta: NodeMeta<NodeIndex>,
    pub cost: ConstraintValue,
    pub flow_constraints: FlowConstraints,
    pub incoming_edges: Vec<Edge>,
}

impl OutputNode {
    fn new(index: &NodeIndex, name: &str, sub_name: Option<&str>) -> Self {
        Self {
            meta: NodeMeta::new(index, name, sub_name),
            cost: ConstraintValue::None,
            flow_constraints: FlowConstraints::new(),
            incoming_edges: Vec::new(),
        }
    }
    fn set_cost(&mut self, value: ConstraintValue) {
        self.cost = value
    }
    fn get_cost(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        match &self.cost {
            ConstraintValue::None => Ok(0.0),
            ConstraintValue::Scalar(v) => Ok(*v),
            ConstraintValue::Parameter(p) => parameter_states.get_value(p.index()),
        }
    }
    fn set_min_flow(&mut self, value: ConstraintValue) {
        self.flow_constraints.min_flow = value;
    }
    fn get_min_flow(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        self.flow_constraints.get_min_flow(parameter_states)
    }
    fn set_max_flow(&mut self, value: ConstraintValue) {
        self.flow_constraints.max_flow = value;
    }
    fn get_max_flow(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        self.flow_constraints.get_max_flow(parameter_states)
    }
    fn add_incoming_edge(&mut self, edge: Edge) {
        self.incoming_edges.push(edge);
    }
}

#[derive(Debug, PartialEq)]
pub struct LinkNode {
    pub meta: NodeMeta<NodeIndex>,
    pub cost: ConstraintValue,
    pub flow_constraints: FlowConstraints,
    pub incoming_edges: Vec<Edge>,
    pub outgoing_edges: Vec<Edge>,
}

impl LinkNode {
    fn new(index: &NodeIndex, name: &str, sub_name: Option<&str>) -> Self {
        Self {
            meta: NodeMeta::new(index, name, sub_name),
            cost: ConstraintValue::None,
            flow_constraints: FlowConstraints::new(),
            incoming_edges: Vec::new(),
            outgoing_edges: Vec::new(),
        }
    }
    fn set_cost(&mut self, value: ConstraintValue) {
        self.cost = value
    }
    fn get_cost(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        match &self.cost {
            ConstraintValue::None => Ok(0.0),
            ConstraintValue::Scalar(v) => Ok(*v),
            ConstraintValue::Parameter(p) => parameter_states.get_value(p.index()),
        }
    }
    fn set_min_flow(&mut self, value: ConstraintValue) {
        self.flow_constraints.min_flow = value;
    }
    fn get_min_flow(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        self.flow_constraints.get_min_flow(parameter_states)
    }
    fn set_max_flow(&mut self, value: ConstraintValue) {
        self.flow_constraints.max_flow = value;
    }
    fn get_max_flow(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        self.flow_constraints.get_max_flow(parameter_states)
    }

    fn add_incoming_edge(&mut self, edge: Edge) {
        self.incoming_edges.push(edge);
    }
    fn add_outgoing_edge(&mut self, edge: Edge) {
        self.outgoing_edges.push(edge);
    }
}

#[derive(Debug, PartialEq)]
pub struct StorageNode {
    pub meta: NodeMeta<NodeIndex>,
    pub cost: ConstraintValue,
    pub initial_volume: f64,
    pub storage_constraints: StorageConstraints,
    pub incoming_edges: Vec<Edge>,
    pub outgoing_edges: Vec<Edge>,
}

impl StorageNode {
    fn new(index: &NodeIndex, name: &str, sub_name: Option<&str>, initial_volume: f64) -> Self {
        Self {
            meta: NodeMeta::new(index, name, sub_name),
            cost: ConstraintValue::None,
            initial_volume,
            storage_constraints: StorageConstraints::new(),
            incoming_edges: Vec::new(),
            outgoing_edges: Vec::new(),
        }
    }
    fn set_cost(&mut self, value: ConstraintValue) {
        self.cost = value
    }
    fn get_cost(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        match &self.cost {
            ConstraintValue::None => Ok(0.0),
            ConstraintValue::Scalar(v) => Ok(*v),
            ConstraintValue::Parameter(p) => parameter_states.get_value(p.index()),
        }
    }
    fn set_min_volume(&mut self, value: f64) {
        // TODO use a set_min_volume method
        self.storage_constraints.min_volume = value;
    }
    fn get_min_volume(&self) -> f64 {
        self.storage_constraints.get_min_volume()
    }
    fn set_max_volume(&mut self, value: f64) {
        // TODO use a set_min_volume method
        self.storage_constraints.max_volume = value;
    }
    fn get_max_volume(&self) -> f64 {
        self.storage_constraints.get_max_volume()
    }
    fn add_incoming_edge(&mut self, edge: Edge) {
        self.incoming_edges.push(edge);
    }
    fn add_outgoing_edge(&mut self, edge: Edge) {
        self.outgoing_edges.push(edge);
    }
}
