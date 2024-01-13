use crate::error::SchemaError;
use crate::model::PywrNetwork;
use serde::{Deserialize, Serialize};

///
#[derive(Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum OutputMetric {
    NodeName(String),
}

impl OutputMetric {
    fn try_clone_into_metric(
        &self,
        network: &pywr_core::network::Network,
        schema: &PywrNetwork,
    ) -> Result<pywr_core::metric::Metric, SchemaError> {
        match self {
            OutputMetric::NodeName(node_name) => {
                // Get the node from the schema; not the model itself
                let node = schema
                    .get_node_by_name(node_name)
                    .ok_or_else(|| SchemaError::NodeNotFound(node_name.to_string()))?;
                // Create and return the node's default metric
                node.default_metric(network)
            }
        }
    }
}

///
#[derive(Deserialize, Serialize, Clone)]
pub struct MetricSet {
    name: String,
    metrics: Vec<OutputMetric>,
}

impl MetricSet {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &PywrNetwork,
    ) -> Result<(), SchemaError> {
        // Convert the schema representation to internal metrics.
        let metrics: Vec<pywr_core::metric::Metric> = self
            .metrics
            .iter()
            .map(|m| m.try_clone_into_metric(network, schema))
            .collect::<Result<_, _>>()?;
        let metric_set = pywr_core::recorders::MetricSet::new(&self.name, None, metrics);
        let _ = network.add_metric_set(metric_set)?;

        Ok(())
    }
}
