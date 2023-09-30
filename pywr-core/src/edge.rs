use crate::model::Model;
use crate::node::NodeVec;
use crate::state::State;
use crate::{NodeIndex, PywrError};
use std::ops::{Deref, DerefMut};

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug, Hash)]
pub struct EdgeIndex(usize);

impl Deref for EdgeIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Edge {
    pub index: EdgeIndex,
    pub from_node_index: NodeIndex,
    pub to_node_index: NodeIndex,
}

impl Edge {
    pub(crate) fn new(index: EdgeIndex, from_node_index: NodeIndex, to_node_index: NodeIndex) -> Self {
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

    pub(crate) fn cost(&self, nodes: &NodeVec, model: &Model, state: &State) -> Result<f64, PywrError> {
        let from_node = nodes.get(&self.from_node_index)?;
        let to_node = nodes.get(&self.to_node_index)?;

        let from_cost = from_node.get_outgoing_cost(model, state)?;
        let to_cost = to_node.get_incoming_cost(model, state)?;

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
    pub fn get(&self, index: &EdgeIndex) -> Result<&Edge, PywrError> {
        self.edges.get(index.0).ok_or(PywrError::EdgeIndexNotFound)
    }

    pub fn get_mut(&mut self, index: &EdgeIndex) -> Result<&mut Edge, PywrError> {
        self.edges.get_mut(index.0).ok_or(PywrError::EdgeIndexNotFound)
    }

    pub fn push(&mut self, from_node_index: NodeIndex, to_node_index: NodeIndex) -> EdgeIndex {
        let index = EdgeIndex(self.edges.len());
        // TODO check whether an edge between these two nodes already exists.

        let node = Edge::new(index, from_node_index, to_node_index);
        self.edges.push(node);
        index
    }
}
