use crate::metric::Metric;
use crate::PywrError;
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
        model: &crate::model::Model,
        schema: &crate::schema::PywrModel,
    ) -> Result<Metric, PywrError> {
        match self {
            OutputMetric::NodeName(node_name) => {
                // Get the node from the schema; not the model itself
                let node = schema
                    .get_node_by_name(node_name)
                    .ok_or_else(|| PywrError::NodeNotFound(node_name.to_string()))?;
                // Create and return the node's default metric
                node.default_metric(model)
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
        model: &mut crate::model::Model,
        schema: &crate::schema::PywrModel,
    ) -> Result<(), PywrError> {
        // Convert the schema representation to internal metrics.
        let metrics: Vec<Metric> = self
            .metrics
            .iter()
            .map(|m| m.try_clone_into_metric(model, schema))
            .collect::<Result<_, _>>()?;
        let metric_set = crate::recorders::MetricSet::new(&self.name, None, metrics);
        let _ = model.add_metric_set(metric_set)?;

        Ok(())
    }
}
