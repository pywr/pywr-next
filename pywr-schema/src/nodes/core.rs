use crate::error::ComponentConversionError;
use crate::metric::Metric;
use crate::nodes::NodeMeta;
use crate::parameters::Parameter;
use crate::v1::{ConversionData, TryFromV1, try_convert_initial_storage, try_convert_node_attr, try_convert_node_meta};
#[cfg(feature = "core")]
use crate::{
    error::SchemaError,
    network::LoadArgs,
    nodes::{NodeAttribute, NodeComponent, NodeSlot},
};
use crate::{mermaid, node_attribute_subset_enum, node_component_subset_enum};
#[cfg(feature = "core")]
use pywr_core::{
    metric::UnresolvedMetricF64,
    node::{UnresolvedNode, UnresolvedStorageInitialVolume},
    parameters::{DeficitParameterBuilder, ParameterName},
};
use pywr_schema_macros::PywrVisitAll;
use pywr_schema_macros::skip_serializing_none;
use pywr_v1_schema::nodes::{
    BreakLinkNode as BreakLinkNodeV1, CatchmentNode as CatchmentNodeV1, InputNode as InputNodeV1,
    LinkNode as LinkNodeV1, OutputNode as OutputNodeV1, ReservoirNode as ReservoirNodeV1, StorageNode as StorageNodeV1,
};
use schemars::JsonSchema;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};
use tracing::warn;

// This macro generates a subset enum for the `InputNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_attribute_subset_enum! {
    pub enum InputNodeAttribute {
        Outflow,
    }
}

node_component_subset_enum! {
    pub enum InputNodeComponent {
        Outflow,
    }
}

/// A node that represents an input to the model, such as a river inflow or a reservoir inflow.
///
/// Flow is constrained by the `max_flow` and `min_flow` metrics. If `max_flow` is not specified,
/// the flow is unconstrained. If `min_flow` is not specified it defaults to 0.
///
/// # Available attributes and components
///
/// The enums [`InputNodeAttribute`] and [`InputNodeComponent`] define the available
/// attributes and components for this node.
///
/// # Examples
///
/// ```json
#[doc = include_str!("doc_examples/input.json")]
/// ```
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct InputNode {
    pub meta: NodeMeta,
    /// Optional local parameters.
    pub parameters: Option<Vec<Parameter>>,
    pub max_flow: Option<Metric>,
    pub min_flow: Option<Metric>,
    pub cost: Option<Metric>,
}

impl InputNode {
    pub const DEFAULT_ATTRIBUTE: InputNodeAttribute = InputNodeAttribute::Outflow;
    pub const DEFAULT_COMPONENT: InputNodeComponent = InputNodeComponent::Outflow;

    pub fn default_attribute(&self) -> InputNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> InputNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl InputNode {
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
        let attr = match component {
            Some(c) => c.try_into()?,
            None => Self::DEFAULT_COMPONENT,
        };

        let name = match attr {
            InputNodeComponent::Outflow => UnresolvedNode::new(self.meta.name.as_str(), None),
        };

        Ok(vec![name])
    }
    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        let mut builder = pywr_core::node::NodeBuilder::input(self.meta.name.as_str());

        if let Some(cost) = &self.cost {
            let value = cost.load(network, args, Some(&self.meta.name))?;
            builder.cost(value);
        }

        if let Some(max_flow) = &self.max_flow {
            let value = max_flow.load(network, args, Some(&self.meta.name))?;
            builder.max_flow(value);
        }

        if let Some(min_flow) = &self.min_flow {
            let value = min_flow.load(network, args, Some(&self.meta.name))?;
            builder.min_flow(value);
        }

        network.node(builder);

        Ok(())
    }

    pub fn create_metric(&self, attribute: Option<NodeAttribute>) -> Result<UnresolvedMetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = match attribute {
            Some(attr) => attr.try_into()?,
            None => Self::DEFAULT_ATTRIBUTE,
        };

        let name = UnresolvedNode::new(self.meta.name.as_str(), None);

        let metric = match attr {
            InputNodeAttribute::Outflow => UnresolvedMetricF64::NodeOutFlow(name),
        };

        Ok(metric)
    }
}

impl TryFromV1<InputNodeV1> for InputNode {
    type Error = Box<ComponentConversionError>;

    fn try_from_v1(
        v1: InputNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = try_convert_node_meta(v1.meta)?;

        let cost = try_convert_node_attr(&meta.name, "cost", v1.cost, parent_node, conversion_data)?;
        let max_flow = try_convert_node_attr(&meta.name, "max_flow", v1.max_flow, parent_node, conversion_data)?;
        let min_flow = try_convert_node_attr(&meta.name, "min_flow", v1.min_flow, parent_node, conversion_data)?;

        let n = Self {
            meta,
            parameters: None,
            max_flow,
            min_flow,
            cost,
        };
        Ok(n)
    }
}

/// Cost and flow metric for soft node's constraints
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
pub struct SoftConstraint {
    pub cost: Option<Metric>,
    pub flow: Option<Metric>,
}

// This macro generates a subset enum for the `LinkNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_attribute_subset_enum! {
    pub enum LinkNodeAttribute {
        Inflow,
        Outflow,
    }
}

// This macro generates a subset enum for the `LinkNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_component_subset_enum! {
    pub enum LinkNodeComponent {
        Inflow,
        Outflow,
    }
}

/// A node with cost, and min and max flow constraints. The node `[name]`, when connected to an upstream
/// node and downstream node, will look like this on the model schematic:
///
#[doc = mermaid!("doc_diagrams/link-node-simple.mmd")]
///
/// # Soft constraints
/// This node allows setting optional maximum and minimum soft constraints via the `soft_min.flow`
/// and `soft_max.flow` properties. These may be breached depending on the costs set on the
/// optional nodes. However, the combined flow through the internal nodes will always be bound
/// between the `min_flow` and `max_flow` attributes.
///
/// ## Implementation
///
///
/// ### Only `soft_min` is defined
/// Normally the minimum flow is delivered through `[name].soft_min` depending on the cost `soft_min.cost`. Any
/// additional flow goes through `[name]`. Depending on the network demand and the value of `soft_min.cost`,
/// the delivered flow via `[name].soft_min` may go below `soft_min.flow`.
///
#[doc = mermaid!("doc_diagrams/link-node-soft-min-only.mmd")]
///
/// The network is set up as follows:
///  - `[name].soft_max` is not added to the network
///  - `[name].soft_min` is added with `soft_min` data
///  - `[name]` is added with `cost`, `min_flow` is set to 0 and `max_flow` is unconstrained.
///  - An aggregated node is added to ensure that combined flow in `[name].soft_min` and `[name]` never exceeds
///    the hard constraints `min_flow` and `max_flow`.
///
/// ### Only `soft_max` is defined
/// Normally the maximum flow `soft_max.max` is delivered through the `[name].soft_max` node and no flow
/// goes through `[name]`. When needed, based on the value of `soft_max.cost`, the maximum `soft_max.max`
/// value can be breached up to a combined flow of `max_flow`.
///
#[doc = mermaid!("doc_diagrams/link-node-soft-max-only.mmd")]
///
/// The network is set up as follows:
///  - `[name].soft_min` is not added to the network.
///  - `[name]` is added with the cost in `soft_max.cost` (i.e. cost of going above soft max).
///  - `[name].soft_max` is added with max flow of `soft_max.max` and cost of `cost`.
///  - An aggregated node is added to ensure that combined flow in `[name].soft_max` and `[name]` never exceeds
///    the hard constraints `min_flow` and `max_flow`.
///
/// ### Both `soft_min` and `soft_max` are defined
///
#[doc = mermaid!("doc_diagrams/link-node-soft-cons.mmd")]
///
/// The network is set up as follows:
/// - `[name].soft_max`'s flow is unconstrained with a cost equal to `soft_max.cost`.
/// - `[name]`'s flow is unconstrained with a cost equal to `cost`.
/// - `[name].soft_min`'s max flow is constrained to `soft_min.flow` with a cost equal to `soft_min.cost`.
/// - An aggregated node is added with `[name]` and `[name].soft_min` to ensure the max flow does not exceed
///   `soft_max.flow`.
/// - An aggregated node is added with `[name].soft_max`, `[name]` and `[name].soft_min` to ensure the flow is between
///   `min_flow` and `max_flow`.
///
/// # Available attributes and components
///
/// The enums [`LinkNodeAttribute`] and [`LinkNodeComponent`] define the available
/// attributes and components for this node.
///
/// ## Examples
/// Link soft constraints may be used in the following scenarios:
///  1) If the link represents a works and its `max_flow` is constrained by a reservoir rule curve,
///   there may be certain circumstances when over-abstracting may be required in a few occasions to
///   ensure that demand is always met. By setting a high tuned cost via [`SoftConstraint`], this will
///   ensure that the abstraction is breached only when needed.
///  2) If the link represents a works and a minimum flow must be guaranteed, `soft_min` may be set
///   with a negative cost to allow the minimum flow requirement. However, when this cannot be met
///   (for example when the abstraction license or the source runs out), the minimum flow will not
///   be honoured and the solver will find a solution.
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct LinkNode {
    pub meta: NodeMeta,
    /// Optional local parameters.
    pub parameters: Option<Vec<Parameter>>,
    /// The optional maximum flow through the node.
    pub max_flow: Option<Metric>,
    /// The optional minimum flow through the node.
    pub min_flow: Option<Metric>,
    /// The cost.
    pub cost: Option<Metric>,
    /// The minimum soft constraints.
    pub soft_min: Option<SoftConstraint>,
    /// The maximum soft constraints.
    pub soft_max: Option<SoftConstraint>,
}

impl LinkNode {
    const DEFAULT_ATTRIBUTE: LinkNodeAttribute = LinkNodeAttribute::Outflow;
    const DEFAULT_COMPONENT: LinkNodeComponent = LinkNodeComponent::Outflow;

    pub fn default_attribute(&self) -> LinkNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> LinkNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl LinkNode {
    fn soft_min_node_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("soft_min"))
    }

    fn soft_max_node_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("soft_max"))
    }

    pub fn input_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<UnresolvedNode>, SchemaError> {
        if let Some(slot) = slot {
            return Err(SchemaError::InputNodeSlotNotSupported { slot: slot.clone() });
        }

        let mut connectors = vec![UnresolvedNode::new(self.meta.name.as_str(), None)];
        if self.soft_min.is_some() {
            connectors.push(self.soft_min_node_sub_name());
        }
        if self.soft_max.is_some() {
            connectors.push(self.soft_max_node_sub_name());
        }
        Ok(connectors)
    }

    pub fn output_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<UnresolvedNode>, SchemaError> {
        if let Some(slot) = slot {
            return Err(SchemaError::OutputNodeSlotNotSupported { slot: slot.clone() });
        }

        let mut connectors = vec![UnresolvedNode::new(self.meta.name.as_str(), None)];
        if self.soft_min.is_some() {
            connectors.push(self.soft_min_node_sub_name());
        }
        if self.soft_max.is_some() {
            connectors.push(self.soft_max_node_sub_name());
        }
        Ok(connectors)
    }

    fn aggregated_node_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("aggregate_node"))
    }

    /// The aggregated node name of `[name]` and `[name].soft_min` when both soft constraints are provided.
    fn aggregated_node_l_l_min_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("aggregate_node_l_l_min"))
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
            // The same nodes are returned for inflow and outflow
            LinkNodeComponent::Inflow | LinkNodeComponent::Outflow => {
                let name = UnresolvedNode::new(self.meta.name.as_str(), None);

                let mut nodes = vec![name];

                if self.soft_min.is_some() {
                    nodes.push(self.soft_min_node_sub_name());
                }

                if self.soft_max.is_some() {
                    nodes.push(self.soft_max_node_sub_name());
                }

                nodes
            }
        };

        Ok(nodes)
    }
    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        let node_name = self.meta.name.as_str();

        let mut link = pywr_core::node::NodeBuilder::link(self.meta.name.as_str());

        // add soft constrained nodes and aggregated node
        match (&self.soft_min, &self.soft_max) {
            (Some(soft_min), None) => {
                // add L_min and aggregated node for L and L_min
                let mut soft_min_node = pywr_core::node::NodeBuilder::link(self.soft_min_node_sub_name());

                let mut agg_node =
                    pywr_core::aggregated_node::AggregatedNodeBuilder::new(self.aggregated_node_sub_name());

                agg_node.nodes(vec![
                    UnresolvedNode::new(node_name, None),
                    self.soft_min_node_sub_name(),
                ]);

                // add L_min constraints
                if let Some(soft_min_flow) = &soft_min.flow {
                    let value = soft_min_flow.load(network, args, Some(&self.meta.name))?;
                    soft_min_node.max_flow(value);
                }
                if let Some(soft_min_cost) = &soft_min.cost {
                    let value = soft_min_cost.load(network, args, Some(&self.meta.name))?;
                    soft_min_node.cost(value);
                }

                // add cost on L
                if let Some(cost) = &self.cost {
                    let value = cost.load(network, args, Some(&self.meta.name))?;
                    link.cost(value);
                }

                // add constraints on aggregated node
                if let Some(max_flow) = &self.max_flow {
                    let value = max_flow.load(network, args, Some(&self.meta.name))?;
                    agg_node.max_flow(value);
                }
                if let Some(min_flow) = &self.min_flow {
                    let value = min_flow.load(network, args, Some(&self.meta.name))?;
                    agg_node.min_flow(value);
                }

                network.node(link);
                network.node(soft_min_node);
                network.agg_node(agg_node);
            }
            (None, Some(soft_max)) => {
                // add L_max and aggregated node for L and L_max
                let mut soft_max_node = pywr_core::node::NodeBuilder::link(self.soft_max_node_sub_name());

                let mut agg_node =
                    pywr_core::aggregated_node::AggregatedNodeBuilder::new(self.aggregated_node_sub_name());

                agg_node.nodes(vec![
                    UnresolvedNode::new(node_name, None),
                    self.soft_max_node_sub_name(),
                ]);

                // add L_max constraints
                if let Some(cost) = &self.cost {
                    let value = cost.load(network, args, Some(&self.meta.name))?;
                    soft_max_node.cost(value);
                }
                if let Some(soft_max_flow) = &soft_max.flow {
                    let value = soft_max_flow.load(network, args, Some(&self.meta.name))?;
                    soft_max_node.max_flow(value);
                }

                // add constraints on L
                if let Some(soft_max_cost) = &soft_max.cost {
                    let value = soft_max_cost.load(network, args, Some(&self.meta.name))?;
                    link.cost(value);
                }

                // add constraints on aggregated node
                if let Some(max_flow) = &self.max_flow {
                    let value = max_flow.load(network, args, Some(&self.meta.name))?;
                    agg_node.max_flow(value);
                }
                if let Some(min_flow) = &self.min_flow {
                    let value = min_flow.load(network, args, Some(&self.meta.name))?;
                    agg_node.min_flow(value);
                }

                network.node(link);
                network.node(soft_max_node);
                network.agg_node(agg_node);
            }
            (Some(soft_min), Some(soft_max)) => {
                // add L_min and L_max, and aggregated node for L, L_min and L_max
                let mut soft_min_node = pywr_core::node::NodeBuilder::link(self.soft_min_node_sub_name());

                let mut soft_max_node = pywr_core::node::NodeBuilder::link(self.soft_max_node_sub_name());

                let mut agg_node =
                    pywr_core::aggregated_node::AggregatedNodeBuilder::new(self.aggregated_node_sub_name());

                agg_node.nodes(vec![
                    UnresolvedNode::new(node_name, None),
                    self.soft_min_node_sub_name(),
                    self.soft_max_node_sub_name(),
                ]);

                let mut agg_node_l_l =
                    pywr_core::aggregated_node::AggregatedNodeBuilder::new(self.aggregated_node_l_l_min_sub_name());

                agg_node_l_l.nodes(vec![
                    UnresolvedNode::new(node_name, None),
                    self.soft_min_node_sub_name(),
                ]);

                // set L_max constraint
                if let Some(soft_max_cost) = &soft_max.cost {
                    let value = soft_max_cost.load(network, args, Some(&self.meta.name))?;
                    soft_max_node.cost(value);
                }
                // set L constraint
                if let Some(cost) = &self.cost {
                    let value = cost.load(network, args, Some(&self.meta.name))?;
                    link.cost(value);
                }
                // set L_min constraints
                if let Some(soft_min_flow) = &soft_min.flow {
                    let value = soft_min_flow.load(network, args, Some(&self.meta.name))?;
                    soft_min_node.max_flow(value);
                }
                if let Some(soft_min_cost) = &soft_min.cost {
                    let value = soft_min_cost.load(network, args, Some(&self.meta.name))?;
                    soft_min_node.cost(value);
                }

                // add constraints on node aggregating all three nodes
                if let Some(max_flow) = &self.max_flow {
                    let value = max_flow.load(network, args, Some(&self.meta.name))?;
                    agg_node.max_flow(value);
                }
                if let Some(min_flow) = &self.min_flow {
                    let value = min_flow.load(network, args, Some(&self.meta.name))?;
                    agg_node.min_flow(value);
                }

                // add constraints on node aggregating `[name]` and `[name].soft_min`
                if let Some(soft_max_flow) = &soft_max.flow {
                    let value = soft_max_flow.load(network, args, Some(&self.meta.name))?;
                    agg_node.max_flow(value);
                }

                network.node(link);
                network.node(soft_min_node);
                network.node(soft_max_node);
                network.agg_node(agg_node);
                network.agg_node(agg_node_l_l);
            }
            (None, None) => {
                // soft constraints not added. Set constraints for L only
                if let Some(cost) = &self.cost {
                    let value = cost.load(network, args, Some(&self.meta.name))?;
                    link.cost(value);
                }

                if let Some(max_flow) = &self.max_flow {
                    let value = max_flow.load(network, args, Some(&self.meta.name))?;
                    link.max_flow(value);
                }

                if let Some(min_flow) = &self.min_flow {
                    let value = min_flow.load(network, args, Some(&self.meta.name))?;
                    link.min_flow(value);
                }
                network.node(link);
            }
        };

        Ok(())
    }

    pub fn create_metric(&self, attribute: Option<NodeAttribute>) -> Result<UnresolvedMetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = match attribute {
            Some(attr) => attr.try_into()?,
            None => Self::DEFAULT_ATTRIBUTE,
        };

        let node_name = self.meta.name.as_str();
        let link_node = UnresolvedNode::new(node_name, None);

        // combine the flow through the nodes
        let nodes = match (&self.soft_min, &self.soft_max) {
            (Some(_), None) => {
                vec![link_node, self.soft_min_node_sub_name()]
            }
            (None, Some(_)) => {
                vec![link_node, self.soft_max_node_sub_name()]
            }
            (Some(_), Some(_)) => {
                vec![link_node, self.soft_min_node_sub_name(), self.soft_max_node_sub_name()]
            }
            (None, None) => vec![link_node],
        };

        let metric = match attr {
            LinkNodeAttribute::Outflow => UnresolvedMetricF64::MultiNodeInFlow {
                nodes,
                name: self.meta.name.to_string(),
            },
            LinkNodeAttribute::Inflow => UnresolvedMetricF64::MultiNodeOutFlow {
                nodes,
                name: self.meta.name.to_string(),
            },
        };

        Ok(metric)
    }
}

impl TryFromV1<LinkNodeV1> for LinkNode {
    type Error = Box<ComponentConversionError>;

    fn try_from_v1(
        v1: LinkNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = try_convert_node_meta(v1.meta)?;

        let cost = try_convert_node_attr(&meta.name, "cost", v1.cost, parent_node, conversion_data)?;
        let max_flow = try_convert_node_attr(&meta.name, "max_flow", v1.max_flow, parent_node, conversion_data)?;
        let min_flow = try_convert_node_attr(&meta.name, "min_flow", v1.min_flow, parent_node, conversion_data)?;
        // not supported in V1
        let soft_min = None;
        let soft_max = None;

        let n = Self {
            meta,
            parameters: None,
            max_flow,
            min_flow,
            soft_min,
            soft_max,
            cost,
        };
        Ok(n)
    }
}

impl TryFromV1<BreakLinkNodeV1> for LinkNode {
    type Error = Box<ComponentConversionError>;

    fn try_from_v1(
        v1: BreakLinkNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = try_convert_node_meta(v1.meta)?;
        let cost = try_convert_node_attr(&meta.name, "cost", v1.cost, parent_node, conversion_data)?;
        let max_flow = try_convert_node_attr(&meta.name, "max_flow", v1.max_flow, parent_node, conversion_data)?;
        let min_flow = try_convert_node_attr(&meta.name, "min_flow", v1.min_flow, parent_node, conversion_data)?;

        warn!(
            "BreakLinkNode is deprecated. Converting node with name '{}' to a LinkNode",
            meta.name
        );

        let n = Self {
            meta,
            max_flow,
            min_flow,
            cost,
            ..Default::default()
        };
        Ok(n)
    }
}

// This macro generates a subset enum for the `OutputNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_attribute_subset_enum! {
    pub enum OutputNodeAttribute {
        Inflow,
        /// The deficit of the inflow compared to the `max_flow` metric.
        Deficit,
    }
}

node_component_subset_enum! {
    pub enum OutputNodeComponent {
        Inflow,
    }
}

/// A node that represents an output from the model, such as a river estuary or demand centre.
///
/// Flow is constrained by the `max_flow` and `min_flow` metrics. If `max_flow` is not specified,
/// the flow is unconstrained. If `min_flow` is not specified it defaults to 0.
///
/// # Available attributes and components
///
/// The enums [`OutputNodeAttribute`] and [`OutputNodeComponent`] define the available
/// attributes and components for this node.
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct OutputNode {
    pub meta: NodeMeta,
    /// Optional local parameters.
    pub parameters: Option<Vec<Parameter>>,
    pub max_flow: Option<Metric>,
    pub min_flow: Option<Metric>,
    pub cost: Option<Metric>,
}

impl OutputNode {
    const DEFAULT_ATTRIBUTE: OutputNodeAttribute = OutputNodeAttribute::Inflow;
    const DEFAULT_COMPONENT: OutputNodeComponent = OutputNodeComponent::Inflow;

    pub fn default_attribute(&self) -> OutputNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> OutputNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl OutputNode {
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
            OutputNodeComponent::Inflow => UnresolvedNode::new(self.meta.name.as_str(), None),
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
            OutputNodeAttribute::Inflow => UnresolvedMetricF64::NodeInFlow(name),
            OutputNodeAttribute::Deficit => {
                let deficit_parameter_name = ParameterName::new("deficit", Some(self.meta.name.as_str()));

                // Create a parameter for the deficit metric if it does not already exist
                if !network.parameters().contains_name(&deficit_parameter_name) {
                    let flow = UnresolvedMetricF64::NodeInFlow(self.meta.name.as_str().into());
                    let max_flow = UnresolvedMetricF64::NodeMaxFlow(self.meta.name.as_str().into());
                    let deficit_builder = DeficitParameterBuilder::new(deficit_parameter_name.clone(), flow, max_flow);

                    network.parameters().f64(Box::new(deficit_builder));
                }

                UnresolvedMetricF64::new_parameter_after(deficit_parameter_name)
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

        if let Some(max_flow) = &self.max_flow {
            let value = max_flow.load(network, args, Some(&self.meta.name))?;
            output.max_flow(value);
        }

        if let Some(min_flow) = &self.min_flow {
            let value = min_flow.load(network, args, Some(&self.meta.name))?;
            output.min_flow(value);
        }

        network.node(output);

        Ok(())
    }
}

impl TryFromV1<OutputNodeV1> for OutputNode {
    type Error = Box<ComponentConversionError>;

    fn try_from_v1(
        v1: OutputNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = try_convert_node_meta(v1.meta)?;

        let cost = try_convert_node_attr(&meta.name, "cost", v1.cost, parent_node, conversion_data)?;
        let max_flow = try_convert_node_attr(&meta.name, "max_flow", v1.max_flow, parent_node, conversion_data)?;
        let min_flow = try_convert_node_attr(&meta.name, "min_flow", v1.min_flow, parent_node, conversion_data)?;

        let n = Self {
            meta,
            parameters: None,
            max_flow,
            min_flow,
            cost,
        };
        Ok(n)
    }
}

#[derive(
    serde::Deserialize,
    serde::Serialize,
    Clone,
    PartialEq,
    Copy,
    Debug,
    JsonSchema,
    PywrVisitAll,
    Display,
    EnumDiscriminants,
)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(StorageInitialVolumeType))]
pub enum StorageInitialVolume {
    Absolute { volume: f64 },
    Proportional { proportion: f64 },
}

impl Default for StorageInitialVolume {
    fn default() -> Self {
        StorageInitialVolume::Proportional { proportion: 1.0 }
    }
}

#[cfg(feature = "core")]
impl From<StorageInitialVolume> for UnresolvedStorageInitialVolume {
    fn from(v: StorageInitialVolume) -> Self {
        match v {
            StorageInitialVolume::Absolute { volume } => UnresolvedStorageInitialVolume::Absolute(volume),
            StorageInitialVolume::Proportional { proportion } => {
                UnresolvedStorageInitialVolume::Proportional(proportion)
            }
        }
    }
}

// This macro generates a subset enum for the `StorageNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_attribute_subset_enum! {
    pub enum StorageNodeAttribute {
        Volume,
        ProportionalVolume,
        MaxVolume,
    }
}

/// A simple store of water, such as a reservoir or aquifer.
///
/// This node has a `max_volume` and `min_volume` which are used to constraint the flow through
/// the node. If `max_volume` is not specified, the flow is unconstrained. If `min_volume` is not specified,
/// it defaults to 0.
///
/// The `cost` is used as the penalty cost for each unit of net increase in volume in the reservoir.
/// I.e. a negative cost will encourage the reservoir to fill, while a positive cost will encourage
/// it to empty.
///
/// # Available attributes and components
///
/// The enum [`StorageNodeAttribute`] defines the available attributes. There are no components
/// to choose from.
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct StorageNode {
    pub meta: NodeMeta,
    /// Optional local parameters.
    pub parameters: Option<Vec<Parameter>>,
    pub max_volume: Option<Metric>,
    pub min_volume: Option<Metric>,
    pub cost: Option<Metric>,
    pub initial_volume: StorageInitialVolume,
}

impl StorageNode {
    const DEFAULT_ATTRIBUTE: StorageNodeAttribute = StorageNodeAttribute::Volume;

    pub fn default_attribute(&self) -> StorageNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl StorageNode {
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
    pub fn nodes_for_storage_constraints(&self) -> Result<Vec<UnresolvedNode>, SchemaError> {
        let node = UnresolvedNode::new(self.meta.name.as_str(), None);

        Ok(vec![node])
    }

    /// Creates a populated node builder.
    pub(crate) fn make_builder(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
    ) -> Result<pywr_core::node::NodeBuilder, SchemaError> {
        let mut storage = pywr_core::node::NodeBuilder::storage(self.meta.name.as_str());

        storage.initial_volume(self.initial_volume.into());

        if let Some(cost) = &self.cost {
            let value = cost.load(network, args, Some(&self.meta.name))?;
            storage.cost(value);
        }

        if let Some(min_volume) = &self.min_volume {
            let value = min_volume.load(network, args, Some(&self.meta.name))?;
            storage.min_volume(value);
        }

        if let Some(max_volume) = &self.max_volume {
            let value = max_volume.load(network, args, Some(&self.meta.name))?;
            storage.max_volume(value);
        }

        Ok(storage)
    }
    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        let storage = self.make_builder(network, args)?;
        network.node(storage);
        Ok(())
    }

    pub fn create_metric(&self, attribute: Option<NodeAttribute>) -> Result<UnresolvedMetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = match attribute {
            Some(attr) => attr.try_into()?,
            None => Self::DEFAULT_ATTRIBUTE,
        };

        let name = UnresolvedNode::new(self.meta.name.as_str(), None);

        let metric = match attr {
            StorageNodeAttribute::Volume => UnresolvedMetricF64::NodeVolume(name),
            StorageNodeAttribute::MaxVolume => UnresolvedMetricF64::NodeMaxVolume(name),
            StorageNodeAttribute::ProportionalVolume => UnresolvedMetricF64::NodeProportionalVolume(name),
        };

        Ok(metric)
    }
}

impl TryFromV1<StorageNodeV1> for StorageNode {
    type Error = Box<ComponentConversionError>;

    fn try_from_v1(
        v1: StorageNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = try_convert_node_meta(v1.meta)?;

        let cost = try_convert_node_attr(&meta.name, "cost", v1.cost, parent_node, conversion_data)?;
        let max_volume = try_convert_node_attr(&meta.name, "max_volume", v1.max_volume, parent_node, conversion_data)?;
        let min_volume = try_convert_node_attr(&meta.name, "min_volume", v1.min_volume, parent_node, conversion_data)?;

        let initial_volume =
            try_convert_initial_storage(&meta.name, "initial_volume", v1.initial_volume, v1.initial_volume_pc)?;

        let n = Self {
            meta,
            parameters: None,
            max_volume,
            min_volume,
            cost,
            initial_volume,
        };
        Ok(n)
    }
}

impl TryFromV1<ReservoirNodeV1> for StorageNode {
    type Error = Box<ComponentConversionError>;

    fn try_from_v1(
        v1: ReservoirNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = try_convert_node_meta(v1.meta)?;

        let cost = try_convert_node_attr(&meta.name, "cost", v1.cost, parent_node, conversion_data)?;
        let max_volume = try_convert_node_attr(&meta.name, "max_volume", v1.max_volume, parent_node, conversion_data)?;
        let min_volume = try_convert_node_attr(&meta.name, "min_volume", v1.min_volume, parent_node, conversion_data)?;

        let initial_volume =
            try_convert_initial_storage(&meta.name, "initial_volume", v1.initial_volume, v1.initial_volume_pc)?;

        let n = Self {
            meta,
            parameters: None,
            max_volume,
            min_volume,
            cost,
            initial_volume,
        };
        Ok(n)
    }
}

// This macro generates a subset enum for the `CatchmentNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_attribute_subset_enum! {
    pub enum CatchmentNodeAttribute {
        Outflow,

    }
}

node_component_subset_enum! {
    pub enum CatchmentNodeComponent {
        Outflow,
    }
}

/// A node to represent a catchment inflow.
///
/// Catchment nodes create a single [`InputNode`] node in the network, but
/// ensure that the maximum and minimum flow are equal to [`Self::flow`].
///
///
/// # Available attributes and components
///
/// The enums [`CatchmentNodeAttribute`] and [`CatchmentNodeComponent`] define the available
/// attributes and components for this node.
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct CatchmentNode {
    pub meta: NodeMeta,
    /// Optional local parameters.
    pub parameters: Option<Vec<Parameter>>,
    pub flow: Option<Metric>,
    pub cost: Option<Metric>,
}

impl CatchmentNode {
    const DEFAULT_ATTRIBUTE: CatchmentNodeAttribute = CatchmentNodeAttribute::Outflow;
    const DEFAULT_COMPONENT: CatchmentNodeComponent = CatchmentNodeComponent::Outflow;

    pub fn default_attribute(&self) -> CatchmentNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> CatchmentNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl CatchmentNode {
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
            CatchmentNodeComponent::Outflow => UnresolvedNode::new(self.meta.name.as_str(), None),
        };
        Ok(vec![node])
    }
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        let mut input = pywr_core::node::NodeBuilder::input(self.meta.name.as_str());

        if let Some(cost) = &self.cost {
            let value = cost.load(network, args, Some(&self.meta.name))?;
            input.cost(value);
        }

        if let Some(flow) = &self.flow {
            let value = flow.load(network, args, Some(&self.meta.name))?;
            input.min_flow(value.clone());
            input.max_flow(value);
        }

        network.node(input);

        Ok(())
    }

    pub fn create_metric(&self, attribute: Option<NodeAttribute>) -> Result<UnresolvedMetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = match attribute {
            Some(attr) => attr.try_into()?,
            None => Self::DEFAULT_ATTRIBUTE,
        };

        let name = UnresolvedNode::new(self.meta.name.as_str(), None);

        let metric = match attr {
            CatchmentNodeAttribute::Outflow => UnresolvedMetricF64::NodeOutFlow(name),
        };

        Ok(metric)
    }
}

impl TryFromV1<CatchmentNodeV1> for CatchmentNode {
    type Error = Box<ComponentConversionError>;

    fn try_from_v1(
        v1: CatchmentNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = try_convert_node_meta(v1.meta)?;

        let cost = try_convert_node_attr(&meta.name, "cost", v1.cost, parent_node, conversion_data)?;
        let flow = try_convert_node_attr(&meta.name, "min_flow", v1.flow, parent_node, conversion_data)?;

        let n = Self {
            meta,
            parameters: None,
            flow,
            cost,
        };
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use crate::nodes::InputNode;
    use crate::nodes::StorageNode;
    use crate::nodes::core::StorageInitialVolume;

    #[test]
    fn test_input() {
        let data = r#"
            {
                "meta": {
                    "name": "supply1"
                },
                "max_flow": {
                    "type": "Literal",
                    "value": 15.0
                }
            }
            "#;

        let node: InputNode = serde_json::from_str(data).unwrap();

        assert_eq!(node.meta.name, "supply1");
    }

    #[test]
    fn test_storage_initial_volume_absolute() {
        let data = r#"
            {
                "meta": {
                    "name": "storage1"
                },
                "max_volume": {
                  "type": "Literal",
                  "value": 10.0
                },
                "initial_volume": {
                  "type": "Absolute",
                  "volume": 12.0
                }
            }
            "#;

        let storage: StorageNode = serde_json::from_str(data).unwrap();

        assert_eq!(storage.initial_volume, StorageInitialVolume::Absolute { volume: 12.0 });
    }

    #[test]
    fn test_storage_initial_volume_proportional() {
        let data = r#"
            {
                "meta": {
                    "name": "storage1"
                },
                "max_volume": {
                  "type": "Literal",
                  "value": 15.0
                },
                "initial_volume": {
                  "type": "Proportional",
                  "proportion": 0.5
                }
            }
            "#;

        let storage: StorageNode = serde_json::from_str(data).unwrap();

        assert_eq!(
            storage.initial_volume,
            StorageInitialVolume::Proportional { proportion: 0.5 }
        );
    }
}
