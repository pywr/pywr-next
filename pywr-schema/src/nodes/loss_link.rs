use crate::error::ComponentConversionError;
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::network::LoadArgs;
#[cfg(feature = "core")]
use crate::nodes::{NodeAttribute, NodeComponent};
use crate::nodes::{NodeMeta, NodeSlot};
use crate::parameters::Parameter;
use crate::v1::{ConversionData, TryFromV1, try_convert_node_attr};
use crate::{node_attribute_subset_enum, node_component_subset_enum};
#[cfg(feature = "core")]
use pywr_core::{aggregated_node::Relationship, metric::MetricF64};
use pywr_schema_macros::PywrVisitAll;
use pywr_schema_macros::skip_serializing_none;
use pywr_v1_schema::nodes::LossLinkNode as LossLinkNodeV1;
use schemars::JsonSchema;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};

/// The type of loss factor applied.
///
/// Gross losses are typically applied as a proportion of the total flow into a node, whereas
/// net losses are applied as a proportion of the net flow. Please see the documentation for
/// specific nodes (e.g. [`LossLinkNode`]) to understand how the loss factor is applied.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll, Display, EnumDiscriminants)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(LossFactorType))]
pub enum LossFactor {
    Gross { factor: Metric },
    Net { factor: Metric },
}

#[cfg(feature = "core")]
impl LossFactor {
    /// Load the loss factor and return a corresponding [`Relationship`] if the loss factor is
    /// not a constant zero. If a zero is loaded, then `None` is returned.
    pub fn load(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<Option<Relationship>, SchemaError> {
        match self {
            LossFactor::Gross { factor } => {
                let lf = factor.load(network, args, parent)?;
                // Handle the case where we are given a zero loss factor
                // The aggregated node does not support zero loss factors so filter them here.
                if lf.is_constant_zero() {
                    return Ok(None);
                }
                // Gross losses are configured as a proportion of the net flow
                Ok(Some(Relationship::new_proportion_factors(&[lf])))
            }
            LossFactor::Net { factor } => {
                let lf = factor.load(network, args, parent)?;
                // Handle the case where we are given a zero loss factor
                // The aggregated node does not support zero loss factors so filter them here.
                if lf.is_constant_zero() {
                    return Ok(None);
                }
                // Net losses are configured as a ratio of the net flow
                Ok(Some(Relationship::new_ratio_factors(&[1.0.into(), lf])))
            }
        }
    }
}

// This macro generates a subset enum for the `LossLinkNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_attribute_subset_enum! {
    pub enum LossLinkNodeAttribute {
        Inflow,
        Outflow,
        Loss,
    }
}

node_component_subset_enum! {
    pub enum LossLinkNodeComponent {
        Inflow,
        Outflow,
        Loss,
    }
}

#[doc = svgbobdoc::transform!(
/// This is used to represent a link with losses.
///
/// The loss is applied using a loss factor, [`LossFactor`], which can be applied to either the
/// gross or net flow. If no loss factor is defined the output node "O" and the associated
/// aggregated node are not created.
///
/// The default output metric for this node is the net flow.
///
/// ```svgbob
///
///            <node>.net    D
///          .------>L ---->*
///      U  |
///     -*--|
///         |
///          '------>O
///            <node>.loss
/// ```
///
/// # Available attributes and components
///
/// The enums [`LossLinkNodeAttribute`] and [`LossLinkNodeComponent`] define the available
/// attributes and components for this node.
///
)]
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct LossLinkNode {
    pub meta: NodeMeta,
    /// Optional local parameters.
    pub parameters: Option<Vec<Parameter>>,
    pub loss_factor: Option<LossFactor>,
    pub min_net_flow: Option<Metric>,
    pub max_net_flow: Option<Metric>,
    pub net_cost: Option<Metric>,
}

impl LossLinkNode {
    const DEFAULT_ATTRIBUTE: LossLinkNodeAttribute = LossLinkNodeAttribute::Outflow;
    const DEFAULT_COMPONENT: LossLinkNodeComponent = LossLinkNodeComponent::Outflow;

    fn loss_sub_name() -> Option<&'static str> {
        Some("loss")
    }

    fn net_sub_name() -> Option<&'static str> {
        Some("net")
    }

    pub fn input_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::InputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            // Gross inflow always goes to the net node ...
            let mut input_connectors = vec![(self.meta.name.as_str(), Self::net_sub_name().map(|s| s.to_string()))];

            // ... but only to the loss node if a loss is defined
            if self.loss_factor.is_some() {
                input_connectors.push((self.meta.name.as_str(), Self::loss_sub_name().map(|s| s.to_string())));
            }

            Ok(input_connectors)
        }
    }

    pub fn output_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::OutputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            // Only net goes to the downstream.
            Ok(vec![(
                self.meta.name.as_str(),
                Self::net_sub_name().map(|s| s.to_string()),
            )])
        }
    }

    pub fn default_attribute(&self) -> LossLinkNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> LossLinkNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl LossLinkNode {
    fn agg_sub_name() -> Option<&'static str> {
        Some("agg")
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
            LossLinkNodeComponent::Inflow => {
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
            LossLinkNodeComponent::Outflow => {
                vec![
                    network
                        .get_node_index_by_name(self.meta.name.as_str(), Self::net_sub_name())
                        .ok_or_else(|| SchemaError::CoreNodeNotFound {
                            name: self.meta.name.clone(),
                            sub_name: Self::net_sub_name().map(String::from),
                        })?,
                ]
            }
            LossLinkNodeComponent::Loss => {
                match network.get_node_index_by_name(self.meta.name.as_str(), Self::loss_sub_name()) {
                    Some(idx) => vec![idx],
                    None => return Ok(vec![]), // No loss node defined, so return empty
                }
            }
        };

        Ok(indices)
    }
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        let idx_net = network.add_link_node(self.meta.name.as_str(), Self::net_sub_name())?;
        // TODO make the loss node configurable (i.e. it could be a link if a network wanted to use the loss)
        // The above would need to support slots in the connections.

        if self.loss_factor.is_some() {
            let idx_loss = network.add_output_node(self.meta.name.as_str(), Self::loss_sub_name())?;
            // This aggregated node will contain the factors to enforce the loss
            network.add_aggregated_node(
                self.meta.name.as_str(),
                Self::agg_sub_name(),
                &[vec![idx_net], vec![idx_loss]],
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
        if let Some(cost) = &self.net_cost {
            let value = cost.load(network, args, Some(&self.meta.name))?;
            network.set_node_cost(self.meta.name.as_str(), Self::net_sub_name(), value.into())?;
        }

        if let Some(max_flow) = &self.max_net_flow {
            let value = max_flow.load(network, args, Some(&self.meta.name))?;
            network.set_node_max_flow(self.meta.name.as_str(), Self::net_sub_name(), value.into())?;
        }

        if let Some(min_flow) = &self.min_net_flow {
            let value = min_flow.load(network, args, Some(&self.meta.name))?;
            network.set_node_min_flow(self.meta.name.as_str(), Self::net_sub_name(), value.into())?;
        }

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
            LossLinkNodeAttribute::Inflow => {
                match network.get_node_index_by_name(self.meta.name.as_str(), Self::loss_sub_name()) {
                    // Loss node is defined. The total inflow is the sum of the net and loss nodes;
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
                    // No loss node defined, so just use the net node
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
            LossLinkNodeAttribute::Outflow => {
                let idx = network
                    .get_node_index_by_name(self.meta.name.as_str(), Self::net_sub_name())
                    .ok_or_else(|| SchemaError::CoreNodeNotFound {
                        name: self.meta.name.clone(),
                        sub_name: Self::net_sub_name().map(String::from),
                    })?;
                MetricF64::NodeOutFlow(idx)
            }
            LossLinkNodeAttribute::Loss => {
                match network.get_node_index_by_name(self.meta.name.as_str(), Self::loss_sub_name()) {
                    Some(idx) => MetricF64::NodeInFlow(idx),
                    None => 0.0.into(),
                }
            }
        };

        Ok(metric)
    }
}

impl TryFromV1<LossLinkNodeV1> for LossLinkNode {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: LossLinkNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

        let loss_factor: Option<Metric> =
            try_convert_node_attr(&meta.name, "loss_factor", v1.loss_factor, parent_node, conversion_data)?;
        let loss_factor = loss_factor.map(|factor| LossFactor::Net { factor });

        let net_cost = try_convert_node_attr(&meta.name, "cost", v1.cost, parent_node, conversion_data)?;
        let max_net_flow = try_convert_node_attr(&meta.name, "max_flow", v1.max_flow, parent_node, conversion_data)?;
        let min_net_flow = try_convert_node_attr(&meta.name, "min_flow", v1.min_flow, parent_node, conversion_data)?;

        let n = Self {
            meta,
            parameters: None,
            loss_factor,
            min_net_flow,
            max_net_flow,
            net_cost,
        };
        Ok(n)
    }
}
