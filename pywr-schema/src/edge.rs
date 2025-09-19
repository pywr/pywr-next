#[cfg(feature = "core")]
use crate::SchemaError;
#[cfg(feature = "core")]
use crate::network::LoadArgs;
#[cfg(feature = "core")]
use pywr_core::{edge::EdgeIndex, metric::MetricF64, node::NodeIndex};
use pywr_schema_macros::skip_serializing_none;
use schemars::JsonSchema;
use std::fmt::{Display, Formatter};

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema, Debug, PartialEq)]
pub struct Edge {
    pub from_node: String,
    pub to_node: String,
    pub from_slot: Option<String>,
    pub to_slot: Option<String>,
}

impl From<pywr_v1_schema::edge::Edge> for Edge {
    fn from(v1: pywr_v1_schema::edge::Edge) -> Self {
        Self {
            from_node: v1.from_node,
            to_node: v1.to_node,
            from_slot: v1.from_slot.flatten(),
            to_slot: v1.to_slot.flatten(),
        }
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
    fn iter_node_index_pairs(
        &self,
        network: &pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<impl Iterator<Item = (NodeIndex, NodeIndex)> + use<>, SchemaError> {
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

        let from_slot = self.from_slot.as_deref();
        let to_slot = self.to_slot.as_deref();

        // Collect the node indices at each end of the edge
        let from_node_indices: Vec<NodeIndex> = from_node
            .output_connectors(from_slot)?
            .into_iter()
            .map(|(name, sub_name)| {
                network
                    .get_node_index_by_name(name, sub_name.as_deref())
                    .ok_or_else(|| SchemaError::CoreNodeNotFound {
                        name: name.to_string(),
                        sub_name,
                    })
            })
            .collect::<Result<_, _>>()?;

        let to_node_indices: Vec<NodeIndex> = to_node
            .input_connectors(to_slot)?
            .into_iter()
            .map(|(name, sub_name)| {
                network
                    .get_node_index_by_name(name, sub_name.as_deref())
                    .ok_or_else(|| SchemaError::CoreNodeNotFound {
                        name: name.to_string(),
                        sub_name,
                    })
            })
            .collect::<Result<_, _>>()?;

        let pairs: Vec<_> = from_node_indices
            .into_iter()
            .flat_map(|from_node_index| std::iter::repeat(from_node_index).zip(to_node_indices.iter().copied()))
            .collect();

        Ok(pairs.into_iter())
    }

    /// Add the edge to the network
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network, args: &LoadArgs) -> Result<(), SchemaError> {
        // Connect each "from" connector to each "to" connector
        for (from_node_index, to_node_index) in self.iter_node_index_pairs(network, args)? {
            network.connect_nodes(from_node_index, to_node_index)?;
        }

        Ok(())
    }

    /// Create a metric that will return this edge's total flow in the model.
    pub fn create_metric(
        &self,
        network: &pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<MetricF64, SchemaError> {
        let indices: Vec<EdgeIndex> = self
            .iter_node_index_pairs(network, args)?
            .map(|(from_node_index, to_node_index)| {
                network
                    .get_edge_index(from_node_index, to_node_index)
                    .ok_or_else(|| SchemaError::EdgeNotFound {
                        from_node: self.from_node.clone(),
                        to_node: self.to_node.clone(),
                    })
            })
            .collect::<Result<_, _>>()?;

        let metric = MetricF64::MultiEdgeFlow {
            indices,
            name: self.to_string(),
        };

        Ok(metric)
    }
}
