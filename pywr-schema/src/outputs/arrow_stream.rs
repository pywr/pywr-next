#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::meta::NamedMeta;
use crate::visit::{Reference, ReferenceMut, VisitReferences};
#[cfg(feature = "core")]
use pywr_core::recorders::ArrowStreamOutputBuilder;
use pywr_schema_macros::PywrVisitPaths;
use schemars::JsonSchema;
use std::num::NonZeroUsize;
#[cfg(feature = "core")]
use std::path::Path;
use std::path::PathBuf;

/// Output one metric set as a batched Arrow IPC stream.
///
/// Each record batch contains up to `batch_size` model timesteps. Metric columns
/// are stored as fixed-size lists of `Float64` values in scenario order.
/// Scenario identities and groups are stored once as JSON in the `PYWR_SCENARIOS`
/// and `PYWR_SCENARIO_GROUPS` schema metadata.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitPaths)]
pub struct ArrowStreamOutput {
    pub meta: NamedMeta,
    pub filename: PathBuf,
    /// The metric set to write.
    pub metric_set: String,
    /// Number of model timesteps to accumulate in each Arrow record batch.
    pub batch_size: NonZeroUsize,
}

/// Written out rather than derived: a derive would walk `metric_set` as a plain `String`.
impl VisitReferences for ArrowStreamOutput {
    fn visit_references<F: FnMut(Reference<'_>)>(&self, visitor: &mut F) {
        visitor(Reference::MetricSet(&self.metric_set));
    }

    fn visit_references_mut<F: FnMut(ReferenceMut<'_>)>(&mut self, visitor: &mut F) {
        visitor(ReferenceMut::MetricSet(&mut self.metric_set));
    }
}

#[cfg(feature = "core")]
impl ArrowStreamOutput {
    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        output_path: Option<&Path>,
    ) -> Result<(), SchemaError> {
        let filename = match (output_path, self.filename.is_relative()) {
            (Some(output_directory), true) => output_directory.join(&self.filename),
            _ => self.filename.to_path_buf(),
        };
        let recorder = ArrowStreamOutputBuilder::new(&self.meta.name, filename, &self.metric_set, self.batch_size);
        network.recorder(Box::new(recorder));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::ModelSchema;
    use crate::visit::{Reference, VisitReferences};
    #[cfg(feature = "core")]
    use arrow::array::{Array, FixedSizeListArray, Float64Array};
    #[cfg(feature = "core")]
    use arrow::ipc::reader::StreamReader;
    #[cfg(feature = "core")]
    use pywr_core::solvers::ClpSolverSettings;
    use std::str::FromStr;
    #[cfg(feature = "core")]
    use tempfile::TempDir;

    const MODEL: &str = r#"
    {
      "metadata": { "title": "Arrow output", "minimum_version": "0.1" },
      "scenarios": { "groups": [{ "name": "case", "size": 2, "labels": ["dry", "wet"] }] },
      "time": { "start": "2015-01-01", "end": "2015-01-03", "timestep": { "type": "Days", "days": 1 } },
      "network": {
        "nodes": [
          { "meta": { "name": "supply" }, "type": "Input", "max_flow": { "type": "Literal", "value": 15 } },
          { "meta": { "name": "demand" }, "type": "Output", "max_flow": { "type": "Literal", "value": 10 } }
        ],
        "edges": [{ "from_node": "supply", "to_node": "demand" }],
        "metric_sets": [{ "meta": { "name": "nodes" }, "metrics": [{ "type": "Node", "name": "demand" }, { "type": "Node", "name": "supply" }] }],
        "outputs": [{
          "meta": { "name": "arrow-output" },
          "type": "ArrowStream",
          "filename": "outputs.arrow",
          "metric_set": "nodes",
          "batch_size": 2
        }]
      }
    }
    "#;

    #[test]
    fn deserializes_and_visits_metric_set() {
        let schema = ModelSchema::from_str(MODEL).unwrap();
        let output = schema.network.outputs.as_ref().unwrap().first().unwrap();
        let mut references = Vec::new();
        output.visit_references(&mut |reference| {
            if let Reference::MetricSet(name) = reference {
                references.push(name.to_string());
            }
        });
        assert_eq!(references, ["nodes"]);
    }

    #[test]
    #[cfg(feature = "core")]
    fn writes_batched_arrow_stream() {
        let schema = ModelSchema::from_str(MODEL).unwrap();
        let output_directory = TempDir::new().unwrap();
        let model = schema
            .create_model_builder(None, Some(output_directory.path()))
            .unwrap()
            .build()
            .unwrap();
        model.run(&ClpSolverSettings::default()).unwrap();

        let path = output_directory.path().join("outputs.arrow");
        let mut reader = StreamReader::try_new_buffered(std::fs::File::open(path).unwrap(), None).unwrap();
        let first_batch = reader.next().unwrap().unwrap();
        assert_eq!(first_batch.num_rows(), 2);
        assert_eq!(first_batch.schema().fields().len(), 4);
        let scenarios: serde_json::Value =
            serde_json::from_str(first_batch.schema().metadata().get("PYWR_SCENARIOS").unwrap()).unwrap();
        assert_eq!(scenarios.as_array().unwrap().len(), 2);
        assert_eq!(scenarios[0]["scenario_labels"][0], "dry");
        assert_eq!(scenarios[1]["scenario_labels"][0], "wet");
        let groups: serde_json::Value =
            serde_json::from_str(first_batch.schema().metadata().get("PYWR_SCENARIO_GROUPS").unwrap()).unwrap();
        assert_eq!(groups, serde_json::json!([{"name": "case", "size": 2}]));
        assert!(
            !first_batch
                .schema()
                .metadata()
                .contains_key("PYWR_SCENARIO_GROUP_00_NAME")
        );
        assert!(
            !first_batch
                .schema()
                .metadata()
                .contains_key("PYWR_SCENARIO_GROUP_00_SIZE")
        );
        assert_eq!(
            first_batch.schema().field(2).metadata().get("ARROW:extension:name"),
            Some(&"org.pywr.metric".to_string())
        );
        for column in first_batch.columns().iter().skip(2) {
            let values = column.as_any().downcast_ref::<FixedSizeListArray>().unwrap();
            assert_eq!(values.value_length(), 2);
            assert_eq!(values.len(), 2);
            assert_eq!(
                values.values().as_any().downcast_ref::<Float64Array>().unwrap().len(),
                4
            );
        }
        assert_eq!(reader.next().unwrap().unwrap().num_rows(), 1);
        assert!(reader.next().is_none());
    }
}
