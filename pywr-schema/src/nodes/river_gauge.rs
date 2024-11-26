use crate::error::ConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::nodes::{NodeAttribute, NodeMeta};
use crate::v1::{ConversionData, TryFromV1, TryIntoV2};
#[cfg(feature = "core")]
use pywr_core::metric::MetricF64;
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::nodes::RiverGaugeNode as RiverGaugeNodeV1;
use schemars::JsonSchema;

#[doc = svgbobdoc::transform!(
/// This is used to represent a minimum residual flow (MRF) at a gauging station.
///
///
/// ```svgbob
///            <node>.mrf
///          .------>L -----.
///      U  |                |     D
///     -*--|                |--->*- - -
///         |                |
///          '------>L -----'
///            <node>.bypass
/// ```
///
)]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct RiverGaugeNode {
    pub meta: NodeMeta,
    pub mrf: Option<Metric>,
    pub mrf_cost: Option<Metric>,
    pub bypass_cost: Option<Metric>,
}

impl RiverGaugeNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Outflow;

    fn mrf_sub_name() -> Option<&'static str> {
        Some("mrf")
    }

    fn bypass_sub_name() -> Option<&'static str> {
        Some("bypass")
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![
            (self.meta.name.as_str(), Self::mrf_sub_name().map(|s| s.to_string())),
            (self.meta.name.as_str(), Self::bypass_sub_name().map(|s| s.to_string())),
        ]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![
            (self.meta.name.as_str(), Self::mrf_sub_name().map(|s| s.to_string())),
            (self.meta.name.as_str(), Self::bypass_sub_name().map(|s| s.to_string())),
        ]
    }

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl RiverGaugeNode {
    pub fn node_indices_for_constraints(
        &self,
        network: &pywr_core::network::Network,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        let indices = vec![
            network.get_node_index_by_name(self.meta.name.as_str(), Self::mrf_sub_name())?,
            network.get_node_index_by_name(self.meta.name.as_str(), Self::bypass_sub_name())?,
        ];
        Ok(indices)
    }
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        network.add_link_node(self.meta.name.as_str(), Self::mrf_sub_name())?;
        network.add_link_node(self.meta.name.as_str(), Self::bypass_sub_name())?;

        Ok(())
    }
    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        // MRF applies as a maximum on the MRF node.
        if let Some(cost) = &self.mrf_cost {
            let value = cost.load(network, args)?;
            network.set_node_cost(self.meta.name.as_str(), Self::mrf_sub_name(), value.into())?;
        }

        if let Some(mrf) = &self.mrf {
            let value = mrf.load(network, args)?;
            network.set_node_max_flow(self.meta.name.as_str(), Self::mrf_sub_name(), value.into())?;
        }

        if let Some(cost) = &self.bypass_cost {
            let value = cost.load(network, args)?;
            network.set_node_cost(self.meta.name.as_str(), Self::bypass_sub_name(), value.into())?;
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

        let indices = vec![
            network.get_node_index_by_name(self.meta.name.as_str(), Self::mrf_sub_name())?,
            network.get_node_index_by_name(self.meta.name.as_str(), Self::bypass_sub_name())?,
        ];

        let metric = match attr {
            NodeAttribute::Inflow => MetricF64::MultiNodeInFlow {
                indices,
                name: self.meta.name.to_string(),
            },
            NodeAttribute::Outflow => MetricF64::MultiNodeOutFlow {
                indices,
                name: self.meta.name.to_string(),
            },
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    ty: "RiverGaugeNode".to_string(),
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }
}

impl TryFromV1<RiverGaugeNodeV1> for RiverGaugeNode {
    type Error = ConversionError;

    fn try_from_v1(
        v1: RiverGaugeNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

        let mrf = v1
            .mrf
            .map(|v| v.try_into_v2(parent_node.or(Some(&meta.name)), conversion_data))
            .transpose()?;

        let mrf_cost = v1
            .mrf_cost
            .map(|v| v.try_into_v2(parent_node.or(Some(&meta.name)), conversion_data))
            .transpose()?;

        let bypass_cost = v1
            .cost
            .map(|v| v.try_into_v2(parent_node.or(Some(&meta.name)), conversion_data))
            .transpose()?;

        let n = Self {
            meta,
            mrf,
            mrf_cost,
            bypass_cost,
        };
        Ok(n)
    }
}
