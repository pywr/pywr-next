use crate::visit::VisitPaths;
use schemars::JsonSchema;
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema)]
pub struct PolarsDataset {
    time_col: Option<String>,
    url: PathBuf,
}

impl VisitPaths for PolarsDataset {
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        visitor(&self.url);
    }

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        visitor(&mut self.url);
    }
}

impl PolarsDataset {
    pub fn new(time_col: Option<String>, url: PathBuf) -> Self {
        Self { time_col, url }
    }
}

#[cfg(feature = "core")]
mod core {
    use super::PolarsDataset;
    use crate::timeseries::align_and_resample::align_and_resample;
    use crate::timeseries::TimeseriesError;
    use polars::{frame::DataFrame, prelude::*};
    use pywr_core::models::ModelDomain;
    use std::path::Path;

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
                        Some("csv") => {
                            let parse_options = CsvParseOptions::default().with_try_parse_dates(true);

                            CsvReadOptions::default()
                                .with_schema(None)
                                .with_has_header(true)
                                .with_parse_options(parse_options)
                                .with_infer_schema_length(None)
                                .try_into_reader_with_file_path(Some(fp))?
                                .finish()?
                        }
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

            df = match self.time_col {
                Some(ref col) => align_and_resample(name, df, col, domain, true)?,
                None => {
                    // If a time col has not been provided assume it is the first column
                    let first_col = df.get_column_names()[0].to_string();
                    align_and_resample(name, df, first_col.as_str(), domain, true)?
                }
            };

            Ok(df)
        }
    }
}
