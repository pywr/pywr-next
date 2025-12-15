//! This module contains utility functions for working with Python code in the Pywr schema.
//!
//!
#[cfg(all(feature = "core", feature = "pyo3"))]
use crate::data_tables::make_path;
#[cfg(all(feature = "core", feature = "pyo3"))]
use crate::error::SchemaError;
#[cfg(all(feature = "core", feature = "pyo3"))]
use pyo3::{
    Bound, IntoPyObjectExt, PyAny, PyErr, Python,
    prelude::{IntoPyObject, Py, PyModule},
    types::{PyDict, PyTuple},
};
use pywr_schema_macros::PywrVisitAll;
use schemars::JsonSchema;
#[cfg(all(feature = "core", feature = "pyo3"))]
use serde_json::Value;
#[cfg(all(feature = "core", feature = "pyo3"))]
use std::collections::HashMap;
#[cfg(all(feature = "core", feature = "pyo3"))]
use std::ffi::CString;
#[cfg(all(feature = "core", feature = "pyo3"))]
use std::path::Path;
use std::path::PathBuf;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};

/// A source for Python code, either a module name or a file path.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, Display, EnumDiscriminants, PywrVisitAll)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(PythonSourceType))]
pub enum PythonSource {
    Module { module: String },
    Path { path: PathBuf },
}

#[cfg(all(feature = "core", feature = "pyo3"))]
impl PythonSource {
    /// Load the Python module specified by this source.
    pub fn load_module<'py>(
        &self,
        py: Python<'py>,
        data_path: Option<&Path>,
    ) -> Result<Bound<'py, PyModule>, SchemaError> {
        let module = match &self {
            PythonSource::Module { module } => PyModule::import(py, module.as_str()),
            PythonSource::Path { path } => {
                let path = &make_path(path, data_path);
                let code = CString::new(std::fs::read_to_string(path).map_err(|error| SchemaError::IO {
                    path: path.to_path_buf(),
                    error,
                })?)
                .unwrap();

                let file_name = CString::new(path.file_name().unwrap().to_str().unwrap()).unwrap();
                let module_name = CString::new(path.file_stem().unwrap().to_str().unwrap()).unwrap();
                PyModule::from_code(py, &code, &file_name, &module_name)
            }
        }?;

        Ok(module)
    }
}

/// Try to load optional positional arguments for a Python function from a JSON value.
///
/// If `args` is `None`, an empty tuple is returned. If `args` is `Some`, each value is converted
/// to a Python object and returned as a tuple.
#[cfg(all(feature = "core", feature = "pyo3"))]
pub fn try_load_optional_py_args(py: Python, args: &Option<Vec<Value>>) -> Result<Py<PyTuple>, PyErr> {
    match args {
        None => Ok(PyTuple::empty(py).unbind()),
        Some(args) => {
            let py_args = args
                .iter()
                .map(|arg| try_json_value_into_py(py, arg))
                .collect::<Result<Vec<_>, PyErr>>()?;
            Ok::<_, PyErr>(PyTuple::new(py, py_args)?.unbind())
        }
    }
}

/// Try to load keyword arguments for a Python function from a JSON value.
///
/// If `kwargs` is `None`, an empty dictionary is returned. If `kwargs` is `Some`, each value is converted
/// to a Python object and returned as a dictionary.
#[cfg(all(feature = "core", feature = "pyo3"))]
pub fn try_load_optional_py_kwargs(py: Python, kwargs: &Option<HashMap<String, Value>>) -> Result<Py<PyDict>, PyErr> {
    match kwargs {
        None => Ok(PyDict::new(py).unbind()),
        Some(kwargs) => {
            let kwargs = kwargs
                .iter()
                .map(|(k, v)| {
                    let key = k.into_pyobject(py)?.unbind();
                    let value = try_json_value_into_py(py, v)?;
                    Ok((key, value))
                })
                .collect::<Result<Vec<_>, PyErr>>()?;
            let seq = PyTuple::new(py, kwargs)?;

            Ok::<_, PyErr>(PyDict::from_sequence(seq.as_any())?.unbind())
        }
    }
}

#[cfg(all(feature = "core", feature = "pyo3"))]
pub fn try_json_value_into_py(py: Python, value: &Value) -> Result<Option<Py<PyAny>>, PyErr> {
    let py_value: Option<Py<PyAny>> = match value {
        Value::Null => None,
        Value::Bool(v) => Some(v.into_py_any(py)?),
        Value::Number(v) => {
            if let Some(i) = v.as_i64() {
                Some(i.into_py_any(py)?)
            } else if let Some(f) = v.as_f64() {
                Some(f.into_py_any(py)?)
            } else {
                panic!("Could not convert JSON number to Python type.");
            }
        }
        Value::String(v) => Some(v.into_py_any(py)?),
        Value::Array(array) => Some(
            array
                .iter()
                .map(|v| try_json_value_into_py(py, v).unwrap())
                .collect::<Vec<_>>()
                .into_py_any(py)?,
        ),
        Value::Object(map) => Some(
            map.iter()
                .map(|(k, v)| (k, try_json_value_into_py(py, v).unwrap()))
                .collect::<HashMap<_, _>>()
                .into_py_any(py)?,
        ),
    };

    Ok(py_value)
}
