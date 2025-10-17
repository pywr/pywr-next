use crate::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::network::LoadArgs;
#[cfg(feature = "core")]
use crate::nodes::{NodeAttribute, NodeComponent};
use crate::nodes::{NodeMeta, NodeSlot};
use crate::parameters::Parameter;
use crate::{node_attribute_subset_enum, node_component_subset_enum};
#[cfg(feature = "core")]
use pywr_core::{
    derived_metric::{DerivedMetric, TurbineData},
    metric::MetricF64,
    parameters::{HydropowerTargetData, ParameterName},
};
use pywr_schema_macros::{PywrVisitAll, skip_serializing_none};
use schemars::JsonSchema;
use strum_macros::EnumIter;

#[derive(
    serde::Deserialize, serde::Serialize, Clone, Debug, strum_macros::Display, JsonSchema, PywrVisitAll, EnumIter,
)]
pub enum TargetType {
    // set flow derived from the hydropower target as a max_flow
    MaxFlow,
    // set flow derived from the hydropower target as a min_flow
    MinFlow,
    // set flow derived from the hydropower target as min_flow and max_flow (like a catchment)
    Both,
}

// This macro generates a subset enum for the `TurbineNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_attribute_subset_enum! {
    pub enum TurbineNodeAttribute {
        Inflow,
        Outflow,
        Power,
    }
}

node_component_subset_enum! {
    pub enum TurbineNodeComponent {
        Inflow,
        Outflow,
    }
}

/// This turbine node can be used to set a flow constraint based on a hydropower production target.
/// The turbine elevation, minimum head and efficiency can also be configured.
///
/// # Available attributes and components
///
/// The enums [`TurbineNodeAttribute`] and [`TurbineNodeComponent`] define the available
/// attributes and components for this node.
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct TurbineNode {
    pub meta: NodeMeta,
    /// Optional local parameters.
    pub parameters: Option<Vec<Parameter>>,
    pub cost: Option<Metric>,
    /// Hydropower production target. If set the node's max flow is limited to the flow
    /// calculated using the hydropower equation. If omitted no flow restriction is set.
    /// Units should be in units of energy per day.
    pub target: Option<Metric>,
    /// This can be used to define where to apply the flow calculated from the hydropower production
    /// target using the inverse hydropower equation. Default to [`TargetType::MaxFlow`])
    pub target_type: TargetType,
    /// The elevation of water entering the turbine. The difference of this value with the
    /// `turbine_elevation` gives the working head of the turbine. This is optional
    /// and can be a constant, a value from a table, a parameter name or an inline parameter
    /// (see [`DynamicFloatValue`]).
    pub water_elevation: Option<Metric>,
    /// The elevation of the turbine. The difference between the `water_elevation` and this value
    /// gives the working head of the turbine. Default to `0.0`.
    pub turbine_elevation: f64,
    /// The minimum head for flow to occur. If the working head is less than this value, zero flow
    /// is returned. Default to `0.0`.
    pub min_head: f64,
    /// The efficiency of the turbine. Default to `1.0`.
    pub efficiency: f64,
    /// The density of water. Default to `1000.0`.
    pub water_density: f64,
    /// A factor used to transform the units of flow to be compatible with the hydropower equation.
    /// This should convert flow to units of m<sup>3</sup> day<sup>-1</sup>. Default to `1.0`.
    pub flow_unit_conversion: f64,
    /// A factor used to transform the units of total energy. Defaults to 1e<sup>-6</sup> to
    /// return `MJ`.
    pub energy_unit_conversion: f64,
}

impl Default for TurbineNode {
    fn default() -> Self {
        Self {
            meta: Default::default(),
            parameters: None,
            cost: None,
            target: None,
            target_type: TargetType::MaxFlow,
            water_elevation: None,
            turbine_elevation: 0.0,
            min_head: 0.0,
            efficiency: 1.0,
            water_density: 1000.0,
            flow_unit_conversion: 1.0,
            energy_unit_conversion: 1e-6,
        }
    }
}

impl TurbineNode {
    const DEFAULT_ATTRIBUTE: TurbineNodeAttribute = TurbineNodeAttribute::Outflow;
    const DEFAULT_COMPONENT: TurbineNodeComponent = TurbineNodeComponent::Outflow;

    pub fn input_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::InputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            Ok(vec![(self.meta.name.as_str(), None)])
        }
    }
    pub fn output_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::OutputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            Ok(vec![(self.meta.name.as_str(), None)])
        }
    }

    pub fn default_attribute(&self) -> TurbineNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> TurbineNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl TurbineNode {
    fn sub_name() -> Option<&'static str> {
        Some("turbine")
    }

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
        let idx = match component {
            TurbineNodeComponent::Inflow | TurbineNodeComponent::Outflow => network
                .get_node_index_by_name(self.meta.name.as_str(), Self::sub_name())
                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                    name: self.meta.name.clone(),
                    sub_name: Self::sub_name().map(String::from),
                })?,
        };

        Ok(vec![idx])
    }
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        network.add_link_node(self.meta.name.as_str(), None)?;
        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        if let Some(cost) = &self.cost {
            let value = cost.load(network, args, Some(&self.meta.name))?;
            network.set_node_cost(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(target) = &self.target {
            let name = ParameterName::new("power", Some(self.meta.name.as_str()));
            let target_value = target.load(network, args, Some(&self.meta.name))?;

            let water_elevation = self
                .water_elevation
                .as_ref()
                .map(|t| t.load(network, args, Some(&self.meta.name)))
                .transpose()?;
            let turbine_data = HydropowerTargetData {
                target: target_value,
                water_elevation,
                elevation: Some(self.turbine_elevation),
                min_head: Some(self.min_head),
                max_flow: None,
                min_flow: None,
                efficiency: Some(self.efficiency),
                water_density: Some(self.water_density),
                flow_unit_conversion: Some(self.flow_unit_conversion),
                energy_unit_conversion: Some(self.energy_unit_conversion),
            };
            let p = pywr_core::parameters::HydropowerTargetParameter::new(name, turbine_data);
            let power_idx = network.add_parameter(Box::new(p))?;
            let metric: MetricF64 = power_idx.into();

            match self.target_type {
                TargetType::MaxFlow => {
                    network.set_node_max_flow(self.meta.name.as_str(), Self::sub_name(), metric.clone().into())?;
                }
                TargetType::MinFlow => {
                    network.set_node_min_flow(self.meta.name.as_str(), Self::sub_name(), metric.clone().into())?;
                }
                TargetType::Both => {
                    network.set_node_max_flow(self.meta.name.as_str(), Self::sub_name(), metric.clone().into())?;
                    network.set_node_min_flow(self.meta.name.as_str(), Self::sub_name(), metric.clone().into())?
                }
            }
        }

        Ok(())
    }

    pub fn create_metric(
        &self,
        network: &mut pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
        args: &LoadArgs,
    ) -> Result<MetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = match attribute {
            Some(attr) => attr.try_into()?,
            None => Self::DEFAULT_ATTRIBUTE,
        };

        let idx = network
            .get_node_index_by_name(self.meta.name.as_str(), None)
            .ok_or_else(|| SchemaError::CoreNodeNotFound {
                name: self.meta.name.clone(),
                sub_name: None,
            })?;

        let metric = match attr {
            TurbineNodeAttribute::Outflow => MetricF64::NodeOutFlow(idx),
            TurbineNodeAttribute::Inflow => MetricF64::NodeInFlow(idx),
            TurbineNodeAttribute::Power => {
                let water_elevation = self
                    .water_elevation
                    .as_ref()
                    .map(|t| t.load(network, args, Some(&self.meta.name)))
                    .transpose()?;

                let turbine_data = TurbineData {
                    elevation: self.turbine_elevation,
                    efficiency: self.efficiency,
                    water_elevation,
                    water_density: self.water_density,
                    flow_unit_conversion: self.flow_unit_conversion,
                    energy_unit_conversion: self.energy_unit_conversion,
                };
                let dm = DerivedMetric::PowerFromNodeFlow(idx, turbine_data);
                let dm_idx = network.add_derived_metric(dm);
                MetricF64::DerivedMetric(dm_idx)
            }
        };

        Ok(metric)
    }
}
