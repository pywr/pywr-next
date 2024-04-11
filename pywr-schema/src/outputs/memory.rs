use crate::metric_sets::MetricAggFunc;
use crate::SchemaError;
use pywr_core::recorders::MemoryRecorder;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct MemoryAggregation {
    pub time: Option<MetricAggFunc>,
    pub scenario: Option<MetricAggFunc>,
    pub metric: Option<MetricAggFunc>,
}

impl From<MemoryAggregation> for pywr_core::recorders::Aggregation {
    fn from(value: MemoryAggregation) -> Self {
        pywr_core::recorders::Aggregation::new(
            value.time.map(|f| f.into()),
            value.scenario.map(|f| f.into()),
            value.metric.map(|f| f.into()),
        )
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct MemoryOutput {
    pub name: String,
    pub metric_set: String,
    pub aggregation: MemoryAggregation,
}

impl MemoryOutput {
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        let metric_set_idx = network.get_metric_set_index_by_name(&self.metric_set)?;
        let recorder = MemoryRecorder::new(&self.name, metric_set_idx, self.aggregation.clone().into());

        network.add_recorder(Box::new(recorder))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::PywrModel;
    use float_cmp::assert_approx_eq;
    use pywr_core::solvers::{ClpSolver, ClpSolverSettings};
    use std::str::FromStr;
    use tempfile::TempDir;

    fn memory1_str() -> &'static str {
        include_str!("../test_models/memory1.json")
    }

    #[test]
    fn test_schema() {
        let data = memory1_str();
        let schema = PywrModel::from_str(data).unwrap();

        assert_eq!(schema.network.nodes.len(), 3);
        assert_eq!(schema.network.edges.len(), 2);
        assert!(schema.network.outputs.is_some_and(|o| o.len() == 1));
    }

    #[test]
    fn test_run() {
        let data = memory1_str();
        let schema = PywrModel::from_str(data).unwrap();

        let temp_dir = TempDir::new().unwrap();

        let model = schema.build_model(None, Some(temp_dir.path())).unwrap();

        let recorder_states = model.run::<ClpSolver>(&ClpSolverSettings::default()).unwrap();

        let result = model
            .network()
            .get_aggregated_value("outputs", &recorder_states)
            .expect("No results found");

        assert_approx_eq!(f64, result, 91.0);
    }
}
