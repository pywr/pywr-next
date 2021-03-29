use crate::metric::Metric;
use crate::parameters::{Parameter, ParameterIndex};
use crate::state::{NetworkState, NodeState, ParameterState};
use crate::{Edge, PywrError};
use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

pub type NodeIndex = usize;
pub type NodeRef = Rc<RefCell<_Node>>;

#[derive(Debug, PartialEq)]
pub enum _Node {
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

#[derive(Debug, Clone, PartialEq)]
pub struct Node(NodeRef);

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
    pub fn new_input(node_index: &NodeIndex, name: &str) -> Self {
        let node = _Node::Input(InputNode::new(node_index, name));
        Node(Rc::new(RefCell::new(node)))
    }

    /// Create a new output node
    pub fn new_output(node_index: &NodeIndex, name: &str) -> Self {
        let node = _Node::Output(OutputNode::new(node_index, name));
        Node(Rc::new(RefCell::new(node)))
    }

    /// Create a new link node
    pub fn new_link(node_index: &NodeIndex, name: &str) -> Self {
        let node = _Node::Link(LinkNode::new(node_index, name));
        Node(Rc::new(RefCell::new(node)))
    }

    /// Create a new storage node
    pub fn new_storage(node_index: &NodeIndex, name: &str, initial_volume: f64) -> Self {
        let node = _Node::Storage(StorageNode::new(node_index, name, initial_volume));
        Node(Rc::new(RefCell::new(node)))
    }

    /// Get a node's name
    pub fn name(&self) -> String {
        match self.0.borrow().deref() {
            _Node::Input(n) => n.meta.name.clone(),
            _Node::Output(n) => n.meta.name.clone(),
            _Node::Link(n) => n.meta.name.clone(),
            _Node::Storage(n) => n.meta.name.clone(),
        }
    }

    /// Get a node's name
    pub fn index(&self) -> NodeIndex {
        match self.0.borrow().deref() {
            _Node::Input(n) => n.meta.index,
            _Node::Output(n) => n.meta.index,
            _Node::Link(n) => n.meta.index,
            _Node::Storage(n) => n.meta.index,
        }
    }

    pub fn node_type(&self) -> NodeType {
        match self.0.borrow().deref() {
            _Node::Input(_) => NodeType::Input,
            _Node::Output(_) => NodeType::Output,
            _Node::Link(_) => NodeType::Link,
            _Node::Storage(_) => NodeType::Storage,
        }
    }

    pub fn apply<F>(&self, f: F)
    where
        F: Fn(&_Node),
    {
        f(self.0.borrow().deref());
    }

    pub fn new_state(&self) -> NodeState {
        // TODO add a reference to the node in the state objects?
        match self.0.borrow().deref() {
            _Node::Input(_n) => NodeState::new_flow_state(),
            _Node::Output(_n) => NodeState::new_flow_state(),
            _Node::Link(_n) => NodeState::new_flow_state(),
            _Node::Storage(n) => NodeState::new_storage_state(n.initial_volume),
        }
    }

    pub fn default_metric(&self) -> Metric {
        match self.0.borrow().deref() {
            _Node::Input(_n) => Metric::NodeOutFlow(self.index()),
            _Node::Output(_n) => Metric::NodeInFlow(self.index()),
            _Node::Link(_n) => Metric::NodeOutFlow(self.index()),
            _Node::Storage(_n) => Metric::NodeVolume(self.index()),
        }
    }

    pub fn add_incoming_edge(&self, edge: Edge) -> Result<(), PywrError> {
        match self.0.borrow_mut().deref_mut() {
            _Node::Input(_n) => Err(PywrError::InvalidNodeConnectionToInput),
            _Node::Output(n) => Ok(n.add_incoming_edge(edge)),
            _Node::Link(n) => Ok(n.add_incoming_edge(edge)),
            _Node::Storage(n) => Ok(n.add_incoming_edge(edge)),
        }
    }

    pub fn add_outgoing_edge(&self, edge: Edge) -> Result<(), PywrError> {
        match self.0.borrow_mut().deref_mut() {
            _Node::Input(n) => Ok(n.add_outgoing_edge(edge)),
            _Node::Output(_n) => Err(PywrError::InvalidNodeConnectionFromOutput),
            _Node::Link(n) => Ok(n.add_outgoing_edge(edge)),
            _Node::Storage(n) => Ok(n.add_outgoing_edge(edge)),
        }
    }

    pub fn get_incoming_edges(&self) -> Result<Vec<Edge>, PywrError> {
        match self.0.borrow().deref() {
            _Node::Input(_n) => Err(PywrError::InvalidNodeConnectionToInput), // TODO better error
            _Node::Output(n) => Ok(n.incoming_edges.clone()),
            _Node::Link(n) => Ok(n.incoming_edges.clone()),
            _Node::Storage(n) => Ok(n.incoming_edges.clone()),
        }
    }

    pub fn get_outgoing_edges(&self) -> Result<Vec<Edge>, PywrError> {
        match self.0.borrow().deref() {
            _Node::Input(n) => Ok(n.outgoing_edges.clone()),
            _Node::Output(_n) => Err(PywrError::InvalidNodeConnectionFromOutput), // TODO better error
            _Node::Link(n) => Ok(n.outgoing_edges.clone()),
            _Node::Storage(n) => Ok(n.outgoing_edges.clone()),
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
    pub fn set_constraint(&self, value: ConstraintValue, constraint: Constraint) -> Result<(), PywrError> {
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

    pub fn set_min_flow_constraint(&self, value: ConstraintValue) -> Result<(), PywrError> {
        match self.0.borrow_mut().deref_mut() {
            _Node::Input(n) => Ok(n.set_min_flow(value)),
            _Node::Link(n) => Ok(n.set_min_flow(value)),
            _Node::Output(n) => Ok(n.set_min_flow(value)),
            _Node::Storage(_) => Err(PywrError::FlowConstraintsUndefined),
        }
    }

    pub fn get_current_min_flow(&self, parameter_states: &[f64]) -> Result<f64, PywrError> {
        match self.0.borrow().deref() {
            _Node::Input(n) => Ok(n.get_min_flow(parameter_states)),
            _Node::Link(n) => Ok(n.get_min_flow(parameter_states)),
            _Node::Output(n) => Ok(n.get_min_flow(parameter_states)),
            _Node::Storage(_) => Err(PywrError::FlowConstraintsUndefined),
        }
    }

    pub fn set_max_flow_constraint(&self, value: ConstraintValue) -> Result<(), PywrError> {
        match self.0.borrow_mut().deref_mut() {
            _Node::Input(n) => Ok(n.set_max_flow(value)),
            _Node::Link(n) => Ok(n.set_max_flow(value)),
            _Node::Output(n) => Ok(n.set_max_flow(value)),
            _Node::Storage(_) => Err(PywrError::FlowConstraintsUndefined),
        }
    }

    pub fn get_current_max_flow(&self, parameter_states: &[f64]) -> Result<f64, PywrError> {
        match self.0.borrow().deref() {
            _Node::Input(n) => Ok(n.get_max_flow(parameter_states)),
            _Node::Link(n) => Ok(n.get_max_flow(parameter_states)),
            _Node::Output(n) => Ok(n.get_max_flow(parameter_states)),
            _Node::Storage(_) => Err(PywrError::FlowConstraintsUndefined),
        }
    }

    pub fn get_current_flow_bounds(&self, parameter_states: &[f64]) -> Result<(f64, f64), PywrError> {
        match (
            self.get_current_min_flow(parameter_states),
            self.get_current_max_flow(parameter_states),
        ) {
            (Ok(min_flow), Ok(max_flow)) => Ok((min_flow, max_flow)),
            _ => Err(PywrError::FlowConstraintsUndefined),
        }
    }

    pub fn set_min_volume_constraint(&self, value: ConstraintValue) -> Result<(), PywrError> {
        match self.0.borrow_mut().deref_mut() {
            _Node::Input(_) => Err(PywrError::StorageConstraintsUndefined),
            _Node::Link(_) => Err(PywrError::StorageConstraintsUndefined),
            _Node::Output(_) => Err(PywrError::StorageConstraintsUndefined),
            _Node::Storage(n) => Ok(n.set_min_volume(value)),
        }
    }

    pub fn get_current_min_volume(&self, parameter_states: &[f64]) -> Result<f64, PywrError> {
        match self.0.borrow().deref() {
            _Node::Input(_) => Err(PywrError::StorageConstraintsUndefined),
            _Node::Link(_) => Err(PywrError::StorageConstraintsUndefined),
            _Node::Output(_) => Err(PywrError::StorageConstraintsUndefined),
            _Node::Storage(n) => Ok(n.get_min_volume(parameter_states)),
        }
    }

    pub fn set_max_volume_constraint(&self, value: ConstraintValue) -> Result<(), PywrError> {
        match self.0.borrow_mut().deref_mut() {
            _Node::Input(_) => Err(PywrError::StorageConstraintsUndefined),
            _Node::Link(_) => Err(PywrError::StorageConstraintsUndefined),
            _Node::Output(_) => Err(PywrError::StorageConstraintsUndefined),
            _Node::Storage(n) => Ok(n.set_max_volume(value)),
        }
    }

    pub fn get_current_max_volume(&self, parameter_states: &[f64]) -> Result<f64, PywrError> {
        match self.0.borrow().deref() {
            _Node::Input(_) => Err(PywrError::StorageConstraintsUndefined),
            _Node::Link(_) => Err(PywrError::StorageConstraintsUndefined),
            _Node::Output(_) => Err(PywrError::StorageConstraintsUndefined),
            _Node::Storage(n) => Ok(n.get_max_volume(parameter_states)),
        }
    }

    pub fn get_current_volume_bounds(&self, parameter_states: &[f64]) -> Result<(f64, f64), PywrError> {
        match (
            self.get_current_min_volume(parameter_states),
            self.get_current_max_volume(parameter_states),
        ) {
            (Ok(min_vol), Ok(max_vol)) => Ok((min_vol, max_vol)),
            _ => Err(PywrError::FlowConstraintsUndefined),
        }
    }

    pub fn get_current_available_volume_bounds(
        &self,
        network_state: &NetworkState,
        parameter_states: &[f64],
    ) -> Result<(f64, f64), PywrError> {
        match (
            self.get_current_min_volume(parameter_states),
            self.get_current_max_volume(parameter_states),
        ) {
            (Ok(min_vol), Ok(max_vol)) => {
                let current_volume = network_state.get_node_volume(self.index())?;

                let available = (current_volume - min_vol).max(0.0);
                let missing = (max_vol - current_volume).max(0.0);

                Ok((available, missing))
            }
            _ => Err(PywrError::FlowConstraintsUndefined),
        }
    }

    pub fn set_cost(&self, value: ConstraintValue) {
        match self.0.borrow_mut().deref_mut() {
            _Node::Input(n) => n.set_cost(value),
            _Node::Link(n) => n.set_cost(value),
            _Node::Output(n) => n.set_cost(value),
            _Node::Storage(n) => n.set_cost(value),
        }
    }

    pub fn get_outgoing_cost(&self, parameter_states: &[f64]) -> f64 {
        match self.0.borrow().deref() {
            _Node::Input(n) => n.get_cost(parameter_states),
            _Node::Link(n) => n.get_cost(parameter_states) / 2.0,
            _Node::Output(n) => n.get_cost(parameter_states),
            _Node::Storage(n) => -n.get_cost(parameter_states),
        }
    }

    pub fn get_incoming_cost(&self, parameter_states: &[f64]) -> f64 {
        match self.0.borrow().deref() {
            _Node::Input(n) => n.get_cost(parameter_states),
            _Node::Link(n) => n.get_cost(parameter_states) / 2.0,
            _Node::Output(n) => n.get_cost(parameter_states),
            _Node::Storage(n) => n.get_cost(parameter_states),
        }
    }
}

/// Meta data common to all nodes.
#[derive(Debug, PartialEq)]
pub struct NodeMeta {
    pub(crate) index: NodeIndex,
    name: String,
    comment: String,
}

impl NodeMeta {
    fn new(index: &NodeIndex, name: &str) -> Self {
        Self {
            index: *index,
            name: name.to_string(),
            comment: "".to_string(),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct FlowConstraints {
    pub(crate) min_flow: ConstraintValue,
    pub(crate) max_flow: ConstraintValue,
}

impl FlowConstraints {
    fn new() -> Self {
        Self {
            min_flow: ConstraintValue::None,
            max_flow: ConstraintValue::None,
        }
    }
    /// Return the current minimum flow from the parameter state
    ///
    /// Defaults to zero if no parameter is defined.
    fn get_min_flow(&self, parameter_states: &[f64]) -> f64 {
        match &self.min_flow {
            ConstraintValue::None => 0.0,
            ConstraintValue::Scalar(v) => *v,
            ConstraintValue::Parameter(p) => parameter_states[p.index()],
        }
    }
    /// Return the current maximum flow from the parameter state
    ///
    /// Defaults to f64::MAX if no parameter is defined.
    fn get_max_flow(&self, parameter_states: &[f64]) -> f64 {
        match &self.max_flow {
            ConstraintValue::None => f64::MAX, // TODO should this return infinity?
            ConstraintValue::Scalar(v) => *v,
            ConstraintValue::Parameter(p) => parameter_states[p.index()],
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct StorageConstraints {
    pub(crate) min_volume: ConstraintValue,
    pub(crate) max_volume: ConstraintValue, // TODO Should this be required (i.e. not an Option)
}

impl StorageConstraints {
    fn new() -> Self {
        Self {
            min_volume: ConstraintValue::None,
            max_volume: ConstraintValue::None,
        }
    }
    /// Return the current minimum volume from the parameter state
    ///
    /// Defaults to zero if no parameter is defined.
    fn get_min_volume(&self, parameter_states: &[f64]) -> f64 {
        match &self.min_volume {
            ConstraintValue::None => 0.0,
            ConstraintValue::Scalar(v) => *v,
            ConstraintValue::Parameter(p) => parameter_states[p.index()],
        }
    }
    /// Return the current maximum volume from the parameter state
    ///
    /// Defaults to f64::MAX if no parameter is defined.
    fn get_max_volume(&self, parameter_states: &[f64]) -> f64 {
        match &self.max_volume {
            ConstraintValue::None => f64::MAX, // TODO should this return infinity?
            ConstraintValue::Scalar(v) => *v,
            ConstraintValue::Parameter(p) => parameter_states[p.index()],
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct InputNode {
    pub meta: NodeMeta,
    pub cost: ConstraintValue,
    pub flow_constraints: FlowConstraints,
    pub outgoing_edges: Vec<Edge>,
}

impl InputNode {
    fn new(index: &NodeIndex, name: &str) -> Self {
        Self {
            meta: NodeMeta::new(index, name),
            cost: ConstraintValue::None,
            flow_constraints: FlowConstraints::new(),
            outgoing_edges: Vec::new(),
        }
    }
    fn set_cost(&mut self, value: ConstraintValue) {
        self.cost = value
    }
    fn get_cost(&self, parameter_states: &[f64]) -> f64 {
        match &self.cost {
            ConstraintValue::None => 0.0,
            ConstraintValue::Scalar(v) => *v,
            ConstraintValue::Parameter(p) => parameter_states[p.index()],
        }
    }
    fn set_min_flow(&mut self, value: ConstraintValue) {
        self.flow_constraints.min_flow = value;
    }
    fn get_min_flow(&self, parameter_states: &[f64]) -> f64 {
        self.flow_constraints.get_min_flow(parameter_states)
    }
    fn set_max_flow(&mut self, value: ConstraintValue) {
        self.flow_constraints.max_flow = value;
    }
    fn get_max_flow(&self, parameter_states: &[f64]) -> f64 {
        self.flow_constraints.get_max_flow(parameter_states)
    }
    fn add_outgoing_edge(&mut self, edge: Edge) {
        self.outgoing_edges.push(edge);
    }
}

#[derive(Debug, PartialEq)]
pub struct OutputNode {
    pub meta: NodeMeta,
    pub cost: ConstraintValue,
    pub flow_constraints: FlowConstraints,
    pub incoming_edges: Vec<Edge>,
}

impl OutputNode {
    fn new(index: &NodeIndex, name: &str) -> Self {
        Self {
            meta: NodeMeta::new(index, name),
            cost: ConstraintValue::None,
            flow_constraints: FlowConstraints::new(),
            incoming_edges: Vec::new(),
        }
    }
    fn set_cost(&mut self, value: ConstraintValue) {
        self.cost = value
    }
    fn get_cost(&self, parameter_states: &[f64]) -> f64 {
        match &self.cost {
            ConstraintValue::None => 0.0,
            ConstraintValue::Scalar(v) => *v,
            ConstraintValue::Parameter(p) => parameter_states[p.index()],
        }
    }
    fn set_min_flow(&mut self, value: ConstraintValue) {
        self.flow_constraints.min_flow = value;
    }
    fn get_min_flow(&self, parameter_states: &[f64]) -> f64 {
        self.flow_constraints.get_min_flow(parameter_states)
    }
    fn set_max_flow(&mut self, value: ConstraintValue) {
        self.flow_constraints.max_flow = value;
    }
    fn get_max_flow(&self, parameter_states: &[f64]) -> f64 {
        self.flow_constraints.get_max_flow(parameter_states)
    }
    fn add_incoming_edge(&mut self, edge: Edge) {
        self.incoming_edges.push(edge);
    }
}

#[derive(Debug, PartialEq)]
pub struct LinkNode {
    pub meta: NodeMeta,
    pub cost: ConstraintValue,
    pub flow_constraints: FlowConstraints,
    pub incoming_edges: Vec<Edge>,
    pub outgoing_edges: Vec<Edge>,
}

impl LinkNode {
    fn new(index: &NodeIndex, name: &str) -> Self {
        Self {
            meta: NodeMeta::new(index, name),
            cost: ConstraintValue::None,
            flow_constraints: FlowConstraints::new(),
            incoming_edges: Vec::new(),
            outgoing_edges: Vec::new(),
        }
    }
    fn set_cost(&mut self, value: ConstraintValue) {
        self.cost = value
    }
    fn get_cost(&self, parameter_states: &[f64]) -> f64 {
        match &self.cost {
            ConstraintValue::None => 0.0,
            ConstraintValue::Scalar(v) => *v,
            ConstraintValue::Parameter(p) => parameter_states[p.index()],
        }
    }
    fn set_min_flow(&mut self, value: ConstraintValue) {
        self.flow_constraints.min_flow = value;
    }
    fn get_min_flow(&self, parameter_states: &[f64]) -> f64 {
        self.flow_constraints.get_min_flow(parameter_states)
    }
    fn set_max_flow(&mut self, value: ConstraintValue) {
        self.flow_constraints.max_flow = value;
    }
    fn get_max_flow(&self, parameter_states: &[f64]) -> f64 {
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
    pub meta: NodeMeta,
    pub cost: ConstraintValue,
    pub initial_volume: f64,
    pub storage_constraints: StorageConstraints,
    pub incoming_edges: Vec<Edge>,
    pub outgoing_edges: Vec<Edge>,
}

impl StorageNode {
    fn new(index: &NodeIndex, name: &str, initial_volume: f64) -> Self {
        Self {
            meta: NodeMeta::new(index, name),
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
    fn get_cost(&self, parameter_states: &[f64]) -> f64 {
        match &self.cost {
            ConstraintValue::None => 0.0,
            ConstraintValue::Scalar(v) => *v,
            ConstraintValue::Parameter(p) => parameter_states[p.index()],
        }
    }
    fn set_min_volume(&mut self, value: ConstraintValue) {
        self.storage_constraints.min_volume = value;
    }
    fn get_min_volume(&self, parameter_states: &[f64]) -> f64 {
        self.storage_constraints.get_min_volume(parameter_states)
    }
    fn set_max_volume(&mut self, value: ConstraintValue) {
        self.storage_constraints.max_volume = value;
    }
    fn get_max_volume(&self, parameter_states: &[f64]) -> f64 {
        self.storage_constraints.get_max_volume(parameter_states)
    }
    fn add_incoming_edge(&mut self, edge: Edge) {
        self.incoming_edges.push(edge);
    }
    fn add_outgoing_edge(&mut self, edge: Edge) {
        self.outgoing_edges.push(edge);
    }
}
