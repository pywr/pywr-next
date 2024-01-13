/// Python API
///
/// The following structures provide a Python API to access the core model structures.
///
///
///

#[cfg(feature = "ipm-ocl")]
use crate::solvers::{ClIpmF32Solver, ClIpmF64Solver, ClIpmSolverSettings};
#[cfg(feature = "highs")]
use crate::solvers::{HighsSolver, HighsSolverSettings};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyDate, PyDateAccess, PyDict, PyType};
use pywr_core::solvers::{ClpSolver, ClpSolverSettings, ClpSolverSettingsBuilder};
use std::fmt;
use std::path::PathBuf;
use time::Date;

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

#[pyclass]
pub struct Schema {
    schema: pywr_schema::PywrModel,
}

#[pymethods]
impl Schema {
    #[new]
    fn new(title: &str, start: &PyDate, end: &PyDate) -> Self {
        // SAFETY: We know that the date & month are valid because it is a Python date.
        let start =
            Date::from_calendar_date(start.get_year(), start.get_month().try_into().unwrap(), start.get_day()).unwrap();
        let end = Date::from_calendar_date(end.get_year(), end.get_month().try_into().unwrap(), end.get_day()).unwrap();

        Self {
            schema: pywr_schema::PywrModel::new(title, &start, &end),
        }
    }

    /// Create a new schema object from a file path.
    #[classmethod]
    fn from_path(_cls: &PyType, path: PathBuf) -> PyResult<Self> {
        Ok(Self {
            schema: pywr_schema::PywrModel::from_path(path)?,
        })
    }

    ///  Create a new schema object from a JSON string.
    #[classmethod]
    fn from_json_string(_cls: &PyType, data: &str) -> PyResult<Self> {
        Ok(Self {
            schema: pywr_schema::PywrModel::from_str(data)?,
        })
    }

    /// Serialize the schema to a JSON string.
    fn to_json_string(&self) -> PyResult<String> {
        let data = serde_json::to_string_pretty(&self.schema).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(data)
    }

    /// Convert the schema to a Pywr model.
    fn build(&mut self, data_path: Option<PathBuf>, output_path: Option<PathBuf>) -> PyResult<Model> {
        let model = self.schema.build_model(data_path.as_deref(), output_path.as_deref())?;
        Ok(Model { model })
    }
}

#[pyclass]
pub struct Model {
    model: pywr_core::models::Model,
}

#[pymethods]
impl Model {
    fn run(&self, solver: &str, solver_kwargs: Option<&PyDict>) -> PyResult<()> {
        match solver {
            "clp" => {
                let settings = build_clp_settings(solver_kwargs)?;
                self.model.run::<ClpSolver>(&settings)?;
            }
            _ => {
                return Err(PyRuntimeError::new_err(format!("Unknown solver: {}", solver)));
            }
        }

        Ok(())
    }
}

fn build_clp_settings(kwargs: Option<&PyDict>) -> PyResult<ClpSolverSettings> {
    let mut builder = ClpSolverSettingsBuilder::default();

    if let Some(kwargs) = kwargs {
        if let Ok(value) = kwargs.get_item("threads") {
            if let Some(threads) = value {
                builder.threads(threads.extract::<usize>()?);
            }
            kwargs.del_item("threads")?;
        }

        if !kwargs.is_empty() {
            return Err(PyRuntimeError::new_err(format!(
                "Unknown keyword arguments: {:?}",
                kwargs
            )));
        }
    }

    Ok(builder.build())
}

/// A Python module implemented in Rust.
#[pymodule]
fn pywr(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<Schema>()?;
    m.add_class::<Model>()?;

    Ok(())
}
