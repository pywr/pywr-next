use crate::error::SchemaError;
use crate::error::{ComponentConversionError, ConversionError};
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::nodes::{LossFactor, NodeMeta};
#[cfg(feature = "core")]
use crate::nodes::{NodeAttribute, NodeComponent};
use crate::parameters::Parameter;
use crate::{node_attribute_subset_enum, node_component_subset_enum};
#[cfg(feature = "core")]
use pywr_core::metric::MetricF64;
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::nodes::LinkNode as LinkNodeV1;
use schemars::JsonSchema;

// This macro generates a subset enum for the `RiverNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_attribute_subset_enum! {
    pub enum RiverNodeAttribute {
        Inflow,
        Outflow,
        Loss,
    }
}

node_component_subset_enum! {
    pub enum RiverNodeComponent {
        Inflow,
        Outflow,
        Loss,
    }
}

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
///
/// # Available attributes and components
///
/// The enums [`RiverNodeAttribute`] and [`RiverNodeComponent`] define the available
/// attributes and components for this node.
///
)]
pub struct RiverNode {
    pub meta: NodeMeta,
    /// Optional local parameters.
    pub parameters: Option<Vec<Parameter>>,
    /// An optional loss. This internally creates an [`crate::nodes::OutputNode`] and
    /// [`pywr_core::nodes::Aggregated`] to handle the loss.
    pub loss_factor: Option<LossFactor>,
}

impl RiverNode {
    const DEFAULT_ATTRIBUTE: RiverNodeAttribute = RiverNodeAttribute::Outflow;
    const DEFAULT_COMPONENT: RiverNodeComponent = RiverNodeComponent::Outflow;

    /// The sub-name of the output node.
    fn loss_sub_name() -> Option<&'static str> {
        Some("loss")
    }

    /// The name of net flow node.
    fn net_sub_name() -> Option<&'static str> {
        Some("net")
    }

    pub fn input_connectors(&self) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        let mut connectors = vec![(self.meta.name.as_str(), Self::net_sub_name().map(|s| s.to_string()))];

        // add the optional loss link
        if self.loss_factor.is_some() {
            connectors.push((self.meta.name.as_str(), Self::loss_sub_name().map(|s| s.to_string())))
        }
        Ok(connectors)
    }
    pub fn output_connectors(&self) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        Ok(vec![(
            self.meta.name.as_str(),
            Self::net_sub_name().map(|s| s.to_string()),
        )])
    }

    pub fn default_attribute(&self) -> RiverNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> RiverNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl RiverNode {
    /// The name of the aggregated node to handle the proportional loss.
    fn agg_sub_name() -> Option<&'static str> {
        Some("aggregated_node")
    }

    pub fn node_indices_for_flow_constraints(
        &self,
        network: &pywr_core::network::Network,
        component: Option<NodeComponent>,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        // Use the default attribute if none is specified
        let component = match component {
            Some(c) => c.try_into()?,
            None => Self::DEFAULT_COMPONENT,
        };

        let indices = match component {
            RiverNodeComponent::Inflow => {
                // If the loss node is defined, we need to return both the net and loss nodes
                match network.get_node_index_by_name(self.meta.name.as_str(), Self::loss_sub_name()) {
                    Some(loss_idx) => {
                        vec![
                            network
                                .get_node_index_by_name(self.meta.name.as_str(), Self::net_sub_name())
                                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                                    name: self.meta.name.clone(),
                                    sub_name: Self::net_sub_name().map(String::from),
                                })?,
                            loss_idx,
                        ]
                    }
                    None => vec![
                        network
                            .get_node_index_by_name(self.meta.name.as_str(), Self::net_sub_name())
                            .ok_or_else(|| SchemaError::CoreNodeNotFound {
                                name: self.meta.name.clone(),
                                sub_name: Self::net_sub_name().map(String::from),
                            })?,
                    ],
                }
            }
            RiverNodeComponent::Outflow => {
                vec![
                    network
                        .get_node_index_by_name(self.meta.name.as_str(), Self::net_sub_name())
                        .ok_or_else(|| SchemaError::CoreNodeNotFound {
                            name: self.meta.name.clone(),
                            sub_name: Self::net_sub_name().map(String::from),
                        })?,
                ]
            }
            RiverNodeComponent::Loss => {
                match network.get_node_index_by_name(self.meta.name.as_str(), Self::loss_sub_name()) {
                    Some(idx) => vec![idx],
                    None => return Ok(vec![]), // No loss node defined, so return empty
                }
            }
        };

        Ok(indices)
    }

    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        let river_idx = network.add_link_node(self.meta.name.as_str(), Self::net_sub_name())?;

        // add nodes and edge
        if self.loss_factor.is_some() {
            let loss_idx = network.add_output_node(self.meta.name.as_str(), Self::loss_sub_name())?;
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
            let factors = loss_factor.load(network, args, Some(&self.meta.name))?;
            if factors.is_none() {
                // Loaded a constant zero factor; ensure that the loss node has zero flow
                network.set_node_max_flow(self.meta.name.as_str(), Self::loss_sub_name(), Some(0.0.into()))?;
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
        let attr = match attribute {
            Some(attr) => attr.try_into()?,
            None => Self::DEFAULT_ATTRIBUTE,
        };

        let metric = match attr {
            RiverNodeAttribute::Inflow => {
                match network.get_node_index_by_name(self.meta.name.as_str(), Self::loss_sub_name()) {
                    // The total inflow with the loss is the sum of the net and loss node
                    Some(loss_idx) => {
                        let indices = vec![
                            network
                                .get_node_index_by_name(self.meta.name.as_str(), Self::net_sub_name())
                                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                                    name: self.meta.name.clone(),
                                    sub_name: Self::net_sub_name().map(String::from),
                                })?,
                            loss_idx,
                        ];
                        MetricF64::MultiNodeInFlow {
                            indices,
                            name: self.meta.name.to_string(),
                        }
                    }
                    // Loss is None
                    None => MetricF64::NodeInFlow(
                        network
                            .get_node_index_by_name(self.meta.name.as_str(), Self::net_sub_name())
                            .ok_or_else(|| SchemaError::CoreNodeNotFound {
                                name: self.meta.name.clone(),
                                sub_name: Self::net_sub_name().map(String::from),
                            })?,
                    ),
                }
            }
            RiverNodeAttribute::Outflow => {
                let idx = network
                    .get_node_index_by_name(self.meta.name.as_str(), Self::net_sub_name())
                    .ok_or_else(|| SchemaError::CoreNodeNotFound {
                        name: self.meta.name.clone(),
                        sub_name: Self::net_sub_name().map(String::from),
                    })?;
                MetricF64::NodeOutFlow(idx)
            }
            RiverNodeAttribute::Loss => {
                match network.get_node_index_by_name(self.meta.name.as_str(), Self::loss_sub_name()) {
                    Some(loss_idx) => MetricF64::NodeInFlow(loss_idx),
                    None => 0.0.into(),
                }
            }
        };

        Ok(metric)
    }
}

impl TryFrom<LinkNodeV1> for RiverNode {
    type Error = ComponentConversionError;

    fn try_from(v1: LinkNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

        if v1.max_flow.is_some() {
            return Err(ComponentConversionError::Node {
                name: meta.name,
                attr: "max_flow".to_string(),
                error: ConversionError::ExtraAttribute {
                    attr: "max_flow".to_string(),
                },
            });
        }
        if v1.min_flow.is_some() {
            return Err(ComponentConversionError::Node {
                name: meta.name,
                attr: "min_flow".to_string(),
                error: ConversionError::ExtraAttribute {
                    attr: "min_flow".to_string(),
                },
            });
        }
        if v1.cost.is_some() {
            return Err(ComponentConversionError::Node {
                name: meta.name,
                attr: "cost".to_string(),
                error: ConversionError::ExtraAttribute {
                    attr: "cost".to_string(),
                },
            });
        }

        let n = Self {
            meta,
            parameters: None,
            loss_factor: None,
        };
        Ok(n)
    }
}
