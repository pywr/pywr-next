use crate::ConversionError;
#[cfg(feature = "core")]
use crate::SchemaError;
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::nodes::NodeSlot;
use crate::visit::{Reference, ReferenceMut, VisitReferences};
#[cfg(feature = "core")]
use pywr_core::{metric::UnresolvedMetricF64, network::UnresolvedEdge, node::UnresolvedNode};
use pywr_schema_macros::skip_serializing_none;
use schemars::JsonSchema;
use std::fmt::{Display, Formatter};
use std::hash::Hash;

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema, Debug)]
pub struct Edge {
    pub from_node: String,
    pub to_node: String,
    pub from_slot: Option<NodeSlot>,
    pub to_slot: Option<NodeSlot>,
    pub meta: Option<EdgeMeta>,
}

/// Optional annotations for an edge. Edges are identified by endpoints and slots, not metadata.
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, JsonSchema, Debug, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct EdgeMeta {}

impl Edge {
    pub fn meta(&self) -> Option<&EdgeMeta> {
        self.meta.as_ref()
    }

    pub fn meta_mut(&mut self) -> Option<&mut EdgeMeta> {
        self.meta.as_mut()
    }
}

impl PartialEq for Edge {
    fn eq(&self, other: &Self) -> bool {
        self.from_node == other.from_node
            && self.to_node == other.to_node
            && self.from_slot == other.from_slot
            && self.to_slot == other.to_slot
    }
}

impl Eq for Edge {}

impl std::hash::Hash for Edge {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.from_node.hash(state);
        self.to_node.hash(state);
        self.from_slot.hash(state);
        self.to_slot.hash(state);
    }
}

impl TryFrom<pywr_v1_schema::edge::Edge> for Edge {
    type Error = ConversionError;
    fn try_from(v1: pywr_v1_schema::edge::Edge) -> Result<Self, Self::Error> {
        let from_slot = match v1.from_slot.flatten() {
            Some(s) => Some(NodeSlot::try_from_v1_str(&s)?),
            None => None,
        };

        let to_slot = match v1.to_slot.flatten() {
            Some(s) => Some(NodeSlot::try_from_v1_str(&s)?),
            None => None,
        };

        Ok(Self {
            from_node: v1.from_node,
            to_node: v1.to_node,
            from_slot,
            to_slot,
            meta: None,
        })
    }
}

/// An edge refers to its two nodes, not to itself: an `edges` entry is a definition, and only
/// [`EdgeReference`](crate::metric::EdgeReference) yields a [`Reference::Edge`].
impl VisitReferences for Edge {
    fn visit_references<F: FnMut(Reference<'_>)>(&self, visitor: &mut F) {
        visitor(Reference::Node(&self.from_node));
        visitor(Reference::Node(&self.to_node));
    }

    fn visit_references_mut<F: FnMut(ReferenceMut<'_>)>(&mut self, visitor: &mut F) {
        visitor(ReferenceMut::Node(&mut self.from_node));
        visitor(ReferenceMut::Node(&mut self.to_node));
    }
}

const EDGE_SYMBOL: &str = "->";

impl Display for Edge {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match (&self.from_slot, &self.to_slot) {
            (Some(from_slot), Some(to_slot)) => {
                write!(
                    f,
                    "{}[{}]{}{}[{}]",
                    self.from_node, from_slot, EDGE_SYMBOL, self.to_node, to_slot
                )
            }
            (Some(from_slot), None) => write!(f, "{}[{}]{}{}", self.from_node, from_slot, EDGE_SYMBOL, self.to_node),
            (None, Some(to_slot)) => {
                write!(f, "{}{}{}[{}]", self.from_node, EDGE_SYMBOL, self.to_node, to_slot)
            }
            (None, None) => write!(f, "{}{}{}", self.from_node, EDGE_SYMBOL, self.to_node),
        }
    }
}

#[cfg(feature = "core")]
impl Edge {
    /// Returns an iterator of the pairs (from, to) of `NodeIndex` that represent this
    /// edge when added to a model. In general this can be several nodes because some nodes
    /// have multiple internal nodes when connected from or to.
    fn iter_node_connection_pairs(
        &self,
        args: &LoadArgs,
    ) -> Result<impl Iterator<Item = (UnresolvedNode, UnresolvedNode)> + use<>, SchemaError> {
        let from_node =
            args.schema
                .get_node_by_name(self.from_node.as_str())
                .ok_or_else(|| SchemaError::NodeNotFound {
                    name: self.from_node.clone(),
                })?;

        let to_node = args
            .schema
            .get_node_by_name(self.to_node.as_str())
            .ok_or_else(|| SchemaError::NodeNotFound {
                name: self.to_node.clone(),
            })?;

        // Collect the node indices at each end of the edge
        let from_node_nodes = from_node.output_connectors(self.from_slot.as_ref())?;
        let to_node_nodes = to_node.input_connectors(self.to_slot.as_ref())?;

        let pairs: Vec<_> = from_node_nodes
            .clone()
            .into_iter()
            .flat_map(|from| std::iter::repeat(from).zip(to_node_nodes.iter().cloned()))
            .collect();

        Ok(pairs.into_iter())
    }

    /// Add the edge to the network
    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        // Connect each "from" connector to each "to" connector
        for (from_node, to_node) in self.iter_node_connection_pairs(args)? {
            network.connect(from_node, to_node);
        }

        Ok(())
    }

    /// Create a metric that will return this edge's total flow in the model.
    pub fn create_metric(&self, args: &LoadArgs) -> Result<UnresolvedMetricF64, SchemaError> {
        let edges = self
            .iter_node_connection_pairs(args)?
            .map(|(from_node, to_node)| UnresolvedEdge::new(from_node, to_node))
            .collect::<Vec<_>>();

        let metric = UnresolvedMetricF64::MultiEdgeFlow {
            edges,
            name: self.to_string(),
        };

        Ok(metric)
    }
}

#[cfg(test)]
mod tests {
    use super::Edge;
    use std::collections::HashSet;

    #[test]
    fn edge_metadata_is_optional_and_not_part_of_identity() {
        let json = r#"{"from_node":"a","to_node":"b","from_slot":null,"to_slot":null}"#;
        let plain: Edge = serde_json::from_str(json).unwrap();
        let annotated: Edge = serde_json::from_str(r#"{"from_node":"a","to_node":"b","meta":{}}"#).unwrap();
        assert_eq!(plain, annotated);
        assert_eq!(HashSet::from([plain.clone()]), HashSet::from([annotated.clone()]));
        assert!(serde_json::to_value(&plain).unwrap().get("meta").is_none());
        assert_eq!(serde_json::to_value(&annotated).unwrap()["meta"], serde_json::json!({}));
    }
}
