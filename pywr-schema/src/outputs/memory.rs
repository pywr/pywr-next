#[cfg(feature = "core")]
use crate::SchemaError;
use crate::agg_funcs::AggFunc;
#[cfg(feature = "core")]
use pywr_core::recorders::MemoryRecorder;
use pywr_schema_macros::{PywrVisitPaths, skip_serializing_none};
use schemars::JsonSchema;
#[cfg(feature = "core")]
use std::path::Path;
use strum_macros::{Display, EnumIter};

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Default, Clone, JsonSchema, PywrVisitPaths)]
pub struct MemoryAggregation {
    pub time: Option<AggFunc>,
    pub scenario: Option<AggFunc>,
    pub metric: Option<AggFunc>,
}

#[cfg(feature = "core")]
impl MemoryAggregation {
    fn load(&self, data_path: Option<&Path>) -> Result<pywr_core::recorders::Aggregation, SchemaError> {
        Ok(pywr_core::recorders::Aggregation::new(
            self.time.as_ref().map(|f| f.load(data_path)).transpose()?,
            self.scenario.as_ref().map(|f| f.load(data_path)).transpose()?,
            self.metric.as_ref().map(|f| f.load(data_path)).transpose()?,
        ))
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Copy, Clone, JsonSchema, PywrVisitPaths, Display, EnumIter)]
pub enum MemoryAggregationOrder {
    MetricTimeScenario,
    TimeMetricScenario,
}

#[cfg(feature = "core")]
impl From<MemoryAggregationOrder> for pywr_core::recorders::AggregationOrder {
    fn from(value: MemoryAggregationOrder) -> Self {
        match value {
            MemoryAggregationOrder::MetricTimeScenario => pywr_core::recorders::AggregationOrder::MetricTimeScenario,
            MemoryAggregationOrder::TimeMetricScenario => pywr_core::recorders::AggregationOrder::TimeMetricScenario,
        }
    }
}

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitPaths)]
pub struct MemoryOutput {
    pub name: String,
    pub metric_set: String,
    pub aggregation: Option<MemoryAggregation>,
    pub order: Option<MemoryAggregationOrder>,
}

#[cfg(feature = "core")]
impl MemoryOutput {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        data_path: Option<&Path>,
    ) -> Result<(), SchemaError> {
        let metric_set_idx = network.get_metric_set_index_by_name(&self.metric_set)?;

        let recorder = MemoryRecorder::new(
            &self.name,
            metric_set_idx,
            self.aggregation.clone().unwrap_or_default().load(data_path)?,
            self.order.map(|o| o.into()).unwrap_or_default(),
        );

        network.add_recorder(Box::new(recorder))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::ModelSchema;
    #[cfg(feature = "core")]
    use float_cmp::assert_approx_eq;
    #[cfg(feature = "core")]
    use pywr_core::solvers::{ClpSolver, ClpSolverSettings};
    use std::fs::read_to_string;
    use std::str::FromStr;
    #[cfg(feature = "core")]
    use tempfile::TempDir;

    fn memory1_str() -> String {
        read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/memory1.json")).expect("Failed to read memory1.json")
    }

    #[test]
    fn test_schema() {
        let data = memory1_str();
        let schema = ModelSchema::from_str(&data).unwrap();

        assert_eq!(schema.network.nodes.len(), 3);
        assert_eq!(schema.network.edges.len(), 2);
        assert!(schema.network.outputs.is_some_and(|o| o.len() == 1));
    }

    #[test]
    #[cfg(feature = "core")]
    fn test_run() {
        let data = memory1_str();
        let schema = ModelSchema::from_str(&data).unwrap();

        let temp_dir = TempDir::new().unwrap();

        let model = schema.build_model(None, Some(temp_dir.path())).unwrap();

        let result = model.run::<ClpSolver>(&ClpSolverSettings::default()).unwrap();

        let result = result
            .network_result
            .get("outputs")
            .expect("`outputs` not found")
            .aggregated_value()
            .expect("No results found");

        assert_approx_eq!(f64, result, 91.0);
    }
}
