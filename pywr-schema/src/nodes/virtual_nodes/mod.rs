mod aggregated;
mod virtual_storage;

use crate::metric::Metric;
use crate::nodes::{NodeAttribute, NodeComponent, NodeMeta, NodePosition, PlaceholderNode};
use crate::parameters::Parameter;
#[cfg(feature = "core")]
use crate::{LoadArgs, SchemaError};
use crate::{VisitMetrics, VisitPaths};
pub use aggregated::{
    AggregatedNode, AggregatedNodeAttribute, AggregatedStorageNode, AggregatedStorageNodeAttribute, Relationship,
};
#[cfg(feature = "core")]
use pywr_core::metric::MetricF64;
use schemars::JsonSchema;
use std::path::{Path, PathBuf};
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};
pub use virtual_storage::{
    AnnualReset, RollingWindow, VirtualStorageNode, VirtualStorageNodeAttribute, VirtualStorageReset,
    VirtualStorageResetVolume,
};

/// The main enum for all nodes in the model.
#[derive(serde::Deserialize, serde::Serialize, Clone, EnumDiscriminants, Debug, JsonSchema, Display)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
// This creates a separate enum called `NodeType` that is available in this module.
#[strum_discriminants(name(VirtualNodeType))]
// This is currently required by the `Reservoir` node. Rather than box it
#[allow(clippy::large_enum_variant)]
pub enum VirtualNode {
    Aggregated(AggregatedNode),
    AggregatedStorage(AggregatedStorageNode),
    VirtualStorage(VirtualStorageNode),
    Placeholder(PlaceholderNode),
}

impl VirtualNode {
    pub fn name(&self) -> &str {
        self.meta().name.as_str()
    }

    pub fn position(&self) -> Option<&NodePosition> {
        self.meta().position.as_ref()
    }

    pub fn node_type(&self) -> VirtualNodeType {
        // Implementation provided by the `EnumDiscriminants` derive macro.
        self.into()
    }

    pub fn meta(&self) -> &NodeMeta {
        match self {
            VirtualNode::Aggregated(n) => &n.meta,
            VirtualNode::AggregatedStorage(n) => &n.meta,
            VirtualNode::VirtualStorage(n) => &n.meta,
            VirtualNode::Placeholder(n) => &n.meta,
        }
    }

    pub fn default_attribute(&self) -> NodeAttribute {
        match self {
            VirtualNode::Aggregated(n) => n.default_attribute().into(),
            VirtualNode::AggregatedStorage(n) => n.default_attribute().into(),
            VirtualNode::VirtualStorage(n) => n.default_attribute().into(),
            VirtualNode::Placeholder(n) => n.default_attribute(),
        }
    }

    /// Returns the default component for the node, if defined.
    pub fn default_component(&self) -> Option<NodeComponent> {
        match self {
            VirtualNode::Aggregated(_) => None,
            VirtualNode::AggregatedStorage(_) => None,
            VirtualNode::VirtualStorage(_) => None,
            VirtualNode::Placeholder(_) => None,
        }
    }

    /// Get the locally defined parameters for this node.
    ///
    /// This does **not** return which parameters this node might reference, but rather
    /// the parameters that are defined on this node itself.
    pub fn local_parameters(&self) -> Option<&[Parameter]> {
        match self {
            VirtualNode::Aggregated(n) => n.parameters.as_deref(),
            VirtualNode::AggregatedStorage(n) => n.parameters.as_deref(),
            VirtualNode::VirtualStorage(n) => n.parameters.as_deref(),
            VirtualNode::Placeholder(_) => None,
        }
    }
}

#[cfg(feature = "core")]
impl VirtualNode {
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network, args: &LoadArgs) -> Result<(), SchemaError> {
        match self {
            VirtualNode::Aggregated(n) => n.add_to_model(network, args),
            VirtualNode::AggregatedStorage(n) => n.add_to_model(network, args),
            VirtualNode::VirtualStorage(n) => n.add_to_model(network, args),
            VirtualNode::Placeholder(n) => n.add_to_model(),
        }
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        match self {
            VirtualNode::Aggregated(n) => n.set_constraints(network, args),
            VirtualNode::AggregatedStorage(_) => Ok(()), // No constraints on aggregated storage nodes.
            VirtualNode::VirtualStorage(n) => n.set_constraints(network, args),
            VirtualNode::Placeholder(n) => n.set_constraints(),
        }
    }

    /// Create a metric for the given attribute on this node.
    pub fn create_metric(
        &self,
        network: &mut pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<MetricF64, SchemaError> {
        match self {
            VirtualNode::Aggregated(n) => n.create_metric(network, attribute),
            VirtualNode::AggregatedStorage(n) => n.create_metric(network, attribute),
            VirtualNode::VirtualStorage(n) => n.create_metric(network, attribute),
            VirtualNode::Placeholder(n) => n.create_metric(),
        }
    }
}

impl VisitMetrics for VirtualNode {
    fn visit_metrics<F: FnMut(&Metric)>(&self, visitor: &mut F) {
        match self {
            VirtualNode::Aggregated(n) => n.visit_metrics(visitor),
            VirtualNode::AggregatedStorage(n) => n.visit_metrics(visitor),
            VirtualNode::VirtualStorage(n) => n.visit_metrics(visitor),
            VirtualNode::Placeholder(n) => n.visit_metrics(visitor),
        }
    }

    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, visitor: &mut F) {
        match self {
            VirtualNode::Aggregated(n) => n.visit_metrics_mut(visitor),
            VirtualNode::AggregatedStorage(n) => n.visit_metrics_mut(visitor),
            VirtualNode::VirtualStorage(n) => n.visit_metrics_mut(visitor),
            VirtualNode::Placeholder(n) => n.visit_metrics_mut(visitor),
        }
    }
}

impl VisitPaths for VirtualNode {
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        match self {
            VirtualNode::Aggregated(n) => n.visit_paths(visitor),
            VirtualNode::AggregatedStorage(n) => n.visit_paths(visitor),
            VirtualNode::VirtualStorage(n) => n.visit_paths(visitor),
            VirtualNode::Placeholder(n) => n.visit_paths(visitor),
        }
    }

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        match self {
            VirtualNode::Aggregated(n) => n.visit_paths_mut(visitor),
            VirtualNode::AggregatedStorage(n) => n.visit_paths_mut(visitor),
            VirtualNode::VirtualStorage(n) => n.visit_paths_mut(visitor),
            VirtualNode::Placeholder(n) => n.visit_paths_mut(visitor),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::VirtualNode;
    use std::fs;
    use std::path::PathBuf;

    /// Test all the documentation examples successfully deserialize.
    #[test]
    fn test_doc_examples() {
        let mut doc_examples = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        doc_examples.push("src/nodes/virtual_nodes/doc_examples");

        for entry in fs::read_dir(doc_examples).unwrap() {
            let p = entry.unwrap().path();
            if p.is_file() {
                let data = fs::read_to_string(&p).unwrap_or_else(|_| panic!("Failed to read file: {p:?}",));

                let value: serde_json::Value =
                    serde_json::from_str(&data).unwrap_or_else(|_| panic!("Failed to deserialize: {p:?}",));

                match value {
                    serde_json::Value::Object(_) => {
                        let _ = serde_json::from_value::<VirtualNode>(value)
                            .unwrap_or_else(|e| panic!("Failed to deserialize `{p:?}`: {e}",));
                    }
                    serde_json::Value::Array(_) => {
                        let _ = serde_json::from_value::<Vec<VirtualNode>>(value)
                            .unwrap_or_else(|e| panic!("Failed to deserialize `{p:?}`: {e}",));
                    }
                    _ => panic!("Expected JSON object or array: {p:?}",),
                }
            }
        }
    }
}
