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
use pywr_v1_schema::nodes::PiecewiseLinkNode as PiecewiseLinkNodeV1;
use schemars::JsonSchema;

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct PiecewiseLinkStep {
    pub max_flow: Option<Metric>,
    pub min_flow: Option<Metric>,
    pub cost: Option<Metric>,
}

// This macro generates a subset enum for the `PiecewiseLinkNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_attribute_subset_enum! {
    pub enum PiecewiseLinkNodeAttribute {
        Inflow,
        Outflow,
    }
}

node_component_subset_enum! {
    pub enum PiecewiseLinkNodeComponent {
        Inflow,
        Outflow,
    }
}

#[doc = svgbobdoc::transform!(
/// This node is used to create a sequence of link nodes with separate costs and constraints.
///
/// Typically this node is used to model an non-linear cost by providing increasing cost
/// values at different flows limits.
///
/// ```svgbob
///
///            <node>.00    D
///          .------>L ---.
///      U  |             |         D
///     -*--|             |-------->*-
///         |  <node>.01  |
///          '------>L --'
///         :             :
///         :             :
///         :  <node>.n   :
///          '~~~~~~>L ~~'
///
/// ```
///
/// # Available attributes and components
///
/// The enums [`PiecewiseLinkNodeAttribute`] and [`PiecewiseLinkNodeComponent`] define the available
/// attributes and components for this node.
///
)]
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct PiecewiseLinkNode {
    pub meta: NodeMeta,
    /// Optional local parameters.
    pub parameters: Option<Vec<Parameter>>,
    pub steps: Vec<PiecewiseLinkStep>,
}

impl PiecewiseLinkNode {
    const DEFAULT_ATTRIBUTE: PiecewiseLinkNodeAttribute = PiecewiseLinkNodeAttribute::Outflow;
    const DEFAULT_COMPONENT: PiecewiseLinkNodeComponent = PiecewiseLinkNodeComponent::Outflow;

    fn step_sub_name(i: usize) -> Option<String> {
        Some(format!("step-{i:02}"))
    }

    pub fn input_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::InputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            Ok(self
                .steps
                .iter()
                .enumerate()
                .map(|(i, _)| (self.meta.name.as_str(), Self::step_sub_name(i)))
                .collect())
        }
    }
    pub fn output_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::OutputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            Ok(self
                .steps
                .iter()
                .enumerate()
                .map(|(i, _)| (self.meta.name.as_str(), Self::step_sub_name(i)))
                .collect())
        }
    }

    pub fn default_attribute(&self) -> PiecewiseLinkNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> PiecewiseLinkNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl PiecewiseLinkNode {
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
            PiecewiseLinkNodeComponent::Inflow | PiecewiseLinkNodeComponent::Outflow => self
                .steps
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    network
                        .get_node_index_by_name(self.meta.name.as_str(), Self::step_sub_name(i).as_deref())
                        .ok_or_else(|| SchemaError::CoreNodeNotFound {
                            name: self.meta.name.clone(),
                            sub_name: Self::step_sub_name(i),
                        })
                })
                .collect::<Result<Vec<_>, _>>()?,
        };

        Ok(indices)
    }
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        // create a link node for each step
        for (i, _) in self.steps.iter().enumerate() {
            network.add_link_node(self.meta.name.as_str(), Self::step_sub_name(i).as_deref())?;
        }
        Ok(())
    }
    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        for (i, step) in self.steps.iter().enumerate() {
            let sub_name = Self::step_sub_name(i);

            if let Some(cost) = &step.cost {
                let value = cost.load(network, args, Some(&self.meta.name))?;
                network.set_node_cost(self.meta.name.as_str(), sub_name.as_deref(), value.into())?;
            }

            if let Some(max_flow) = &step.max_flow {
                let value = max_flow.load(network, args, Some(&self.meta.name))?;
                network.set_node_max_flow(self.meta.name.as_str(), sub_name.as_deref(), value.into())?;
            }

            if let Some(min_flow) = &step.min_flow {
                let value = min_flow.load(network, args, Some(&self.meta.name))?;
                network.set_node_min_flow(self.meta.name.as_str(), sub_name.as_deref(), value.into())?;
            }
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

        let indices = self
            .steps
            .iter()
            .enumerate()
            .map(|(i, _)| {
                network
                    .get_node_index_by_name(self.meta.name.as_str(), Self::step_sub_name(i).as_deref())
                    .ok_or_else(|| SchemaError::CoreNodeNotFound {
                        name: self.meta.name.clone(),
                        sub_name: Self::step_sub_name(i),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let metric = match attr {
            PiecewiseLinkNodeAttribute::Inflow => MetricF64::MultiNodeInFlow {
                indices,
                name: self.meta.name.to_string(),
            },
            PiecewiseLinkNodeAttribute::Outflow => MetricF64::MultiNodeOutFlow {
                indices,
                name: self.meta.name.to_string(),
            },
        };

        Ok(metric)
    }
}

impl TryFromV1<PiecewiseLinkNodeV1> for PiecewiseLinkNode {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: PiecewiseLinkNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

        let costs = match v1.costs {
            None => vec![None; v1.nsteps],
            Some(v1_costs) => v1_costs
                .into_iter()
                .map(|v| {
                    try_convert_node_attr(
                        &meta.name,
                        "costs",
                        v,
                        parent_node.or(Some(&meta.name)),
                        conversion_data,
                    )
                    .map(Some)
                })
                .collect::<Result<Vec<_>, _>>()?,
        };

        let max_flows = match v1.max_flows {
            None => vec![None; v1.nsteps],
            Some(v1_max_flows) => v1_max_flows
                .into_iter()
                .map(|v| match v {
                    None => Ok(None),
                    Some(v) => {
                        try_convert_node_attr(&meta.name, "max_flows", v, parent_node, conversion_data).map(Some)
                    }
                })
                .collect::<Result<Vec<_>, _>>()?,
        };

        let steps = costs
            .into_iter()
            .zip(max_flows)
            .map(|(cost, max_flow)| PiecewiseLinkStep {
                max_flow,
                min_flow: None,
                cost,
            })
            .collect::<Vec<_>>();

        let n = Self {
            meta,
            parameters: None,
            steps,
        };
        Ok(n)
    }
}
