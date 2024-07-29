use chrono::NaiveDateTime;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyType};

/// Python API
///
/// The following structures provide a Python API to access the core model structures.
///
///
///

#[cfg(feature = "ipm-ocl")]
use pywr_core::solvers::{ClIpmF32Solver, ClIpmF64Solver, ClIpmSolverSettings};
use pywr_core::solvers::{ClpSolver, ClpSolverSettings, ClpSolverSettingsBuilder};
#[cfg(feature = "highs")]
use pywr_core::solvers::{HighsSolver, HighsSolverSettings, HighsSolverSettingsBuilder};
use pywr_schema::model::DateType;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

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
    error: pywr_core::PywrError,
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

#[pyclass]
pub struct Schema {
    schema: pywr_schema::PywrModel,
}

#[pymethods]
impl Schema {
    #[new]
    fn new(title: &str, start: NaiveDateTime, end: NaiveDateTime) -> Self {
        let start = DateType::DateTime(start);
        let end = DateType::DateTime(end);

        Self {
            schema: pywr_schema::PywrModel::new(title, &start, &end),
        }
    }

    /// Create a new schema object from a file path.
    #[classmethod]
    fn from_path(_cls: &Bound<'_, PyType>, path: PathBuf) -> PyResult<Self> {
        Ok(Self {
            schema: pywr_schema::PywrModel::from_path(path)?,
        })
    }

    ///  Create a new schema object from a JSON string.
    #[classmethod]
    fn from_json_string(_cls: &Bound<'_, PyType>, data: &str) -> PyResult<Self> {
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
    fn run(&self, solver_name: &str, solver_kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
        match solver_name {
            "clp" => {
                let settings = build_clp_settings(solver_kwargs)?;
                self.model.run::<ClpSolver>(&settings)?;
            }
            #[cfg(feature = "highs")]
            "highs" => {
                let settings = build_highs_settings(solver_kwargs)?;
                self.model.run::<HighsSolver>(&settings)?;
            }
            #[cfg(feature = "ipm-ocl")]
            "clipm-f32" => self
                .model
                .run_multi_scenario::<ClIpmF32Solver>(&ClIpmSolverSettings::default()),
            #[cfg(feature = "ipm-ocl")]
            "clipm-f64" => self
                .model
                .run_multi_scenario::<ClIpmF64Solver>(&ClIpmSolverSettings::default()),
            _ => return Err(PyRuntimeError::new_err(format!("Unknown solver: {}", solver_name))),
        }

        Ok(())
    }
}

fn build_clp_settings(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<ClpSolverSettings> {
    let mut builder = ClpSolverSettingsBuilder::default();

    if let Some(kwargs) = kwargs {
        if let Ok(value) = kwargs.get_item("threads") {
            if let Some(threads) = value {
                builder.threads(threads.extract::<usize>()?);
            }
            kwargs.del_item("threads")?;
        }

        if let Ok(value) = kwargs.get_item("parallel") {
            if let Some(parallel) = value {
                if parallel.extract::<bool>()? {
                    builder.parallel();
                }
            }
            kwargs.del_item("parallel")?;
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

#[cfg(feature = "highs")]
fn build_highs_settings(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<HighsSolverSettings> {
    let mut builder = HighsSolverSettingsBuilder::default();

    if let Some(kwargs) = kwargs {
        if let Ok(value) = kwargs.get_item("threads") {
            if let Some(threads) = value {
                builder.threads(threads.extract::<usize>()?);
            }
            kwargs.del_item("threads")?;
        }

        if let Ok(value) = kwargs.get_item("parallel") {
            if let Some(parallel) = value {
                if parallel.extract::<bool>()? {
                    builder.parallel();
                }
            }
            kwargs.del_item("parallel")?;
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
fn pywr(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    pyo3_log::init();

    m.add_class::<Schema>()?;
    m.add_class::<Model>()?;

    Ok(())
}
