use std::path::Path;

use polars::{frame::DataFrame, prelude::*};

use crate::SchemaError;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct PolarsDataset {
    #[serde(flatten)]
    index_col: Option<usize>,
}

impl PolarsDataset {
    pub fn load(&self, url: &Path, data_path: Option<&Path>) -> Result<DataFrame, SchemaError> {
        let fp = if url.is_absolute() {
            url.to_path_buf()
        } else if let Some(data_path) = data_path {
            data_path.join(url)
        } else {
            url.to_path_buf()
        };

        let df = match fp.extension() {
            Some(ext) => match ext.to_str() {
                Some("csv") => CsvReader::from_path(fp)?
                    .infer_schema(None)
                    .with_try_parse_dates(true)
                    .has_header(true)
                    .finish()?,
                Some("parquet") => {
                    todo!()
                }
                Some(other_ext) => {
                    return Err(SchemaError::TimeseriesUnsupportedFileFormat {
                        provider: "polars".to_string(),
                        fmt: other_ext.to_string(),
                    })
                }
                None => {
                    return Err(SchemaError::TimeseriesUnparsableFileFormat {
                        provider: "polars".to_string(),
                        path: url.to_string_lossy().to_string(),
                    })
                }
            },
            None => {
                return Err(SchemaError::TimeseriesUnparsableFileFormat {
                    provider: "polars".to_string(),
                    path: url.to_string_lossy().to_string(),
                })
            }
        };

        Ok(df)
    }
}
