use crate::error::ConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::nodes::{NodeAttribute, NodeMeta};
use crate::parameters::TryIntoV2Parameter;
#[cfg(feature = "core")]
use pywr_core::{aggregated_node::Factors, metric::MetricF64};
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::nodes::LossLinkNode as LossLinkNodeV1;
use schemars::JsonSchema;

/// The type of loss factor applied.
///
/// Gross losses are typically applied as a proportion of the total flow into a node, whereas
/// net losses are applied as a proportion of the net flow. Please see the documentation for
/// specific nodes (e.g. [`LossLinkNode`]) to understand how the loss factor is applied.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum LossFactor {
    Gross { factor: Metric },
    Net { factor: Metric },
}

#[cfg(feature = "core")]
impl LossFactor {
    pub fn load(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<Option<Factors>, SchemaError> {
        match self {
            LossFactor::Gross { factor } => {
                let lf = factor.load(network, args)?;
                // Handle the case where we a given a zero loss factor
                // The aggregated node does not support zero loss factors so filter them here.
                if lf.is_constant_zero() {
                    return Ok(None);
                }
                // Gross losses are configured as a proportion of the net flow
                Ok(Some(Factors::Proportion(vec![lf])))
            }
            LossFactor::Net { factor } => {
                let lf = factor.load(network, args)?;
                // Handle the case where we a given a zero loss factor
                // The aggregated node does not support zero loss factors so filter them here.
                if lf.is_constant_zero() {
                    return Ok(None);
                }
                // Net losses are configured as a ratio of the net flow
                Ok(Some(Factors::Ratio(vec![1.0.into(), lf])))
            }
        }
    }
}

#[doc = svgbobdoc::transform!(
/// This is used to represent a link with losses.
///
/// The loss is applied using a loss factor, [`LossFactor`], which can be applied to either the
/// gross or net flow. If no loss factor is defined the output node "O" and the associated
/// aggregated node are not created.
///
/// The default output metric for this node is the net flow.
///
/// ```svgbob
///
///            <node>.net    D
///          .------>L ---->*
///      U  |
///     -*--|
///         |
///          '------>O
///            <node>.loss
/// ```
///
)]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct LossLinkNode {
    pub meta: NodeMeta,
    pub loss_factor: Option<LossFactor>,
    pub min_net_flow: Option<Metric>,
    pub max_net_flow: Option<Metric>,
    pub net_cost: Option<Metric>,
}

impl LossLinkNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Outflow;

    fn loss_sub_name() -> Option<&'static str> {
        Some("loss")
    }

    fn net_sub_name() -> Option<&'static str> {
        Some("net")
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        // Gross inflow always goes to the net node ...
        let mut input_connectors = vec![(self.meta.name.as_str(), Self::net_sub_name().map(|s| s.to_string()))];

        // ... but only to the loss node if a loss is defined
        if self.loss_factor.is_some() {
            input_connectors.push((self.meta.name.as_str(), Self::loss_sub_name().map(|s| s.to_string())));
        }

        input_connectors
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        // Only net goes to the downstream.
        vec![(self.meta.name.as_str(), Self::net_sub_name().map(|s| s.to_string()))]
    }

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl LossLinkNode {
    fn agg_sub_name() -> Option<&'static str> {
        Some("agg")
    }
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        let idx_net = network.add_link_node(self.meta.name.as_str(), Self::net_sub_name())?;
        // TODO make the loss node configurable (i.e. it could be a link if a network wanted to use the loss)
        // The above would need to support slots in the connections.

        if self.loss_factor.is_some() {
            let idx_loss = network.add_output_node(self.meta.name.as_str(), Self::loss_sub_name())?;
            // This aggregated node will contain the factors to enforce the loss
            network.add_aggregated_node(
                self.meta.name.as_str(),
                Self::agg_sub_name(),
                &[idx_net, idx_loss],
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
        if let Some(cost) = &self.net_cost {
            let value = cost.load(network, args)?;
            network.set_node_cost(self.meta.name.as_str(), Self::net_sub_name(), value.into())?;
        }

        if let Some(max_flow) = &self.max_net_flow {
            let value = max_flow.load(network, args)?;
            network.set_node_max_flow(self.meta.name.as_str(), Self::net_sub_name(), value.into())?;
        }

        if let Some(min_flow) = &self.min_net_flow {
            let value = min_flow.load(network, args)?;
            network.set_node_min_flow(self.meta.name.as_str(), Self::net_sub_name(), value.into())?;
        }

        if let Some(loss_factor) = &self.loss_factor {
            let factors = loss_factor.load(network, args)?;
            network.set_aggregated_node_factors(self.meta.name.as_str(), Self::agg_sub_name(), factors)?;
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
                    ty: "LossLinkNode".to_string(),
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }
}

impl TryFrom<LossLinkNodeV1> for LossLinkNode {
    type Error = ConversionError;

    fn try_from(v1: LossLinkNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();
        let mut unnamed_count = 0;

        let loss_factor = v1
            .loss_factor
            .map(|v| {
                let factor = v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count)?;
                Ok::<_, Self::Error>(LossFactor::Net { factor })
            })
            .transpose()?;

        let min_net_flow = v1
            .min_flow
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let max_net_flow = v1
            .max_flow
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let net_cost = v1
            .cost
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let n = Self {
            meta,
            loss_factor,
            min_net_flow,
            max_net_flow,
            net_cost,
        };
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use crate::model::PywrModel;
    #[cfg(feature = "core")]
    use pywr_core::test_utils::{run_all_solvers, ExpectedOutputs};
    #[cfg(feature = "core")]
    use tempfile::TempDir;

    fn loss_link1_str() -> &'static str {
        include_str!("../test_models/loss_link1.json")
    }

    #[cfg(feature = "core")]
    fn loss_link1_outputs_str() -> &'static str {
        include_str!("../test_models/loss_link1-expected.csv")
    }

    #[test]
    fn test_model_schema() {
        let data = loss_link1_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();

        assert_eq!(schema.network.nodes.len(), 7);
        assert_eq!(schema.network.edges.len(), 6);
    }

    #[test]
    #[cfg(feature = "core")]
    fn test_model_run() {
        let data = loss_link1_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();
        let temp_dir = TempDir::new().unwrap();

        let model = schema.build_model(None, Some(temp_dir.path())).unwrap();
        // After model run there should be an output file.
        let expected_outputs = [ExpectedOutputs::new(
            temp_dir.path().join("loss_link1.csv"),
            loss_link1_outputs_str(),
        )];

        // Test all solvers
        run_all_solvers(&model, &[], &expected_outputs);
    }
}
