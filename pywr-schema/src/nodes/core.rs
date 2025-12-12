use crate::error::ComponentConversionError;
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::network::LoadArgs;
#[cfg(feature = "core")]
use crate::nodes::{NodeAttribute, NodeComponent};
use crate::nodes::{NodeMeta, NodeSlot};
use crate::parameters::Parameter;
use crate::v1::{ConversionData, TryFromV1, try_convert_initial_storage, try_convert_node_attr};
use crate::{node_attribute_subset_enum, node_component_subset_enum};
#[cfg(feature = "core")]
use pywr_core::{
    derived_metric::DerivedMetric, metric::MetricF64, node::StorageInitialVolume as CoreStorageInitialVolume,
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

    pub fn default_attribute(&self) -> InputNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> InputNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl InputNode {
    pub fn node_indices_for_flow_constraints(
        &self,
        network: &pywr_core::network::Network,
        component: Option<NodeComponent>,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        // Use the default component if none is specified
        let attr = match component {
            Some(c) => c.try_into()?,
            None => Self::DEFAULT_COMPONENT,
        };

        let idx = match attr {
            InputNodeComponent::Outflow => network
                .get_node_index_by_name(self.meta.name.as_str(), None)
                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                    name: self.meta.name.clone(),
                    sub_name: None,
                })?,
        };

        Ok(vec![idx])
    }
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        network.add_input_node(self.meta.name.as_str(), None)?;
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

        if let Some(max_flow) = &self.max_flow {
            let value = max_flow.load(network, args, Some(&self.meta.name))?;
            network.set_node_max_flow(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(min_flow) = &self.min_flow {
            let value = min_flow.load(network, args, Some(&self.meta.name))?;
            network.set_node_min_flow(self.meta.name.as_str(), None, value.into())?;
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

        let idx = network
            .get_node_index_by_name(self.meta.name.as_str(), None)
            .ok_or_else(|| SchemaError::CoreNodeNotFound {
                name: self.meta.name.clone(),
                sub_name: None,
            })?;

        let metric = match attr {
            InputNodeAttribute::Outflow => MetricF64::NodeOutFlow(idx),
        };

        Ok(metric)
    }
}

impl TryFromV1<InputNodeV1> for InputNode {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: InputNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

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

#[doc = svgbobdoc::transform!(
/// A node with cost, and min and max flow constraints. The node `L`, when connected to an upstream
/// node `U` and downstream node `D`, will look like this on the model schematic:
///
/// ```svgbob
///
///          U         L         D
///    - - ->*-------->*--------->*- - -
/// ```
///
/// # Soft constraints
/// This node allows setting optional maximum and minimum soft constraints via the `soft_min.flow`
/// and `soft_max.flow` properties. These may be breached depending on the costs set on the
/// optional nodes. However, the combined flow through the internal nodes will always be bound
/// between the `min_flow` and `max_flow` attributes. When the two attributes are provided, the
/// internal representation of the link will look like this:
///
/// ```svgbob
///                <Link>.soft_max
///              .------>L_max -----.
///             |                   |
///          U  |                   |     D
///     - - -*--|-------->L --------|--->*- - -
///             |                   |
///             |                   |
///             '------>L_min -----'
///                <Link>.soft_min
/// ```
/// ## Implementation
///
///
/// ### Only `soft_min` is defined
/// Normally the minimum flow is delivered through `L_min` depending on the cost `soft_min.cost`. Any
/// additional flow goes through `L`. Depending on the network demand and the value of `soft_min.cost`,
/// the delivered flow via `L_min` may go below `soft_min.flow`.
/// ```svgbob
///          U                            D
///     - - -*----------->L ------------>*- - -
///             |                   |
///             |                   |                () <Link>.aggregated_node
///             '------>L_min -----'                         [ L_min, L ]
///                <Link>.soft_min
/// ```
///
/// The network is set up as follows:
///  - `L_max` is not added to the network
///  - `L_min` is added with `soft_min` data
///  - `L` is added with `cost`, `min_flow` is set to 0 and `max_flow` is unconstrained.
///  - An aggregated node is added to ensure that combined flow in `L_min` and `L` never exceeds
///    the hard constraints `min_flow` and `max_flow`.
///
/// ### Only `soft_max` is defined
/// Normally the maximum flow `soft_max.max` is delivered through the `L_max` node and no flow
/// goes through `L`. When needed, based on the value of `soft_max.cost`, the maximum `soft_max.max`
/// value can be breached up to a combined flow of `max_flow`.
/// ```svgbob
///          U                            D
///     - - -*----------->L ------------>*- - -
///             |                   |
///             |                   |                () <Link>.aggregated_node
///             '------>L_max -----'                         [ L_max, L ]
///                <Link>.soft_max
/// ```
///
/// The network is set up as follows:
///  - `L_min` is not added to the network.
///  - `L` is added with the cost in `soft_max.cost` (i.e. cost of going above soft max).
///  - `L_max` is added with max flow of `soft_max.max` and cost of `cost`.
///  - An aggregated node is added to ensure that combined flow in `L_max` and `L` never exceeds
///    the hard constraints `min_flow` and `max_flow`.
///
/// ### Both `soft_min` and `soft_max` are defined
///
/// ```svgbob
///                <Link>.soft_max
///              .------>L_max -----.
///             |                   |                    () <Link>.aggregated_node
///          U  |                   |     D                   [ L_max, L_min, L ]
///     - - -*--|-------->L --------|--->*- - -
///             |                   |                    () <Link>.aggregate_node_l_l_min
///             |                   |                            [ L_min, L ]
///             '------>L_min -----'
///                <Link>.soft_min
/// ```
///
/// The network is set up as follows:
/// - `L_max`'s flow is unconstrained with a cost equal to `soft_max.cost`.
/// - `L`'s flow is unconstrained with a cost equal to `cost`.
/// - `L_min`'s max flow is constrained to `soft_min.flow` with a cost equal to `soft_min.cost`.
/// - An aggregated node is added with `L` and `L_min` to ensure the max flow does not exceed
///   `soft_max.flow`.
/// - An aggregated node is added with `L`, `L_max` and `L_min` to ensure the flow is between
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
)]
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

    fn soft_min_node_sub_name() -> Option<&'static str> {
        Some("soft_min_node")
    }

    fn soft_max_node_sub_name() -> Option<&'static str> {
        Some("soft_max_node")
    }

    pub fn input_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        if let Some(slot) = slot {
            return Err(SchemaError::InputNodeSlotNotSupported { slot: slot.clone() });
        }

        let mut connectors = vec![(self.meta.name.as_str(), None)];
        if self.soft_min.is_some() {
            connectors.push((
                self.meta.name.as_str(),
                Self::soft_min_node_sub_name().map(|s| s.to_string()),
            ));
        }
        if self.soft_max.is_some() {
            connectors.push((
                self.meta.name.as_str(),
                Self::soft_max_node_sub_name().map(|s| s.to_string()),
            ));
        }
        Ok(connectors)
    }

    pub fn output_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        if let Some(slot) = slot {
            return Err(SchemaError::OutputNodeSlotNotSupported { slot: slot.clone() });
        }

        let mut connectors = vec![(self.meta.name.as_str(), None)];
        if self.soft_min.is_some() {
            connectors.push((
                self.meta.name.as_str(),
                Self::soft_min_node_sub_name().map(|s| s.to_string()),
            ));
        }
        if self.soft_max.is_some() {
            connectors.push((
                self.meta.name.as_str(),
                Self::soft_max_node_sub_name().map(|s| s.to_string()),
            ));
        }
        Ok(connectors)
    }

    pub fn default_attribute(&self) -> LinkNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> LinkNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl LinkNode {
    fn aggregated_node_sub_name() -> Option<&'static str> {
        Some("aggregate_node")
    }

    /// The aggregated node name of `L` and `L_min` when both soft constraints are provided.
    fn aggregated_node_l_l_min_sub_name() -> Option<&'static str> {
        Some("aggregate_node_l_l_min")
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

        let indices = match component {
            // The same nodes are returned for inflow and outflow
            LinkNodeComponent::Inflow | LinkNodeComponent::Outflow => {
                let idx = network
                    .get_node_index_by_name(self.meta.name.as_str(), None)
                    .ok_or_else(|| SchemaError::CoreNodeNotFound {
                        name: self.meta.name.clone(),
                        sub_name: None,
                    })?;

                let mut indices = vec![idx];

                if self.soft_min.is_some() {
                    let idx = network
                        .get_node_index_by_name(self.meta.name.as_str(), Self::soft_min_node_sub_name())
                        .ok_or_else(|| SchemaError::CoreNodeNotFound {
                            name: self.meta.name.clone(),
                            sub_name: Self::soft_min_node_sub_name().map(|s| s.to_string()),
                        })?;

                    indices.push(idx);
                }

                if self.soft_max.is_some() {
                    let idx = network
                        .get_node_index_by_name(self.meta.name.as_str(), Self::soft_max_node_sub_name())
                        .ok_or_else(|| SchemaError::CoreNodeNotFound {
                            name: self.meta.name.clone(),
                            sub_name: Self::soft_max_node_sub_name().map(|s| s.to_string()),
                        })?;

                    indices.push(idx);
                }

                indices
            }
        };

        Ok(indices)
    }
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        let node_name = self.meta.name.as_str();
        let link = network.add_link_node(node_name, None)?;
        // add soft constrained nodes and aggregated node
        match (&self.soft_min, &self.soft_max) {
            (Some(_), None) => {
                // add L_min and aggregated node for L and L_min
                let soft_min_node = network.add_link_node(node_name, Self::soft_min_node_sub_name())?;
                network.add_aggregated_node(
                    node_name,
                    Self::aggregated_node_sub_name(),
                    &[vec![link], vec![soft_min_node]],
                    None,
                )?;
            }
            (None, Some(_)) => {
                // add L_max and aggregated node for L and L_max
                let soft_max_node = network.add_link_node(node_name, Self::soft_max_node_sub_name())?;
                network.add_aggregated_node(
                    node_name,
                    Self::aggregated_node_sub_name(),
                    &[vec![link], vec![soft_max_node]],
                    None,
                )?;
            }
            (Some(_), Some(_)) => {
                // add L_min and L_max, and aggregated node for L, L_min and L_max
                let soft_min_node = network.add_link_node(node_name, Self::soft_min_node_sub_name())?;
                let soft_max_node = network.add_link_node(node_name, Self::soft_max_node_sub_name())?;
                network.add_aggregated_node(
                    node_name,
                    Self::aggregated_node_sub_name(),
                    &[vec![link], vec![soft_min_node], vec![soft_max_node]],
                    None,
                )?;
                network.add_aggregated_node(
                    node_name,
                    Self::aggregated_node_l_l_min_sub_name(),
                    &[vec![link], vec![soft_min_node]],
                    None,
                )?;
            }
            (None, None) => {}
        };
        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        let node_name = self.meta.name.as_str();
        match (&self.soft_min, &self.soft_max) {
            (None, None) => {
                // soft constraints not added. Set constraints for L only
                if let Some(cost) = &self.cost {
                    let value = cost.load(network, args, Some(&self.meta.name))?;
                    network.set_node_cost(node_name, None, value.into())?;
                }

                if let Some(max_flow) = &self.max_flow {
                    let value = max_flow.load(network, args, Some(&self.meta.name))?;
                    network.set_node_max_flow(node_name, None, value.into())?;
                }

                if let Some(min_flow) = &self.min_flow {
                    let value = min_flow.load(network, args, Some(&self.meta.name))?;
                    network.set_node_min_flow(node_name, None, value.into())?;
                }
            }
            (Some(soft_min), None) => {
                // add L_min constraints
                if let Some(soft_min_flow) = &soft_min.flow {
                    let value = soft_min_flow.load(network, args, Some(&self.meta.name))?;
                    network.set_node_max_flow(node_name, Self::soft_min_node_sub_name(), value.into())?;
                }
                if let Some(soft_min_cost) = &soft_min.cost {
                    let value = soft_min_cost.load(network, args, Some(&self.meta.name))?;
                    network.set_node_cost(node_name, Self::soft_min_node_sub_name(), value.into())?;
                }

                // add cost on L
                if let Some(cost) = &self.cost {
                    let value = cost.load(network, args, Some(&self.meta.name))?;
                    network.set_node_cost(node_name, None, value.into())?;
                }

                // add constraints on aggregated node
                if let Some(max_flow) = &self.max_flow {
                    let value = max_flow.load(network, args, Some(&self.meta.name))?;
                    network.set_aggregated_node_max_flow(node_name, Self::aggregated_node_sub_name(), value.into())?;
                }
                if let Some(min_flow) = &self.min_flow {
                    let value = min_flow.load(network, args, Some(&self.meta.name))?;
                    network.set_aggregated_node_min_flow(node_name, Self::aggregated_node_sub_name(), value.into())?;
                }
            }
            (None, Some(soft_max)) => {
                // add L_max constraints
                if let Some(cost) = &self.cost {
                    let value = cost.load(network, args, Some(&self.meta.name))?;
                    network.set_node_cost(node_name, Self::soft_max_node_sub_name(), value.into())?;
                }
                if let Some(soft_max_flow) = &soft_max.flow {
                    let value = soft_max_flow.load(network, args, Some(&self.meta.name))?;
                    network.set_node_max_flow(node_name, Self::soft_max_node_sub_name(), value.into())?;
                }

                // add constraints on L
                if let Some(soft_max_cost) = &soft_max.cost {
                    let value = soft_max_cost.load(network, args, Some(&self.meta.name))?;
                    network.set_node_cost(node_name, None, value.into())?;
                }

                // add constraints on aggregated node
                if let Some(max_flow) = &self.max_flow {
                    let value = max_flow.load(network, args, Some(&self.meta.name))?;
                    network.set_aggregated_node_max_flow(node_name, Self::aggregated_node_sub_name(), value.into())?;
                }
                if let Some(min_flow) = &self.min_flow {
                    let value = min_flow.load(network, args, Some(&self.meta.name))?;
                    network.set_aggregated_node_min_flow(node_name, Self::aggregated_node_sub_name(), value.into())?;
                }
            }
            (Some(soft_min), Some(soft_max)) => {
                // set L_max constraint
                if let Some(soft_max_cost) = &soft_max.cost {
                    let value = soft_max_cost.load(network, args, Some(&self.meta.name))?;
                    network.set_node_cost(node_name, Self::soft_max_node_sub_name(), value.into())?;
                }
                // set L constraint
                if let Some(cost) = &self.cost {
                    let value = cost.load(network, args, Some(&self.meta.name))?;
                    network.set_node_cost(node_name, None, value.into())?;
                }
                // set L_min constraints
                if let Some(soft_min_flow) = &soft_min.flow {
                    let value = soft_min_flow.load(network, args, Some(&self.meta.name))?;
                    network.set_node_max_flow(node_name, Self::soft_min_node_sub_name(), value.into())?;
                }
                if let Some(soft_min_cost) = &soft_min.cost {
                    let value = soft_min_cost.load(network, args, Some(&self.meta.name))?;
                    network.set_node_cost(node_name, Self::soft_min_node_sub_name(), value.into())?;
                }

                // add constraints on node aggregating all three nodes
                if let Some(max_flow) = &self.max_flow {
                    let value = max_flow.load(network, args, Some(&self.meta.name))?;
                    network.set_aggregated_node_max_flow(node_name, Self::aggregated_node_sub_name(), value.into())?;
                }
                if let Some(min_flow) = &self.min_flow {
                    let value = min_flow.load(network, args, Some(&self.meta.name))?;
                    network.set_aggregated_node_min_flow(node_name, Self::aggregated_node_sub_name(), value.into())?;
                }

                // add constraints on node aggregating `L` and `L_min`
                if let Some(soft_max_flow) = &soft_max.flow {
                    let value = soft_max_flow.load(network, args, Some(&self.meta.name))?;
                    network.set_aggregated_node_max_flow(
                        node_name,
                        Self::aggregated_node_l_l_min_sub_name(),
                        value.into(),
                    )?;
                }
            }
        };

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

        let node_name = self.meta.name.as_str();
        let link_node =
            network
                .get_node_index_by_name(node_name, None)
                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                    name: node_name.to_string(),
                    sub_name: None,
                })?;

        // combine the flow through the nodes
        let indices = match (&self.soft_min, &self.soft_max) {
            (Some(_), None) => {
                let soft_min_node = network
                    .get_node_index_by_name(node_name, Self::soft_min_node_sub_name())
                    .ok_or_else(|| SchemaError::CoreNodeNotFound {
                        name: node_name.to_string(),
                        sub_name: Self::soft_min_node_sub_name().map(String::from),
                    })?;
                vec![link_node, soft_min_node]
            }
            (None, Some(_)) => {
                let soft_max_node = network
                    .get_node_index_by_name(node_name, Self::soft_max_node_sub_name())
                    .ok_or_else(|| SchemaError::CoreNodeNotFound {
                        name: node_name.to_string(),
                        sub_name: Self::soft_max_node_sub_name().map(String::from),
                    })?;
                vec![link_node, soft_max_node]
            }
            (Some(_), Some(_)) => {
                let soft_min_node = network
                    .get_node_index_by_name(node_name, Self::soft_min_node_sub_name())
                    .ok_or_else(|| SchemaError::CoreNodeNotFound {
                        name: node_name.to_string(),
                        sub_name: Self::soft_min_node_sub_name().map(String::from),
                    })?;
                let soft_max_node = network
                    .get_node_index_by_name(node_name, Self::soft_max_node_sub_name())
                    .ok_or_else(|| SchemaError::CoreNodeNotFound {
                        name: node_name.to_string(),
                        sub_name: Self::soft_max_node_sub_name().map(String::from),
                    })?;
                vec![link_node, soft_min_node, soft_max_node]
            }
            (None, None) => vec![link_node],
        };

        let metric = match attr {
            LinkNodeAttribute::Outflow => MetricF64::MultiNodeInFlow {
                indices,
                name: self.meta.name.to_string(),
            },
            LinkNodeAttribute::Inflow => MetricF64::MultiNodeOutFlow {
                indices,
                name: self.meta.name.to_string(),
            },
        };

        Ok(metric)
    }
}

impl TryFromV1<LinkNodeV1> for LinkNode {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: LinkNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

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
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: BreakLinkNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();
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

    pub fn default_attribute(&self) -> OutputNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> OutputNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl OutputNode {
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
            OutputNodeComponent::Inflow => network
                .get_node_index_by_name(self.meta.name.as_str(), None)
                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                    name: self.meta.name.clone(),
                    sub_name: None,
                })?,
        };

        Ok(vec![idx])
    }
    pub fn create_metric(
        &self,
        network: &mut pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
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
            OutputNodeAttribute::Inflow => MetricF64::NodeInFlow(idx),
            OutputNodeAttribute::Deficit => {
                let dm = DerivedMetric::NodeInFlowDeficit(idx);
                let dm_idx = network.add_derived_metric(dm);
                MetricF64::DerivedMetric(dm_idx)
            }
        };

        Ok(metric)
    }

    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        network.add_output_node(self.meta.name.as_str(), None)?;
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

        if let Some(max_flow) = &self.max_flow {
            let value = max_flow.load(network, args, Some(&self.meta.name))?;
            network.set_node_max_flow(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(min_flow) = &self.min_flow {
            let value = min_flow.load(network, args, Some(&self.meta.name))?;
            network.set_node_min_flow(self.meta.name.as_str(), None, value.into())?;
        }

        Ok(())
    }
}

impl TryFromV1<OutputNodeV1> for OutputNode {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: OutputNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

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
impl From<StorageInitialVolume> for CoreStorageInitialVolume {
    fn from(v: StorageInitialVolume) -> Self {
        match v {
            StorageInitialVolume::Absolute { volume } => CoreStorageInitialVolume::Absolute(volume),
            StorageInitialVolume::Proportional { proportion } => CoreStorageInitialVolume::Proportional(proportion),
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

    pub fn default_attribute(&self) -> StorageNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl StorageNode {
    pub fn node_indices_for_storage_constraints(
        &self,
        network: &pywr_core::network::Network,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        let idx = network
            .get_node_index_by_name(self.meta.name.as_str(), None)
            .ok_or_else(|| SchemaError::CoreNodeNotFound {
                name: self.meta.name.clone(),
                sub_name: None,
            })?;
        Ok(vec![idx])
    }
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        // Add the node with no constraints
        network.add_storage_node(self.meta.name.as_str(), None, self.initial_volume.into(), None, None)?;
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

        if let Some(min_volume) = &self.min_volume {
            let value = min_volume.load(network, args, Some(&self.meta.name))?;
            network.set_node_min_volume(self.meta.name.as_str(), None, Some(value.try_into()?))?;
        }

        if let Some(max_volume) = &self.max_volume {
            let value = max_volume.load(network, args, Some(&self.meta.name))?;
            network.set_node_max_volume(self.meta.name.as_str(), None, Some(value.try_into()?))?;
        }

        Ok(())
    }

    pub fn create_metric(
        &self,
        network: &mut pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
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
            StorageNodeAttribute::Volume => MetricF64::NodeVolume(idx),
            StorageNodeAttribute::MaxVolume => MetricF64::NodeMaxVolume(idx),
            StorageNodeAttribute::ProportionalVolume => {
                let dm = DerivedMetric::NodeProportionalVolume(idx);
                let derived_metric_idx = network.add_derived_metric(dm);
                MetricF64::DerivedMetric(derived_metric_idx)
            }
        };

        Ok(metric)
    }
}

impl TryFromV1<StorageNodeV1> for StorageNode {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: StorageNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

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
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: ReservoirNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

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

#[doc = svgbobdoc::transform!(
/// A node to represent a catchment inflow.
///
/// Catchment nodes create a single [`InputNode`] node in the network, but
/// ensure that the maximum and minimum flow are equal to [`Self::flow`].
///
/// ```svgbob
///  <node>     D
///     *----->*- - -
/// ```
///
/// # Available attributes and components
///
/// The enums [`CatchmentNodeAttribute`] and [`CatchmentNodeComponent`] define the available
/// attributes and components for this node.
///
)]
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

    pub fn default_attribute(&self) -> CatchmentNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> CatchmentNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl CatchmentNode {
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
            CatchmentNodeComponent::Outflow => network
                .get_node_index_by_name(self.meta.name.as_str(), None)
                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                    name: self.meta.name.clone(),
                    sub_name: None,
                })?,
        };
        Ok(vec![idx])
    }
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        network.add_input_node(self.meta.name.as_str(), None)?;
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

        if let Some(flow) = &self.flow {
            let value = flow.load(network, args, Some(&self.meta.name))?;
            network.set_node_min_flow(self.meta.name.as_str(), None, value.clone().into())?;
            network.set_node_max_flow(self.meta.name.as_str(), None, value.into())?;
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

        let idx = network
            .get_node_index_by_name(self.meta.name.as_str(), None)
            .ok_or_else(|| SchemaError::CoreNodeNotFound {
                name: self.meta.name.clone(),
                sub_name: None,
            })?;

        let metric = match attr {
            CatchmentNodeAttribute::Outflow => MetricF64::NodeOutFlow(idx),
        };

        Ok(metric)
    }
}

impl TryFromV1<CatchmentNodeV1> for CatchmentNode {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: CatchmentNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

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
