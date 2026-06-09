use crate::error::{ComponentConversionError, ConversionError};
use crate::metric::Metric;
use crate::nodes::{LossFactor, NodeMeta};
use crate::parameters::{ConstantValue, Parameter};
use crate::v1::try_convert_node_meta;
#[cfg(feature = "core")]
use crate::{
    error::SchemaError,
    network::LoadArgs,
    nodes::{NodeAttribute, NodeComponent, NodeSlot},
};
use crate::{mermaid, node_attribute_subset_enum, node_component_subset_enum};
#[cfg(feature = "core")]
use pywr_core::{
    AggregatedNodeBuilder, aggregated_node::CoefficientFactorsBuilder, metric::UnresolvedMetricF64,
    node::UnresolvedNode, parameters::MuskingumParameterBuilder, parameters::ParameterName,
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
/// A link node representing a river with an optional proportional loss and routing method.
///
/// With no routing method or loss this is simply a link node. With only a loss factor it
/// creates a link node with an output node to represent the loss:
///
#[doc = mermaid!("doc_diagrams/river-no-routing.mmd")]
///
/// With only a routing method it creates an input and output node with an aggregated node
/// to represent the routing:
///
#[doc = mermaid!("doc_diagrams/river-delay.mmd")]
///
/// With both a loss factor and routing method it creates a link node, output node, input node
/// and two aggregated nodes to represent the loss and routing:
///
#[doc = mermaid!("doc_diagrams/river-delay-with-loss.mmd")]
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

    pub fn default_attribute(&self) -> RiverNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> RiverNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl RiverNode {
    /// The sub-name of the output node.
    fn loss_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("loss"))
    }

    /// The name of net flow node.
    fn net_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("net"))
    }
    fn output_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("inflow"))
    }

    fn input_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("outflow"))
    }

    /// The name of the aggregated node to handle the proportional loss.
    fn agg_loss_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("aggregated_loss_node"))
    }

    fn agg_routing_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("aggregated_routing_node"))
    }

    pub fn input_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<UnresolvedNode>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::InputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            let connectors = match (self.loss_factor.is_some(), self.routing_method.is_some()) {
                (false, false) => vec![self.net_sub_name()],
                (true, false) => vec![self.net_sub_name(), self.loss_sub_name()],
                // If there is routing directly to the output node
                _ => vec![self.output_sub_name()],
            };

            Ok(connectors)
        }
    }

    pub fn output_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<UnresolvedNode>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::OutputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            let connectors = match (self.loss_factor.is_some(), self.routing_method.is_some()) {
                // If there is routing, but no loss connect directly from the input node
                (false, true) => vec![self.input_sub_name()],
                _ => vec![self.net_sub_name()],
            };

            Ok(connectors)
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
            RiverNodeComponent::Inflow => {
                match (self.loss_factor.is_some(), self.routing_method.is_some()) {
                    (false, false) => {
                        // Simple link node
                        vec![self.net_sub_name()]
                    }
                    (true, false) => {
                        // Loss, but no routing
                        vec![self.net_sub_name(), self.loss_sub_name()]
                    }
                    (false, true) => {
                        // Routing, but no loss
                        vec![self.output_sub_name()]
                    }
                    (true, true) => {
                        // Both loss and routing
                        vec![self.output_sub_name()]
                    }
                }
            }
            RiverNodeComponent::Outflow => {
                match self.routing_method.is_some() {
                    false => {
                        // Simple link node
                        vec![self.net_sub_name()]
                    }
                    true => {
                        // Routing
                        vec![self.input_sub_name()]
                    }
                }
            }
            RiverNodeComponent::Loss => {
                match self.loss_factor.is_some() {
                    true => vec![self.loss_sub_name()],
                    false => vec![], // No loss node defined, so return empty
                }
            }
        };

        Ok(indices)
    }

    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        // Create nodes based on the presence of loss and routing method
        match (self.loss_factor.is_some(), self.routing_method.is_some()) {
            (false, false) => {
                // Simple link node
                let link_node = pywr_core::node::NodeBuilder::link(self.net_sub_name());
                network.node(link_node);
            }
            (true, false) => {
                // Loss, but no routing
                let river_node = pywr_core::node::NodeBuilder::link(self.net_sub_name());
                let mut loss_node = pywr_core::node::NodeBuilder::output(self.loss_sub_name());
                // The aggregated node factors to handle the loss
                self.add_loss_to_network(network, &mut loss_node, args)?;

                network.node(river_node);
                network.node(loss_node);
            }
            (false, true) => {
                // Routing, but no loss
                let inflow_node = pywr_core::node::NodeBuilder::output(self.output_sub_name());
                let mut outflow_node = pywr_core::node::NodeBuilder::input(self.input_sub_name());

                // Add aggregated node to enforce the routing
                self.add_routing_to_network(network, &mut outflow_node, args)?;

                network.node(inflow_node);
                network.node(outflow_node);
            }
            (true, true) => {
                // Both loss and routing
                let river_node = pywr_core::node::NodeBuilder::link(self.net_sub_name());
                let mut loss_node = pywr_core::node::NodeBuilder::output(self.loss_sub_name());

                // The aggregated node factors to handle the loss
                self.add_loss_to_network(network, &mut loss_node, args)?;

                let inflow_node = pywr_core::node::NodeBuilder::output(self.output_sub_name());
                let mut outflow_node = pywr_core::node::NodeBuilder::input(self.input_sub_name());
                // Add aggregated node to enforce the routing
                self.add_routing_to_network(network, &mut outflow_node, args)?;

                // The input node needs connecting to the main link node and loss node.
                network.connect(self.input_sub_name(), self.net_sub_name());
                network.connect(self.input_sub_name(), self.loss_sub_name());

                network.node(river_node);
                network.node(loss_node);
                network.node(inflow_node);
                network.node(outflow_node);
            }
        }

        Ok(())
    }

    /// If there is a loss factor defined add the relevant aggregated node and corresponding parameters and factors.
    fn add_loss_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        loss_node: &mut pywr_core::node::NodeBuilder,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        if let Some(loss_factor) = &self.loss_factor {
            let factors = loss_factor.load(network, args, Some(&self.meta.name))?;

            match factors {
                Some(relationship) => {
                    let mut agg_node = AggregatedNodeBuilder::new(self.agg_loss_sub_name());

                    agg_node
                        .nodes(vec![self.net_sub_name()])
                        .nodes(vec![self.loss_sub_name()])
                        .relationship(relationship);

                    network.agg_node(agg_node);
                }
                None => {
                    // Loaded a constant zero factor; ensure that the loss node has zero flow
                    loss_node.max_flow(0.0.into());
                }
            }
        }

        Ok(())
    }

    /// If there is routing defined add the relevant aggregated node and corresponding parametrs
    /// and factors.
    fn add_routing_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        input_node: &mut pywr_core::node::NodeBuilder,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        if let Some(routing_method) = &self.routing_method {
            match routing_method {
                RoutingMethod::Delay { delay, initial_value } => {
                    // Create a delay parameter using the node's name as the parent identifier
                    let name = ParameterName::new("delay", Some(self.meta.name.as_str()));

                    let metric = UnresolvedMetricF64::NodeInFlow(self.output_sub_name());

                    let p = pywr_core::parameters::DelayParameterBuilder::new(
                        name.clone(),
                        metric,
                        *delay,
                        initial_value.load(args.tables)?,
                    );
                    network.parameters().f64(Box::new(p));

                    // Apply it as a constraint on the input node.
                    let metric = UnresolvedMetricF64::new_parameter_before(name);

                    input_node.min_flow(metric.clone());
                    input_node.max_flow(metric.clone());
                }
                RoutingMethod::Muskingum {
                    travel_time,
                    weight,
                    initial_condition,
                } => {
                    // Create a Muskingum parameter using the node's name as the parent identifier
                    let name = ParameterName::new("muskingum", Some(self.meta.name.as_str()));
                    let inflow_metric = UnresolvedMetricF64::NodeInFlow(self.output_sub_name());
                    let outflow_metric = UnresolvedMetricF64::NodeOutFlow(self.input_sub_name());

                    let travel_time = travel_time.load(network, args, Some(&self.meta.name))?;
                    let weight = weight.load(network, args, Some(&self.meta.name))?;

                    let muskingum_parameter = MuskingumParameterBuilder::new(
                        name.clone(),
                        inflow_metric,
                        outflow_metric,
                        travel_time,
                        weight,
                        initial_condition.clone().into(),
                    );

                    network.parameters().multi(Box::new(muskingum_parameter));

                    // Set the relationship on the aggregated node to enforce the Muskingum routing
                    let mut factors = CoefficientFactorsBuilder::default();
                    factors
                        .factor(1.0.into())
                        .factor(UnresolvedMetricF64::new_parameter_before_key(
                            name.clone(),
                            "inflow_factor",
                        ))
                        .rhs(UnresolvedMetricF64::new_parameter_before_key(name, "rhs"));

                    let mut agg_node = pywr_core::AggregatedNodeBuilder::new(self.agg_routing_sub_name());
                    agg_node
                        .nodes(vec![self.input_sub_name()])
                        .nodes(vec![self.output_sub_name()])
                        .relationship(Box::new(factors));

                    network.agg_node(agg_node);
                }
            }
        }

        Ok(())
    }

    pub fn create_metric(&self, attribute: Option<NodeAttribute>) -> Result<UnresolvedMetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = match attribute {
            Some(attr) => attr.try_into()?,
            None => Self::DEFAULT_ATTRIBUTE,
        };

        let metric = match attr {
            RiverNodeAttribute::Inflow => {
                let nodes = self.nodes_for_flow_constraints(Some(RiverNodeComponent::Inflow.into()))?;

                UnresolvedMetricF64::MultiNodeInFlow {
                    nodes,
                    name: self.meta.name.to_string(),
                }
            }
            RiverNodeAttribute::Outflow => {
                let nodes = self.nodes_for_flow_constraints(Some(RiverNodeComponent::Outflow.into()))?;

                UnresolvedMetricF64::MultiNodeOutFlow {
                    nodes,
                    name: self.meta.name.to_string(),
                }
            }
            RiverNodeAttribute::Loss => {
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

impl TryFrom<LinkNodeV1> for RiverNode {
    type Error = Box<ComponentConversionError>;

    fn try_from(v1: LinkNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = try_convert_node_meta(v1.meta)?;

        if v1.max_flow.is_some() {
            return Err(Box::new(ComponentConversionError::Node {
                name: meta.name,
                attr: "max_flow".to_string(),
                error: ConversionError::ExtraAttribute {
                    attr: "max_flow".to_string(),
                },
            }));
        }
        if v1.min_flow.is_some() {
            return Err(Box::new(ComponentConversionError::Node {
                name: meta.name,
                attr: "min_flow".to_string(),
                error: ConversionError::ExtraAttribute {
                    attr: "min_flow".to_string(),
                },
            }));
        }
        if v1.cost.is_some() {
            return Err(Box::new(ComponentConversionError::Node {
                name: meta.name,
                attr: "cost".to_string(),
                error: ConversionError::ExtraAttribute {
                    attr: "cost".to_string(),
                },
            }));
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
