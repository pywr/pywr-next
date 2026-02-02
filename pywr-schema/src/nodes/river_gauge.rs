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
use pywr_core::metric::MetricF64;
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

#[doc = svgbobdoc::transform!(
/// This is used to represent a minimum residual flow (MRF) at a gauging station.
///
///
/// ```svgbob
///            <node>.mrf
///          .------>L -----.
///      U  |                |     D
///     -*--|                |--->*- - -
///         |                |
///          '------>L -----'
///            <node>.bypass
/// ```
///
/// # Available attributes and components
///
/// The enums [`RiverGaugeNodeAttribute`] and [`RiverGaugeNodeComponent`] define the available
/// attributes and components for this node.
///
)]
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

    fn mrf_sub_name() -> Option<&'static str> {
        Some("mrf")
    }

    fn bypass_sub_name() -> Option<&'static str> {
        Some("bypass")
    }

    pub fn input_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::InputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            Ok(vec![
                (self.meta.name.as_str(), Self::mrf_sub_name().map(|s| s.to_string())),
                (self.meta.name.as_str(), Self::bypass_sub_name().map(|s| s.to_string())),
            ])
        }
    }

    pub fn output_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::OutputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            Ok(vec![
                (self.meta.name.as_str(), Self::mrf_sub_name().map(|s| s.to_string())),
                (self.meta.name.as_str(), Self::bypass_sub_name().map(|s| s.to_string())),
            ])
        }
    }

    pub fn default_attribute(&self) -> RiverGaugeNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> RiverGaugeNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl RiverGaugeNode {
    pub fn node_indices_for_flow_constraints(
        &self,
        network: &pywr_core::network::Network,
        component: Option<NodeComponent>,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        // Use the default component if none is specified
        let component = match component {
            Some(c) => c.try_into()?,
            None => Self::DEFAULT_COMPONENT,
        };

        let indices = match component {
            // Inflow and Outflow components both use the same nodes.
            RiverGaugeNodeComponent::Inflow | RiverGaugeNodeComponent::Outflow => {
                vec![
                    network
                        .get_node_index_by_name(self.meta.name.as_str(), Self::mrf_sub_name())
                        .ok_or_else(|| SchemaError::CoreNodeNotFound {
                            name: self.meta.name.clone(),
                            sub_name: Self::mrf_sub_name().map(String::from),
                        })?,
                    network
                        .get_node_index_by_name(self.meta.name.as_str(), Self::bypass_sub_name())
                        .ok_or_else(|| SchemaError::CoreNodeNotFound {
                            name: self.meta.name.clone(),
                            sub_name: Self::bypass_sub_name().map(String::from),
                        })?,
                ]
            }
        };
        Ok(indices)
    }
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        network.add_link_node(self.meta.name.as_str(), Self::mrf_sub_name())?;
        network.add_link_node(self.meta.name.as_str(), Self::bypass_sub_name())?;

        Ok(())
    }
    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        // MRF applies as a maximum on the MRF node.
        if let Some(cost) = &self.mrf_cost {
            let value = cost.load(network, args, Some(&self.meta.name))?;
            network.set_node_cost(self.meta.name.as_str(), Self::mrf_sub_name(), value.into())?;
        }

        if let Some(mrf) = &self.mrf {
            let value = mrf.load(network, args, Some(&self.meta.name))?;
            network.set_node_max_flow(self.meta.name.as_str(), Self::mrf_sub_name(), value.into())?;
        }

        if let Some(cost) = &self.bypass_cost {
            let value = cost.load(network, args, Some(&self.meta.name))?;
            network.set_node_cost(self.meta.name.as_str(), Self::bypass_sub_name(), value.into())?;
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

        let indices = vec![
            network
                .get_node_index_by_name(self.meta.name.as_str(), Self::mrf_sub_name())
                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                    name: self.meta.name.clone(),
                    sub_name: Self::mrf_sub_name().map(String::from),
                })?,
            network
                .get_node_index_by_name(self.meta.name.as_str(), Self::bypass_sub_name())
                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                    name: self.meta.name.clone(),
                    sub_name: Self::bypass_sub_name().map(String::from),
                })?,
        ];

        let metric = match attr {
            RiverGaugeNodeAttribute::Inflow => MetricF64::MultiNodeInFlow {
                indices,
                name: self.meta.name.to_string(),
            },
            RiverGaugeNodeAttribute::Outflow => MetricF64::MultiNodeOutFlow {
                indices,
                name: self.meta.name.to_string(),
            },
        };

        Ok(metric)
    }
}

impl TryFromV1<RiverGaugeNodeV1> for RiverGaugeNode {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: RiverGaugeNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

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
