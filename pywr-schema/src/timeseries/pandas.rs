use crate::digest::Checksum;
use crate::parameters::ParameterMeta;
use crate::visit::VisitPaths;
use pywr_schema_macros::skip_serializing_none;
use schemars::JsonSchema;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A dataset that can be loaded using Pandas.
///
/// This dataset is loaded using Pandas. This is done via a callback to Python to load the dataset.
/// It is then converted to a Polars DataFrame and returned.
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PandasTimeseries {
    pub meta: ParameterMeta,
    pub time_col: Option<String>,
    pub url: PathBuf,
    /// Keyword arguments to pass to the relevant Pandas load function.
    pub kwargs: Option<HashMap<String, serde_json::Value>>,
    /// Optional checksum to verify the dataset.
    pub checksum: Option<Checksum>,
}

impl VisitPaths for PandasTimeseries {
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        visitor(&self.url);
    }

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        visitor(&mut self.url);
    }
}

#[cfg(all(feature = "core", not(feature = "pyo3")))]
mod core {
    use super::PandasTimeseries;
    use crate::timeseries::TimeseriesError;
    use polars::frame::DataFrame;
    use pywr_core::models::ModelDomain;
    use std::path::Path;

    impl PandasTimeseries {
        pub fn load(&self, _data_path: Option<&Path>, _domain: &ModelDomain) -> Result<DataFrame, TimeseriesError> {
            Err(TimeseriesError::PythonNotEnabled)
        }
    }
}

#[cfg(all(feature = "core", feature = "pyo3"))]
mod core {
    const PANDAS_LOAD_SCRIPT: &CStr = c_str!(include_str!("pandas_load.py"));

    use super::PandasTimeseries;
    use crate::py_utils::try_json_value_into_py;
    use crate::timeseries::TimeseriesError;
    use crate::timeseries::align_and_resample::align_and_resample;
    use polars::frame::DataFrame;
    use pyo3::ffi::c_str;
    use pyo3::prelude::{PyAnyMethods, PyModule};
    use pyo3::types::{PyDict, PyString, PyTuple};
    use pyo3::{IntoPyObject, IntoPyObjectExt, Py, PyAny, PyErr, PyResult, Python};
    use pyo3_polars::PyDataFrame;
    use pywr_core::models::ModelDomain;
    use std::ffi::CStr;
    use std::path::Path;

    impl PandasTimeseries {
        pub fn load(&self, data_path: Option<&Path>, domain: &ModelDomain) -> Result<DataFrame, TimeseriesError> {
            // Prepare the Python interpreter if not already
            Python::initialize();

            let fp = if self.url.is_absolute() {
                self.url.clone()
            } else if let Some(data_path) = data_path {
                data_path.join(self.url.as_path())
            } else {
                self.url.clone()
            };

            // Validate the checksum if provided
            if let Some(checksum) = &self.checksum {
                checksum.check(&fp)?;
            }

            let df: PyDataFrame = Python::attach(|py| -> PyResult<PyDataFrame> {
                let pandas_load =
                    PyModule::from_code(py, PANDAS_LOAD_SCRIPT, c_str!("pandas_load.py"), c_str!("pandas_load"))?;

                let kwargs = self
                    .kwargs
                    .as_ref()
                    .map(|kwargs| {
                        let kwargs: Vec<(Py<PyString>, Option<Py<PyAny>>)> = kwargs
                            .iter()
                            .map(|(k, v)| {
                                let key = k.into_pyobject(py)?.unbind();
                                let value = try_json_value_into_py(py, v)?;
                                Ok((key, value))
                            })
                            .collect::<Result<Vec<_>, PyErr>>()?;

                        let seq = PyTuple::new(py, kwargs)?;

                        PyDict::from_sequence(seq.as_any())
                    })
                    .transpose()?;

                // Time column used as the index, and then Pandas will parse the dates.
                let index_col = self
                    .time_col
                    .as_ref()
                    .map(|col| col.into_bound_py_any(py))
                    .unwrap_or_else(|| 0.into_bound_py_any(py))?;

                let df: PyDataFrame = pandas_load
                    .getattr("load_pandas")?
                    .call((fp, index_col), kwargs.as_ref())?
                    .extract()?;

                Ok(df)
            })?;

            let mut df = df.0;

            df = match self.time_col {
                Some(ref col) => align_and_resample(&self.meta.name, df, col, domain.time(), true)?,
                None => {
                    // If a time col has not been provided assume it is the first column
                    let first_col = df.get_column_names()[0].to_string();
                    align_and_resample(&self.meta.name, df, first_col.as_str(), domain.time(), true)?
                }
            };

            Ok(df)
        }
    }
}
