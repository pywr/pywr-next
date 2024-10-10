use crate::error::ConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::nodes::{LossFactor, NodeAttribute, NodeMeta};
#[cfg(feature = "core")]
use pywr_core::metric::MetricF64;
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::nodes::LinkNode as LinkNodeV1;
use schemars::JsonSchema;

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
#[doc = svgbobdoc::transform!(
/// A link node representing a river with an optional proportional loss.
///
/// ```svgbob
///
///             <RiverNode>.net    D
///          .-------->L---------->*
///      U  |
///     -*--|
///         !
///         '-.-.-.-.->O
///               <RiverNode>.loss
///
/// ```
)]
pub struct RiverNode {
    pub meta: NodeMeta,
    /// An optional loss. This internally creates an [`crate::nodes::OutputNode`] and
    /// [`pywr_core::nodes::Aggregated`] to handle the loss.
    pub loss_factor: Option<LossFactor>,
}

impl RiverNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Outflow;

    /// The sub-name of the output node.
    fn loss_node_sub_name() -> Option<&'static str> {
        Some("loss")
    }

    /// The name of net flow node.
    fn net_node_sub_name() -> Option<&'static str> {
        Some("net")
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        let mut connectors = vec![(self.meta.name.as_str(), None)];

        // add the optional loss link
        if self.loss_factor.is_some() {
            connectors.push((
                self.meta.name.as_str(),
                Self::loss_node_sub_name().map(|s| s.to_string()),
            ))
        }
        connectors
    }
    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(
            self.meta.name.as_str(),
            Self::net_node_sub_name().map(|s| s.to_string()),
        )]
    }

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl RiverNode {
    /// The name of the aggregated node to handle the proportional loss.
    fn agg_sub_name() -> Option<&'static str> {
        Some("aggregated_node")
    }

    pub fn node_indices_for_constraints(
        &self,
        network: &pywr_core::network::Network,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        let idx = network.get_node_index_by_name(self.meta.name.as_str(), None)?;
        Ok(vec![idx])
    }

    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        let river_idx = network.add_link_node(self.meta.name.as_str(), None)?;

        // add nodes and edge
        if self.loss_factor.is_some() {
            let loss_idx = network.add_output_node(self.meta.name.as_str(), Self::loss_node_sub_name())?;
            // The aggregated node factors to handle the loss
            network.add_aggregated_node(
                self.meta.name.as_str(),
                Self::agg_sub_name(),
                &[vec![river_idx], vec![loss_idx]],
                None,
            )?;
        }

        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        if let Some(loss_factor) = &self.loss_factor {
            let factors = loss_factor.load(network, args)?;
            if factors.is_none() {
                // Loaded a constant zero factor; ensure that the loss node has zero flow
                network.set_node_max_flow(self.meta.name.as_str(), Self::loss_node_sub_name(), Some(0.0.into()))?;
            }
            network.set_aggregated_node_relationship(self.meta.name.as_str(), Self::agg_sub_name(), factors)?;
        }

        Ok(())
    }

    pub fn create_metric(
        &self,
        network: &pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<MetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let metric = match attr {
            NodeAttribute::Inflow => {
                match network.get_node_index_by_name(self.meta.name.as_str(), Self::loss_node_sub_name()) {
                    // The total inflow with the loss is the sum of the net and loss node
                    Ok(loss_idx) => {
                        let indices = vec![
                            network.get_node_index_by_name(self.meta.name.as_str(), Self::net_node_sub_name())?,
                            loss_idx,
                        ];
                        MetricF64::MultiNodeInFlow {
                            indices,
                            name: self.meta.name.to_string(),
                        }
                    }
                    // Loss is None
                    Err(_) => MetricF64::NodeInFlow(
                        network.get_node_index_by_name(self.meta.name.as_str(), Self::net_node_sub_name())?,
                    ),
                }
            }
            NodeAttribute::Outflow => {
                let idx = network.get_node_index_by_name(self.meta.name.as_str(), Self::net_node_sub_name())?;
                MetricF64::NodeOutFlow(idx)
            }
            NodeAttribute::Loss => {
                match network.get_node_index_by_name(self.meta.name.as_str(), Self::loss_node_sub_name()) {
                    Ok(loss_idx) => MetricF64::NodeInFlow(loss_idx),
                    Err(_) => 0.0.into(),
                }
            }
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    ty: "RiverNode".to_string(),
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }
}

impl TryFrom<LinkNodeV1> for RiverNode {
    type Error = ConversionError;

    fn try_from(v1: LinkNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

        if v1.max_flow.is_some() {
            return Err(ConversionError::ExtraNodeAttribute {
                name: meta.name,
                attr: "max_flow".to_string(),
            });
        }
        if v1.min_flow.is_some() {
            return Err(ConversionError::ExtraNodeAttribute {
                name: meta.name,
                attr: "min_flow".to_string(),
            });
        }
        if v1.cost.is_some() {
            return Err(ConversionError::ExtraNodeAttribute {
                name: meta.name,
                attr: "cost".to_string(),
            });
        }

        let n = Self {
            meta,
            loss_factor: None,
        };
        Ok(n)
    }
}
