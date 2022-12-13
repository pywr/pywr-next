use crate::node::{FlowConstraints, NodeMeta};
use crate::{NodeIndex, PywrError};
use std::ops::{Deref, DerefMut};

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct VirtualStorageIndex(usize);

impl Deref for VirtualStorageIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Default)]
pub struct VirtualStorageVec {
    nodes: Vec<VirtualStorage>,
}

impl Deref for VirtualStorageVec {
    type Target = Vec<VirtualStorage>;

    fn deref(&self) -> &Self::Target {
        &self.nodes
    }
}

impl DerefMut for VirtualStorageVec {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.nodes
    }
}

impl VirtualStorageVec {
    pub fn get(&self, index: &VirtualStorageIndex) -> Result<&VirtualStorage, PywrError> {
        self.nodes.get(index.0).ok_or(PywrError::NodeIndexNotFound)
    }

    pub fn get_mut(&mut self, index: &VirtualStorageIndex) -> Result<&mut VirtualStorage, PywrError> {
        self.nodes.get_mut(index.0).ok_or(PywrError::NodeIndexNotFound)
    }

    pub fn push_new(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        nodes: Vec<NodeIndex>,
        factors: Option<Vec<f64>>,
    ) -> VirtualStorageIndex {
        let node_index = VirtualStorageIndex(self.nodes.len());
        let node = VirtualStorage::new(&node_index, name, sub_name, nodes, factors);
        self.nodes.push(node);
        node_index
    }
}

#[derive(Debug, PartialEq)]
pub struct VirtualStorage {
    pub meta: NodeMeta<VirtualStorageIndex>,
    pub flow_constraints: FlowConstraints,
    pub nodes: Vec<NodeIndex>,
    pub factors: Option<Vec<f64>>,
}

impl VirtualStorage {
    pub fn new(
        index: &VirtualStorageIndex,
        name: &str,
        sub_name: Option<&str>,
        nodes: Vec<NodeIndex>,
        factors: Option<Vec<f64>>,
    ) -> Self {
        Self {
            meta: NodeMeta::new(index, name, sub_name),
            flow_constraints: FlowConstraints::new(),
            nodes,
            factors,
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

    pub fn index(&self) -> VirtualStorageIndex {
        *self.meta.index()
    }

    pub fn has_factors(&self) -> bool {
        self.factors.is_some()
    }

    pub fn get_nodes(&self) -> Vec<NodeIndex> {
        self.nodes.to_vec()
    }

    pub fn get_nodes_with_factors(&self) -> Option<Vec<(NodeIndex, f64)>> {
        self.factors
            .as_ref()
            .map(|factors| self.nodes.iter().zip(factors.iter()).map(|(n, f)| (*n, *f)).collect())
    }
}
