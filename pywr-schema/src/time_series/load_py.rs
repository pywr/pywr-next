use crate::py_utils::{try_json_value_into_py, try_load_optional_py_kwargs};
use crate::time_series::{LoadedTimeSeries, TimeSeriesError};
use arrow::array::RecordBatch;
use arrow::pyarrow::PyArrowType;
use pyo3::ffi::c_str;
use pyo3::prelude::{PyAnyMethods, PyModule};
use pyo3::types::PyTuple;
use pyo3::{Bound, IntoPyObject, PyAny, PyErr, PyResult, Python};
use std::collections::HashMap;
use std::ffi::CStr;
use std::path::Path;

const LOAD_SCRIPT: &CStr = c_str!(include_str!("load.py"));

/// An enum representing the module to load for a Python time series loader.
pub enum LoadModule {
    /// Use the built-in "load.py" module.
    Builtin,
    /// Use a custom module specified by the user.
    Custom(String),
}

/// Load a record batch from a Python callback using the provided URL and optional data path.
pub fn load_record_batch_from_py_callback(
    module: LoadModule,
    function: &str,
    path: &Path,
    time_column: Option<&str>,
    args: &Option<Vec<serde_json::Value>>,
    kwargs: &HashMap<String, serde_json::Value>,
) -> Result<LoadedTimeSeries, TimeSeriesError> {
    // Prepare the Python interpreter if not already
    Python::initialize();

    let df: PyArrowType<RecordBatch> = Python::attach(|py| -> PyResult<PyArrowType<RecordBatch>> {
        let module = match module {
            LoadModule::Builtin => PyModule::from_code(py, LOAD_SCRIPT, c_str!("load.py"), c_str!("load"))?,
            LoadModule::Custom(module_name) => PyModule::import(py, module_name)?,
        };
        let mut py_args: Vec<Bound<PyAny>> = vec![
            path.to_str()
                .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyValueError, _>("Invalid path"))?
                .into_pyobject(py)?
                .into_any(),
        ];

        if let Some(args) = args {
            let optional_py_args = args
                .iter()
                .map(|arg| Ok(try_json_value_into_py(py, arg)?.into_pyobject(py)?.into_any()))
                .collect::<Result<Vec<_>, PyErr>>()?;
            py_args.extend(optional_py_args);
        }

        let py_args = PyTuple::new(py, py_args)?;

        let py_kwargs = try_load_optional_py_kwargs(py, Some(kwargs))?;

        let df: PyArrowType<RecordBatch> = module
            .getattr(function)?
            .call(py_args, Some(py_kwargs.bind(py)))?
            .extract()?;

        Ok(df)
    })?;

    Ok(LoadedTimeSeries::new(df.0, time_column.map(|s| s.to_string())))
}
