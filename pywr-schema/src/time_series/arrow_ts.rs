use crate::parameters::ParameterMeta;
use crate::{Checksum, VisitPaths};
use pywr_schema_macros::skip_serializing_none;
use schemars::JsonSchema;
use std::path::{Path, PathBuf};
use strum_macros::{Display, EnumIter};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy, JsonSchema, Display, EnumIter)]
pub enum ArrowFormat {
    #[allow(clippy::upper_case_acronyms)] // These are valid acronyms and should be upper case.
    CSV,
    #[allow(clippy::upper_case_acronyms)]
    IPC,
}

/// A dataset that can be loaded using Apache Arrow.
///
/// This dataset is loaded using Apache Arrow. This is done using the Rust Arrow library to load
/// the dataset.
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArrowTimeSeries {
    pub meta: ParameterMeta,
    pub time_col: Option<String>,
    /// Path to the dataset. If this is a relative path, it will be resolved relative to the provided data path.
    pub path: PathBuf,
    /// The format of the dataset. This can be either CSV or IPC (Arrow's binary format).
    /// If not specified, the format will be inferred from the file extension.
    pub format: Option<ArrowFormat>,
    /// Optional checksum to verify the dataset.
    pub checksum: Option<Checksum>,
}

impl VisitPaths for ArrowTimeSeries {
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        visitor(&self.path);
    }

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        visitor(&mut self.path);
    }
}

#[cfg(feature = "core")]
mod core {
    use super::{ArrowFormat, ArrowTimeSeries};
    use crate::time_series::{LoadedTimeSeries, TimeSeriesError};
    use arrow::array::RecordBatch;
    use arrow::compute::concat_batches;
    use arrow::csv::ReaderBuilder;
    use arrow::csv::reader::Format;
    use arrow::ipc::reader::FileReaderBuilder;
    use std::io::Seek;
    use std::path::Path;
    use std::sync::Arc;

    impl ArrowTimeSeries {
        pub fn load(&self, data_path: Option<&Path>) -> Result<LoadedTimeSeries, TimeSeriesError> {
            let fp = if self.path.is_absolute() {
                self.path.clone()
            } else if let Some(data_path) = data_path {
                data_path.join(self.path.as_path())
            } else {
                self.path.clone()
            };

            let format = match &self.format {
                Some(f) => *f,
                None => {
                    let ext = fp.extension().and_then(|s| s.to_str()).unwrap_or("");
                    // Infer format from file extension
                    match ext {
                        "csv" => ArrowFormat::CSV,
                        "ipc" | "arrow" => ArrowFormat::IPC,
                        _ => {
                            return Err(TimeSeriesError::UnsupportedFileFormat {
                                provider: "Arrow".to_string(),
                                fmt: ext.to_string(),
                            });
                        }
                    }
                }
            };

            // Validate the checksum if provided
            if let Some(checksum) = &self.checksum {
                checksum.check(&fp)?;
            }

            let record_batch = match format {
                ArrowFormat::CSV => load_arrow_time_series_from_csv(&fp)?,
                ArrowFormat::IPC => load_arrow_time_series_from_ipc(&fp)?,
            };

            Ok(LoadedTimeSeries::new(record_batch, self.time_col.clone()))
        }
    }

    fn load_arrow_time_series_from_csv(path: &Path) -> Result<RecordBatch, TimeSeriesError> {
        let mut file = std::fs::File::open(path).map_err(|source| TimeSeriesError::IOError {
            source,
            path: path.to_path_buf(),
        })?;

        // Default format but with a header row.
        let format = Format::default().with_header(true);

        let (schema, _num_records) = format
            .infer_schema(
                &mut file, None, // You can specify the number of rows to sample for schema inference if needed
            )
            .map_err(|source| TimeSeriesError::ArrowError {
                path: path.to_path_buf(),
                source,
            })?;

        let schema = Arc::new(schema);

        // Rewind the file to the beginning after inferring the schema
        file.rewind().map_err(|source| TimeSeriesError::IOError {
            path: path.to_path_buf(),
            source,
        })?;

        let reader = ReaderBuilder::new(schema.clone())
            .with_format(format)
            .build(file)
            .map_err(|source| TimeSeriesError::ArrowError {
                path: path.to_path_buf(),
                source,
            })?;

        let record_batches: Vec<_> =
            reader
                .collect::<Result<_, _>>()
                .map_err(|source| TimeSeriesError::ArrowError {
                    path: path.to_path_buf(),
                    source,
                })?;

        let record_batch =
            concat_batches(&schema, record_batches.iter()).map_err(|source| TimeSeriesError::ArrowError {
                path: path.to_path_buf(),
                source,
            })?;

        Ok(record_batch)
    }

    fn load_arrow_time_series_from_ipc(path: &Path) -> Result<RecordBatch, TimeSeriesError> {
        let mut file = std::fs::File::open(path).map_err(|source| TimeSeriesError::IOError {
            source,
            path: path.to_path_buf(),
        })?;

        let reader = FileReaderBuilder::default()
            .build(&mut file)
            .map_err(|source| TimeSeriesError::ArrowError {
                path: path.to_path_buf(),
                source,
            })?;

        let record_batches: Vec<_> =
            reader
                .collect::<Result<_, _>>()
                .map_err(|source| TimeSeriesError::ArrowError {
                    path: path.to_path_buf(),
                    source,
                })?;

        let schema = record_batches
            .first()
            .ok_or_else(|| TimeSeriesError::ArrowError {
                path: path.to_path_buf(),
                source: arrow::error::ArrowError::SchemaError("No record batches found".to_string()),
            })?
            .schema();

        let record_batch =
            concat_batches(&schema, record_batches.iter()).map_err(|source| TimeSeriesError::ArrowError {
                path: path.to_path_buf(),
                source,
            })?;

        Ok(record_batch)
    }
}
