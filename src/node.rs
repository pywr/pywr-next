use crate::parameters::ParameterIndex;
use crate::{Edge, EdgeIndex, PywrError};

pub(crate) type NodeIndex = usize;

#[derive(Debug)]
pub(crate) enum Node {
    Input(InputNode),
    Output(OutputNode),
    Link(LinkNode),
    Storage(StorageNode),
}

#[derive(Debug, Clone)]
pub enum Constraint {
    MinFlow,
    MaxFlow,
    MinAndMaxFlow,
    MinVolume,
    MaxVolume,
}

impl Node {
    /// Create a new input node
    pub fn new_input(node_index: &NodeIndex, name: &str) -> Self {
        Self::Input(InputNode::new(node_index, name))
    }

    /// Create a new output node
    pub fn new_output(node_index: &NodeIndex, name: &str) -> Self {
        Self::Output(OutputNode::new(node_index, name))
    }

    /// Create a new link node
    pub fn new_link(node_index: &NodeIndex, name: &str) -> Self {
        Self::Link(LinkNode::new(node_index, name))
    }

    /// Create a new storage node
    pub fn new_storage(node_index: &NodeIndex, name: &str, initial_volume: f64) -> Self {
        Self::Storage(StorageNode::new(node_index, name, initial_volume))
    }

    /// Get a node's name
    pub(crate) fn name(&self) -> &str {
        &self.meta().name
    }

    /// Get a node's name
    pub(crate) fn index(&self) -> &NodeIndex {
        &self.meta().index
    }

    /// Get a node's metadata
    fn meta(&self) -> &NodeMeta {
        match self {
            Node::Input(n) => &n.meta,
            Node::Output(n) => &n.meta,
            Node::Link(n) => &n.meta,
            Node::Storage(n) => &n.meta,
        }
    }

    /// Connect one node to another
    pub(crate) fn connect(&mut self, other: &mut Node, next_edge_index: &EdgeIndex) -> Result<Edge, PywrError> {
        // Connections to from output nodes are invalid.
        if let Node::Output(_) = self {
            return Err(PywrError::InvalidNodeConnectionFromOutput);
        };

        // Connections to input nodes are invalid.
        if let Node::Input(_) = other {
            return Err(PywrError::InvalidNodeConnectionToInput);
        };

        // Create the edge
        let edge = Edge::new(next_edge_index, self.index(), other.index());

        // Add the outgoing connection
        match self {
            Node::Input(n) => n.outgoing_edges.push(*next_edge_index),
            Node::Link(n) => n.outgoing_edges.push(*next_edge_index),
            Node::Storage(n) => n.outgoing_edges.push(*next_edge_index),
            _ => panic!("This should not happen!!"),
        }

        // Add the outgoing connection
        match other {
            Node::Output(n) => n.incoming_edges.push(*next_edge_index),
            Node::Link(n) => n.incoming_edges.push(*next_edge_index),
            Node::Storage(n) => n.incoming_edges.push(*next_edge_index),
            _ => panic!("This should not happen!!"),
        }

        Ok(edge)
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

    /// Return a mutable reference to a node's flow constraints if they exist.
    fn flow_constraints_mut(&mut self) -> Result<&mut FlowConstraints, PywrError> {
        match self {
            Node::Input(n) => Ok(&mut n.flow_constraints),
            Node::Link(n) => Ok(&mut n.flow_constraints),
            Node::Output(n) => Ok(&mut n.flow_constraints),
            Node::Storage(_) => Err(PywrError::FlowConstraintsUndefined),
        }
    }

    // /// Return a reference to a node's storage constraints if they exist.
    // fn storage_constraints(&self) -> Result<&StorageConstraints, PywrError> {
    //     match self {
    //         Node::Input(_) => Err(PywrError::StorageConstraintsUndefined),
    //         Node::Link(_) => Err(PywrError::StorageConstraintsUndefined),
    //         Node::Output(_) => Err(PywrError::StorageConstraintsUndefined),
    //         Node::Storage(n) => Ok(&n.storage_constraints),
    //     }
    // }

    /// Return a mutable reference to a node's storage constraints if they exist.
    fn storage_constraints_mut(&mut self) -> Result<&mut StorageConstraints, PywrError> {
        match self {
            Node::Input(_) => Err(PywrError::StorageConstraintsUndefined),
            Node::Link(_) => Err(PywrError::StorageConstraintsUndefined),
            Node::Output(_) => Err(PywrError::StorageConstraintsUndefined),
            Node::Storage(n) => Ok(&mut n.storage_constraints),
        }
    }

    /// Set a constraint on a node.
    pub(crate) fn set_constraint(
        &mut self,
        param_idx: Option<ParameterIndex>,
        constraint: Constraint,
    ) -> Result<(), PywrError> {
        match constraint {
            Constraint::MinFlow => {
                let flow_constraints = self.flow_constraints_mut()?;
                flow_constraints.min_flow = param_idx;
            }
            Constraint::MaxFlow => {
                let flow_constraints = self.flow_constraints_mut()?;
                flow_constraints.max_flow = param_idx;
            }
            Constraint::MinAndMaxFlow => {
                let flow_constraints = self.flow_constraints_mut()?;
                flow_constraints.min_flow = param_idx;
                flow_constraints.max_flow = param_idx;
            }
            Constraint::MinVolume => {
                let storage_constraints = self.storage_constraints_mut()?;
                storage_constraints.min_volume = param_idx;
            }
            Constraint::MaxVolume => {
                let storage_constraints = self.storage_constraints_mut()?;
                storage_constraints.max_volume = param_idx;
            }
        }
        Ok(())
    }

    // fn cost(&self) -> Result<Option<ParameterIndex>, PywrError> {
    //     match self {
    //         Node::Input(n) => Ok(n.cost),
    //         Node::Link(n) => Ok(n.cost),
    //         Node::Output(n) => Ok(n.cost),
    //         Node::Storage(n) => Ok(n.cost),
    //     }
    // }

    pub(crate) fn set_cost(&mut self, param_idx: Option<ParameterIndex>) -> Result<(), PywrError> {
        match self {
            Node::Input(n) => n.cost = param_idx,
            Node::Link(n) => n.cost = param_idx,
            Node::Output(n) => n.cost = param_idx,
            Node::Storage(n) => n.cost = param_idx,
        }
        Ok(())
    }
}

/// Meta data common to all nodes.
#[derive(Debug)]
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

#[derive(Debug)]
pub struct FlowConstraints {
    pub(crate) min_flow: Option<ParameterIndex>,
    pub(crate) max_flow: Option<ParameterIndex>,
}

impl FlowConstraints {
    fn new() -> Self {
        Self {
            min_flow: None,
            max_flow: None,
        }
    }
}

#[derive(Debug)]
pub struct StorageConstraints {
    pub(crate) min_volume: Option<ParameterIndex>,
    pub(crate) max_volume: Option<ParameterIndex>,
}

impl StorageConstraints {
    fn new() -> Self {
        Self {
            min_volume: None,
            max_volume: None,
        }
    }
}

#[derive(Debug)]
pub struct InputNode {
    pub meta: NodeMeta,
    pub cost: Option<ParameterIndex>,
    pub flow_constraints: FlowConstraints,
    pub outgoing_edges: Vec<EdgeIndex>,
}

impl InputNode {
    fn new(index: &NodeIndex, name: &str) -> Self {
        Self {
            meta: NodeMeta::new(index, name),
            cost: None,
            flow_constraints: FlowConstraints::new(),
            outgoing_edges: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct OutputNode {
    pub meta: NodeMeta,
    pub cost: Option<ParameterIndex>,
    pub flow_constraints: FlowConstraints,
    pub incoming_edges: Vec<EdgeIndex>,
}

impl OutputNode {
    fn new(index: &NodeIndex, name: &str) -> Self {
        Self {
            meta: NodeMeta::new(index, name),
            cost: None,
            flow_constraints: FlowConstraints::new(),
            incoming_edges: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct LinkNode {
    pub meta: NodeMeta,
    pub cost: Option<ParameterIndex>,
    pub flow_constraints: FlowConstraints,
    pub incoming_edges: Vec<EdgeIndex>,
    pub outgoing_edges: Vec<EdgeIndex>,
}

impl LinkNode {
    fn new(index: &NodeIndex, name: &str) -> Self {
        Self {
            meta: NodeMeta::new(index, name),
            cost: None,
            flow_constraints: FlowConstraints::new(),
            incoming_edges: Vec::new(),
            outgoing_edges: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct StorageNode {
    pub meta: NodeMeta,
    pub cost: Option<ParameterIndex>,
    pub initial_volume: f64,
    pub storage_constraints: StorageConstraints,
    pub incoming_edges: Vec<EdgeIndex>,
    pub outgoing_edges: Vec<EdgeIndex>,
}

impl StorageNode {
    fn new(index: &NodeIndex, name: &str, initial_volume: f64) -> Self {
        Self {
            meta: NodeMeta::new(index, name),
            cost: None,
            initial_volume,
            storage_constraints: StorageConstraints::new(),
            incoming_edges: Vec::new(),
            outgoing_edges: Vec::new(),
        }
    }
}
