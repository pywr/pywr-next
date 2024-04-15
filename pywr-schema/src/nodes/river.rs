use crate::error::{ConversionError, SchemaError};
use crate::metric::Metric;
use crate::nodes::{NodeAttribute, NodeMeta};
use pywr_core::metric::MetricF64;
use pywr_schema_macros::PywrNode;
use pywr_v1_schema::nodes::LinkNode as LinkNodeV1;
use std::collections::HashMap;

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, PywrNode)]
pub struct RiverNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
}

impl RiverNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Outflow;

    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        network.add_link_node(self.meta.name.as_str(), None)?;
        Ok(())
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }
    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
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

        let n = Self { meta };
        Ok(n)
    }
}
