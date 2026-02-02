use crate::error::SchemaError;
use crate::error::{ComponentConversionError, ConversionError};
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::nodes::{LossFactor, NodeMeta, NodeSlot};
#[cfg(feature = "core")]
use crate::nodes::{NodeAttribute, NodeComponent};
use crate::parameters::{ConstantValue, Parameter};
use crate::{node_attribute_subset_enum, node_component_subset_enum};
#[cfg(feature = "core")]
use pywr_core::{
    aggregated_node::Relationship,
    metric::MetricF64,
    parameters::{MuskingumParameter, ParameterName},
};
use pywr_schema_macros::{PywrVisitAll, skip_serializing_none};
use pywr_v1_schema::nodes::LinkNode as LinkNodeV1;
use schemars::JsonSchema;

// This macro generates a subset enum for the `RiverNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_attribute_subset_enum! {
    pub enum RiverNodeAttribute {
        Inflow,
        Outflow,
        Loss,
    }
}

node_component_subset_enum! {
    pub enum RiverNodeComponent {
        Inflow,
        Outflow,
        Loss,
    }
}

/// The initial condition for the Muskingum routing method.
///
/// - `SteadyState`: Assumes that the inflow and outflow are equal at the first time-step.
/// - `Specified`: Allows the user to specify the initial inflow and outflow values.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll, strum_macros::Display)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum MuskingumInitialCondition {
    SteadyState,
    Specified { inflow: f64, outflow: f64 },
}

#[cfg(feature = "core")]
impl From<MuskingumInitialCondition> for pywr_core::parameters::MuskingumInitialCondition {
    fn from(val: MuskingumInitialCondition) -> Self {
        match val {
            MuskingumInitialCondition::SteadyState => pywr_core::parameters::MuskingumInitialCondition::SteadyState,
            MuskingumInitialCondition::Specified { inflow, outflow } => {
                pywr_core::parameters::MuskingumInitialCondition::Specified { inflow, outflow }
            }
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll, strum_macros::Display)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum RoutingMethod {
    Delay {
        delay: u64,
        initial_value: ConstantValue<f64>,
    },
    Muskingum {
        travel_time: Metric,
        weight: Metric,
        initial_condition: MuskingumInitialCondition,
    },
}

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
#[doc = svgbobdoc::transform!(
/// A link node representing a river with an optional proportional loss and routing method.
///
/// With no routing method or loss this is simply a link node. With only a loss factor it
/// creates a link node with an output node to represent the loss:
///
/// ```svgbob
///
///             <RiverNode>.net    D
///          .-------->L---------->*
///      U  |
///     -*--|
///         !
///         '-.-.-.-.->O
///               <RiverNode>.loss
///
/// ```
///
/// With only a routing method it creates an input and output node with an aggregated node
/// to represent the routing:
///
/// ```svgbob
///
///
///      U                D
///     -*---> O    I --->*
///
/// ```
///
/// With both a loss factor and routing method it creates a link node, output node, input node
/// and two aggregated nodes to represent the loss and routing:
///
/// ```svgbob
///
///                            <RiverNode>.net    D
///                         .-------->L---------->*
///      U                  |
///     -*---> O    I --->*-|
///                         !
///                         '-.-.-.-.->O
///                                 <RiverNode>.loss
///
/// ```
///
/// # Routing methods
///
/// Routing methods can be used to represent the travel time and attenuation of flood waves
/// through a river reach.
///
/// ## Delay routing
///
/// ## Muskingum routing
///
/// # Available attributes and components
///
/// The enums [`RiverNodeAttribute`] and [`RiverNodeComponent`] define the available
/// attributes and components for this node.
///
)]
pub struct RiverNode {
    pub meta: NodeMeta,
    /// Optional local parameters.
    pub parameters: Option<Vec<Parameter>>,
    /// An optional loss. This internally creates an [`crate::nodes::OutputNode`] and
    /// [`pywr_core::nodes::Aggregated`] to handle the loss.
    pub loss_factor: Option<LossFactor>,
    /// The routing method to use.
    pub routing_method: Option<RoutingMethod>,
}

impl RiverNode {
    const DEFAULT_ATTRIBUTE: RiverNodeAttribute = RiverNodeAttribute::Outflow;
    const DEFAULT_COMPONENT: RiverNodeComponent = RiverNodeComponent::Outflow;

    /// The sub-name of the output node.
    fn loss_sub_name() -> Option<&'static str> {
        Some("loss")
    }

    /// The name of net flow node.
    fn net_sub_name() -> Option<&'static str> {
        Some("net")
    }
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
            let connectors = match (self.loss_factor.is_some(), self.routing_method.is_some()) {
                (false, false) => vec![(self.meta.name.as_str(), Self::net_sub_name().map(|s| s.to_string()))],
                (true, false) => vec![
                    (self.meta.name.as_str(), Self::net_sub_name().map(|s| s.to_string())),
                    (self.meta.name.as_str(), Self::loss_sub_name().map(|s| s.to_string())),
                ],
                // If there is routing directly to the output node
                _ => vec![(self.meta.name.as_str(), Self::output_sub_name().map(|s| s.to_string()))],
            };

            Ok(connectors)
        }
    }

    pub fn output_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::OutputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            let connectors = match (self.loss_factor.is_some(), self.routing_method.is_some()) {
                // If there is routing, but no loss connect directly from the input node
                (false, true) => vec![(self.meta.name.as_str(), Self::input_sub_name().map(|s| s.to_string()))],
                _ => vec![(self.meta.name.as_str(), Self::net_sub_name().map(|s| s.to_string()))],
            };

            Ok(connectors)
        }
    }

    pub fn default_attribute(&self) -> RiverNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> RiverNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl RiverNode {
    /// The name of the aggregated node to handle the proportional loss.
    fn agg_loss_sub_name() -> Option<&'static str> {
        Some("aggregated_loss_node")
    }

    fn agg_routing_sub_name() -> Option<&'static str> {
        Some("aggregated_routing_node")
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
            RiverNodeComponent::Inflow => {
                match (self.loss_factor.is_some(), self.routing_method.is_some()) {
                    (false, false) => {
                        // Simple link node
                        vec![
                            network
                                .get_node_index_by_name(self.meta.name.as_str(), Self::net_sub_name())
                                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                                    name: self.meta.name.clone(),
                                    sub_name: Self::net_sub_name().map(String::from),
                                })?,
                        ]
                    }
                    (true, false) => {
                        // Loss, but no routing
                        vec![
                            network
                                .get_node_index_by_name(self.meta.name.as_str(), Self::net_sub_name())
                                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                                    name: self.meta.name.clone(),
                                    sub_name: Self::net_sub_name().map(String::from),
                                })?,
                            network
                                .get_node_index_by_name(self.meta.name.as_str(), Self::loss_sub_name())
                                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                                    name: self.meta.name.clone(),
                                    sub_name: Self::loss_sub_name().map(String::from),
                                })?,
                        ]
                    }
                    (false, true) => {
                        // Routing, but no loss
                        vec![
                            network
                                .get_node_index_by_name(self.meta.name.as_str(), Self::output_sub_name())
                                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                                    name: self.meta.name.clone(),
                                    sub_name: Self::output_sub_name().map(String::from),
                                })?,
                        ]
                    }
                    (true, true) => {
                        // Both loss and routing
                        vec![
                            network
                                .get_node_index_by_name(self.meta.name.as_str(), Self::output_sub_name())
                                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                                    name: self.meta.name.clone(),
                                    sub_name: Self::output_sub_name().map(String::from),
                                })?,
                        ]
                    }
                }
            }
            RiverNodeComponent::Outflow => {
                match self.routing_method.is_some() {
                    false => {
                        // Simple link node
                        vec![
                            network
                                .get_node_index_by_name(self.meta.name.as_str(), Self::net_sub_name())
                                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                                    name: self.meta.name.clone(),
                                    sub_name: Self::net_sub_name().map(String::from),
                                })?,
                        ]
                    }
                    true => {
                        // Routing
                        vec![
                            network
                                .get_node_index_by_name(self.meta.name.as_str(), Self::input_sub_name())
                                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                                    name: self.meta.name.clone(),
                                    sub_name: Self::input_sub_name().map(String::from),
                                })?,
                        ]
                    }
                }
            }
            RiverNodeComponent::Loss => {
                match network.get_node_index_by_name(self.meta.name.as_str(), Self::loss_sub_name()) {
                    Some(idx) => vec![idx],
                    None => return Ok(vec![]), // No loss node defined, so return empty
                }
            }
        };

        Ok(indices)
    }

    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        // Create nodes based on the presence of loss and routing method
        match (self.loss_factor.is_some(), self.routing_method.is_some()) {
            (false, false) => {
                // Simple link node
                network.add_link_node(self.meta.name.as_str(), Self::net_sub_name())?;
            }
            (true, false) => {
                // Loss, but no routing
                let river_idx = network.add_link_node(self.meta.name.as_str(), Self::net_sub_name())?;
                let loss_idx = network.add_output_node(self.meta.name.as_str(), Self::loss_sub_name())?;
                // The aggregated node factors to handle the loss
                network.add_aggregated_node(
                    self.meta.name.as_str(),
                    Self::agg_loss_sub_name(),
                    &[vec![river_idx], vec![loss_idx]],
                    None,
                )?;
            }
            (false, true) => {
                // Routing, but no loss
                let inflow_idx = network.add_output_node(self.meta.name.as_str(), Self::output_sub_name())?;
                let outflow_idx = network.add_input_node(self.meta.name.as_str(), Self::input_sub_name())?;

                // This aggregated node will contain the factors to enforce the Muskingum routing
                network.add_aggregated_node(
                    self.meta.name.as_str(),
                    Self::agg_routing_sub_name(),
                    &[vec![outflow_idx], vec![inflow_idx]],
                    None,
                )?;
            }
            (true, true) => {
                // Both loss and routing
                let river_idx = network.add_link_node(self.meta.name.as_str(), Self::net_sub_name())?;
                let loss_idx = network.add_output_node(self.meta.name.as_str(), Self::loss_sub_name())?;
                // The aggregated node factors to handle the loss
                network.add_aggregated_node(
                    self.meta.name.as_str(),
                    Self::agg_loss_sub_name(),
                    &[vec![river_idx], vec![loss_idx]],
                    None,
                )?;
                let inflow_idx = network.add_output_node(self.meta.name.as_str(), Self::output_sub_name())?;
                let outflow_idx = network.add_input_node(self.meta.name.as_str(), Self::input_sub_name())?;

                // This aggregated node will contain the factors to enforce the Muskingum routing
                network.add_aggregated_node(
                    self.meta.name.as_str(),
                    Self::agg_routing_sub_name(),
                    &[vec![outflow_idx], vec![inflow_idx]],
                    None,
                )?;

                // The input node needs connecting to the main link node and loss node.
                network.connect_nodes(outflow_idx, river_idx)?;
                network.connect_nodes(outflow_idx, loss_idx)?;
            }
        }

        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        if let Some(loss_factor) = &self.loss_factor {
            let factors = loss_factor.load(network, args, Some(&self.meta.name))?;
            if factors.is_none() {
                // Loaded a constant zero factor; ensure that the loss node has zero flow
                network.set_node_max_flow(self.meta.name.as_str(), Self::loss_sub_name(), Some(0.0.into()))?;
            }
            network.set_aggregated_node_relationship(self.meta.name.as_str(), Self::agg_loss_sub_name(), factors)?;
        }

        if let Some(routing_method) = &self.routing_method {
            match routing_method {
                RoutingMethod::Delay { delay, initial_value } => {
                    // Create a delay parameter using the node's name as the parent identifier
                    let name = ParameterName::new("delay", Some(self.meta.name.as_str()));
                    let inflow_idx = network
                        .get_node_index_by_name(self.meta.name.as_str(), Self::output_sub_name())
                        .ok_or_else(|| SchemaError::CoreNodeNotFound {
                            name: self.meta.name.clone(),
                            sub_name: Self::output_sub_name().map(String::from),
                        })?;
                    let metric = MetricF64::NodeInFlow(inflow_idx);

                    let p = pywr_core::parameters::DelayParameter::new(
                        name,
                        metric,
                        *delay,
                        initial_value.load(args.tables)?,
                    );
                    let delay_idx = network.add_parameter(Box::new(p))?;

                    // Apply it as a constraint on the input node.
                    let metric: MetricF64 = delay_idx.into();
                    network.set_node_max_flow(
                        self.meta.name.as_str(),
                        Self::input_sub_name(),
                        metric.clone().into(),
                    )?;
                    network.set_node_min_flow(self.meta.name.as_str(), Self::input_sub_name(), metric.into())?;
                }
                RoutingMethod::Muskingum {
                    travel_time,
                    weight,
                    initial_condition,
                } => {
                    // Create a Muskingum parameter using the node's name as the parent identifier
                    let name = ParameterName::new("muskingum", Some(self.meta.name.as_str()));
                    let inflow_idx = network
                        .get_node_index_by_name(self.meta.name.as_str(), Self::output_sub_name())
                        .ok_or_else(|| SchemaError::CoreNodeNotFound {
                            name: self.meta.name.clone(),
                            sub_name: Self::output_sub_name().map(String::from),
                        })?;
                    let outflow_idx = network
                        .get_node_index_by_name(self.meta.name.as_str(), Self::input_sub_name())
                        .ok_or_else(|| SchemaError::CoreNodeNotFound {
                            name: self.meta.name.clone(),
                            sub_name: Self::input_sub_name().map(String::from),
                        })?;

                    let travel_time = travel_time.load(network, args, Some(&self.meta.name))?;
                    let weight = weight.load(network, args, Some(&self.meta.name))?;

                    let muskingum_parameter = MuskingumParameter::new(
                        name.clone(),
                        MetricF64::NodeInFlow(inflow_idx),
                        MetricF64::NodeOutFlow(outflow_idx),
                        travel_time,
                        weight,
                        initial_condition.clone().into(),
                    );

                    let muskingum_idx = network.add_multi_value_parameter(Box::new(muskingum_parameter))?;

                    // Set the relationship on the aggregated node to enforce the Muskingum routing
                    let factors = Relationship::new_coefficient_factors(
                        &[1.0.into(), (muskingum_idx.clone(), "inflow_factor".to_string()).into()],
                        Some((muskingum_idx, "rhs".to_string()).into()),
                    );

                    network.set_aggregated_node_relationship(
                        self.meta.name.as_str(),
                        Self::agg_routing_sub_name(),
                        Some(factors),
                    )?;
                }
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

        let metric = match attr {
            RiverNodeAttribute::Inflow => {
                let indices =
                    self.node_indices_for_flow_constraints(network, Some(RiverNodeComponent::Inflow.into()))?;

                MetricF64::MultiNodeInFlow {
                    indices,
                    name: self.meta.name.to_string(),
                }
            }
            RiverNodeAttribute::Outflow => {
                let indices =
                    self.node_indices_for_flow_constraints(network, Some(RiverNodeComponent::Outflow.into()))?;

                MetricF64::MultiNodeOutFlow {
                    indices,
                    name: self.meta.name.to_string(),
                }
            }
            RiverNodeAttribute::Loss => {
                match network.get_node_index_by_name(self.meta.name.as_str(), Self::loss_sub_name()) {
                    Some(loss_idx) => MetricF64::NodeInFlow(loss_idx),
                    None => 0.0.into(),
                }
            }
        };

        Ok(metric)
    }
}

impl TryFrom<LinkNodeV1> for RiverNode {
    type Error = ComponentConversionError;

    fn try_from(v1: LinkNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

        if v1.max_flow.is_some() {
            return Err(ComponentConversionError::Node {
                name: meta.name,
                attr: "max_flow".to_string(),
                error: ConversionError::ExtraAttribute {
                    attr: "max_flow".to_string(),
                },
            });
        }
        if v1.min_flow.is_some() {
            return Err(ComponentConversionError::Node {
                name: meta.name,
                attr: "min_flow".to_string(),
                error: ConversionError::ExtraAttribute {
                    attr: "min_flow".to_string(),
                },
            });
        }
        if v1.cost.is_some() {
            return Err(ComponentConversionError::Node {
                name: meta.name,
                attr: "cost".to_string(),
                error: ConversionError::ExtraAttribute {
                    attr: "cost".to_string(),
                },
            });
        }

        let n = Self {
            meta,
            parameters: None,
            loss_factor: None,
            routing_method: None,
        };
        Ok(n)
    }
}
