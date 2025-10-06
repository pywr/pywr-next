use pyo3::IntoPyObjectExt;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use pywr_core::models::{
    Model, ModelResult, ModelRunError, ModelTimings, MultiNetworkModel, MultiNetworkModelResult,
    MultiNetworkModelTimings,
};
use pywr_core::network::NetworkResult;
use pywr_core::parameters::ParameterInfo;
use pywr_core::scenario::ScenarioIndex;
use pywr_core::timestep::Timestep;
use pywr_schema::metric::Metric;
use pywr_schema::{
    ComponentConversionError, ConversionData, ConversionError, ModelSchema, MultiNetworkModelSchema, TryIntoV2,
};
use schemars::schema_for;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
struct PySchemaError {
    error: pywr_schema::SchemaError,
}

impl std::error::Error for PySchemaError {}

impl fmt::Display for PySchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.error)
    }
}

impl From<PySchemaError> for PyErr {
    fn from(err: PySchemaError) -> PyErr {
        PyRuntimeError::new_err(err.to_string())
    }
}

#[derive(Debug)]
struct PyPywrError {
    error: ModelRunError,
}

impl From<PyPywrError> for PyErr {
    fn from(err: PyPywrError) -> PyErr {
        PyRuntimeError::new_err(err.to_string())
    }
}

impl fmt::Display for PyPywrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.error)
    }
}

/// Convert a Pywr v1.x JSON string to a Pywr v2.x schema.
#[pyfunction]
fn convert_model_from_v1_json_string(py: Python, data: &str) -> PyResult<Py<PyTuple>> {
    // Try to convert
    let (schema, errors) = ModelSchema::from_v1_str(data).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

    let py_errors = errors
        .into_iter()
        .map(|e| e.into_pyobject(py))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(PyTuple::new(py, &[schema.into_bound_py_any(py)?, py_errors.into_bound_py_any(py)?])?.unbind())
}

/// Convert a Pywr v1.x JSON string to a Pywr v2.x metric.
#[pyfunction]
fn convert_metric_from_v1_json_string(_py: Python, data: &str) -> PyResult<Metric> {
    let v1: pywr_v1_schema::parameters::ParameterValue =
        serde_json::from_str(data).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

    let metric = v1
        .try_into_v2(None, &mut ConversionData::default())
        .map_err(|e: ConversionError| PyRuntimeError::new_err(e.to_string()))?;

    Ok(metric)
}

/// Export the Pywr schema to a JSON file at the given path.
#[pyfunction]
fn export_schema(_py: Python, out_path: PathBuf) -> PyResult<()> {
    let schema = schema_for!(ModelSchema);

    let contents = serde_json::to_string_pretty(&schema)
        .map_err(|e| PyRuntimeError::new_err(format!("Failed serialise Pywr schema: {}", e)))?;

    std::fs::write(out_path, contents)
        .map_err(|e| PyRuntimeError::new_err(format!("Failed to write schema file: {}", e)))?;

    Ok(())
}

/// A Python module implemented in Rust.
#[pymodule]
fn pywr(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    pyo3_log::init();

    m.add_function(wrap_pyfunction!(convert_model_from_v1_json_string, m)?)?;
    m.add_function(wrap_pyfunction!(convert_metric_from_v1_json_string, m)?)?;
    m.add_function(wrap_pyfunction!(export_schema, m)?)?;
    m.add_class::<ModelSchema>()?;
    m.add_class::<MultiNetworkModelSchema>()?;
    m.add_class::<Model>()?;
    m.add_class::<ModelResult>()?;
    m.add_class::<MultiNetworkModel>()?;
    m.add_class::<MultiNetworkModelResult>()?;
    m.add_class::<ModelTimings>()?;
    m.add_class::<MultiNetworkModelTimings>()?;
    m.add_class::<NetworkResult>()?;
    m.add_class::<Metric>()?;
    m.add_class::<Timestep>()?;
    m.add_class::<ScenarioIndex>()?;
    m.add_class::<ParameterInfo>()?;

    // Error classes
    m.add_class::<ComponentConversionError>()?;
    m.add_class::<ConversionError>()?;

    Ok(())
}
