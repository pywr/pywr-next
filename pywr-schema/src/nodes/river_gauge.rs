use crate::data_tables::LoadedTableCollection;
use crate::error::{ConversionError, SchemaError};
use crate::nodes::NodeMeta;
use crate::parameters::{DynamicFloatValue, TryIntoV2Parameter};
use pywr_core::metric::Metric;
use pywr_core::models::ModelDomain;
use pywr_v1_schema::nodes::RiverGaugeNode as RiverGaugeNodeV1;
use std::path::Path;

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
#[derive(serde::Deserialize, serde::Serialize, Clone, Default)]
pub struct RiverGaugeNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub mrf: Option<DynamicFloatValue>,
    pub mrf_cost: Option<DynamicFloatValue>,
}

impl RiverGaugeNode {
    fn mrf_sub_name() -> Option<&'static str> {
        Some("mrf")
    }

    fn bypass_sub_name() -> Option<&'static str> {
        Some("bypass")
    }

    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        network.add_link_node(self.meta.name.as_str(), Self::mrf_sub_name())?;
        network.add_link_node(self.meta.name.as_str(), Self::bypass_sub_name())?;

        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<(), SchemaError> {
        // MRF applies as a maximum on the MRF node.
        if let Some(cost) = &self.mrf_cost {
            let value = cost.load(network, domain, tables, data_path)?;
            network.set_node_cost(self.meta.name.as_str(), Self::mrf_sub_name(), value.into())?;
        }

        if let Some(mrf) = &self.mrf {
            let value = mrf.load(network, domain, tables, data_path)?;
            network.set_node_max_flow(self.meta.name.as_str(), Self::mrf_sub_name(), value.into())?;
        }

        Ok(())
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

    pub fn default_metric(&self, network: &pywr_core::network::Network) -> Result<Metric, SchemaError> {
        let indices = vec![
            network.get_node_index_by_name(self.meta.name.as_str(), Self::mrf_sub_name())?,
            network.get_node_index_by_name(self.meta.name.as_str(), Self::bypass_sub_name())?,
        ];

        Ok(Metric::MultiNodeInFlow {
            indices,
            name: self.meta.name.to_string(),
            sub_name: Some("total".to_string()),
        })
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
    use pywr_core::test_utils::run_all_solvers;
    use pywr_core::timestep::Timestepper;

    fn model_str() -> &'static str {
        r#"
            {
                "metadata": {
                    "title": "Simple 1",
                    "description": "A very simple example.",
                    "minimum_version": "0.1"
                },
                "timestepper": {
                    "start": "2015-01-01",
                    "end": "2015-12-31",
                    "timestep": 1
                },
                "network": {
                    "nodes": [
                        {
                            "name": "catchment1",
                            "type": "Catchment",
                            "flow": 15
                        },
                        {
                            "name": "gauge1",
                            "type": "RiverGauge",
                            "mrf": 5.0,
                            "mrf_cost": -20.0
                        },
                        {
                            "name": "term1",
                            "type": "Output"
                        },
                        {
                            "name": "demand1",
                            "type": "Output",
                            "max_flow": 15.0,
                            "cost": -10
                        }
                    ],
                    "edges": [
                        {
                            "from_node": "catchment1",
                            "to_node": "gauge1"
                        },
                        {
                            "from_node": "gauge1",
                            "to_node": "term1"
                        },
                        {
                            "from_node": "gauge1",
                            "to_node": "demand1"
                        }
                    ]
                }
            }
            "#
    }

    #[test]
    fn test_model_schema() {
        let data = model_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();

        assert_eq!(schema.network.nodes.len(), 4);
        assert_eq!(schema.network.edges.len(), 3);
    }

    #[test]
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
