use crate::data_tables::LoadedTableCollection;
use crate::error::{ConversionError, SchemaError};
use crate::model::PywrMultiNetworkTransfer;
use crate::nodes::{NodeAttribute, NodeMeta};
use crate::parameters::{DynamicFloatValue, TryIntoV2Parameter};
use pywr_core::metric::Metric;
use pywr_core::models::ModelDomain;
use pywr_v1_schema::nodes::LossLinkNode as LossLinkNodeV1;
use std::path::Path;

#[doc = svgbobdoc::transform!(
/// This is used to represent link with losses.
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
#[derive(serde::Deserialize, serde::Serialize, Clone, Default)]
pub struct LossLinkNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub loss_factor: Option<DynamicFloatValue>,
    pub min_net_flow: Option<DynamicFloatValue>,
    pub max_net_flow: Option<DynamicFloatValue>,
    pub net_cost: Option<DynamicFloatValue>,
}

impl LossLinkNode {
    fn loss_sub_name() -> Option<&'static str> {
        Some("loss")
    }

    fn net_sub_name() -> Option<&'static str> {
        Some("net")
    }

    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        network.add_link_node(self.meta.name.as_str(), Self::net_sub_name())?;
        // TODO make the loss node configurable (i.e. it could be a link if a network wanted to use the loss)
        // The above would need to support slots in the connections.
        network.add_output_node(self.meta.name.as_str(), Self::loss_sub_name())?;

        // TODO add the aggregated node that actually does the losses!
        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &crate::model::PywrNetwork,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
    ) -> Result<(), SchemaError> {
        if let Some(cost) = &self.net_cost {
            let value = cost.load(network, schema, domain, tables, data_path, inter_network_transfers)?;
            network.set_node_cost(self.meta.name.as_str(), Self::net_sub_name(), value.into())?;
        }

        if let Some(max_flow) = &self.max_net_flow {
            let value = max_flow.load(network, schema, domain, tables, data_path, inter_network_transfers)?;
            network.set_node_max_flow(self.meta.name.as_str(), Self::net_sub_name(), value.into())?;
        }

        if let Some(min_flow) = &self.min_net_flow {
            let value = min_flow.load(network, schema, domain, tables, data_path, inter_network_transfers)?;
            network.set_node_min_flow(self.meta.name.as_str(), Self::net_sub_name(), value.into())?;
        }

        Ok(())
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        // Gross inflow goes to both nodes
        vec![
            (self.meta.name.as_str(), Self::loss_sub_name().map(|s| s.to_string())),
            (self.meta.name.as_str(), Self::net_sub_name().map(|s| s.to_string())),
        ]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        // Only net goes to the downstream.
        vec![(self.meta.name.as_str(), Self::net_sub_name().map(|s| s.to_string()))]
    }

    pub fn create_metric(
        &self,
        network: &pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<Metric, SchemaError> {
        // Use the default attribute if none is specified
        let attr = attribute.unwrap_or_else(|| self.default_attribute());

        let metric = match attr {
            NodeAttribute::Inflow => {
                let indices = vec![
                    network.get_node_index_by_name(self.meta.name.as_str(), Self::net_sub_name())?,
                    network.get_node_index_by_name(self.meta.name.as_str(), Self::loss_sub_name())?,
                ];

                Metric::MultiNodeInFlow {
                    indices,
                    name: self.meta.name.to_string(),
                }
            }
            NodeAttribute::Outflow => {
                let idx = network.get_node_index_by_name(self.meta.name.as_str(), Self::net_sub_name().as_deref())?;
                Metric::NodeOutFlow(idx)
            }
            NodeAttribute::Loss => {
                let idx = network.get_node_index_by_name(self.meta.name.as_str(), Self::loss_sub_name().as_deref())?;
                // This is an output node that only supports inflow
                Metric::NodeInFlow(idx)
            }
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }

    pub fn default_attribute(&self) -> NodeAttribute {
        NodeAttribute::Outflow
    }
}

impl TryFrom<LossLinkNodeV1> for LossLinkNode {
    type Error = ConversionError;

    fn try_from(v1: LossLinkNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();
        let mut unnamed_count = 0;

        let loss_factor = v1
            .loss_factor
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
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
