use crate::error::ConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
use crate::model::LoadArgs;
use crate::nodes::{NodeAttribute, NodeMeta};
#[cfg(feature = "core")]
use pywr_core::metric::MetricF64;
use pywr_core::parameters::{AggFunc, ParameterName};
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::nodes::LinkNode as LinkNodeV1;
use schemars::JsonSchema;

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
/// The river loss data.
pub enum RiverLossData {
    /// The loss is proportional to the flow through the node and is calculated as product between
    /// the given factor and the node's flow.
    Proportional(f64),
    /// Provide a flow-loss relationship which is then piecewise linearly interpolated based on the
    /// flow through the node using an [`pywr_core::parameters::InterpolatedParameter`].
    Interpolated { river_flow: Metric, loss: Metric },
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
/// The river loss.
pub struct RiverLoss {
    /// The loss data.
    pub data: RiverLossData,
    /// The optional cost to add to the output node.
    pub cost: Option<Metric>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
#[doc = svgbobdoc::transform!(
/// A link node representing a river with an optional loss.
///
/// ```svgbob
///
///        U      <node>      D
///   - - -*--------->*------>*- - -
///                   !
///                   !
///                   V
///                   o
///              <node>.loss
/// ```
)]
pub struct RiverNode {
    pub meta: NodeMeta,
    /// An optional loss. This internally creates an [`crate::nodes::OutputNode`] with a given flow
    /// and optional cost.
    pub loss: Option<RiverLoss>,
}

impl RiverNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Outflow;

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }
    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    /// The sub-name of the output node.
    fn loss_node_sub_name() -> Option<&'static str> {
        Some("loss")
    }

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl RiverNode {
    pub fn node_indices_for_constraints(
        &self,
        network: &pywr_core::network::Network,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        let idx = network.get_node_index_by_name(self.meta.name.as_str(), None)?;
        Ok(vec![idx])
    }

    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        network.add_link_node(self.meta.name.as_str(), None)?;

        // add output node and edge
        if let Some(_) = &self.loss {
            network.add_output_node(self.meta.name.as_str(), Self::loss_node_sub_name())?;

            let river = network.get_node_index_by_name(self.meta.name.as_str(), None)?;
            let loss = network.get_node_index_by_name(self.meta.name.as_str(), Self::loss_node_sub_name())?;
            network.connect_nodes(river, loss)?;
        }

        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        let river_node = network.get_node_index_by_name(self.meta.name.as_str(), None)?;
        let current_flow_metric = MetricF64::NodeInFlow(river_node);

        if let Some(loss) = &self.loss {
            // add the flow
            let loss_metric = match &loss.data {
                RiverLossData::Proportional(factor) => {
                    let proportional_loss_parameter = pywr_core::parameters::AggregatedParameter::new(
                        ParameterName::new("loss", Some(self.meta.name.as_str())),
                        &[current_flow_metric, (*factor).into()],
                        AggFunc::Product,
                    );
                    let loss_idx = network.add_parameter(Box::new(proportional_loss_parameter))?;
                    let proportional_loss_metric: MetricF64 = loss_idx.into();
                    proportional_loss_metric
                }
                RiverLossData::Interpolated { river_flow, loss } => {
                    let flow_metric = river_flow.load(network, args)?;
                    let loss_metric = loss.load(network, args)?;

                    let interpolated_loss_parameter = pywr_core::parameters::InterpolatedParameter::new(
                        ParameterName::new("loss", Some(self.meta.name.as_str())),
                        current_flow_metric,
                        vec![(flow_metric, loss_metric)],
                        true,
                    );
                    let loss_idx = network.add_parameter(Box::new(interpolated_loss_parameter))?;
                    let interpolated_loss_metric: MetricF64 = loss_idx.into();
                    interpolated_loss_metric.into()
                }
            };

            network.set_node_max_flow(
                self.meta.name.as_str(),
                Self::loss_node_sub_name(),
                Some(loss_metric.into()),
            )?;

            // add the optional cost
            if let Some(cost) = &loss.cost {
                let value = cost.load(network, args)?;
                network.set_node_cost(self.meta.name.as_str(), Self::loss_node_sub_name(), value.into())?;
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
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let idx = network.get_node_index_by_name(self.meta.name.as_str(), None)?;
        let metric = match attr {
            NodeAttribute::Outflow => MetricF64::NodeOutFlow(idx),
            NodeAttribute::Inflow => MetricF64::NodeInFlow(idx),
            NodeAttribute::Loss => {
                match network.get_node_index_by_name(self.meta.name.as_str(), Self::loss_node_sub_name()) {
                    Ok(loss_idx) => MetricF64::NodeInFlow(loss_idx),
                    Err(_) => 0.0.into(),
                }
            }
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    ty: "RiverNode".to_string(),
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }
}

impl TryFrom<LinkNodeV1> for RiverNode {
    type Error = ConversionError;

    fn try_from(v1: LinkNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

        if v1.max_flow.is_some() {
            return Err(ConversionError::ExtraNodeAttribute {
                name: meta.name,
                attr: "max_flow".to_string(),
            });
        }
        if v1.min_flow.is_some() {
            return Err(ConversionError::ExtraNodeAttribute {
                name: meta.name,
                attr: "min_flow".to_string(),
            });
        }
        if v1.cost.is_some() {
            return Err(ConversionError::ExtraNodeAttribute {
                name: meta.name,
                attr: "cost".to_string(),
            });
        }

        let n = Self { meta, loss: None };
        Ok(n)
    }
}
