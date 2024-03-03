use std::path::{Path, PathBuf};

use polars::{frame::DataFrame, prelude::*};
use pywr_core::models::ModelDomain;

use crate::timeseries::TimeseriesError;

use super::align_and_resample::align_and_resample;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct PolarsDataset {
    time_col: String,
    url: PathBuf,
}

impl PolarsDataset {
    pub fn load(
        &self,
        name: &str,
        data_path: Option<&Path>,
        domain: &ModelDomain,
    ) -> Result<DataFrame, TimeseriesError> {
        let fp = if self.url.is_absolute() {
            self.url.clone()
        } else if let Some(data_path) = data_path {
            data_path.join(self.url.as_path())
        } else {
            self.url.clone()
        };

        let mut df = match fp.extension() {
            Some(ext) => {
                let ext = ext.to_str().map(|s| s.to_lowercase());
                match ext.as_deref() {
                    Some("csv") => CsvReader::from_path(fp)?
                        .infer_schema(None)
                        .with_try_parse_dates(true)
                        .has_header(true)
                        .finish()?,
                    Some("parquet") => {
                        todo!()
                    }
                    Some(other_ext) => {
                        return Err(TimeseriesError::TimeseriesUnsupportedFileFormat {
                            provider: "polars".to_string(),
                            fmt: other_ext.to_string(),
                        })
                    }
                    None => {
                        return Err(TimeseriesError::TimeseriesUnparsableFileFormat {
                            provider: "polars".to_string(),
                            path: self.url.to_string_lossy().to_string(),
                        })
                    }
                }
            }
            None => {
                return Err(TimeseriesError::TimeseriesUnparsableFileFormat {
                    provider: "polars".to_string(),
                    path: self.url.to_string_lossy().to_string(),
                })
            }
        };

        df = align_and_resample(name, df, self.time_col.as_str(), domain)?;

        Ok(df)
    }
}
