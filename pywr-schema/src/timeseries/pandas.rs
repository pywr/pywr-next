use crate::visit::VisitPaths;
use schemars::JsonSchema;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A dataset that can be loaded using Pandas.
///
/// This dataset is loaded using Pandas. This is done via a callback to Python to load the dataset.
/// It is then converted to a Polars DataFrame and returned.
///
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema)]
pub struct PandasDataset {
    pub time_col: Option<String>,
    pub url: PathBuf,
    /// Keyword arguments to pass to the relevant Pandas load function.
    pub kwargs: Option<HashMap<String, Value>>,
}

impl VisitPaths for PandasDataset {
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        visitor(&self.url);
    }

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        visitor(&mut self.url);
    }
}

#[cfg(all(feature = "core", not(feature = "pyo3")))]
mod core {
    use super::PandasDataset;
    use crate::timeseries::TimeseriesError;
    use polars::frame::DataFrame;
    use pywr_core::models::ModelDomain;
    use std::path::Path;

    impl PandasDataset {
        pub fn load(
            &self,
            _name: &str,
            _data_path: Option<&Path>,
            _domain: &ModelDomain,
        ) -> Result<DataFrame, TimeseriesError> {
            Err(TimeseriesError::PythonNotEnabled)
        }
    }
}

#[cfg(all(feature = "core", feature = "pyo3"))]
mod core {
    const PANDAS_LOAD_SCRIPT: &str = include_str!("pandas_load.py");

    use super::PandasDataset;
    use crate::parameters::try_json_value_into_py;
    use crate::timeseries::align_and_resample::align_and_resample;
    use crate::timeseries::TimeseriesError;
    use polars::frame::DataFrame;
    use pyo3::prelude::{PyAnyMethods, PyModule};
    use pyo3::types::IntoPyDict;
    use pyo3::{IntoPy, PyResult, Python};
    use pyo3_polars::PyDataFrame;
    use pywr_core::models::ModelDomain;
    use std::path::Path;

    impl PandasDataset {
        pub fn load(
            &self,
            name: &str,
            data_path: Option<&Path>,
            domain: &ModelDomain,
        ) -> Result<DataFrame, TimeseriesError> {
            // Prepare the Python interpreter if not already
            pyo3::prepare_freethreaded_python();

            let fp = if self.url.is_absolute() {
                self.url.clone()
            } else if let Some(data_path) = data_path {
                data_path.join(self.url.as_path())
            } else {
                self.url.clone()
            };

            let df: PyDataFrame = Python::with_gil(|py| -> PyResult<PyDataFrame> {
                let pandas_load = PyModule::from_code_bound(py, PANDAS_LOAD_SCRIPT, "pandas_load.py", "pandas_load")?;

                let kwargs = self.kwargs.as_ref().map(|kwargs| {
                    let seq = kwargs
                        .iter()
                        .map(|(k, v)| (k.into_py(py), try_json_value_into_py(py, v).unwrap()));

                    seq.into_py_dict_bound(py)
                });

                // Time column used as the index, and then Pandas will parse the dates.
                let index_col = self
                    .time_col
                    .as_ref()
                    .map(|col| col.into_py(py))
                    .unwrap_or_else(|| 0.into_py(py));

                let df: PyDataFrame = pandas_load
                    .getattr("load_pandas")?
                    .call((fp, index_col), kwargs.as_ref())?
                    .extract()?;

                Ok(df)
            })
            .expect("Failed to load Pandas dataset");

            let mut df = df.0;

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
