use crate::error::ConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::{Metric, SimpleNodeReference};
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::nodes::{NodeAttribute, NodeMeta};

use crate::parameters::TryIntoV2Parameter;
#[cfg(feature = "core")]
use pywr_core::{
    derived_metric::DerivedMetric, metric::MetricF64, node::StorageInitialVolume as CoreStorageInitialVolume,
};
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::nodes::{
    AggregatedNode as AggregatedNodeV1, AggregatedStorageNode as AggregatedStorageNodeV1,
    CatchmentNode as CatchmentNodeV1, InputNode as InputNodeV1, LinkNode as LinkNodeV1, OutputNode as OutputNodeV1,
    ReservoirNode as ReservoirNodeV1, StorageNode as StorageNodeV1,
};
use schemars::JsonSchema;

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct InputNode {
    pub meta: NodeMeta,
    pub max_flow: Option<Metric>,
    pub min_flow: Option<Metric>,
    pub cost: Option<Metric>,
}

impl InputNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Outflow;

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }
    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl InputNode {
    pub fn node_indices_for_constraints(
        &self,
        network: &pywr_core::network::Network,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        let idx = network.get_node_index_by_name(self.meta.name.as_str(), None)?;
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
            let value = cost.load(network, args)?;
            network.set_node_cost(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(max_flow) = &self.max_flow {
            let value = max_flow.load(network, args)?;
            network.set_node_max_flow(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(min_flow) = &self.min_flow {
            let value = min_flow.load(network, args)?;
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
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let idx = network.get_node_index_by_name(self.meta.name.as_str(), None)?;

        let metric = match attr {
            NodeAttribute::Outflow => MetricF64::NodeOutFlow(idx),
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    ty: "InputNode".to_string(),
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }
}

impl TryFrom<InputNodeV1> for InputNode {
    type Error = ConversionError;

    fn try_from(v1: InputNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();
        let mut unnamed_count = 0;

        let max_flow = v1
            .max_flow
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let min_flow = v1
            .min_flow
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;
        let cost = v1
            .cost
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let n = Self {
            meta,
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
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct LinkNode {
    pub meta: NodeMeta,
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
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Outflow;

    fn soft_min_node_sub_name() -> Option<&'static str> {
        Some("soft_min_node")
    }

    fn soft_max_node_sub_name() -> Option<&'static str> {
        Some("soft_max_node")
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
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
        connectors
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
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
        connectors
    }

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
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

    pub fn node_indices_for_constraints(
        &self,
        network: &pywr_core::network::Network,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        let idx = network.get_node_index_by_name(self.meta.name.as_str(), None)?;
        Ok(vec![idx])
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
                    let value = cost.load(network, args)?;
                    network.set_node_cost(node_name, None, value.into())?;
                }

                if let Some(max_flow) = &self.max_flow {
                    let value = max_flow.load(network, args)?;
                    network.set_node_max_flow(node_name, None, value.into())?;
                }

                if let Some(min_flow) = &self.min_flow {
                    let value = min_flow.load(network, args)?;
                    network.set_node_min_flow(node_name, None, value.into())?;
                }
            }
            (Some(soft_min), None) => {
                // add L_min constraints
                if let Some(soft_min_flow) = &soft_min.flow {
                    let value = soft_min_flow.load(network, args)?;
                    network.set_node_max_flow(node_name, Self::soft_min_node_sub_name(), value.into())?;
                }
                if let Some(soft_min_cost) = &soft_min.cost {
                    let value = soft_min_cost.load(network, args)?;
                    network.set_node_cost(node_name, Self::soft_min_node_sub_name(), value.into())?;
                }

                // add cost on L
                if let Some(cost) = &self.cost {
                    let value = cost.load(network, args)?;
                    network.set_node_cost(node_name, None, value.into())?;
                }

                // add constraints on aggregated node
                if let Some(max_flow) = &self.max_flow {
                    let value = max_flow.load(network, args)?;
                    network.set_aggregated_node_max_flow(node_name, Self::aggregated_node_sub_name(), value.into())?;
                }
                if let Some(min_flow) = &self.min_flow {
                    let value = min_flow.load(network, args)?;
                    network.set_aggregated_node_min_flow(node_name, Self::aggregated_node_sub_name(), value.into())?;
                }
            }
            (None, Some(soft_max)) => {
                // add L_max constraints
                if let Some(cost) = &self.cost {
                    let value = cost.load(network, args)?;
                    network.set_node_cost(node_name, Self::soft_max_node_sub_name(), value.into())?;
                }
                if let Some(soft_max_flow) = &soft_max.flow {
                    let value = soft_max_flow.load(network, args)?;
                    network.set_node_max_flow(node_name, Self::soft_max_node_sub_name(), value.into())?;
                }

                // add constraints on L
                if let Some(soft_max_cost) = &soft_max.cost {
                    let value = soft_max_cost.load(network, args)?;
                    network.set_node_cost(node_name, None, value.into())?;
                }

                // add constraints on aggregated node
                if let Some(max_flow) = &self.max_flow {
                    let value = max_flow.load(network, args)?;
                    network.set_aggregated_node_max_flow(node_name, Self::aggregated_node_sub_name(), value.into())?;
                }
                if let Some(min_flow) = &self.min_flow {
                    let value = min_flow.load(network, args)?;
                    network.set_aggregated_node_min_flow(node_name, Self::aggregated_node_sub_name(), value.into())?;
                }
            }
            (Some(soft_min), Some(soft_max)) => {
                // set L_max constraint
                if let Some(soft_max_cost) = &soft_max.cost {
                    let value = soft_max_cost.load(network, args)?;
                    network.set_node_cost(node_name, Self::soft_max_node_sub_name(), value.into())?;
                }
                // set L constraint
                if let Some(cost) = &self.cost {
                    let value = cost.load(network, args)?;
                    network.set_node_cost(node_name, None, value.into())?;
                }
                // set L_min constraints
                if let Some(soft_min_flow) = &soft_min.flow {
                    let value = soft_min_flow.load(network, args)?;
                    network.set_node_max_flow(node_name, Self::soft_min_node_sub_name(), value.into())?;
                }
                if let Some(soft_min_cost) = &soft_min.cost {
                    let value = soft_min_cost.load(network, args)?;
                    network.set_node_cost(node_name, Self::soft_min_node_sub_name(), value.into())?;
                }

                // add constraints on node aggregating all three nodes
                if let Some(max_flow) = &self.max_flow {
                    let value = max_flow.load(network, args)?;
                    network.set_aggregated_node_max_flow(node_name, Self::aggregated_node_sub_name(), value.into())?;
                }
                if let Some(min_flow) = &self.min_flow {
                    let value = min_flow.load(network, args)?;
                    network.set_aggregated_node_min_flow(node_name, Self::aggregated_node_sub_name(), value.into())?;
                }

                // add constraints on node aggregating `L` and `L_min`
                if let Some(soft_max_flow) = &soft_max.flow {
                    let value = soft_max_flow.load(network, args)?;
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
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);
        let node_name = self.meta.name.as_str();
        let link_node = network.get_node_index_by_name(node_name, None)?;

        // combine the flow through the nodes
        let indices = match (&self.soft_min, &self.soft_max) {
            (Some(_), None) => {
                let soft_min_node = network.get_node_index_by_name(node_name, Self::soft_min_node_sub_name())?;
                vec![link_node, soft_min_node]
            }
            (None, Some(_)) => {
                let soft_max_node = network.get_node_index_by_name(node_name, Self::soft_max_node_sub_name())?;
                vec![link_node, soft_max_node]
            }
            (Some(_), Some(_)) => {
                let soft_min_node = network.get_node_index_by_name(node_name, Self::soft_min_node_sub_name())?;
                let soft_max_node = network.get_node_index_by_name(node_name, Self::soft_max_node_sub_name())?;
                vec![link_node, soft_min_node, soft_max_node]
            }
            (None, None) => vec![link_node],
        };

        let metric = match attr {
            NodeAttribute::Outflow => MetricF64::MultiNodeInFlow {
                indices,
                name: self.meta.name.to_string(),
            },
            NodeAttribute::Inflow => MetricF64::MultiNodeOutFlow {
                indices,
                name: self.meta.name.to_string(),
            },
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    ty: "LinkNode".to_string(),
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }
}

impl TryFrom<LinkNodeV1> for LinkNode {
    type Error = ConversionError;

    fn try_from(v1: LinkNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();
        let mut unnamed_count = 0;

        let max_flow = v1
            .max_flow
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;
        let min_flow = v1
            .min_flow
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;
        let cost = v1
            .cost
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;
        // not supported in V1
        let soft_min = None;
        let soft_max = None;

        let n = Self {
            meta,
            max_flow,
            min_flow,
            soft_min,
            soft_max,
            cost,
        };
        Ok(n)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct OutputNode {
    pub meta: NodeMeta,
    pub max_flow: Option<Metric>,
    pub min_flow: Option<Metric>,
    pub cost: Option<Metric>,
}

impl OutputNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Inflow;

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl OutputNode {
    pub fn node_indices_for_constraints(
        &self,
        network: &pywr_core::network::Network,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        let idx = network.get_node_index_by_name(self.meta.name.as_str(), None)?;
        Ok(vec![idx])
    }
    pub fn create_metric(
        &self,
        network: &mut pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<MetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let idx = network.get_node_index_by_name(self.meta.name.as_str(), None)?;

        let metric = match attr {
            NodeAttribute::Inflow => MetricF64::NodeInFlow(idx),
            NodeAttribute::Deficit => {
                let dm = DerivedMetric::NodeInFlowDeficit(idx);
                let dm_idx = network.add_derived_metric(dm);
                MetricF64::DerivedMetric(dm_idx)
            }
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    ty: "OutputNode".to_string(),
                    name: self.meta.name.clone(),
                    attr,
                })
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
            let value = cost.load(network, args)?;
            network.set_node_cost(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(max_flow) = &self.max_flow {
            let value = max_flow.load(network, args)?;
            network.set_node_max_flow(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(min_flow) = &self.min_flow {
            let value = min_flow.load(network, args)?;
            network.set_node_min_flow(self.meta.name.as_str(), None, value.into())?;
        }

        Ok(())
    }
}

impl TryFrom<OutputNodeV1> for OutputNode {
    type Error = ConversionError;

    fn try_from(v1: OutputNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();
        let mut unnamed_count = 0;

        let max_flow = v1
            .max_flow
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;
        let min_flow = v1
            .min_flow
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;
        let cost = v1
            .cost
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let n = Self {
            meta,
            max_flow,
            min_flow,
            cost,
        };
        Ok(n)
    }
}

#[derive(
    serde::Deserialize, serde::Serialize, Clone, PartialEq, Copy, Debug, JsonSchema, PywrVisitAll, strum_macros::Display,
)]
pub enum StorageInitialVolume {
    Absolute(f64),
    Proportional(f64),
}

impl Default for StorageInitialVolume {
    fn default() -> Self {
        StorageInitialVolume::Proportional(1.0)
    }
}

#[cfg(feature = "core")]
impl From<StorageInitialVolume> for CoreStorageInitialVolume {
    fn from(v: StorageInitialVolume) -> Self {
        match v {
            StorageInitialVolume::Absolute(v) => CoreStorageInitialVolume::Absolute(v),
            StorageInitialVolume::Proportional(v) => CoreStorageInitialVolume::Proportional(v),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct StorageNode {
    pub meta: NodeMeta,
    pub max_volume: Option<Metric>,
    pub min_volume: Option<Metric>,
    pub cost: Option<Metric>,
    pub initial_volume: StorageInitialVolume,
}

impl StorageNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Volume;

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl StorageNode {
    pub fn node_indices_for_constraints(
        &self,
        network: &pywr_core::network::Network,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        let idx = network.get_node_index_by_name(self.meta.name.as_str(), None)?;
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
            let value = cost.load(network, args)?;
            network.set_node_cost(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(min_volume) = &self.min_volume {
            let value = min_volume.load(network, args)?;
            network.set_node_min_volume(self.meta.name.as_str(), None, Some(value.try_into()?))?;
        }

        if let Some(max_volume) = &self.max_volume {
            let value = max_volume.load(network, args)?;
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
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let idx = network.get_node_index_by_name(self.meta.name.as_str(), None)?;

        let metric = match attr {
            NodeAttribute::Volume => MetricF64::NodeVolume(idx),
            NodeAttribute::ProportionalVolume => {
                let dm = DerivedMetric::NodeProportionalVolume(idx);
                let derived_metric_idx = network.add_derived_metric(dm);
                MetricF64::DerivedMetric(derived_metric_idx)
            }
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    ty: "StorageNode".to_string(),
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }
}

impl TryFrom<StorageNodeV1> for StorageNode {
    type Error = ConversionError;

    fn try_from(v1: StorageNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();
        let mut unnamed_count = 0;

        let cost = v1
            .cost
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()
            .map_err(|source| ConversionError::NodeAttribute {
                attr: "cost".to_string(),
                name: meta.name.clone(),
                source: Box::new(source),
            })?;

        let max_volume = v1
            .max_volume
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()
            .map_err(|source| ConversionError::NodeAttribute {
                attr: "max_volume".to_string(),
                name: meta.name.clone(),
                source: Box::new(source),
            })?;

        let min_volume = v1
            .min_volume
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()
            .map_err(|source| ConversionError::NodeAttribute {
                attr: "min_volume".to_string(),
                name: meta.name.clone(),
                source: Box::new(source),
            })?;

        let initial_volume = if let Some(v) = v1.initial_volume {
            StorageInitialVolume::Absolute(v)
        } else if let Some(v) = v1.initial_volume_pc {
            StorageInitialVolume::Proportional(v)
        } else {
            return Err(ConversionError::MissingAttribute {
                name: meta.name,
                attrs: vec!["initial_volume".to_string(), "initial_volume_pc".to_string()],
            });
        };

        let n = Self {
            meta,
            max_volume,
            min_volume,
            cost,
            initial_volume,
        };
        Ok(n)
    }
}

impl TryFrom<ReservoirNodeV1> for StorageNode {
    type Error = ConversionError;

    fn try_from(v1: ReservoirNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();
        let mut unnamed_count = 0;

        let cost = v1
            .cost
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let max_volume = v1
            .max_volume
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let min_volume = v1
            .min_volume
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let initial_volume = if let Some(v) = v1.initial_volume {
            StorageInitialVolume::Absolute(v)
        } else if let Some(v) = v1.initial_volume_pc {
            StorageInitialVolume::Proportional(v)
        } else {
            StorageInitialVolume::default()
        };

        let n = Self {
            meta,
            max_volume,
            min_volume,
            cost,
            initial_volume,
        };
        Ok(n)
    }
}

#[doc = svgbobdoc::transform!(
/// This is used to represent a catchment inflow.
///
/// Catchment nodes create a single [`crate::node::InputNode`] node in the network, but
/// ensure that the maximum and minimum flow are equal to [`Self::flow`].
///
/// ```svgbob
///  <node>     D
///     *----->*- - -
/// ```
///
)]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct CatchmentNode {
    pub meta: NodeMeta,
    pub flow: Option<Metric>,
    pub cost: Option<Metric>,
}

impl CatchmentNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Outflow;

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl CatchmentNode {
    pub fn node_indices_for_constraints(
        &self,
        network: &pywr_core::network::Network,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        let idx = network.get_node_index_by_name(self.meta.name.as_str(), None)?;
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
            let value = cost.load(network, args)?;
            network.set_node_cost(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(flow) = &self.flow {
            let value = flow.load(network, args)?;
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
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let idx = network.get_node_index_by_name(self.meta.name.as_str(), None)?;

        let metric = match attr {
            NodeAttribute::Outflow => MetricF64::NodeOutFlow(idx),
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    ty: "CatchmentNode".to_string(),
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }
}

impl TryFrom<CatchmentNodeV1> for CatchmentNode {
    type Error = ConversionError;

    fn try_from(v1: CatchmentNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();
        let mut unnamed_count = 0;

        let flow = v1
            .flow
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;
        let cost = v1
            .cost
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let n = Self { meta, flow, cost };
        Ok(n)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll, strum_macros::Display)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Relationship {
    Proportion {
        factors: Vec<Metric>,
    },
    Ratio {
        factors: Vec<Metric>,
    },
    Exclusive {
        min_active: Option<usize>,
        max_active: Option<usize>,
    },
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct AggregatedNode {
    pub meta: NodeMeta,
    pub nodes: Vec<SimpleNodeReference>,
    pub max_flow: Option<Metric>,
    pub min_flow: Option<Metric>,
    pub relationship: Option<Relationship>,
}

impl AggregatedNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Outflow;

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        // Not connectable
        // TODO this should be a trait? And error if you try to connect to a non-connectable node.
        vec![]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        // Not connectable
        vec![]
    }

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl AggregatedNode {
    pub fn node_indices_for_constraints(
        &self,
        network: &pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        let indices = self
            .nodes
            .iter()
            .map(|node_ref| {
                args.schema
                    .get_node_by_name(&node_ref.name)
                    .ok_or_else(|| SchemaError::NodeNotFound(node_ref.name.to_string()))?
                    .node_indices_for_constraints(network, args)
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();
        Ok(indices)
    }
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network, args: &LoadArgs) -> Result<(), SchemaError> {
        let nodes: Vec<Vec<_>> = self
            .nodes
            .iter()
            .map(|node_ref| {
                let node = args
                    .schema
                    .get_node_by_name(&node_ref.name)
                    .ok_or_else(|| SchemaError::NodeNotFound(node_ref.name.to_string()))?;
                node.node_indices_for_constraints(network, args)
            })
            .collect::<Result<Vec<_>, _>>()?;

        // We initialise with no factors, but will update them in the `set_constraints` method
        // once all the parameters are loaded.
        network.add_aggregated_node(self.meta.name.as_str(), None, nodes.as_slice(), None)?;
        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        if let Some(max_flow) = &self.max_flow {
            let value = max_flow.load(network, args)?;
            network.set_aggregated_node_max_flow(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(min_flow) = &self.min_flow {
            let value = min_flow.load(network, args)?;
            network.set_aggregated_node_min_flow(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(relationship) = &self.relationship {
            let r = match relationship {
                Relationship::Proportion { factors } => {
                    pywr_core::aggregated_node::Relationship::new_proportion_factors(
                        &factors
                            .iter()
                            .map(|f| f.load(network, args))
                            .collect::<Result<Vec<_>, _>>()?,
                    )
                }
                Relationship::Ratio { factors } => pywr_core::aggregated_node::Relationship::new_ratio_factors(
                    &factors
                        .iter()
                        .map(|f| f.load(network, args))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
                Relationship::Exclusive { min_active, max_active } => {
                    pywr_core::aggregated_node::Relationship::new_exclusive(
                        min_active.unwrap_or(0),
                        max_active.unwrap_or(1),
                    )
                }
            };

            network.set_aggregated_node_relationship(self.meta.name.as_str(), None, Some(r))?;
        }

        Ok(())
    }

    pub fn create_metric(
        &self,
        network: &pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<MetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let idx = network.get_aggregated_node_index_by_name(self.meta.name.as_str(), None)?;

        let metric = match attr {
            NodeAttribute::Outflow => MetricF64::AggregatedNodeOutFlow(idx),
            NodeAttribute::Inflow => MetricF64::AggregatedNodeInFlow(idx),
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    ty: "AggregatedNode".to_string(),
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }
}

impl TryFrom<AggregatedNodeV1> for AggregatedNode {
    type Error = ConversionError;

    fn try_from(v1: AggregatedNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();
        let mut unnamed_count = 0;

        let relationship = match v1.factors {
            Some(f) => Some(Relationship::Ratio {
                factors: f
                    .into_iter()
                    .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
                    .collect::<Result<_, _>>()?,
            }),
            None => None,
        };

        let max_flow = v1
            .max_flow
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let min_flow = v1
            .min_flow
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let nodes = v1.nodes.into_iter().map(|n| n.into()).collect();

        let n = Self {
            meta,
            nodes,
            max_flow,
            min_flow,
            relationship,
        };
        Ok(n)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct AggregatedStorageNode {
    pub meta: NodeMeta,
    pub storage_nodes: Vec<SimpleNodeReference>,
}

impl AggregatedStorageNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Volume;

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        // Not connectable
        // TODO this should be a trait? And error if you try to connect to a non-connectable node.
        vec![]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        // Not connectable
        vec![]
    }

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl AggregatedStorageNode {
    pub fn node_indices_for_constraints(
        &self,
        network: &pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        let indices = self
            .storage_nodes
            .iter()
            .map(|node_ref| {
                args.schema
                    .get_node_by_name(&node_ref.name)
                    .ok_or_else(|| SchemaError::NodeNotFound(node_ref.name.to_string()))?
                    .node_indices_for_constraints(network, args)
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();
        Ok(indices)
    }
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        let nodes = self
            .storage_nodes
            .iter()
            .map(|node_ref| network.get_node_index_by_name(&node_ref.name, None))
            .collect::<Result<_, _>>()?;

        network.add_aggregated_storage_node(self.meta.name.as_str(), None, nodes)?;
        Ok(())
    }

    pub fn create_metric(
        &self,
        network: &mut pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<MetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let idx = network.get_aggregated_storage_node_index_by_name(self.meta.name.as_str(), None)?;

        let metric = match attr {
            NodeAttribute::Volume => MetricF64::AggregatedNodeVolume(idx),
            NodeAttribute::ProportionalVolume => {
                let dm = DerivedMetric::AggregatedNodeProportionalVolume(idx);
                let derived_metric_idx = network.add_derived_metric(dm);
                MetricF64::DerivedMetric(derived_metric_idx)
            }
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    ty: "AggregatedStorageNode".to_string(),
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }
}

impl TryFrom<AggregatedStorageNodeV1> for AggregatedStorageNode {
    type Error = ConversionError;

    fn try_from(v1: AggregatedStorageNodeV1) -> Result<Self, Self::Error> {
        let storage_nodes = v1.storage_nodes.into_iter().map(|n| n.into()).collect();

        let n = Self {
            meta: v1.meta.into(),
            storage_nodes,
        };
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use crate::nodes::core::StorageInitialVolume;
    use crate::nodes::InputNode;
    use crate::nodes::StorageNode;
    use crate::PywrModel;
    #[cfg(feature = "core")]
    use pywr_core::test_utils::{run_all_solvers, ExpectedOutputs};
    #[cfg(feature = "core")]
    use std::str::FromStr;
    #[cfg(feature = "core")]
    use tempfile::TempDir;

    #[test]
    fn test_input() {
        let data = r#"
            {
                "meta": {
                    "name": "supply1"
                },
                "max_flow": {
                    "type": "Constant",
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
                  "type": "Constant",
                  "value": 10.0
                },
                "initial_volume": {
                    "Absolute": 12.0
                }
            }
            "#;

        let storage: StorageNode = serde_json::from_str(data).unwrap();

        assert_eq!(storage.initial_volume, StorageInitialVolume::Absolute(12.0));
    }

    #[test]
    fn test_storage_initial_volume_proportional() {
        let data = r#"
            {
                "meta": {
                    "name": "storage1"
                },
                "max_volume": {
                  "type": "Constant",
                  "value": 15.0
                },
                "initial_volume": {
                    "Proportional": 0.5
                }
            }
            "#;

        let storage: StorageNode = serde_json::from_str(data).unwrap();

        assert_eq!(storage.initial_volume, StorageInitialVolume::Proportional(0.5));
    }

    #[cfg(feature = "core")]
    fn storage_max_volumes_str() -> &'static str {
        include_str!("../test_models/storage_max_volumes.json")
    }

    #[test]
    #[cfg(feature = "core")]
    fn test_storage_max_volumes_run() {
        let data = storage_max_volumes_str();
        let schema = PywrModel::from_str(data).unwrap();
        let model: pywr_core::models::Model = schema.build_model(None, None).unwrap();
        // Test all solvers
        run_all_solvers(&model, &[], &[]);
    }

    fn me1_str() -> &'static str {
        include_str!("../test_models/mutual-exclusivity1.json")
    }
    fn me2_str() -> &'static str {
        include_str!("../test_models/mutual-exclusivity2.json")
    }

    #[cfg(feature = "core")]
    fn me3_str() -> &'static str {
        include_str!("../test_models/mutual-exclusivity3.json")
    }
    #[cfg(feature = "core")]
    fn me1_outputs_str() -> &'static str {
        include_str!("../test_models/mutual-exclusivity1.csv")
    }
    #[cfg(feature = "core")]
    fn me2_outputs_str() -> &'static str {
        include_str!("../test_models/mutual-exclusivity2.csv")
    }
    #[cfg(feature = "core")]
    fn me3_outputs_str() -> &'static str {
        include_str!("../test_models/mutual-exclusivity3.csv")
    }
    #[test]
    fn test_me1_model_schema() {
        let data = me1_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();

        assert_eq!(schema.network.nodes.len(), 6);
        assert_eq!(schema.network.edges.len(), 4);
    }
    #[test]
    fn test_me2_model_schema() {
        let data = me2_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();

        assert_eq!(schema.network.nodes.len(), 6);
        assert_eq!(schema.network.edges.len(), 4);
    }
    #[test]
    #[cfg(feature = "core")]
    fn test_me1_model_run() {
        let data = me1_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();
        let temp_dir = TempDir::new().unwrap();

        let mut model = schema.build_model(None, Some(temp_dir.path())).unwrap();

        let network = model.network_mut();
        assert_eq!(network.nodes().len(), 5);
        assert_eq!(network.edges().len(), 4);

        // After model run there should be an output file.
        let expected_outputs = [ExpectedOutputs::new(
            temp_dir.path().join("output.csv"),
            me1_outputs_str(),
        )];

        // Test all solvers
        run_all_solvers(&model, &["clp"], &expected_outputs);
    }
    #[test]
    #[cfg(feature = "core")]
    fn test_me2_model_run() {
        let data = me2_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();
        let temp_dir = TempDir::new().unwrap();

        let mut model = schema.build_model(None, Some(temp_dir.path())).unwrap();

        let network = model.network_mut();
        assert_eq!(network.nodes().len(), 10);
        assert_eq!(network.edges().len(), 11);

        // After model run there should be an output file.
        let expected_outputs = [ExpectedOutputs::new(
            temp_dir.path().join("output.csv"),
            me2_outputs_str(),
        )];

        // Test all solvers
        run_all_solvers(&model, &["clp"], &expected_outputs);
    }

    #[cfg(feature = "core")]
    fn link_with_soft_min_str() -> &'static str {
        include_str!("../test_models/link_with_soft_min.json")
    }

    #[test]
    #[cfg(feature = "core")]
    fn test_link_with_soft_min() {
        let data = link_with_soft_min_str();
        let schema = PywrModel::from_str(data).unwrap();
        let model: pywr_core::models::Model = schema.build_model(None, None).unwrap();

        // Test all solvers
        run_all_solvers(&model, &[], &[]);
    }

    #[cfg(feature = "core")]
    fn link_with_soft_max_str() -> &'static str {
        include_str!("../test_models/link_with_soft_max.json")
    }

    #[test]
    #[cfg(feature = "core")]
    fn test_link_with_soft_max() {
        let data = link_with_soft_max_str();
        let schema = PywrModel::from_str(data).unwrap();
        let model: pywr_core::models::Model = schema.build_model(None, None).unwrap();

        // Test all solvers
        run_all_solvers(&model, &[], &[]);
    }

    #[test]
    #[cfg(feature = "core")]
    fn test_me3_model_run() {
        let data = me3_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();
        let temp_dir = TempDir::new().unwrap();

        let mut model = schema.build_model(None, Some(temp_dir.path())).unwrap();

        let network = model.network_mut();
        assert_eq!(network.nodes().len(), 7);
        assert_eq!(network.edges().len(), 8);

        // After model run there should be an output file.
        let expected_outputs = [ExpectedOutputs::new(
            temp_dir.path().join("output.csv"),
            me3_outputs_str(),
        )];

        // Test all solvers
        run_all_solvers(&model, &["clp"], &expected_outputs);
    }
}
