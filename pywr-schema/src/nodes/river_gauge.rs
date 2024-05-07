use crate::error::ConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::nodes::{NodeAttribute, NodeMeta};
use crate::parameters::TryIntoV2Parameter;
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
pub struct RiverGaugeNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub mrf: Option<Metric>,
    pub mrf_cost: Option<Metric>,
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

impl TryFrom<RiverGaugeNodeV1> for RiverGaugeNode {
    type Error = ConversionError;

    fn try_from(v1: RiverGaugeNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();
        let mut unnamed_count = 0;

        let mrf = v1
            .mrf
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let mrf_cost = v1
            .mrf_cost
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let n = Self { meta, mrf, mrf_cost };
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use crate::model::PywrModel;
    #[cfg(feature = "core")]
    use pywr_core::test_utils::run_all_solvers;

    fn model_str() -> &'static str {
        include_str!("../test_models/river_gauge1.json")
    }

    #[test]
    fn test_model_schema() {
        let data = model_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();

        assert_eq!(schema.network.nodes.len(), 4);
        assert_eq!(schema.network.edges.len(), 3);
    }

    #[test]
    #[cfg(feature = "core")]
    fn test_model_run() {
        let data = model_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();
        let model = schema.build_model(None, None).unwrap();

        let network = model.network();
        assert_eq!(network.nodes().len(), 5);
        assert_eq!(network.edges().len(), 6);

        // Test all solvers
        run_all_solvers(&model);

        // TODO assert the results!
    }
}
