use crate::metric::Metric;
use crate::nodes::NodeMeta;
use crate::parameters::Parameter;
#[cfg(feature = "core")]
use crate::{
    error::SchemaError,
    network::LoadArgs,
    nodes::{NodeAttribute, NodeComponent, NodeSlot},
};
use crate::{node_attribute_subset_enum, node_component_subset_enum};
#[cfg(feature = "core")]
use pywr_core::{
    metric::UnresolvedMetricF64,
    node::UnresolvedNode,
    parameters::{DeficitParameterBuilder, ParameterName},
};
use pywr_schema_macros::{PywrVisitAll, skip_serializing_none};
use schemars::JsonSchema;

// This macro generates a subset enum for the `DemandNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_attribute_subset_enum! {
    pub enum DemandNodeAttribute {
        Inflow,
        /// The deficit of the inflow compared to the `max_flow` metric.
        Deficit,
        /// The value of the `max_flow` metric.
        MaxFlow,
        /// The value of the `min_flow` metric. Defaults to 0 if `min_flow` is not defined.
        MinFlow,
    }
}

node_component_subset_enum! {
    pub enum DemandNodeComponent {
        Inflow,
    }
}

/// A node that represents a demand for water, such as a demand centre or abstraction licence.
///
/// This is an output from the model with a required `max_flow`, which is the demand. The
/// difference between the demand and the flow actually delivered to the node is available as
/// the `Deficit` attribute. Flow can also be constrained by the optional `min_flow` metric,
/// which defaults to 0.
///
/// # Available attributes and components
///
/// The enums [`DemandNodeAttribute`] and [`DemandNodeComponent`] define the available
/// attributes and components for this node.
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct DemandNode {
    pub meta: NodeMeta,
    /// Optional local parameters.
    pub parameters: Option<Vec<Parameter>>,
    /// The demand, which is the maximum flow to this node.
    pub max_flow: Metric,
    pub min_flow: Option<Metric>,
    pub cost: Option<Metric>,
}

impl DemandNode {
    const DEFAULT_ATTRIBUTE: DemandNodeAttribute = DemandNodeAttribute::Inflow;
    const DEFAULT_COMPONENT: DemandNodeComponent = DemandNodeComponent::Inflow;

    pub fn default_attribute(&self) -> DemandNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> DemandNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl DemandNode {
    pub fn input_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<UnresolvedNode>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::InputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            Ok(vec![UnresolvedNode::new(self.meta.name.as_str(), None)])
        }
    }

    pub fn output_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<UnresolvedNode>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::OutputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            Ok(vec![UnresolvedNode::new(self.meta.name.as_str(), None)])
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
            DemandNodeComponent::Inflow => UnresolvedNode::new(self.meta.name.as_str(), None),
        };

        Ok(vec![node])
    }

    pub fn create_metric(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        attribute: Option<NodeAttribute>,
    ) -> Result<UnresolvedMetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = match attribute {
            Some(attr) => attr.try_into()?,
            None => Self::DEFAULT_ATTRIBUTE,
        };

        let name = UnresolvedNode::new(self.meta.name.as_str(), None);

        let metric = match attr {
            DemandNodeAttribute::Inflow => UnresolvedMetricF64::NodeInFlow(name),
            DemandNodeAttribute::MaxFlow => UnresolvedMetricF64::NodeMaxFlow(name),
            DemandNodeAttribute::MinFlow => UnresolvedMetricF64::NodeMinFlow(name),
            DemandNodeAttribute::Deficit => {
                let deficit_parameter_name = ParameterName::new("deficit", Some(self.meta.name.as_str()));

                // Create a parameter for the deficit metric if it does not already exist
                if !network.parameters().contains_name(&deficit_parameter_name) {
                    let flow = UnresolvedMetricF64::NodeInFlow(name.clone());
                    let max_flow = UnresolvedMetricF64::NodeMaxFlow(name);
                    let deficit_builder = DeficitParameterBuilder::new(deficit_parameter_name.clone(), flow, max_flow);

                    network.parameters().f64(Box::new(deficit_builder));
                }

                UnresolvedMetricF64::new_parameter_after_else_initial(deficit_parameter_name)
            }
        };

        Ok(metric)
    }

    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        let mut output = pywr_core::node::NodeBuilder::output(self.meta.name.as_str());

        if let Some(cost) = &self.cost {
            let value = cost.load(network, args, Some(&self.meta.name))?;
            output.cost(value);
        }

        let value = self.max_flow.load(network, args, Some(&self.meta.name))?;
        output.max_flow(value);

        if let Some(min_flow) = &self.min_flow {
            let value = min_flow.load(network, args, Some(&self.meta.name))?;
            output.min_flow(value);
        }

        network.node(output);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::DemandNode;

    #[test]
    fn test_demand_requires_max_flow() {
        let data = r#"
            {
                "meta": {
                    "name": "demand1"
                }
            }
            "#;

        assert!(serde_json::from_str::<DemandNode>(data).is_err());

        let data = r#"
            {
                "meta": {
                    "name": "demand1"
                },
                "max_flow": {
                    "type": "Literal",
                    "value": 10.0
                }
            }
            "#;

        let node: DemandNode = serde_json::from_str(data).unwrap();
        assert_eq!(node.meta.name, "demand1");
    }
}
