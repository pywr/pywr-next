use crate::node::{NodeIndex, NodeVec};
use crate::state::ParameterState;
use crate::PywrError;
use std::cell::RefCell;
use std::rc::Rc;

pub type EdgeIndex = usize;
pub type EdgeRef = Rc<RefCell<_Edge>>;

#[derive(Debug, PartialEq, Eq)]
pub struct _Edge {
    pub index: EdgeIndex,
    pub from_node_index: NodeIndex,
    pub to_node_index: NodeIndex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge(EdgeRef);

impl Edge {
    pub(crate) fn new(index: &EdgeIndex, from_node_index: NodeIndex, to_node_index: NodeIndex) -> Self {
        let edge = _Edge {
            index: *index,
            from_node_index,
            to_node_index,
        };
        Edge(Rc::new(RefCell::new(edge)))
    }

    pub fn index(&self) -> EdgeIndex {
        self.0.borrow().index
    }

    pub fn from_node_index(&self) -> NodeIndex {
        self.0.borrow().from_node_index
    }

    pub fn to_node_index(&self) -> NodeIndex {
        self.0.borrow().to_node_index
    }

    pub(crate) fn cost(&self, nodes: &NodeVec, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        let from_node = nodes.get(&self.0.borrow().from_node_index)?;
        let to_node = nodes.get(&self.0.borrow().to_node_index)?;

        let from_cost = from_node.get_outgoing_cost(parameter_states)?;
        let to_cost = to_node.get_incoming_cost(parameter_states)?;

        Ok(from_cost + to_cost)
    }
}
