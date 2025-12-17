use crate::error::SchemaError;
use crate::error::{ComponentConversionError, ConversionError};
#[cfg(feature = "core")]
use crate::network::LoadArgs;
#[cfg(feature = "core")]
use crate::nodes::{NodeAttribute, NodeComponent};
use crate::nodes::{NodeMeta, NodeSlot};
use crate::parameters::{ConstantValue, Parameter};
use crate::{node_attribute_subset_enum, node_component_subset_enum};
#[cfg(feature = "core")]
use pywr_core::{metric::MetricF64, parameters::ParameterName};
use pywr_schema_macros::{PywrVisitAll, skip_serializing_none};
use pywr_v1_schema::nodes::DelayNode as DelayNodeV1;
use schemars::JsonSchema;

// This macro generates a subset enum for the `DelayNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_attribute_subset_enum! {
    pub enum DelayNodeAttribute {
        Inflow,
        Outflow,
    }
}

node_component_subset_enum! {
    pub enum DelayNodeComponent {
        Inflow,
        Outflow,
    }
}

#[doc = svgbobdoc::transform!(
/// This node is used to introduce a delay between flows entering and leaving the node.
///
/// This is often useful in long river reaches as a simply way to model time-of-travel. Internally
/// an `Output` node is used to terminate flows entering the node and an `Input` node is created
/// with flow constraints set by a [`pywr_core::parameters::DelayParameter`]. These constraints set the minimum and
/// maximum flow on the `Input` node equal to the flow reaching the `Output` node N time-steps
/// ago. The internally created [`pywr_core::parameters::DelayParameter`] is created with this node's name and the suffix
/// "-delay".
///
///
/// ```svgbob
///
///      U  <node.inflow>  D
///     -*---> O    I --->*-
///             <node.outflow>
/// ```
///
/// # Available attributes and components
///
/// The enums [`DelayNodeAttribute`] and [`DelayNodeComponent`] define the available
/// attributes and components for this node.
///
)]
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct DelayNode {
    pub meta: NodeMeta,
    /// Optional local parameters.
    pub parameters: Option<Vec<Parameter>>,
    pub delay: ConstantValue<u64>,
    pub initial_value: ConstantValue<f64>,
}

impl DelayNode {
    const DEFAULT_ATTRIBUTE: DelayNodeAttribute = DelayNodeAttribute::Outflow;
    const DEFAULT_COMPONENT: DelayNodeComponent = DelayNodeComponent::Outflow;

    fn output_sub_name() -> Option<&'static str> {
        Some("inflow")
    }

    fn input_sub_name() -> Option<&'static str> {
        Some("outflow")
    }

    pub fn input_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::InputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            // Inflow goes to the output node
            Ok(vec![(
                self.meta.name.as_str(),
                Self::output_sub_name().map(|s| s.to_string()),
            )])
        }
    }

    pub fn output_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::OutputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            // Outflow goes from the input node
            Ok(vec![(
                self.meta.name.as_str(),
                Self::input_sub_name().map(|s| s.to_string()),
            )])
        }
    }

    pub fn default_attribute(&self) -> DelayNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> DelayNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl DelayNode {
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
            DelayNodeComponent::Inflow => network
                .get_node_index_by_name(self.meta.name.as_str(), Self::output_sub_name())
                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                    name: self.meta.name.clone(),
                    sub_name: Self::output_sub_name().map(String::from),
                })?,
            DelayNodeComponent::Outflow => network
                .get_node_index_by_name(self.meta.name.as_str(), Self::input_sub_name())
                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                    name: self.meta.name.clone(),
                    sub_name: Self::input_sub_name().map(String::from),
                })?,
        };

        Ok(vec![idx])
    }
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        network.add_output_node(self.meta.name.as_str(), Self::output_sub_name())?;
        network.add_input_node(self.meta.name.as_str(), Self::input_sub_name())?;

        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        // Create the delay parameter using the node's name as the parent identifier
        let name = ParameterName::new("delay", Some(self.meta.name.as_str()));
        let output_idx = network
            .get_node_index_by_name(self.meta.name.as_str(), Self::output_sub_name())
            .ok_or_else(|| SchemaError::CoreNodeNotFound {
                name: self.meta.name.clone(),
                sub_name: Self::output_sub_name().map(String::from),
            })?;
        let metric = MetricF64::NodeInFlow(output_idx);
        let p = pywr_core::parameters::DelayParameter::new(
            name,
            metric,
            self.delay.load(args.tables)?,
            self.initial_value.load(args.tables)?,
        );
        let delay_idx = network.add_parameter(Box::new(p))?;

        // Apply it as a constraint on the input node.
        let metric: MetricF64 = delay_idx.into();
        network.set_node_max_flow(self.meta.name.as_str(), Self::input_sub_name(), metric.clone().into())?;
        network.set_node_min_flow(self.meta.name.as_str(), Self::input_sub_name(), metric.into())?;

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
            DelayNodeAttribute::Outflow => {
                let idx = network
                    .get_node_index_by_name(self.meta.name.as_str(), Self::input_sub_name())
                    .ok_or_else(|| SchemaError::CoreNodeNotFound {
                        name: self.meta.name.clone(),
                        sub_name: Self::input_sub_name().map(String::from),
                    })?;
                MetricF64::NodeOutFlow(idx)
            }
            DelayNodeAttribute::Inflow => {
                let idx = network
                    .get_node_index_by_name(self.meta.name.as_str(), Self::output_sub_name())
                    .ok_or_else(|| SchemaError::CoreNodeNotFound {
                        name: self.meta.name.clone(),
                        sub_name: Self::output_sub_name().map(String::from),
                    })?;
                MetricF64::NodeInFlow(idx)
            }
        };

        Ok(metric)
    }
}

impl TryFrom<DelayNodeV1> for DelayNode {
    type Error = ComponentConversionError;

    fn try_from(v1: DelayNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

        // TODO convert days & timesteps to a usize as we don;t support non-daily timesteps at the moment
        let delay = match v1.days {
            Some(days) => days,
            None => match v1.timesteps {
                Some(ts) => ts,
                None => {
                    return Err(ComponentConversionError::Node {
                        name: meta.name,
                        attr: "delay".to_string(),
                        error: ConversionError::MissingAttribute {
                            attrs: vec!["days".to_string(), "timesteps".to_string()],
                        },
                    });
                }
            },
        } as u64;

        let initial_value = v1.initial_flow.unwrap_or_default().into();

        let n = Self {
            meta,
            parameters: None,
            delay: delay.into(),
            initial_value,
        };
        Ok(n)
    }
}
