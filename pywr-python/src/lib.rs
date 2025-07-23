use chrono::NaiveDateTime;
use pyo3::IntoPyObjectExt;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple, PyType};
use pywr_core::models::ModelRunError;
#[cfg(any(feature = "ipm-ocl", feature = "ipm-simd"))]
use pywr_core::solvers::MultiStateSolver;
#[cfg(feature = "ipm-ocl")]
use pywr_core::solvers::{ClIpmF32Solver, ClIpmF64Solver, ClIpmSolverSettings};
use pywr_core::solvers::{ClpSolver, ClpSolverSettings, ClpSolverSettingsBuilder, Solver, SolverSettings};
#[cfg(feature = "highs")]
use pywr_core::solvers::{HighsSolver, HighsSolverSettings, HighsSolverSettingsBuilder};
#[cfg(feature = "ipm-simd")]
use pywr_core::solvers::{SimdIpmF64Solver, SimdIpmSolverSettings, SimdIpmSolverSettingsBuilder};
use pywr_schema::model::Date;
use pywr_schema::{ComponentConversionError, ConversionData, ConversionError, TryIntoV2};
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

#[pyclass]
pub struct Schema {
    schema: pywr_schema::PywrModel,
}

#[pymethods]
impl Schema {
    #[new]
    fn new(title: &str, start: NaiveDateTime, end: NaiveDateTime) -> Self {
        let start = Date::DateTime(start);
        let end = Date::DateTime(end);

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

    /// Build the schema in to a Pywr model.
    #[pyo3(signature = (data_path=None, output_path=None))]
    fn build(&mut self, data_path: Option<PathBuf>, output_path: Option<PathBuf>) -> PyResult<Model> {
        let model = self.schema.build_model(data_path.as_deref(), output_path.as_deref())?;
        Ok(Model { model })
    }
}

/// Convert a Pywr v1.x JSON string to a Pywr v2.x schema.
#[pyfunction]
fn convert_model_from_v1_json_string(py: Python, data: &str) -> PyResult<Py<PyTuple>> {
    // Try to convert
    let (schema, errors) =
        pywr_schema::PywrModel::from_v1_str(data).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

    // Create a new schema object
    let py_schema = Schema { schema };
    let py_errors = errors
        .into_iter()
        .map(|e| e.into_pyobject(py))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(PyTuple::new(
        py,
        &[py_schema.into_bound_py_any(py)?, py_errors.into_bound_py_any(py)?],
    )?
    .unbind())
}

#[pyclass]
pub struct Metric {
    metric: pywr_schema::metric::Metric,
}

#[pymethods]
impl Metric {
    /// Serialize the metric to a JSON string.
    fn to_json_string(&self) -> PyResult<String> {
        let data = serde_json::to_string_pretty(&self.metric).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(data)
    }
}

/// Convert a Pywr v1.x JSON string to a Pywr v2.x metric.
#[pyfunction]
fn convert_metric_from_v1_json_string(_py: Python, data: &str) -> PyResult<Metric> {
    let v1: pywr_v1_schema::parameters::ParameterValue =
        serde_json::from_str(data).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

    let metric = v1
        .try_into_v2(None, &mut ConversionData::default())
        .map_err(|e: ConversionError| PyRuntimeError::new_err(e.to_string()))?;

    let py_metric = Metric { metric };
    Ok(py_metric)
}

/// Run a model using the specified solver unlocking the GIL
fn run_allowing_threads<S>(
    py: Python<'_>,
    model: &pywr_core::models::Model,
    settings: &S::Settings,
) -> Result<(), PyErr>
where
    S: Solver,
    <S as Solver>::Settings: SolverSettings + Sync,
{
    py.allow_threads(|| {
        let _results = model.run::<S>(settings)?;
        Ok::<(), ModelRunError>(())
    })?;
    Ok(())
}

/// Run a model using the specified multi solver unlocking the GIL
#[cfg(any(feature = "ipm-ocl", feature = "ipm-simd"))]
fn run_multi_allowing_threads<S>(
    py: Python<'_>,
    model: &pywr_core::models::Model,
    settings: &S::Settings,
) -> Result<(), PyErr>
where
    S: MultiStateSolver,
    <S as MultiStateSolver>::Settings: SolverSettings + Sync,
{
    py.allow_threads(|| {
        let _results = model.run_multi_scenario::<S>(settings)?;
        Ok::<(), ModelRunError>(())
    })?;
    Ok(())
}

#[pyclass]
pub struct Model {
    model: pywr_core::models::Model,
}

#[pymethods]
impl Model {
    #[pyo3(signature = (solver_name, solver_kwargs=None))]
    fn run(&self, py: Python<'_>, solver_name: &str, solver_kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
        match solver_name {
            "clp" => {
                let settings = build_clp_settings(solver_kwargs)?;
                run_allowing_threads::<ClpSolver>(py, &self.model, &settings)?;
            }
            #[cfg(feature = "highs")]
            "highs" => {
                let settings = build_highs_settings(solver_kwargs)?;
                run_allowing_threads::<HighsSolver>(py, &self.model, &settings)?;
            }
            #[cfg(feature = "ipm-simd")]
            "ipm-simd" => {
                let settings = build_ipm_simd_settings(solver_kwargs)?;
                run_multi_allowing_threads::<SimdIpmF64Solver>(py, &self.model, &settings)?;
            }
            #[cfg(feature = "ipm-ocl")]
            "clipm-f32" => {
                run_multi_allowing_threads::<ClIpmF32Solver>(py, &self.model, &ClIpmSolverSettings::default())?
            }

            #[cfg(feature = "ipm-ocl")]
            "clipm-f64" => {
                run_multi_allowing_threads::<ClIpmF64Solver>(py, &self.model, &ClIpmSolverSettings::default())?
            }
            _ => return Err(PyRuntimeError::new_err(format!("Unknown solver: {solver_name}",))),
        }

        Ok(())
    }
}

fn build_clp_settings(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<ClpSolverSettings> {
    let mut builder = ClpSolverSettingsBuilder::default();

    if let Some(kwargs) = kwargs {
        if let Ok(value) = kwargs.get_item("threads") {
            if let Some(threads) = value {
                builder = builder.threads(threads.extract::<usize>()?);
            }
            kwargs.del_item("threads")?;
        }

        if let Ok(value) = kwargs.get_item("parallel") {
            if let Some(parallel) = value {
                if parallel.extract::<bool>()? {
                    builder = builder.parallel();
                }
            }
            kwargs.del_item("parallel")?;
        }

        if !kwargs.is_empty() {
            return Err(PyRuntimeError::new_err(format!(
                "Unknown keyword arguments: {kwargs:?}",
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
                builder = builder.threads(threads.extract::<usize>()?);
            }
            kwargs.del_item("threads")?;
        }

        if let Ok(value) = kwargs.get_item("parallel") {
            if let Some(parallel) = value {
                if parallel.extract::<bool>()? {
                    builder = builder.parallel();
                }
            }
            kwargs.del_item("parallel")?;
        }

        if !kwargs.is_empty() {
            return Err(PyRuntimeError::new_err(format!(
                "Unknown keyword arguments: {kwargs:?}",
            )));
        }
    }

    Ok(builder.build())
}

#[cfg(feature = "ipm-simd")]
fn build_ipm_simd_settings(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<SimdIpmSolverSettings> {
    let mut builder = SimdIpmSolverSettingsBuilder::default();

    if let Some(kwargs) = kwargs {
        if let Ok(value) = kwargs.get_item("threads") {
            if let Some(threads) = value {
                builder = builder.threads(threads.extract::<usize>()?);
            }
            kwargs.del_item("threads")?;
        }

        if let Ok(value) = kwargs.get_item("parallel") {
            if let Some(parallel) = value {
                if parallel.extract::<bool>()? {
                    builder = builder.parallel();
                }
            }
            kwargs.del_item("parallel")?;
        }

        if let Ok(value) = kwargs.get_item("ignore_feature_requirements") {
            if let Some(ignore) = value {
                if ignore.extract::<bool>()? {
                    builder = builder.ignore_feature_requirements();
                }
            }
            kwargs.del_item("ignore_feature_requirements")?;
        }

        if !kwargs.is_empty() {
            return Err(PyRuntimeError::new_err(format!(
                "Unknown keyword arguments: {kwargs:?}",
            )));
        }
    }

    Ok(builder.build())
}

/// A Python module implemented in Rust.
#[pymodule]
fn pywr(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    pyo3_log::init();

    m.add_function(wrap_pyfunction!(convert_model_from_v1_json_string, m)?)?;
    m.add_function(wrap_pyfunction!(convert_metric_from_v1_json_string, m)?)?;
    m.add_class::<Schema>()?;
    m.add_class::<Model>()?;
    m.add_class::<Metric>()?;

    // Error classes
    m.add_class::<ComponentConversionError>()?;
    m.add_class::<ConversionError>()?;

    Ok(())
}
