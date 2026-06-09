use crate::metric::Metric;
use crate::nodes::NodeMeta;
use crate::nodes::loss_link::LossFactor;
use crate::parameters::Parameter;
#[cfg(feature = "core")]
use crate::{
    error::SchemaError,
    network::LoadArgs,
    nodes::{NodeAttribute, NodeComponent, NodeSlot},
};
use crate::{mermaid, node_attribute_subset_enum, node_component_subset_enum};
#[cfg(feature = "core")]
use pywr_core::{metric::UnresolvedMetricF64, node::UnresolvedNode};
use pywr_schema_macros::{PywrVisitAll, skip_serializing_none};
use schemars::JsonSchema;

// This macro generates a subset enum for the `WaterTreatmentWorksNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_attribute_subset_enum! {
    pub enum WaterTreatmentWorksNodeAttribute {
        Inflow,
        Outflow,
        Loss,
    }
}

node_component_subset_enum! {
    pub enum WaterTreatmentWorksNodeComponent {
        Inflow,
        Outflow,
        Loss,
    }
}

/// A node used to represent a water treatment works (WTW) with optional losses.
///
/// This node comprises an internal structure that allows specifying a minimum and
/// maximum total net flow, an optional loss factor applied as a proportion of either net
/// or gross flow, and an optional "soft" minimum flow.
///
/// When a loss factor is not given the `loss` node is not created. When a non-zero loss
/// factor is provided [`pywr_core::node::OutputNode`] and [`pywr_core::aggregated_node::AggregatedNode`]
/// nodes are created.
///
///
#[doc = mermaid!("doc_diagrams/wtw.mmd")]
///
/// # Available attributes and components
///
/// The enums [`WaterTreatmentWorksNodeAttribute`] and [`WaterTreatmentWorksNodeComponent`] define the available
/// attributes and components for this node.
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct WaterTreatmentWorksNode {
    /// Node metadata
    pub meta: NodeMeta,
    /// Optional local parameters.
    pub parameters: Option<Vec<Parameter>>,
    /// The proportion of net flow that is lost to the loss node.
    pub loss_factor: Option<LossFactor>,
    /// The minimum flow through the `net` flow node.
    pub min_flow: Option<Metric>,
    /// The maximum flow through the `net` flow node.
    pub max_flow: Option<Metric>,
    /// The maximum flow applied to the `net_soft_min_flow` node which is typically
    /// used as a "soft" minimum flow.
    pub soft_min_flow: Option<Metric>,
    /// The cost applied to the `net_soft_min_flow` node.
    pub soft_min_flow_cost: Option<Metric>,
    /// The cost applied to the `net` flow node.
    pub cost: Option<Metric>,
}

impl WaterTreatmentWorksNode {
    const DEFAULT_ATTRIBUTE: WaterTreatmentWorksNodeAttribute = WaterTreatmentWorksNodeAttribute::Outflow;
    const DEFAULT_COMPONENT: WaterTreatmentWorksNodeComponent = WaterTreatmentWorksNodeComponent::Outflow;

    pub fn default_attribute(&self) -> WaterTreatmentWorksNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> WaterTreatmentWorksNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl WaterTreatmentWorksNode {
    fn loss_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("loss"))
    }

    fn net_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("net"))
    }

    fn net_soft_min_flow_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("net_soft_min_flow"))
    }

    fn net_above_soft_min_flow_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("net_above_soft_min_flow"))
    }
    fn agg_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("agg"))
    }
    pub fn input_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<UnresolvedNode>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::InputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            // Connect directly to the total net
            let mut connectors = vec![self.net_sub_name()];
            // Only connect to the loss link if it is created
            if self.loss_factor.is_some() {
                connectors.push(self.loss_sub_name())
            }
            Ok(connectors)
        }
    }

    pub fn output_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<UnresolvedNode>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::OutputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            // Connect to the split of the net flow.
            Ok(vec![
                self.net_soft_min_flow_sub_name(),
                self.net_above_soft_min_flow_sub_name(),
            ])
        }
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

        let indices = match component {
            WaterTreatmentWorksNodeComponent::Inflow => {
                // If the loss node is defined, we need to return both the net and loss nodes
                match self.loss_factor.is_some() {
                    true => vec![self.net_sub_name(), self.loss_sub_name()],
                    false => vec![self.net_sub_name()],
                }
            }
            WaterTreatmentWorksNodeComponent::Outflow => {
                vec![self.net_sub_name()]
            }
            WaterTreatmentWorksNodeComponent::Loss => {
                vec![self.loss_sub_name()]
            }
        };

        Ok(indices)
    }
    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        let mut net_node = pywr_core::node::NodeBuilder::link(self.net_sub_name());
        let mut soft_min_flow_node = pywr_core::node::NodeBuilder::link(self.net_soft_min_flow_sub_name());
        let above_soft_min_flow = pywr_core::node::NodeBuilder::link(self.net_above_soft_min_flow_sub_name());

        if let Some(cost) = &self.cost {
            let value = cost.load(network, args, Some(&self.meta.name))?;
            net_node.cost(value);
        }

        if let Some(max_flow) = &self.max_flow {
            let value = max_flow.load(network, args, Some(&self.meta.name))?;
            net_node.max_flow(value);
        }

        if let Some(min_flow) = &self.min_flow {
            let value = min_flow.load(network, args, Some(&self.meta.name))?;
            net_node.min_flow(value);
        }

        // soft min flow constraints; This typically applies a negative cost upto a maximum
        // defined by the `soft_min_flow`
        if let Some(cost) = &self.soft_min_flow_cost {
            let value = cost.load(network, args, Some(&self.meta.name))?;
            soft_min_flow_node.cost(value);
        }
        if let Some(min_flow) = &self.soft_min_flow {
            let value = min_flow.load(network, args, Some(&self.meta.name))?;
            soft_min_flow_node.max_flow(value);
        }

        // Create the internal connections
        network.connect(self.net_sub_name(), self.net_soft_min_flow_sub_name());
        network.connect(self.net_sub_name(), self.net_above_soft_min_flow_sub_name());

        if let Some(loss_factor) = &self.loss_factor {
            let mut loss_node = pywr_core::node::NodeBuilder::output(self.loss_sub_name());

            let factors = loss_factor.load(network, args, Some(&self.meta.name))?;
            match factors {
                Some(relationship) => {
                    // This aggregated node will contain the factors to enforce the loss
                    let mut agg_node = pywr_core::AggregatedNodeBuilder::new(self.agg_sub_name());
                    agg_node
                        .nodes(vec![self.net_sub_name()])
                        .nodes(vec![self.loss_sub_name()])
                        .relationship(relationship);

                    network.agg_node(agg_node);
                }
                None => {
                    loss_node.max_flow(0.0.into());
                }
            }

            network.node(loss_node);
        }

        network.node(net_node);
        network.node(soft_min_flow_node);
        network.node(above_soft_min_flow);

        Ok(())
    }

    pub fn create_metric(&self, attribute: Option<NodeAttribute>) -> Result<UnresolvedMetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = match attribute {
            Some(attr) => attr.try_into()?,
            None => Self::DEFAULT_ATTRIBUTE,
        };

        let metric = match attr {
            WaterTreatmentWorksNodeAttribute::Inflow => {
                match self.loss_factor.is_some() {
                    // Loss node is defined. The total inflow is the sum of the net and loss nodes;
                    true => {
                        let nodes = vec![self.net_sub_name(), self.loss_sub_name()];
                        UnresolvedMetricF64::MultiNodeInFlow {
                            nodes,
                            name: self.meta.name.to_string(),
                        }
                    }
                    // No loss node defined, so just use the net node
                    false => UnresolvedMetricF64::NodeInFlow(self.net_sub_name()),
                }
            }
            WaterTreatmentWorksNodeAttribute::Outflow => UnresolvedMetricF64::NodeOutFlow(self.net_sub_name()),
            WaterTreatmentWorksNodeAttribute::Loss => match self.loss_factor.is_some() {
                true => UnresolvedMetricF64::NodeInFlow(self.loss_sub_name()),
                false => 0.0.into(),
            },
        };

        Ok(metric)
    }
}
