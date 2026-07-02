use crate::NodeIndex;
use crate::network::{EdgeIndex, Network};
use crate::node::{Node, NodeError};
use crate::state::State;
use std::ops::Deref;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EdgeError {
    #[error("From node index not found: {0}")]
    FromNodeIndexNotFound(NodeIndex),
    #[error("To node index not found: {0}")]
    ToNodeIndexNotFound(NodeIndex),
    #[error("Node error: {0}")]
    NodeError(#[from] Box<NodeError>),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Edge {
    pub index: EdgeIndex,
    pub from_node_index: NodeIndex,
    pub to_node_index: NodeIndex,
}

impl Edge {
    pub fn new(index: EdgeIndex, from_node_index: NodeIndex, to_node_index: NodeIndex) -> Self {
        Self {
            index,
            from_node_index,
            to_node_index,
        }
    }

    pub fn index(&self) -> EdgeIndex {
        self.index
    }

    pub fn from_node_index(&self) -> NodeIndex {
        self.from_node_index
    }

    pub fn to_node_index(&self) -> NodeIndex {
        self.to_node_index
    }

    pub fn cost(&self, nodes: &[Node], model: &Network, state: &State) -> Result<f64, EdgeError> {
        let from_node = nodes
            .get(*self.from_node_index.deref())
            .ok_or(EdgeError::FromNodeIndexNotFound(self.from_node_index))?;
        let to_node = nodes
            .get(*self.to_node_index.deref())
            .ok_or(EdgeError::ToNodeIndexNotFound(self.from_node_index))?;

        let from_cost = from_node
            .get_outgoing_cost(model, state)
            .map_err(|e| EdgeError::NodeError(Box::new(e)))?;
        let to_cost = to_node
            .get_incoming_cost(model, state)
            .map_err(|e| EdgeError::NodeError(Box::new(e)))?;

        Ok(from_cost + to_cost)
    }
}
