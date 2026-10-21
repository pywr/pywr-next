#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::meta::NamedMeta;
use crate::visit::{Reference, ReferenceMut, VisitReferences};
#[cfg(feature = "core")]
use pywr_core::recorders::ParquetOutputBuilder;
use pywr_schema_macros::{PywrVisitPaths, PywrVisitReferences};
use schemars::JsonSchema;
use std::num::NonZeroUsize;
#[cfg(feature = "core")]
use std::path::Path;
use std::path::PathBuf;
use strum_macros::{Display, EnumIter};

/// The codec used to compress a Parquet file's pages.
#[derive(
    serde::Deserialize,
    serde::Serialize,
    Debug,
    Clone,
    Copy,
    Default,
    JsonSchema,
    PywrVisitPaths,
    PywrVisitReferences,
    Display,
    EnumIter,
)]
pub enum ParquetCompression {
    Uncompressed,
    Snappy,
    Lz4,
    #[default]
    Zstd,
}

#[cfg(feature = "core")]
impl From<ParquetCompression> for parquet::basic::Compression {
    fn from(value: ParquetCompression) -> Self {
        match value {
            ParquetCompression::Uncompressed => Self::UNCOMPRESSED,
            ParquetCompression::Snappy => Self::SNAPPY,
            ParquetCompression::Lz4 => Self::LZ4_RAW,
            ParquetCompression::Zstd => Self::ZSTD(Default::default()),
        }
    }
}

/// Output one metric set to a Parquet file.
///
/// The file holds the same table as an [`ArrowStreamOutput`](super::ArrowStreamOutput): a row
/// for each model timestep, and a column for each metric storing a fixed-size list of `Float64`
/// values in scenario order. The Arrow schema is stored in the file, so Arrow readers get the
/// same column types and the `PYWR_SCENARIOS` and `PYWR_SCENARIO_GROUPS` metadata. That metadata
/// is also written as plain Parquet key-value metadata.
///
/// Each row group contains up to `batch_size` model timesteps. The file is only complete once
/// the model run finishes.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitPaths)]
pub struct ParquetOutput {
    pub meta: NamedMeta,
    pub filename: PathBuf,
    /// The metric set to write.
    pub metric_set: String,
    /// Number of model timesteps to write in each Parquet row group.
    pub batch_size: NonZeroUsize,
    /// The codec used to compress the file's pages. Defaults to Zstandard.
    #[serde(default)]
    pub compression: ParquetCompression,
}

/// Written out rather than derived: a derive would walk `metric_set` as a plain `String`.
impl VisitReferences for ParquetOutput {
    fn visit_references<F: FnMut(Reference<'_>)>(&self, visitor: &mut F) {
        visitor(Reference::MetricSet(&self.metric_set));
    }

    fn visit_references_mut<F: FnMut(ReferenceMut<'_>)>(&mut self, visitor: &mut F) {
        visitor(ReferenceMut::MetricSet(&mut self.metric_set));
    }
}

#[cfg(feature = "core")]
impl ParquetOutput {
    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        output_path: Option<&Path>,
    ) -> Result<(), SchemaError> {
        let filename = match (output_path, self.filename.is_relative()) {
            (Some(output_directory), true) => output_directory.join(&self.filename),
            _ => self.filename.to_path_buf(),
        };
        let mut recorder = ParquetOutputBuilder::new(&self.meta.name, &filename, &self.metric_set, self.batch_size);
        recorder.compression(self.compression.into());
        network.recorder(Box::new(recorder));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "core")]
    use crate::FileSystem;
    use crate::ModelSchema;
    #[cfg(feature = "core")]
    use arrow::compute::concat_batches;
    #[cfg(feature = "core")]
    use arrow::ipc::reader::StreamReader;
    #[cfg(feature = "core")]
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    #[cfg(feature = "core")]
    use pywr_core::solvers::ClpSolverSettings;
    use std::str::FromStr;
    #[cfg(feature = "core")]
    use tempfile::TempDir;

    /// The same metric set written by an Arrow stream and a Parquet output.
    const MODEL: &str = r#"
    {
      "metadata": { "title": "Parquet output", "minimum_version": "0.1" },
      "scenarios": { "groups": [{ "name": "case", "size": 2, "labels": ["dry", "wet"] }] },
      "time": { "start": "2015-01-01", "end": "2015-01-05", "timestep": { "type": "Days", "days": 1 } },
      "network": {
        "nodes": [
          { "meta": { "name": "supply" }, "type": "Input", "max_flow": { "type": "Literal", "value": 15 } },
          { "meta": { "name": "demand" }, "type": "Output", "max_flow": { "type": "Literal", "value": 10 } }
        ],
        "edges": [{ "from_node": "supply", "to_node": "demand" }],
        "metric_sets": [{ "meta": { "name": "nodes" }, "metrics": [{ "type": "Node", "name": "demand" }, { "type": "Node", "name": "supply" }] }],
        "outputs": [
          { "meta": { "name": "arrow-output" }, "type": "ArrowStream", "filename": "outputs.arrow", "metric_set": "nodes", "batch_size": 2 },
          { "meta": { "name": "parquet-output" }, "type": "Parquet", "filename": "outputs.parquet", "metric_set": "nodes", "batch_size": 2 }
        ]
      }
    }
    "#;

    #[test]
    fn compression_defaults_to_zstd() {
        let schema = ModelSchema::from_str(MODEL).unwrap();
        let outputs = schema.network.outputs.as_ref().unwrap();
        let crate::outputs::Output::Parquet(output) = &outputs[1] else {
            panic!("expected a Parquet output");
        };
        assert!(matches!(output.compression, super::ParquetCompression::Zstd));
    }

    #[test]
    #[cfg(feature = "core")]
    fn writes_the_same_table_as_the_arrow_stream() {
        let schema = ModelSchema::from_str(MODEL).unwrap();
        let output_directory = TempDir::new().unwrap();
        let model = schema
            .create_model_builder(&FileSystem, None, Some(output_directory.path()))
            .unwrap()
            .build()
            .unwrap();
        model.run(&ClpSolverSettings::default()).unwrap();

        let stream = StreamReader::try_new_buffered(
            std::fs::File::open(output_directory.path().join("outputs.arrow")).unwrap(),
            None,
        )
        .unwrap();
        let stream_schema = stream.schema();
        let stream_batches: Vec<_> = stream.collect::<Result<_, _>>().unwrap();

        let builder = ParquetRecordBatchReaderBuilder::try_new(
            std::fs::File::open(output_directory.path().join("outputs.parquet")).unwrap(),
        )
        .unwrap();
        // One row group per batch of two timesteps, the last holding the fifth.
        assert_eq!(builder.metadata().num_row_groups(), 3);
        let parquet_schema = builder.schema().clone();
        assert_eq!(parquet_schema, stream_schema);
        let parquet_batches: Vec<_> = builder.build().unwrap().collect::<Result<_, _>>().unwrap();

        // Parquet's batches don't carry the schema metadata, so compare their columns.
        let stream_table = concat_batches(&stream_schema, &stream_batches).unwrap();
        let parquet_table = concat_batches(&parquet_batches[0].schema(), &parquet_batches).unwrap();
        assert_eq!(stream_table.num_rows(), 5);
        assert_eq!(stream_table.columns(), parquet_table.columns());
    }
}
