use crate::parameters::ParameterMeta;
use crate::{Checksum, VisitPaths};
use pywr_schema_macros::skip_serializing_none;
use schemars::JsonSchema;
use std::path::{Path, PathBuf};

/// A dataset that can be loaded using Apache Arrow.
///
/// This dataset is loaded using Apache Arrow. This is done using the Rust Arrow library to load
/// the dataset.
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ParquetTimeseries {
    pub meta: ParameterMeta,
    pub time_col: Option<String>,
    /// Path to the dataset. If this is a relative path, it will be resolved relative to the provided data path.
    pub path: PathBuf,
    /// Optional checksum to verify the dataset.
    pub checksum: Option<Checksum>,
}

impl VisitPaths for ParquetTimeseries {
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        visitor(&self.path);
    }

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        visitor(&mut self.path);
    }
}

#[cfg(feature = "core")]
mod core {
    use super::ParquetTimeseries;
    use crate::timeseries::{LoadedTimeseries, TimeseriesError};
    use arrow::compute::concat_batches;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    use std::path::Path;

    impl ParquetTimeseries {
        pub fn load(&self, data_path: Option<&Path>) -> Result<LoadedTimeseries, TimeseriesError> {
            let fp = if self.path.is_absolute() {
                self.path.clone()
            } else if let Some(data_path) = data_path {
                data_path.join(self.path.as_path())
            } else {
                self.path.clone()
            };

            // Validate the checksum if provided
            if let Some(checksum) = &self.checksum {
                checksum.check(&fp)?;
            }

            let file = std::fs::File::open(&fp).map_err(|source| TimeseriesError::IOError {
                source,
                path: fp.to_path_buf(),
            })?;

            let builder =
                ParquetRecordBatchReaderBuilder::try_new(file).map_err(|e| TimeseriesError::ParquetError {
                    path: fp.to_path_buf(),
                    source: e,
                })?;

            let reader = builder.build().map_err(|e| TimeseriesError::ParquetError {
                path: fp.to_path_buf(),
                source: e,
            })?;

            let record_batches: Vec<_> =
                reader
                    .collect::<Result<_, _>>()
                    .map_err(|source| TimeseriesError::ArrowError {
                        path: fp.to_path_buf(),
                        source,
                    })?;

            let schema = record_batches
                .first()
                .ok_or_else(|| TimeseriesError::ArrowError {
                    path: fp.to_path_buf(),
                    source: arrow::error::ArrowError::SchemaError("No record batches found".to_string()),
                })?
                .schema();

            let record_batch =
                concat_batches(&schema, record_batches.iter()).map_err(|source| TimeseriesError::ArrowError {
                    path: fp.to_path_buf(),
                    source,
                })?;

            Ok(LoadedTimeseries::new(record_batch, self.time_col.clone()))
        }
    }
}
