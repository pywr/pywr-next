mod exceptions;
mod solver_settings;

use crate::exceptions::{
    PyModelBuilderError, PyModelProblemBuildError, PyModelProblemEvaluateError, PyModelRunError,
    PyModelSchemaBuildError, PyModelSchemaReadError, PyMultiNetworkModelBuilderError, PyMultiNetworkModelRunError,
    PyMultiNetworkModelSchemaBuildError, PyRecorderAggregationError,
};
use anyhow::anyhow;
use arrow::array::{Float64Builder, RecordBatch, StringBuilder, TimestampMillisecondBuilder, UInt64Builder};
use arrow::pyarrow::PyArrowType;
use jiff::civil::DateTime;
use log::info;
use pyo3::IntoPyObjectExt;
use pyo3::exceptions::{PyKeyError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple, PyType};
use pywr_core::models::{
    Model, ModelResult, ModelTimings, MultiNetworkModel, MultiNetworkModelResult, MultiNetworkModelTimings,
};
use pywr_core::network::NetworkResult;
use pywr_core::parameters::{ParameterInfo, PyScenarioIndex, PyTimestep};
use pywr_core::problem::{Evaluation, ModelProblem, ObjectiveSense};
use pywr_core::recorders::LongFmtArrowRecord;
#[cfg(any(feature = "ipm-simd", feature = "ipm-ocl"))]
use pywr_core::solvers::MultiStateSolverConfig;
use pywr_core::solvers::SolverConfig;
#[cfg(feature = "ipm-ocl")]
use pywr_core::solvers::{ClIpmF32Settings, ClIpmF64Settings};
use pywr_runner_service::{install_interrupt_handler, install_log_router_with};
use pywr_schema::metric::Metric;
use pywr_schema::{
    ComponentConversionError, ConversionData, ConversionError, FileSystem, ModelSchema, MultiNetworkModelSchema,
    ProblemConfig, TryIntoV2,
};
use rayon::prelude::*;
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

    /// Return an output as an Arrow record batch.
    pub fn to_record_batch(&self, name: &str) -> PyResult<PyArrowType<RecordBatch>> {
        self.inner
            .results
            .get(name)
            .ok_or_else(|| PyKeyError::new_err(format!("Output `{}` not found in results", name)))
            .and_then(|r| {
                let schema = Arc::new(LongFmtArrowRecord::schema());

                let mut time_start = TimestampMillisecondBuilder::new();
                let mut time_end = TimestampMillisecondBuilder::new();
                let mut simulation_id = UInt64Builder::new();
                let mut label = StringBuilder::new();
                let mut metric_set = StringBuilder::new();
                let mut name = StringBuilder::new();
                let mut attribute = StringBuilder::new();
                let mut value = Float64Builder::new();

                for record in r.iter_long_fmt_records() {
                    let a_record: LongFmtArrowRecord = record
                        .try_into()
                        .map_err(|e| PyRuntimeError::new_err(format!("Failed to serialise record: {}", e)))?;
                    time_start.append_value(a_record.time_start);
                    time_end.append_value(a_record.time_end);
                    simulation_id.append_value(a_record.simulation_id);
                    label.append_value(a_record.label);
                    metric_set.append_value(a_record.metric_set);
                    name.append_value(a_record.name);
                    attribute.append_value(a_record.attribute);
                    value.append_value(a_record.value);
                }
                let record_batch = RecordBatch::try_new(
                    schema,
                    vec![
                        Arc::new(time_start.finish()),
                        Arc::new(time_end.finish()),
                        Arc::new(simulation_id.finish()),
                        Arc::new(label.finish()),
                        Arc::new(metric_set.finish()),
                        Arc::new(name.finish()),
                        Arc::new(attribute.finish()),
                        Arc::new(value.finish()),
                    ],
                )
                .map_err(|e| PyRuntimeError::new_err(format!("Failed to create record batch: {}", e)))?;

                Ok(PyArrowType(record_batch))
            })
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
    fn run_allowing_threads_py<C>(&self, py: Python<'_>, solver_config: &C) -> PyResult<PyModelResult>
    where
        C: SolverConfig + Sync,
    {
        let inner = py
            .detach(|| self.inner.run(solver_config))
            .map_err(PyModelRunError::from)?;
        Ok(PyModelResult { inner })
    }

    /// Run a model using the specified multi solver unlocking the GIL
    #[cfg(any(feature = "ipm-simd", feature = "ipm-ocl"))]
    fn run_multi_allowing_threads_py<C>(&self, py: Python<'_>, solver_config: &C) -> PyResult<PyModelResult>
    where
        C: MultiStateSolverConfig + Sync,
    {
        let inner = py
            .detach(|| self.inner.run_multi_scenario(solver_config))
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
                let solver_config = solver_settings::build_clp_settings_py(solver_kwargs)?;
                self.run_allowing_threads_py(py, &solver_config)
            }
            #[cfg(feature = "cbc")]
            "cbc" => {
                let solver_config = solver_settings::build_cbc_settings_py(solver_kwargs)?;
                self.run_allowing_threads_py(py, &solver_config)
            }
            #[cfg(feature = "highs")]
            "highs" => {
                let solver_config = solver_settings::build_highs_settings_py(solver_kwargs)?;
                self.run_allowing_threads_py(py, &solver_config)
            }
            #[cfg(feature = "ipm-simd")]
            "ipm-simd" => {
                let solver_config = solver_settings::build_ipm_simd_settings_py(solver_kwargs)?;
                self.run_multi_allowing_threads_py(py, &solver_config)
            }
            #[cfg(feature = "ipm-ocl")]
            "clipm-f32" => self.run_multi_allowing_threads_py(py, &ClIpmF32Settings::default()),

            #[cfg(feature = "ipm-ocl")]
            "clipm-f64" => self.run_multi_allowing_threads_py(py, &ClIpmF64Settings::default()),
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
    fn run_allowing_threads_py<C>(&self, py: Python<'_>, solver_config: &C) -> PyResult<PyMultiNetworkModelResult>
    where
        C: SolverConfig + Sync,
    {
        let inner = py
            .detach(|| self.inner.run(solver_config))
            .map_err(PyMultiNetworkModelRunError::from)?;
        Ok(PyMultiNetworkModelResult { inner })
    }

    /// Run a model using the specified multi solver unlocking the GIL
    #[cfg(any(feature = "ipm-simd", feature = "ipm-ocl"))]
    fn run_multi_allowing_threads_py<C>(&self, py: Python<'_>, solver_config: &C) -> PyResult<PyMultiNetworkModelResult>
    where
        C: MultiStateSolverConfig + Sync,
    {
        let inner = py
            .detach(|| self.inner.run_multi_scenario(solver_config))
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
                let solver_config = solver_settings::build_clp_settings_py(solver_kwargs)?;
                self.run_allowing_threads_py(py, &solver_config)
            }
            #[cfg(feature = "cbc")]
            "cbc" => {
                let solver_config = solver_settings::build_cbc_settings_py(solver_kwargs)?;
                self.run_allowing_threads_py(py, &solver_config)
            }
            #[cfg(feature = "highs")]
            "highs" => {
                let solver_config = solver_settings::build_highs_settings_py(solver_kwargs)?;
                self.run_allowing_threads_py(py, &solver_config)
            }
            #[cfg(feature = "ipm-simd")]
            "ipm-simd" => {
                let solver_config = solver_settings::build_ipm_simd_settings_py(solver_kwargs)?;
                self.run_multi_allowing_threads_py(py, &solver_config)
            }
            #[cfg(feature = "ipm-ocl")]
            "clipm-f32" => self.run_multi_allowing_threads_py(py, &ClIpmF32Settings::default()),

            #[cfg(feature = "ipm-ocl")]
            "clipm-f64" => self.run_multi_allowing_threads_py(py, &ClIpmF64Settings::default()),
            _ => Err(PyRuntimeError::new_err(format!("Unknown solver: {solver_name}",))),
        }
    }
}

/// The result of evaluating a [`PyModelProblem`].
#[pyclass(name = "Evaluation", frozen)]
struct PyEvaluation {
    objectives: Vec<f64>,
    constraints: Vec<f64>,
    constraint_violations: Vec<f64>,
}

#[pymethods]
impl PyEvaluation {
    /// The objective values, in the order of the problem's objectives.
    #[getter]
    fn objectives(&self) -> Vec<f64> {
        self.objectives.clone()
    }

    /// The constraint values, in the order of the problem's constraints.
    #[getter]
    fn constraints(&self) -> Vec<f64> {
        self.constraints.clone()
    }

    /// The amount by which each constraint value is outside its bounds (zero if satisfied).
    #[getter]
    fn constraint_violations(&self) -> Vec<f64> {
        self.constraint_violations.clone()
    }

    /// Whether all the constraints are satisfied.
    #[getter]
    fn is_feasible(&self) -> bool {
        self.constraint_violations.iter().all(|v| *v <= 0.0)
    }

    fn __repr__(&self) -> String {
        format!(
            "<Evaluation objectives={:?} constraints={:?}>",
            self.objectives, self.constraints
        )
    }
}

/// An optimisation problem for a model.
#[pyclass(name = "ModelProblem", frozen)]
struct PyModelProblem {
    inner: ModelProblem,
}

impl PyModelProblem {
    fn to_py_evaluation(&self, evaluation: Evaluation) -> PyEvaluation {
        let constraint_violations = self
            .inner
            .constraints()
            .iter()
            .zip(&evaluation.constraints)
            .map(|(constraint, value)| constraint.violation(*value))
            .collect();

        PyEvaluation {
            objectives: evaluation.objectives,
            constraints: evaluation.constraints,
            constraint_violations,
        }
    }

    /// Evaluate each set of variable values in parallel with the given solver, unlocking the GIL.
    #[cfg(any(feature = "clp", feature = "cbc", feature = "highs"))]
    fn evaluate_batch_with<C>(
        &self,
        py: Python<'_>,
        solver_config: &C,
        values: &[(Vec<f64>, Vec<u64>)],
    ) -> PyResult<Vec<PyEvaluation>>
    where
        C: SolverConfig + Sync,
    {
        let evaluations = py
            .detach(|| {
                values
                    .par_iter()
                    .map(|(values_f64, values_u64)| self.inner.evaluate(solver_config, values_f64, values_u64))
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(PyModelProblemEvaluateError::from)?;

        Ok(evaluations.into_iter().map(|e| self.to_py_evaluation(e)).collect())
    }

    #[cfg_attr(
        not(any(feature = "clp", feature = "cbc", feature = "highs")),
        allow(unused_variables)
    )]
    fn evaluate_batch_py(
        &self,
        py: Python<'_>,
        solver_name: &str,
        solver_kwargs: Option<&Bound<'_, PyDict>>,
        values: &[(Vec<f64>, Vec<u64>)],
    ) -> PyResult<Vec<PyEvaluation>> {
        match solver_name {
            #[cfg(feature = "clp")]
            "clp" => {
                let solver_config = solver_settings::build_clp_settings_py(solver_kwargs)?;
                self.evaluate_batch_with(py, &solver_config, values)
            }
            #[cfg(feature = "cbc")]
            "cbc" => {
                let solver_config = solver_settings::build_cbc_settings_py(solver_kwargs)?;
                self.evaluate_batch_with(py, &solver_config, values)
            }
            #[cfg(feature = "highs")]
            "highs" => {
                let solver_config = solver_settings::build_highs_settings_py(solver_kwargs)?;
                self.evaluate_batch_with(py, &solver_config, values)
            }
            _ => Err(PyRuntimeError::new_err(format!(
                "Unknown or unsupported solver for problems: {solver_name}",
            ))),
        }
    }
}

#[pymethods]
impl PyModelProblem {
    /// The number of f64 and u64 variable values required to evaluate the problem.
    #[getter]
    fn size(&self) -> PyResult<(usize, usize)> {
        self.inner
            .size()
            .map_err(|e| PyRuntimeError::new_err(format!("{:?}", anyhow!(e))))
    }

    /// The lower bounds of the f64 and u64 variable values.
    fn lower_bounds(&self) -> PyResult<(Vec<f64>, Vec<u64>)> {
        let bounds = self
            .inner
            .lower_bounds()
            .map_err(|e| PyRuntimeError::new_err(format!("{:?}", anyhow!(e))))?;
        Ok((bounds.f64, bounds.u64))
    }

    /// The upper bounds of the f64 and u64 variable values.
    fn upper_bounds(&self) -> PyResult<(Vec<f64>, Vec<u64>)> {
        let bounds = self
            .inner
            .upper_bounds()
            .map_err(|e| PyRuntimeError::new_err(format!("{:?}", anyhow!(e))))?;
        Ok((bounds.f64, bounds.u64))
    }

    /// The names of the outputs used as objectives.
    #[getter]
    fn objective_names(&self) -> Vec<String> {
        self.inner
            .objectives()
            .iter()
            .map(|o| o.recorder().to_string())
            .collect()
    }

    /// Whether each objective should be minimised (`"Minimise"`) or maximised (`"Maximise"`).
    #[getter]
    fn objective_senses(&self) -> Vec<&'static str> {
        self.inner
            .objectives()
            .iter()
            .map(|o| match o.sense() {
                ObjectiveSense::Minimise => "Minimise",
                ObjectiveSense::Maximise => "Maximise",
            })
            .collect()
    }

    /// The names of the outputs used as constraints.
    #[getter]
    fn constraint_names(&self) -> Vec<String> {
        self.inner
            .constraints()
            .iter()
            .map(|c| c.recorder().to_string())
            .collect()
    }

    /// The lower and upper bound of each constraint. A bound is `None` if it is not defined.
    #[getter]
    fn constraint_bounds(&self) -> Vec<(Option<f64>, Option<f64>)> {
        self.inner
            .constraints()
            .iter()
            .map(|c| (c.lower_bound(), c.upper_bound()))
            .collect()
    }

    /// Evaluate the problem with the given variable values.
    #[pyo3(signature = (values_f64, values_u64=None, solver_name="clp", solver_kwargs=None))]
    fn evaluate(
        &self,
        py: Python<'_>,
        values_f64: Vec<f64>,
        values_u64: Option<Vec<u64>>,
        solver_name: &str,
        solver_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyEvaluation> {
        let values = [(values_f64, values_u64.unwrap_or_default())];
        let mut evaluations = self.evaluate_batch_py(py, solver_name, solver_kwargs, &values)?;
        evaluations
            .pop()
            .ok_or_else(|| PyRuntimeError::new_err("No evaluation was returned."))
    }

    /// Evaluate the problem with each set of variable values in parallel.
    ///
    /// The number of threads used can be controlled with the `RAYON_NUM_THREADS` environment variable.
    #[pyo3(signature = (values_f64, values_u64=None, solver_name="clp", solver_kwargs=None))]
    fn evaluate_batch(
        &self,
        py: Python<'_>,
        values_f64: Vec<Vec<f64>>,
        values_u64: Option<Vec<Vec<u64>>>,
        solver_name: &str,
        solver_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Vec<PyEvaluation>> {
        let values_u64 = match values_u64 {
            Some(values_u64) if values_u64.len() != values_f64.len() => {
                return Err(PyValueError::new_err(format!(
                    "Expected {} sets of u64 values to match the f64 values, but received {}.",
                    values_f64.len(),
                    values_u64.len()
                )));
            }
            Some(values_u64) => values_u64,
            None => vec![Vec::new(); values_f64.len()],
        };

        let values: Vec<_> = values_f64.into_iter().zip(values_u64).collect();
        self.evaluate_batch_py(py, solver_name, solver_kwargs, &values)
    }

    fn __repr__(&self) -> String {
        format!(
            "<ModelProblem with {} objectives and {} constraints>",
            self.inner.objectives().len(),
            self.inner.constraints().len()
        )
    }
}

/// The definition of an optimisation problem for a model.
#[pyclass(name = "ProblemConfig", frozen)]
struct PyProblemConfig {
    inner: ProblemConfig,
}

#[pymethods]
impl PyProblemConfig {
    /// Create a new problem config from a file path.
    #[classmethod]
    fn from_path(_cls: &Bound<'_, PyType>, path: PathBuf) -> PyResult<Self> {
        let inner = ProblemConfig::from_path(path)
            .map_err(|e| PyErr::new::<exceptions::SchemaReadError, _>(format!("{:?}", anyhow!(e))))?;
        Ok(Self { inner })
    }

    /// Create a new problem config from a JSON string.
    #[classmethod]
    fn from_json_string(_cls: &Bound<'_, PyType>, data: &str) -> PyResult<Self> {
        let inner = serde_json::from_str(data)
            .map_err(|e| PyErr::new::<exceptions::SchemaReadError, _>(format!("{:?}", anyhow!(e))))?;
        Ok(Self { inner })
    }

    /// Serialize the problem config to a JSON string.
    fn to_json_string(&self) -> PyResult<String> {
        let data = serde_json::to_string_pretty(&self.inner)
            .map_err(|e| PyRuntimeError::new_err(format!("{:?}", anyhow!(e))))?;
        Ok(data)
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
        let inner = ModelSchema::from_path(path).map_err(PyModelSchemaReadError::from)?;
        Ok(Self { inner })
    }

    ///  Create a new schema object from a JSON string.
    #[classmethod]
    fn from_json_string(_cls: &Bound<'_, PyType>, data: &str) -> PyResult<Self> {
        let inner = ModelSchema::from_str(data).map_err(PyModelSchemaReadError::from)?;
        Ok(Self { inner })
    }

    /// Serialize the schema to a JSON string.
    fn to_json_string(&self) -> PyResult<String> {
        let data = serde_json::to_string_pretty(&self.inner)
            .map_err(|e| PyRuntimeError::new_err(format!("{:?}", anyhow!(e))))?;
        Ok(data)
    }

    /// Build the schema in to a Pywr model.
    #[pyo3(signature = (data_path=None, output_path=None))]
    fn build(&mut self, data_path: Option<PathBuf>, output_path: Option<PathBuf>) -> PyResult<PyModel> {
        let builder = self
            .inner
            .create_model_builder(&FileSystem, data_path.as_deref(), output_path.as_deref())
            .map_err(PyModelSchemaBuildError::from)?;
        let inner = builder.build().map_err(PyModelBuilderError::from)?;
        Ok(PyModel { inner })
    }

    /// Build the schema in to a Pywr model, and create an optimisation problem for it from the
    /// given problem config.
    #[pyo3(signature = (problem_config, data_path=None, output_path=None))]
    fn build_problem(
        &mut self,
        problem_config: PyRef<'_, PyProblemConfig>,
        data_path: Option<PathBuf>,
        output_path: Option<PathBuf>,
    ) -> PyResult<PyModelProblem> {
        let builder = self
            .inner
            .create_model_builder(&FileSystem, data_path.as_deref(), output_path.as_deref())
            .map_err(PyModelSchemaBuildError::from)?;
        let model = builder.build().map_err(PyModelBuilderError::from)?;
        let inner = problem_config
            .inner
            .build_model_problem(model)
            .map_err(PyModelProblemBuildError::from)?;
        Ok(PyModelProblem { inner })
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
        let inner = MultiNetworkModelSchema::from_path(path).map_err(PyModelSchemaReadError::from)?;
        Ok(Self { inner })
    }

    ///  Create a new schema object from a JSON string.
    #[classmethod]
    fn from_json_string(_cls: &Bound<'_, PyType>, data: &str) -> PyResult<Self> {
        let inner = MultiNetworkModelSchema::from_str(data).map_err(PyModelSchemaReadError::from)?;
        Ok(Self { inner })
    }

    /// Serialize the schema to a JSON string.
    fn to_json_string(&self) -> PyResult<String> {
        let data = serde_json::to_string_pretty(&self.inner)
            .map_err(|e| PyRuntimeError::new_err(format!("{:?}", anyhow!(e))))?;
        Ok(data)
    }

    /// Build the schema in to a Pywr model.
    #[pyo3(signature = (data_path=None, output_path=None))]
    fn build(&mut self, data_path: Option<PathBuf>, output_path: Option<PathBuf>) -> PyResult<PyMultiNetworkModel> {
        let builder = self
            .inner
            .create_model_builder(&FileSystem, data_path.as_deref(), output_path.as_deref())
            .map_err(PyMultiNetworkModelSchemaBuildError::from)?;
        let inner = builder.build().map_err(PyMultiNetworkModelBuilderError::from)?;
        Ok(PyMultiNetworkModel { inner })
    }
}

#[pyfunction]
fn run_server(py: Python<'_>, socket_name: &str) -> PyResult<()> {
    use pywr_runner_service::{RunnerServiceConfigBuilder, run_local_socket_server};

    fn py_check_signals() -> bool {
        Python::attach(|py| {
            // Check for Python signals (like KeyboardInterrupt)
            if let Err(error) = py.check_signals() {
                info!("Received signal, shutting down server: {}", error);
                true
            } else {
                false
            }
        })
    }

    install_interrupt_handler(py_check_signals);

    let config_builder = RunnerServiceConfigBuilder::new();

    let socket_name = socket_name.to_string();
    let config = config_builder.build();

    py.detach(|| run_local_socket_server(&socket_name, config).map_err(|e| PyRuntimeError::new_err(e.to_string())))
}

/// A Python module implemented in Rust.
#[pymodule]
#[pyo3(name = "_pywr")]
fn pywr(py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let python_logger = pyo3_log::Logger::new(py, pyo3_log::Caching::LoggersAndLevels)?;
    install_log_router_with(Box::new(python_logger))
        .map_err(|_| PyRuntimeError::new_err("a global logger has already been installed"))?;

    m.add_function(wrap_pyfunction!(convert_model_from_v1_json_string, m)?)?;
    m.add_function(wrap_pyfunction!(convert_metric_from_v1_json_string, m)?)?;
    m.add_function(wrap_pyfunction!(export_schema, m)?)?;
    // m.add_function(wrap_pyfunction!(build_model_from_project_json, m)?)?;
    m.add_class::<PyModelSchema>()?;
    m.add_class::<PyMultiNetworkModelSchema>()?;
    m.add_class::<PyModel>()?;
    m.add_class::<PyModelResult>()?;
    m.add_class::<PyMultiNetworkModel>()?;
    m.add_class::<PyMultiNetworkModelResult>()?;
    m.add_class::<PyModelTimings>()?;
    m.add_class::<PyMultiNetworkModelTimings>()?;
    m.add_class::<PyNetworkResult>()?;
    m.add_class::<PyProblemConfig>()?;
    m.add_class::<PyModelProblem>()?;
    m.add_class::<PyEvaluation>()?;
    m.add_class::<Metric>()?;
    m.add_class::<PyTimestep>()?;
    m.add_class::<PyScenarioIndex>()?;
    m.add_class::<ParameterInfo>()?;

    // Runner service
    m.add_function(wrap_pyfunction!(run_server, m)?)?;

    // Error classes
    m.add_class::<ComponentConversionError>()?;
    m.add_class::<ConversionError>()?;

    // Exceptions
    m.add("PywrError", py.get_type::<exceptions::PywrError>())?;

    m.add("SchemaReadError", py.get_type::<exceptions::SchemaReadError>())?;
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

    m.add("ProblemBuildError", py.get_type::<exceptions::ProblemBuildError>())?;
    m.add(
        "ProblemEvaluateError",
        py.get_type::<exceptions::ProblemEvaluateError>(),
    )?;

    Ok(())
}
