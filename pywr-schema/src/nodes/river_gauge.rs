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
use pywr_core::{metric::UnresolvedMetricF64, node::UnresolvedNode};
use pywr_schema_macros::PywrVisitAll;
use pywr_schema_macros::skip_serializing_none;
use pywr_v1_schema::nodes::RiverGaugeNode as RiverGaugeNodeV1;
use schemars::JsonSchema;

// This macro generates a subset enum for the `RiverGaugeNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_attribute_subset_enum! {
    pub enum RiverGaugeNodeAttribute {
        Inflow,
        Outflow,
    }
}

node_component_subset_enum! {
    pub enum RiverGaugeNodeComponent {
        Inflow,
        Outflow,
    }
}

/// This is used to represent a minimum residual flow (MRF) at a gauging station.
///
///
#[doc = mermaid!("doc_diagrams/river-gauge.mmd")]
///
/// # Available attributes and components
///
/// The enums [`RiverGaugeNodeAttribute`] and [`RiverGaugeNodeComponent`] define the available
/// attributes and components for this node.
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct RiverGaugeNode {
    pub meta: NodeMeta,
    /// Optional local parameters.
    pub parameters: Option<Vec<Parameter>>,
    pub mrf: Option<Metric>,
    pub mrf_cost: Option<Metric>,
    pub bypass_cost: Option<Metric>,
}

impl RiverGaugeNode {
    const DEFAULT_ATTRIBUTE: RiverGaugeNodeAttribute = RiverGaugeNodeAttribute::Outflow;
    const DEFAULT_COMPONENT: RiverGaugeNodeComponent = RiverGaugeNodeComponent::Outflow;

    pub fn default_attribute(&self) -> RiverGaugeNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> RiverGaugeNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl RiverGaugeNode {
    fn mrf_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("mrf"))
    }

    fn bypass_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("bypass"))
    }

    pub fn input_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<UnresolvedNode>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::InputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            Ok(vec![self.mrf_sub_name(), self.bypass_sub_name()])
        }
    }

    pub fn output_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<UnresolvedNode>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::OutputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            Ok(vec![self.mrf_sub_name(), self.bypass_sub_name()])
        }
    }
    pub fn nodes_for_flow_constraints(
        &self,
        component: Option<NodeComponent>,
    ) -> Result<Vec<UnresolvedNode>, SchemaError> {
        // Use the default component if none is specified
        let component = match component {
            Some(c) => c.try_into()?,
            None => Self::DEFAULT_COMPONENT,
        };

        let nodes = match component {
            // Inflow and Outflow components both use the same nodes.
            RiverGaugeNodeComponent::Inflow | RiverGaugeNodeComponent::Outflow => {
                vec![self.mrf_sub_name(), self.bypass_sub_name()]
            }
        };
        Ok(nodes)
    }
    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        let mut mrf_node = pywr_core::node::NodeBuilder::link(self.mrf_sub_name());
        let bypass_node = pywr_core::node::NodeBuilder::link(self.bypass_sub_name());

        // MRF applies as a maximum on the MRF node.
        if let Some(cost) = &self.mrf_cost {
            let value = cost.load(network, args, Some(&self.meta.name))?;
            mrf_node.cost(value);
        }

        if let Some(mrf) = &self.mrf {
            let value = mrf.load(network, args, Some(&self.meta.name))?;
            mrf_node.max_flow(value);
        }

        if let Some(cost) = &self.bypass_cost {
            let value = cost.load(network, args, Some(&self.meta.name))?;
            mrf_node.min_flow(value);
        }

        network.node(mrf_node);
        network.node(bypass_node);

        Ok(())
    }

    pub fn create_metric(&self, attribute: Option<NodeAttribute>) -> Result<UnresolvedMetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = match attribute {
            Some(attr) => attr.try_into()?,
            None => Self::DEFAULT_ATTRIBUTE,
        };

        let nodes = vec![self.mrf_sub_name(), self.bypass_sub_name()];

        let metric = match attr {
            RiverGaugeNodeAttribute::Inflow => UnresolvedMetricF64::MultiNodeInFlow {
                nodes,
                name: self.meta.name.to_string(),
            },
            RiverGaugeNodeAttribute::Outflow => UnresolvedMetricF64::MultiNodeOutFlow {
                nodes,
                name: self.meta.name.to_string(),
            },
        };

        Ok(metric)
    }
}

impl TryFromV1<RiverGaugeNodeV1> for RiverGaugeNode {
    type Error = Box<ComponentConversionError>;

    fn try_from_v1(
        v1: RiverGaugeNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = try_convert_node_meta(v1.meta)?;

        let mrf = try_convert_node_attr(&meta.name, "mrf", v1.mrf, parent_node, conversion_data)?;
        let mrf_cost = try_convert_node_attr(&meta.name, "mrf_cost", v1.mrf_cost, parent_node, conversion_data)?;
        let bypass_cost = try_convert_node_attr(&meta.name, "cost", v1.cost, parent_node, conversion_data)?;

        let n = Self {
            meta,
            parameters: None,
            mrf,
            mrf_cost,
            bypass_cost,
        };
        Ok(n)
    }
}
