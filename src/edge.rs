use crate::node::{Node, NodeIndex};
use crate::PywrError;
use std::cell::RefCell;
use std::rc::Rc;

pub type EdgeIndex = usize;
pub type EdgeRef = Rc<RefCell<_Edge>>;

#[derive(Debug, PartialEq)]
pub struct _Edge {
    pub index: EdgeIndex,
    pub from_node: Node,
    pub to_node: Node,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Edge(EdgeRef);

impl Edge {
    pub(crate) fn new(index: &EdgeIndex, from_node: &Node, to_node: &Node) -> Self {
        let edge = _Edge {
            index: *index,
            from_node: from_node.clone(),
            to_node: to_node.clone(),
        };
        Edge(Rc::new(RefCell::new(edge)))
    }

    pub fn index(&self) -> EdgeIndex {
        self.0.borrow().index
    }

    pub fn from_node_index(&self) -> NodeIndex {
        self.0.borrow().from_node.index()
    }

    pub fn to_node_index(&self) -> NodeIndex {
        self.0.borrow().to_node.index()
    }

    pub(crate) fn cost(&self, parameter_states: &[f64]) -> Result<f64, PywrError> {
        let from_node = &self.0.borrow().from_node;
        let to_node = &self.0.borrow().to_node;

        let from_cost = from_node.get_outgoing_cost(parameter_states);
        let to_cost = to_node.get_incoming_cost(parameter_states);

        Ok(from_cost + to_cost)
    }
}
