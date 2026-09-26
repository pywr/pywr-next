use crate::digest::Checksum;
use crate::meta::NamedMeta;
use crate::visit::VisitPaths;
use pywr_schema_macros::skip_serializing_none;
use schemars::JsonSchema;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PolarsTimeSeries {
    pub meta: NamedMeta,
    pub time_col: Option<String>,
    /// Path to the dataset. If this is a relative path, it will be resolved relative to the provided data path.
    pub path: PathBuf,
    /// Keyword arguments to pass to the relevant Polars load function.
    pub kwargs: Option<HashMap<String, serde_json::Value>>,
    /// Optional checksum to verify the dataset.
    pub checksum: Option<Checksum>,
}

impl VisitPaths for PolarsTimeSeries {
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        visitor(&self.path);
    }

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        visitor(&mut self.path);
    }
}

#[cfg(all(feature = "core", not(feature = "pyo3")))]
mod core {
    use super::PolarsTimeSeries;
    use crate::time_series::{LoadedTimeSeries, TimeSeriesError};
    use std::path::Path;

    impl PolarsTimeSeries {
        pub fn load(&self, _data_path: Option<&Path>) -> Result<LoadedTimeSeries, TimeSeriesError> {
            Err(TimeSeriesError::PythonNotEnabled)
        }
    }
}

#[cfg(all(feature = "core", feature = "pyo3"))]
mod core {
    use super::PolarsTimeSeries;
    use crate::time_series::load_py::{LoadModule, load_record_batch_from_py_callback};
    use crate::time_series::{LoadedTimeSeries, TimeSeriesError};
    use std::path::Path;

    impl PolarsTimeSeries {
        pub fn load(&self, data_path: Option<&Path>) -> Result<LoadedTimeSeries, TimeSeriesError> {
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

            let kwargs = self.kwargs.clone().unwrap_or_default();

            let lt = load_record_batch_from_py_callback(
                LoadModule::Builtin,
                "load_polars",
                &fp,
                self.time_col.as_deref(),
                &None,
                &kwargs,
            )?;

            Ok(lt)
        }
    }
}
