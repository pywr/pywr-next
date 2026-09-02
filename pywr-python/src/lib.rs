mod exceptions;
mod solver_settings;

use crate::exceptions::{
    PyModelBuilderError, PyModelRunError, PyModelSchemaBuildError, PyMultiNetworkModelBuilderError,
    PyMultiNetworkModelRunError, PyMultiNetworkModelSchemaBuildError, PyRecorderAggregationError,
};
use jiff::civil::DateTime;
use polars::df;
use pyo3::IntoPyObjectExt;
use pyo3::exceptions::{PyKeyError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple, PyType};
use pyo3_polars::PyDataFrame;
use pywr_core::models::{
    Model, ModelResult, ModelTimings, MultiNetworkModel, MultiNetworkModelResult, MultiNetworkModelTimings,
};
use pywr_core::network::NetworkResult;
use pywr_core::parameters::{ParameterInfo, PyScenarioIndex, PyTimestep};
#[cfg(feature = "cbc")]
use pywr_core::solvers::CbcSolver;
#[cfg(feature = "clp")]
use pywr_core::solvers::ClpSolver;
#[cfg(feature = "highs")]
use pywr_core::solvers::HighsSolver;
#[cfg(any(feature = "ipm-simd", feature = "ipm-ocl"))]
use pywr_core::solvers::MultiStateSolver;
#[cfg(feature = "ipm-simd")]
use pywr_core::solvers::SimdIpmF64Solver;
#[cfg(feature = "ipm-ocl")]
use pywr_core::solvers::{ClIpmF32Solver, ClIpmF64Solver, ClIpmSolverSettings};
use pywr_core::solvers::{Solver, SolverSettings};
use pywr_schema::metric::Metric;
use pywr_schema::{
    ComponentConversionError, ConversionData, ConversionError, ModelSchema, MultiNetworkModelSchema, TryIntoV2,
};
use schemars::schema_for;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

/// Convert a Pywr v1.x JSON string to a Pywr v2.x schema.
#[pyfunction]
fn convert_model_from_v1_json_string(py: Python, data: &str) -> PyResult<Py<PyTuple>> {
    // Try to convert
    let (inner, errors) = ModelSchema::from_v1_str(data).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    let schema = PyModelSchema { inner };

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

#[pyclass(name = "NetworkResult", frozen)]
struct PyNetworkResult {
    inner: Arc<NetworkResult>,
}

#[pymethods]
impl PyNetworkResult {
    /// Get the aggregated value of a recorder by name, if it exists and can be aggregated.
    pub fn aggregated_value(&self, name: &str) -> PyResult<f64> {
        self.inner
            .results
            .get(name)
            .ok_or_else(|| PyKeyError::new_err(format!("Output `{}` not found in results", name)))
            .and_then(|r| Ok(r.aggregated_value().map_err(PyRecorderAggregationError::from)?))
    }

    /// An iterator over the names of all available outputs.
    pub fn output_names(&self) -> Vec<String> {
        self.inner.results.keys().map(|k| k.to_string()).collect()
    }

    /// Return an output as a dataframe.
    pub fn to_dataframe(&self, name: &str) -> PyResult<PyDataFrame> {
        self.inner
            .results
            .get(name)
            .ok_or_else(|| PyKeyError::new_err(format!("Output `{}` not found in results", name)))
            .and_then(|r| {
                let records: Vec<_> = r.iter_long_fmt_records().collect();

                df!(
                    "time_start" => records.iter().map(|r| r.time_start.to_string()).collect::<Vec<_>>(),
                    "time_end" => records.iter().map(|r| r.time_end.to_string()).collect::<Vec<_>>(),
                    "simulation_id" => records.iter().map(|r| r.simulation_id as u32).collect::<Vec<_>>(),
                    "label" => records.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
                    "metric_set" => records.iter().map(|r| r.metric_set.as_str()).collect::<Vec<_>>(),
                    "name" => records.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
                    "attribute" => records.iter().map(|r| r.attribute.as_str()).collect::<Vec<_>>(),
                    "value" => records.iter().map(|r| r.value).collect::<Vec<_>>(),
                )
                .map_err(|source| PyRuntimeError::new_err(format!("Failed to create dataframe: {}", source)))
            })
            .map(PyDataFrame)
    }
}

#[pyclass(name = "ModelTimings", frozen)]
pub struct PyModelTimings {
    inner: ModelTimings,
}

#[pymethods]
impl PyModelTimings {
    /// Total duration of the model run in seconds.
    #[getter]
    pub fn total_duration(&self) -> f64 {
        self.inner.total_duration()
    }

    #[getter]
    pub fn speed(&self) -> f64 {
        self.inner.speed()
    }

    fn __repr__(&self) -> String {
        format!(
            "<ModelTimings completed in {:.2} seconds with speed {:.2} time-steps/second>",
            self.total_duration(),
            self.speed()
        )
    }
}

#[pyclass(name = "ModelResult", frozen)]
struct PyModelResult {
    inner: ModelResult,
}

#[pymethods]
impl PyModelResult {
    #[getter]
    fn timings(&self) -> PyModelTimings {
        PyModelTimings {
            inner: self.inner.timings.clone(),
        }
    }
    #[getter]
    fn network_result(&self) -> PyNetworkResult {
        PyNetworkResult {
            inner: self.inner.network_result.clone(),
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "<ModelResult with {} recorder results; {} scenarios completed in {:.2} seconds with speed {:.2} time-steps/second>",
            self.inner.network_result.len(),
            self.inner.domain.scenarios().len(),
            self.inner.timings.total_duration(),
            self.inner.timings.speed()
        )
    }
}

#[pyclass(name = "MultiNetworkModelTimings", frozen)]
struct PyMultiNetworkModelTimings {
    inner: MultiNetworkModelTimings,
}

#[pymethods]
impl PyMultiNetworkModelTimings {
    /// Total duration of the model run in seconds.
    #[getter]
    fn total_duration(&self) -> f64 {
        self.inner.total_duration()
    }

    #[getter]
    fn speed(&self) -> f64 {
        self.inner.speed()
    }

    fn __repr__(&self) -> String {
        format!(
            "<MultiNetworkModelTimings completed in {:.2} seconds with speed {:.2} time-steps/second>",
            self.total_duration(),
            self.speed()
        )
    }
}

#[pyclass(name = "MultiNetworkModelResult", frozen)]
struct PyMultiNetworkModelResult {
    inner: MultiNetworkModelResult,
}

#[pymethods]
impl PyMultiNetworkModelResult {
    #[getter]
    fn timings(&self) -> PyMultiNetworkModelTimings {
        PyMultiNetworkModelTimings {
            inner: self.inner.timings.clone(),
        }
    }
    /// Get a reference to the results map.
    pub fn network_results(&self, name: &str) -> PyResult<PyNetworkResult> {
        self.inner
            .network_results
            .get(name)
            .ok_or_else(|| PyKeyError::new_err(format!("Network result `{}` not found", name)))
            .map(|r| PyNetworkResult { inner: r.clone() })
    }

    fn __repr__(&self) -> String {
        format!(
            "<MultiNetworkModelResult with {} network results; completed in {:.2} seconds with speed {:.2} time-steps/second>",
            self.inner.network_results.len(),
            self.inner.timings.total_duration(),
            self.inner.timings.speed()
        )
    }
}

#[pyclass(name = "Model", frozen)]
struct PyModel {
    inner: Model,
}

impl PyModel {
    /// Run a model using the specified solver unlocking the GIL
    #[cfg(any(feature = "clp", feature = "highs"))]
    fn run_allowing_threads_py<S>(&self, py: Python<'_>, settings: &S::Settings) -> PyResult<PyModelResult>
    where
        S: Solver,
        <S as Solver>::Settings: SolverSettings + Sync,
    {
        let inner = py
            .detach(|| self.inner.run::<S>(settings))
            .map_err(PyModelRunError::from)?;
        Ok(PyModelResult { inner })
    }

    /// Run a model using the specified multi solver unlocking the GIL
    #[cfg(any(feature = "ipm-simd", feature = "ipm-ocl"))]
    fn run_multi_allowing_threads_py<S>(&self, py: Python<'_>, settings: &S::Settings) -> PyResult<PyModelResult>
    where
        S: MultiStateSolver,
        <S as MultiStateSolver>::Settings: SolverSettings + Sync,
    {
        let inner = py
            .detach(|| self.inner.run_multi_scenario::<S>(settings))
            .map_err(PyModelRunError::from)?;
        Ok(PyModelResult { inner })
    }
}

#[pymethods]
impl PyModel {
    /// Run a model using the specified multi solver unlocking the GIL
    #[pyo3(signature = (solver_name, solver_kwargs=None))]
    fn run(
        &self,
        #[cfg_attr(
            not(any(feature = "clp", feature = "highs", feature = "ipm-simd", feature = "ipm-ocl")),
            allow(unused_variables)
        )]
        py: Python<'_>,
        #[cfg_attr(
            not(any(feature = "clp", feature = "highs", feature = "ipm-simd", feature = "ipm-ocl")),
            allow(unused_variables)
        )]
        solver_name: &str,
        #[cfg_attr(
            not(any(feature = "clp", feature = "highs", feature = "ipm-simd", feature = "ipm-ocl")),
            allow(unused_variables)
        )]
        solver_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyModelResult> {
        match solver_name {
            #[cfg(feature = "clp")]
            "clp" => {
                let settings = solver_settings::build_clp_settings_py(solver_kwargs)?;
                self.run_allowing_threads_py::<ClpSolver>(py, &settings)
            }
            #[cfg(feature = "cbc")]
            "cbc" => {
                let settings = solver_settings::build_cbc_settings_py(solver_kwargs)?;
                self.run_allowing_threads_py::<CbcSolver>(py, &settings)
            }
            #[cfg(feature = "highs")]
            "highs" => {
                let settings = solver_settings::build_highs_settings_py(solver_kwargs)?;
                self.run_allowing_threads_py::<HighsSolver>(py, &settings)
            }
            #[cfg(feature = "ipm-simd")]
            "ipm-simd" => {
                let settings = solver_settings::build_ipm_simd_settings_py(solver_kwargs)?;
                self.run_multi_allowing_threads_py::<SimdIpmF64Solver>(py, &settings)
            }
            #[cfg(feature = "ipm-ocl")]
            "clipm-f32" => self.run_multi_allowing_threads_py::<ClIpmF32Solver>(py, &ClIpmSolverSettings::default()),

            #[cfg(feature = "ipm-ocl")]
            "clipm-f64" => self.run_multi_allowing_threads_py::<ClIpmF64Solver>(py, &ClIpmSolverSettings::default()),
            _ => Err(PyRuntimeError::new_err(format!("Unknown solver: {solver_name}",))),
        }
    }
}

#[pyclass(name = "MultiNetworkModel", frozen)]
struct PyMultiNetworkModel {
    inner: MultiNetworkModel,
}

impl PyMultiNetworkModel {
    /// Run a model using the specified solver unlocking the GIL
    #[cfg(any(feature = "clp", feature = "highs"))]
    fn run_allowing_threads_py<S>(&self, py: Python<'_>, settings: &S::Settings) -> PyResult<PyMultiNetworkModelResult>
    where
        S: Solver,
        <S as Solver>::Settings: SolverSettings + Sync,
    {
        let inner = py
            .detach(|| self.inner.run::<S>(settings))
            .map_err(PyMultiNetworkModelRunError::from)?;
        Ok(PyMultiNetworkModelResult { inner })
    }

    /// Run a model using the specified multi solver unlocking the GIL
    #[cfg(any(feature = "ipm-simd", feature = "ipm-ocl"))]
    fn run_multi_allowing_threads_py<S>(
        &self,
        py: Python<'_>,
        settings: &S::Settings,
    ) -> PyResult<PyMultiNetworkModelResult>
    where
        S: MultiStateSolver,
        <S as MultiStateSolver>::Settings: SolverSettings + Sync,
    {
        let inner = py
            .detach(|| self.inner.run_multi_scenario::<S>(settings))
            .map_err(PyMultiNetworkModelRunError::from)?;
        Ok(PyMultiNetworkModelResult { inner })
    }
}

#[pymethods]
impl PyMultiNetworkModel {
    /// Run a model using the specified multi solver unlocking the GIL
    #[pyo3(signature = (solver_name, solver_kwargs=None))]
    fn run(
        &self,
        #[cfg_attr(
            not(any(feature = "clp", feature = "highs", feature = "ipm-simd", feature = "ipm-ocl")),
            allow(unused_variables)
        )]
        py: Python<'_>,
        #[cfg_attr(
            not(any(feature = "clp", feature = "highs", feature = "ipm-simd", feature = "ipm-ocl")),
            allow(unused_variables)
        )]
        solver_name: &str,
        #[cfg_attr(
            not(any(feature = "clp", feature = "highs", feature = "ipm-simd", feature = "ipm-ocl")),
            allow(unused_variables)
        )]
        solver_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyMultiNetworkModelResult> {
        match solver_name {
            #[cfg(feature = "clp")]
            "clp" => {
                let settings = solver_settings::build_clp_settings_py(solver_kwargs)?;
                self.run_allowing_threads_py::<ClpSolver>(py, &settings)
            }
            #[cfg(feature = "cbc")]
            "cbc" => {
                let settings = solver_settings::build_cbc_settings_py(solver_kwargs)?;
                self.run_allowing_threads_py::<CbcSolver>(py, &settings)
            }
            #[cfg(feature = "highs")]
            "highs" => {
                let settings = solver_settings::build_highs_settings_py(solver_kwargs)?;
                self.run_allowing_threads_py::<HighsSolver>(py, &settings)
            }
            #[cfg(feature = "ipm-simd")]
            "ipm-simd" => {
                let settings = solver_settings::build_ipm_simd_settings_py(solver_kwargs)?;
                self.run_multi_allowing_threads_py::<SimdIpmF64Solver>(py, &settings)
            }
            #[cfg(feature = "ipm-ocl")]
            "clipm-f32" => self.run_multi_allowing_threads_py::<ClIpmF32Solver>(py, &ClIpmSolverSettings::default()),

            #[cfg(feature = "ipm-ocl")]
            "clipm-f64" => self.run_multi_allowing_threads_py::<ClIpmF64Solver>(py, &ClIpmSolverSettings::default()),
            _ => Err(PyRuntimeError::new_err(format!("Unknown solver: {solver_name}",))),
        }
    }
}

#[pyclass(name = "ModelSchema")]
struct PyModelSchema {
    inner: ModelSchema,
}

#[pymethods]
impl PyModelSchema {
    #[new]
    fn new(title: &str, start: DateTime, end: DateTime) -> Self {
        let inner = ModelSchema::new(title, &start, &end);
        Self { inner }
    }

    /// Create a new schema object from a file path.
    #[classmethod]
    fn from_path(_cls: &Bound<'_, PyType>, path: PathBuf) -> PyResult<Self> {
        let inner = ModelSchema::from_path(path).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(Self { inner })
    }

    ///  Create a new schema object from a JSON string.
    #[classmethod]
    fn from_json_string(_cls: &Bound<'_, PyType>, data: &str) -> PyResult<Self> {
        let inner = ModelSchema::from_str(data).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(Self { inner })
    }

    /// Serialize the schema to a JSON string.
    fn to_json_string(&self) -> PyResult<String> {
        let data = serde_json::to_string_pretty(&self.inner).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(data)
    }

    /// Build the schema in to a Pywr model.
    #[pyo3(signature = (data_path=None, output_path=None))]
    fn build(&mut self, data_path: Option<PathBuf>, output_path: Option<PathBuf>) -> PyResult<PyModel> {
        let builder = self
            .inner
            .create_model_builder(data_path.as_deref(), output_path.as_deref())
            .map_err(PyModelSchemaBuildError::from)?;
        let inner = builder.build().map_err(PyModelBuilderError::from)?;
        Ok(PyModel { inner })
    }
}

#[pyclass(name = "MultiNetworkModelSchema")]
struct PyMultiNetworkModelSchema {
    inner: MultiNetworkModelSchema,
}

#[pymethods]
impl PyMultiNetworkModelSchema {
    #[new]
    fn new(title: &str, start: DateTime, end: DateTime) -> Self {
        let inner = MultiNetworkModelSchema::new(title, &start, &end);
        Self { inner }
    }

    /// Create a new schema object from a file path.
    #[classmethod]
    fn from_path(_cls: &Bound<'_, PyType>, path: PathBuf) -> PyResult<Self> {
        let inner = MultiNetworkModelSchema::from_path(path).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(Self { inner })
    }

    ///  Create a new schema object from a JSON string.
    #[classmethod]
    fn from_json_string(_cls: &Bound<'_, PyType>, data: &str) -> PyResult<Self> {
        let inner = MultiNetworkModelSchema::from_str(data).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(Self { inner })
    }

    /// Serialize the schema to a JSON string.
    fn to_json_string(&self) -> PyResult<String> {
        let data = serde_json::to_string_pretty(&self.inner).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(data)
    }

    /// Build the schema in to a Pywr model.
    #[pyo3(signature = (data_path=None, output_path=None))]
    fn build(&mut self, data_path: Option<PathBuf>, output_path: Option<PathBuf>) -> PyResult<PyMultiNetworkModel> {
        let builder = self
            .inner
            .create_model_builder(data_path.as_deref(), output_path.as_deref())
            .map_err(PyMultiNetworkModelSchemaBuildError::from)?;
        let inner = builder.build().map_err(PyMultiNetworkModelBuilderError::from)?;
        Ok(PyMultiNetworkModel { inner })
    }
}

/// A Python module implemented in Rust.
#[pymodule]
#[pyo3(name = "_pywr")]
fn pywr(py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    pyo3_log::init();

    m.add_function(wrap_pyfunction!(convert_model_from_v1_json_string, m)?)?;
    m.add_function(wrap_pyfunction!(convert_metric_from_v1_json_string, m)?)?;
    m.add_function(wrap_pyfunction!(export_schema, m)?)?;
    m.add_class::<PyModelSchema>()?;
    m.add_class::<PyMultiNetworkModelSchema>()?;
    m.add_class::<PyModel>()?;
    m.add_class::<PyModelResult>()?;
    m.add_class::<PyMultiNetworkModel>()?;
    m.add_class::<PyMultiNetworkModelResult>()?;
    m.add_class::<PyModelTimings>()?;
    m.add_class::<PyMultiNetworkModelTimings>()?;
    m.add_class::<PyNetworkResult>()?;
    m.add_class::<Metric>()?;
    m.add_class::<PyTimestep>()?;
    m.add_class::<PyScenarioIndex>()?;
    m.add_class::<ParameterInfo>()?;

    // Error classes
    m.add_class::<ComponentConversionError>()?;
    m.add_class::<ConversionError>()?;

    // Exceptions
    m.add("PywrError", py.get_type::<exceptions::PywrError>())?;

    m.add("SchemaBuildError", py.get_type::<exceptions::SchemaBuildError>())?;
    m.add(
        "MultiNetworkSchemaBuildError",
        py.get_type::<exceptions::MultiNetworkSchemaBuildError>(),
    )?;
    m.add("NetworkBuildError", py.get_type::<exceptions::NetworkBuildError>())?;
    m.add(
        "NetworkTransferError",
        py.get_type::<exceptions::NetworkTransferError>(),
    )?;

    m.add("SetupError", py.get_type::<exceptions::SetupError>())?;
    m.add("StepError", py.get_type::<exceptions::StepError>())?;
    m.add("FinaliseError", py.get_type::<exceptions::FinaliseError>())?;

    m.add("AggregationError", py.get_type::<exceptions::AggregationError>())?;
    m.add(
        "RecorderDoesNotSupportAggregation",
        py.get_type::<exceptions::RecorderDoesNotSupportAggregation>(),
    )?;

    Ok(())
}
