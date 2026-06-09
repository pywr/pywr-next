use crate::error::ComponentConversionError;
use crate::metric::Metric;
use crate::nodes::NodeMeta;
use crate::parameters::Parameter;
use crate::v1::{ConversionData, TryFromV1, try_convert_node_attr, try_convert_node_meta};
#[cfg(feature = "core")]
use crate::{
    error::SchemaError,
    network::LoadArgs,
    nodes::{NodeAttribute, NodeComponent, NodeSlot},
};
use crate::{mermaid, node_attribute_subset_enum, node_component_subset_enum};
#[cfg(feature = "core")]
use pywr_core::{
    aggregated_node::{ProportionalFactorsBuilder, RatioFactorsBuilder, RelationshipBuilder},
    metric::UnresolvedMetricF64,
    node::UnresolvedNode,
};
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
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<Option<Box<dyn RelationshipBuilder>>, SchemaError> {
        match self {
            LossFactor::Gross { factor } => {
                let lf = factor.load(network, args, parent)?;
                // Handle the case where we are given a zero loss factor
                // The aggregated node does not support zero loss factors so filter them here.
                if lf.is_constant_zero() {
                    return Ok(None);
                }
                // Gross losses are configured as a proportion of the net flow
                let mut builder = ProportionalFactorsBuilder::default();
                builder.factor(lf);

                Ok(Some(Box::new(builder)))
            }
            LossFactor::Net { factor } => {
                let lf = factor.load(network, args, parent)?;
                // Handle the case where we are given a zero loss factor
                // The aggregated node does not support zero loss factors so filter them here.
                if lf.is_constant_zero() {
                    return Ok(None);
                }
                // Net losses are configured as a ratio of the net flow
                let mut builder = RatioFactorsBuilder::default();
                builder.factor(1.0.into());
                builder.factor(lf);
                Ok(Some(Box::new(builder)))
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

/// This is used to represent a link with losses.
///
/// The loss is applied using a loss factor, [`LossFactor`], which can be applied to either the
/// gross or net flow. If no loss factor is defined the output node "O" and the associated
/// aggregated node are not created.
///
/// The default output metric for this node is the net flow.
///
#[doc = mermaid!("doc_diagrams/loss-link.mmd")]
///
/// # Available attributes and components
///
/// The enums [`LossLinkNodeAttribute`] and [`LossLinkNodeComponent`] define the available
/// attributes and components for this node.
///
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

    pub fn default_attribute(&self) -> LossLinkNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> LossLinkNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl LossLinkNode {
    fn loss_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("loss"))
    }

    fn net_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("net"))
    }

    pub fn input_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<UnresolvedNode>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::InputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            // Gross inflow always goes to the net node ...
            let mut input_connectors = vec![self.net_sub_name()];

            // ... but only to the loss node if a loss is defined
            if self.loss_factor.is_some() {
                input_connectors.push(self.loss_sub_name());
            }

            Ok(input_connectors)
        }
    }

    pub fn output_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<UnresolvedNode>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::OutputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            // Only net goes to the downstream.
            Ok(vec![self.net_sub_name()])
        }
    }
    fn agg_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("agg"))
    }

    pub fn nodes_for_flow_constraints(
        &self,
        component: Option<NodeComponent>,
    ) -> Result<Vec<UnresolvedNode>, SchemaError> {
        // Use the default attribute if none is specified
        let component = match component {
            Some(c) => c.try_into()?,
            None => Self::DEFAULT_COMPONENT,
        };

        let nodes = match component {
            LossLinkNodeComponent::Inflow => {
                // If the loss node is defined, we need to return both the net and loss nodes
                if self.loss_factor.is_some() {
                    vec![self.net_sub_name(), self.loss_sub_name()]
                } else {
                    vec![self.net_sub_name()]
                }
            }
            LossLinkNodeComponent::Outflow => {
                vec![self.net_sub_name()]
            }
            LossLinkNodeComponent::Loss => {
                if self.loss_factor.is_some() {
                    vec![self.loss_sub_name()]
                } else {
                    vec![]
                }
            }
        };

        Ok(nodes)
    }
    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        let mut net_node = pywr_core::node::NodeBuilder::link(self.net_sub_name());
        // TODO make the loss node configurable (i.e. it could be a link if a network wanted to use the loss)
        // The above would need to support slots in the connections.

        if let Some(cost) = &self.net_cost {
            let value = cost.load(network, args, Some(&self.meta.name))?;
            net_node.cost(value);
        }

        if let Some(max_flow) = &self.max_net_flow {
            let value = max_flow.load(network, args, Some(&self.meta.name))?;
            net_node.max_flow(value);
        }

        if let Some(min_flow) = &self.min_net_flow {
            let value = min_flow.load(network, args, Some(&self.meta.name))?;
            net_node.min_flow(value);
        }

        if let Some(loss_factor) = &self.loss_factor {
            let mut loss_node = pywr_core::node::NodeBuilder::output(self.loss_sub_name());
            // This aggregated node will contain the factors to enforce the loss
            let mut agg_node = pywr_core::AggregatedNodeBuilder::new(self.agg_sub_name());

            agg_node.nodes(vec![net_node.name().clone()]);
            agg_node.nodes(vec![loss_node.name().clone()]);

            let factors = loss_factor.load(network, args, Some(&self.meta.name))?;

            if let Some(factors) = factors {
                agg_node.relationship(factors);
            } else {
                // Loaded a constant zero factor; ensure that the loss node has zero flow
                loss_node.max_flow(0.0.into());
            }

            network.agg_node(agg_node);
            network.node(loss_node);
        }
        network.node(net_node);

        Ok(())
    }

    pub fn create_metric(&self, attribute: Option<NodeAttribute>) -> Result<UnresolvedMetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = match attribute {
            Some(attr) => attr.try_into()?,
            None => Self::DEFAULT_ATTRIBUTE,
        };

        let metric = match attr {
            LossLinkNodeAttribute::Inflow => {
                if self.loss_factor.is_some() {
                    let nodes = vec![self.net_sub_name(), self.loss_sub_name()];
                    UnresolvedMetricF64::MultiNodeInFlow {
                        nodes,
                        name: self.meta.name.to_string(),
                    }
                } else {
                    // No loss node defined, so just use the net node
                    UnresolvedMetricF64::NodeInFlow(self.net_sub_name())
                }
            }
            LossLinkNodeAttribute::Outflow => UnresolvedMetricF64::NodeOutFlow(self.net_sub_name()),
            LossLinkNodeAttribute::Loss => {
                if self.loss_factor.is_some() {
                    UnresolvedMetricF64::NodeInFlow(self.loss_sub_name())
                } else {
                    0.0.into()
                }
            }
        };

        Ok(metric)
    }
}

impl TryFromV1<LossLinkNodeV1> for LossLinkNode {
    type Error = Box<ComponentConversionError>;

    fn try_from_v1(
        v1: LossLinkNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = try_convert_node_meta(v1.meta)?;

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
