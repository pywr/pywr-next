use crate::error::{ComponentConversionError, ConversionError};
use crate::nodes::NodeMeta;
use crate::parameters::{ConstantValue, Parameter};
#[cfg(feature = "core")]
use crate::{
    error::SchemaError,
    network::LoadArgs,
    nodes::{NodeAttribute, NodeComponent, NodeSlot},
};
use crate::v1::try_convert_node_meta;
use crate::{mermaid, node_attribute_subset_enum, node_component_subset_enum};
#[cfg(feature = "core")]
use pywr_core::{metric::UnresolvedMetricF64, node::UnresolvedNode, parameters::ParameterName};
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
#[doc = mermaid!("doc_diagrams/delay.mmd")]
///
/// # Available attributes and components
///
/// The enums [`DelayNodeAttribute`] and [`DelayNodeComponent`] define the available
/// attributes and components for this node.
///
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

    pub fn default_attribute(&self) -> DelayNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> DelayNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl DelayNode {
    fn output_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("inflow"))
    }

    fn input_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("outflow"))
    }

    pub fn input_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<UnresolvedNode>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::InputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            // Inflow goes to the output node
            Ok(vec![self.output_sub_name()])
        }
    }

    pub fn output_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<UnresolvedNode>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::OutputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            // Outflow goes from the input node
            Ok(vec![self.input_sub_name()])
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

        let node = match component {
            DelayNodeComponent::Inflow => self.output_sub_name(),
            DelayNodeComponent::Outflow => self.input_sub_name(),
        };

        Ok(vec![node])
    }
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        let output_node = pywr_core::node::NodeBuilder::output(self.output_sub_name());
        let mut input_node = pywr_core::node::NodeBuilder::input(self.input_sub_name());

        // Create the delay parameter using the node's name as the parent identifier
        let name = ParameterName::new("delay", Some(self.meta.name.as_str()));

        let metric = UnresolvedMetricF64::NodeInFlow(output_node.name().clone());
        let p = pywr_core::parameters::DelayParameterBuilder::new(
            name.clone(),
            metric,
            self.delay.load(args.tables)?,
            self.initial_value.load(args.tables)?,
        );
        network.parameters().f64(Box::new(p));

        // Apply it as a constraint on the input node.
        let metric = UnresolvedMetricF64::new_parameter_before(name);
        input_node.min_flow(metric.clone());
        input_node.max_flow(metric);

        network.node(output_node);
        network.node(input_node);

        Ok(())
    }

    pub fn create_metric(&self, attribute: Option<NodeAttribute>) -> Result<UnresolvedMetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = match attribute {
            Some(attr) => attr.try_into()?,
            None => Self::DEFAULT_ATTRIBUTE,
        };

        let metric = match attr {
            DelayNodeAttribute::Outflow => UnresolvedMetricF64::NodeOutFlow(self.input_sub_name()),
            DelayNodeAttribute::Inflow => UnresolvedMetricF64::NodeInFlow(self.output_sub_name()),
        };

        Ok(metric)
    }
}

impl TryFrom<DelayNodeV1> for DelayNode {
    type Error = Box<ComponentConversionError>;

    fn try_from(v1: DelayNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = try_convert_node_meta(v1.meta)?;

        // TODO convert days & timesteps to a usize as we don;t support non-daily timesteps at the moment
        let delay = match v1.days {
            Some(days) => days,
            None => match v1.timesteps {
                Some(ts) => ts,
                None => {
                    return Err(Box::new(ComponentConversionError::Node {
                        name: meta.name,
                        attr: "delay".to_string(),
                        error: ConversionError::MissingAttribute {
                            attrs: vec!["days".to_string(), "timesteps".to_string()],
                        },
                    }));
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
