use crate::NodeIndex;
use crate::network::Network;
use crate::node::{NodeError, NodeVec};
use crate::state::State;
use std::fmt::{Display, Formatter};
use std::ops::{Deref, DerefMut};
use thiserror::Error;

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug, Hash)]
pub struct EdgeIndex(usize);

impl Deref for EdgeIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for EdgeIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

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

    pub fn cost(&self, nodes: &NodeVec, model: &Network, state: &State) -> Result<f64, EdgeError> {
        let from_node = nodes
            .get(&self.from_node_index)
            .ok_or(EdgeError::FromNodeIndexNotFound(self.from_node_index))?;
        let to_node = nodes
            .get(&self.to_node_index)
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

#[derive(Default)]
pub struct EdgeVec {
    edges: Vec<Edge>,
}

impl Deref for EdgeVec {
    type Target = Vec<Edge>;

    fn deref(&self) -> &Self::Target {
        &self.edges
    }
}

impl DerefMut for EdgeVec {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.edges
    }
}

impl EdgeVec {
    pub fn get(&self, index: &EdgeIndex) -> Option<&Edge> {
        self.edges.get(index.0)
    }

    pub fn get_mut(&mut self, index: &EdgeIndex) -> Option<&mut Edge> {
        self.edges.get_mut(index.0)
    }

    pub fn push(&mut self, from_node_index: NodeIndex, to_node_index: NodeIndex) -> EdgeIndex {
        let index = EdgeIndex(self.edges.len());
        // TODO check whether an edge between these two nodes already exists.

        let node = Edge::new(index, from_node_index, to_node_index);
        self.edges.push(node);
        index
    }
}
