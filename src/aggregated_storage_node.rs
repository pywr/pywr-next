use crate::metric::Metric;
use crate::node::{Constraint, ConstraintValue, FlowConstraints, Node, NodeMeta};
use crate::state::ParameterState;
use crate::{NodeIndex, PywrError};
use std::ops::{Deref, DerefMut};

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct AggregatedStorageNodeIndex(usize);

impl Deref for AggregatedStorageNodeIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct AggregatedStorageNodeVec {
    nodes: Vec<AggregatedStorageNode>,
}

impl Deref for AggregatedStorageNodeVec {
    type Target = Vec<AggregatedStorageNode>;

    fn deref(&self) -> &Self::Target {
        &self.nodes
    }
}

impl DerefMut for AggregatedStorageNodeVec {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.nodes
    }
}

impl AggregatedStorageNodeVec {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }
    pub fn get(&self, index: &AggregatedStorageNodeIndex) -> Result<&AggregatedStorageNode, PywrError> {
        self.nodes.get(index.0).ok_or(PywrError::NodeIndexNotFound)
    }

    pub fn get_mut(&mut self, index: &AggregatedStorageNodeIndex) -> Result<&mut AggregatedStorageNode, PywrError> {
        self.nodes.get_mut(index.0).ok_or(PywrError::NodeIndexNotFound)
    }

    pub fn push_new(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        nodes: Vec<NodeIndex>,
    ) -> AggregatedStorageNodeIndex {
        let node_index = AggregatedStorageNodeIndex(self.nodes.len());
        let node = AggregatedStorageNode::new(&node_index, name, sub_name, nodes);
        self.nodes.push(node);
        node_index
    }
}

#[derive(Debug, PartialEq)]
pub struct AggregatedStorageNode {
    pub meta: NodeMeta<AggregatedStorageNodeIndex>,
    pub nodes: Vec<NodeIndex>,
}

impl AggregatedStorageNode {
    pub fn new(index: &AggregatedStorageNodeIndex, name: &str, sub_name: Option<&str>, nodes: Vec<NodeIndex>) -> Self {
        Self {
            meta: NodeMeta::new(index, name, sub_name),
            nodes,
        }
    }

    pub fn name(&self) -> &str {
        self.meta.name()
    }

    /// Get a node's sub_name
    pub fn sub_name(&self) -> Option<&str> {
        self.meta.sub_name()
    }

    /// Get a node's full name
    pub fn full_name(&self) -> (&str, Option<&str>) {
        self.meta.full_name()
    }

    pub fn index(&self) -> AggregatedStorageNodeIndex {
        *self.meta.index()
    }

    pub fn get_nodes(&self) -> Vec<NodeIndex> {
        self.nodes.to_vec()
    }

    pub fn default_metric(&self) -> Vec<Metric> {
        self.nodes.iter().map(|n| Metric::NodeOutFlow(*n)).collect()
    }
}
