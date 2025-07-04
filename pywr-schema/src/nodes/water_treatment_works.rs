#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::nodes::loss_link::LossFactor;
use crate::nodes::{NodeAttribute, NodeMeta};
use crate::parameters::Parameter;
#[cfg(feature = "core")]
use pywr_core::metric::MetricF64;
use pywr_schema_macros::PywrVisitAll;
use schemars::JsonSchema;

#[doc = svgbobdoc::transform!(
/// A node used to represent a water treatment works (WTW) with optional losses.
///
/// This node comprises an internal structure that allows specifying a minimum and
/// maximum total net flow, an optional loss factor applied as a proportion of either net
/// or gross flow, and an optional "soft" minimum flow.
///
/// When a loss factor is not given the `loss` node is not created. When a non-zero loss
/// factor is provided [`pywr_core::node::OutputNode`] and [`pywr_core::aggregated_node::AggregatedNode`]
/// nodes are created.
///
///
/// ```svgbob
///                          <node>.net_soft_min_flow
///                           .--->L ---.
///            <node>.net    |           |     D
///          .------>L ------|           |--->*- - -
///      U  |                |           |
///     -*--|                 '--->L ---'
///         |                <node>.net_above_soft_min_flow
///          '------>O
///            <node>.loss
/// ```
///
)]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct WaterTreatmentWorks {
    /// Node metadata
    pub meta: NodeMeta,
    /// Optional local parameters.
    pub parameters: Option<Vec<Parameter>>,
    /// The proportion of net flow that is lost to the loss node.
    pub loss_factor: Option<LossFactor>,
    /// The minimum flow through the `net` flow node.
    pub min_flow: Option<Metric>,
    /// The maximum flow through the `net` flow node.
    pub max_flow: Option<Metric>,
    /// The maximum flow applied to the `net_soft_min_flow` node which is typically
    /// used as a "soft" minimum flow.
    pub soft_min_flow: Option<Metric>,
    /// The cost applied to the `net_soft_min_flow` node.
    pub soft_min_flow_cost: Option<Metric>,
    /// The cost applied to the `net` flow node.
    pub cost: Option<Metric>,
}

impl WaterTreatmentWorks {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Outflow;

    fn loss_sub_name() -> Option<&'static str> {
        Some("loss")
    }

    fn net_sub_name() -> Option<&'static str> {
        Some("net")
    }

    fn net_soft_min_flow_sub_name() -> Option<&'static str> {
        Some("net_soft_min_flow")
    }

    fn net_above_soft_min_flow_sub_name() -> Option<&'static str> {
        Some("net_above_soft_min_flow")
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        // Connect directly to the total net
        let mut connectors = vec![(self.meta.name.as_str(), Self::net_sub_name().map(|s| s.to_string()))];
        // Only connect to the loss link if it is created
        if self.loss_factor.is_some() {
            connectors.push((self.meta.name.as_str(), Self::loss_sub_name().map(|s| s.to_string())))
        }
        connectors
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        // Connect to the split of the net flow.
        vec![
            (
                self.meta.name.as_str(),
                Self::net_soft_min_flow_sub_name().map(|s| s.to_string()),
            ),
            (
                self.meta.name.as_str(),
                Self::net_above_soft_min_flow_sub_name().map(|s| s.to_string()),
            ),
        ]
    }

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl WaterTreatmentWorks {
    fn agg_sub_name() -> Option<&'static str> {
        Some("agg")
    }

    pub fn node_indices_for_constraints(
        &self,
        network: &pywr_core::network::Network,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        let idx = network.get_node_index_by_name(self.meta.name.as_str(), Self::net_sub_name())?;
        Ok(vec![idx])
    }
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        let idx_net = network.add_link_node(self.meta.name.as_str(), Self::net_sub_name())?;
        let idx_soft_min_flow = network.add_link_node(self.meta.name.as_str(), Self::net_soft_min_flow_sub_name())?;
        let idx_above_soft_min_flow =
            network.add_link_node(self.meta.name.as_str(), Self::net_above_soft_min_flow_sub_name())?;

        // Create the internal connections
        network.connect_nodes(idx_net, idx_soft_min_flow)?;
        network.connect_nodes(idx_net, idx_above_soft_min_flow)?;

        if self.loss_factor.is_some() {
            let idx_loss = network.add_output_node(self.meta.name.as_str(), Self::loss_sub_name())?;
            // This aggregated node will contain the factors to enforce the loss
            network.add_aggregated_node(
                self.meta.name.as_str(),
                Self::agg_sub_name(),
                &[vec![idx_net], vec![idx_loss]],
                None,
            )?;
        }

        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        if let Some(cost) = &self.cost {
            let value = cost.load(network, args, Some(&self.meta.name))?;
            network.set_node_cost(self.meta.name.as_str(), Self::net_sub_name(), value.into())?;
        }

        if let Some(max_flow) = &self.max_flow {
            let value = max_flow.load(network, args, Some(&self.meta.name))?;
            network.set_node_max_flow(self.meta.name.as_str(), Self::net_sub_name(), value.into())?;
        }

        if let Some(min_flow) = &self.min_flow {
            let value = min_flow.load(network, args, Some(&self.meta.name))?;
            network.set_node_min_flow(self.meta.name.as_str(), Self::net_sub_name(), value.into())?;
        }

        // soft min flow constraints; This typically applies a negative cost upto a maximum
        // defined by the `soft_min_flow`
        if let Some(cost) = &self.soft_min_flow_cost {
            let value = cost.load(network, args, Some(&self.meta.name))?;
            network.set_node_cost(
                self.meta.name.as_str(),
                Self::net_soft_min_flow_sub_name(),
                value.into(),
            )?;
        }
        if let Some(min_flow) = &self.soft_min_flow {
            let value = min_flow.load(network, args, Some(&self.meta.name))?;
            network.set_node_max_flow(
                self.meta.name.as_str(),
                Self::net_soft_min_flow_sub_name(),
                value.into(),
            )?;
        }

        if let Some(loss_factor) = &self.loss_factor {
            let factors = loss_factor.load(network, args, Some(&self.meta.name))?;
            if factors.is_none() {
                // Loaded a constant zero factor; ensure that the loss node has zero flow
                network.set_node_max_flow(self.meta.name.as_str(), Self::loss_sub_name(), Some(0.0.into()))?;
            }
            network.set_aggregated_node_relationship(self.meta.name.as_str(), Self::agg_sub_name(), factors)?;
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

        let metric = match attr {
            NodeAttribute::Inflow => {
                match network.get_node_index_by_name(self.meta.name.as_str(), Self::loss_sub_name()) {
                    // Loss node is defined. The total inflow is the sum of the net and loss nodes;
                    Ok(loss_idx) => {
                        let indices = vec![
                            network.get_node_index_by_name(self.meta.name.as_str(), Self::net_sub_name())?,
                            loss_idx,
                        ];
                        MetricF64::MultiNodeInFlow {
                            indices,
                            name: self.meta.name.to_string(),
                        }
                    }
                    // No loss node defined, so just use the net node
                    Err(_) => MetricF64::NodeInFlow(
                        network.get_node_index_by_name(self.meta.name.as_str(), Self::net_sub_name())?,
                    ),
                }
            }
            NodeAttribute::Outflow => {
                let idx = network.get_node_index_by_name(self.meta.name.as_str(), Self::net_sub_name())?;
                MetricF64::NodeOutFlow(idx)
            }
            NodeAttribute::Loss => {
                match network.get_node_index_by_name(self.meta.name.as_str(), Self::loss_sub_name()) {
                    Ok(idx) => MetricF64::NodeInFlow(idx),
                    Err(_) => 0.0.into(),
                }
            }
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    ty: "WaterTreatmentWorksNode".to_string(),
                    name: self.meta.name.clone(),
                    attr,
                });
            }
        };

        Ok(metric)
    }
}
