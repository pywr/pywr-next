#![warn(clippy::pedantic)]
use crate::NodeIndex;
use crate::metric::MetricF64;
use crate::network::{AggregatedStorageNodeIndex, ResolutionMaps};
use crate::node::{NodeMeta, UnresolvedNode};
use thiserror::Error;

#[derive(Debug, PartialEq, Eq)]
pub struct AggregatedStorageNode {
    meta: NodeMeta<AggregatedStorageNodeIndex>,
    nodes: Vec<NodeIndex>,
}

impl AggregatedStorageNode {
    pub fn name(&self) -> &str {
        self.meta.name()
    }

    /// Get a node's sub-name
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

    pub fn iter_nodes(&self) -> impl Iterator<Item = &NodeIndex> {
        self.nodes.iter()
    }

    pub fn default_metric(&self) -> Vec<MetricF64> {
        self.nodes.iter().map(|n| MetricF64::NodeOutFlow(*n)).collect()
    }
}

#[derive(Debug, Error)]
pub enum AggregatedStorageNodeBuilderError {
    #[error("Index not found in resolution map.")]
    IndexNotFound,
    #[error("Reference to node not found.")]
    NodeIndexNotFound { node: UnresolvedNode },
}

pub struct AggregatedStorageNodeBuilder {
    name: UnresolvedNode,
    nodes: Vec<UnresolvedNode>,
}

impl AggregatedStorageNodeBuilder {
    pub fn new(name: &str) -> Self {
        let name = UnresolvedNode::new(name, None);

        Self {
            name,
            nodes: Vec::new(),
        }
    }

    pub fn name(&self) -> &UnresolvedNode {
        &self.name
    }

    pub fn sub_name(&mut self, sub_name: &str) -> &mut Self {
        self.name.set_sub_name(Some(sub_name));
        self
    }

    pub fn node(&mut self, node: UnresolvedNode) -> &mut Self {
        self.nodes.push(node);
        self
    }

    pub fn build(
        &self,
        resolution_maps: &ResolutionMaps,
    ) -> Result<AggregatedStorageNode, AggregatedStorageNodeBuilderError> {
        let index = resolution_maps
            .aggregated_storage_nodes
            .get(&self.name)
            .ok_or(AggregatedStorageNodeBuilderError::IndexNotFound)?;
        let meta = NodeMeta::from_unresolved_name(self.name.clone(), *index);

        let nodes = self
            .nodes
            .iter()
            .map(|unresolved| {
                resolution_maps.nodes.get(unresolved).copied().ok_or_else(|| {
                    AggregatedStorageNodeBuilderError::NodeIndexNotFound {
                        node: unresolved.clone(),
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(AggregatedStorageNode { meta, nodes })
    }
}
