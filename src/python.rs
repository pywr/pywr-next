use crate::aggregated_node::AggregatedNodeIndex;
use crate::model::Model;
use crate::recorders::HDF5Recorder;
use crate::schema::model::PywrModel;
#[cfg(feature = "ipm-ocl")]
use crate::solvers::{ClIpmF32Solver, ClIpmF64Solver, ClIpmSolverSettings};
use crate::solvers::{ClpSolver, ClpSolverSettings};
#[cfg(feature = "highs")]
use crate::solvers::{HighsSolver, HighsSolverSettings};
use crate::timestep::Timestepper;
use crate::virtual_storage::VirtualStorageIndex;
use crate::{IndexParameterIndex, ParameterIndex, RecorderIndex};
use crate::{NodeIndex, PywrError};
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::PyErr;
use std::ops::Deref;
use std::path::PathBuf;

/// Python API
///
/// The following structures provide a Python API to access the core model structures.
///
///
///

impl IntoPy<PyObject> for ParameterIndex {
    fn into_py(self, py: Python<'_>) -> PyObject {
        // delegates to i32's IntoPy implementation.
        self.deref().into_py(py)
    }
}

impl IntoPy<PyObject> for IndexParameterIndex {
    fn into_py(self, py: Python<'_>) -> PyObject {
        // delegates to i32's IntoPy implementation.
        self.deref().into_py(py)
    }
}

#[derive(FromPyObject)]
enum PyConstraintValue<'a> {
    Scalar(f64),
    Parameter(String),
    #[pyo3(transparent)]
    CatchAll(&'a PyAny), // This extraction never fails
}

create_exception!(pywr, ParameterNotFoundError, PyException);

impl std::convert::From<PywrError> for PyErr {
    fn from(err: PywrError) -> PyErr {
        match err {
            PywrError::ParameterNotFound(name) => ParameterNotFoundError::new_err(name),
            _ => PyRuntimeError::new_err(err.to_string()),
        }
    }
}

// #[derive(FromPyObject)]
// struct PyMetric {
//     metric_type: String,
//     name: Option<String>,
//     component: Option<String>,
//     value: Option<f64>,
// }

// #[pyclass]
// struct PyModel {
//     model: Model,
// }
//
// impl PyModel {
//     fn try_pymetric_into_metric(&self, metric: PyMetric) -> Result<Metric, PywrError> {
//         match metric.metric_type.as_str() {
//             "node_volume" => {
//                 let metric = Metric::NodeVolume(
//                     self.model
//                         .get_node_by_name(
//                             &metric.name.ok_or(PywrError::InvalidMetricType(metric.metric_type))?,
//                             metric.component.as_deref(),
//                         )?
//                         .index(),
//                 );
//                 Ok(metric)
//             }
//             "node_proportional_volume" => {
//                 let metric = Metric::NodeProportionalVolume(
//                     self.model
//                         .get_node_by_name(
//                             &metric.name.ok_or(PywrError::InvalidMetricType(metric.metric_type))?,
//                             metric.component.as_deref(),
//                         )?
//                         .index(),
//                 );
//                 Ok(metric)
//             }
//             "virtual_storage_proportional_volume" => {
//                 let metric = Metric::VirtualStorageProportionalVolume(
//                     self.model
//                         .get_virtual_storage_node_by_name(
//                             &metric.name.ok_or(PywrError::InvalidMetricType(metric.metric_type))?,
//                             metric.component.as_deref(),
//                         )?
//                         .index(),
//                 );
//                 Ok(metric)
//             }
//             "parameter_value" => {
//                 let metric = Metric::ParameterValue(self.model.get_parameter_index_by_name(
//                     &metric.name.ok_or(PywrError::InvalidMetricType(metric.metric_type))?,
//                 )?);
//                 Ok(metric)
//             }
//             "constant_float" => {
//                 let metric = Metric::Constant(metric.value.ok_or(PywrError::InvalidMetricType(metric.metric_type))?);
//                 Ok(metric)
//             }
//             _ => Err(PywrError::InvalidMetricType(metric.metric_type)),
//         }
//     }
//
//     fn to_constraint_value(&self, value: PyConstraintValue) -> Result<ConstraintValue, PywrError> {
//         match value {
//             PyConstraintValue::Scalar(v) => Ok(ConstraintValue::Scalar(v)),
//             PyConstraintValue::Parameter(name) => {
//                 let parameter = self.model.get_parameter_index_by_name(&name)?;
//                 Ok(ConstraintValue::Parameter(parameter))
//             }
//             PyConstraintValue::CatchAll(obj) => {
//                 if obj.is_none() {
//                     Ok(ConstraintValue::None)
//                 } else {
//                     Err(PywrError::InvalidConstraintValue(obj.to_string()))
//                 }
//             }
//         }
//     }
// }

impl IntoPy<PyObject> for NodeIndex {
    fn into_py(self, py: Python) -> PyObject {
        self.deref().into_py(py)
    }
}

impl IntoPy<PyObject> for AggregatedNodeIndex {
    fn into_py(self, py: Python) -> PyObject {
        self.deref().into_py(py)
    }
}

impl IntoPy<PyObject> for VirtualStorageIndex {
    fn into_py(self, py: Python) -> PyObject {
        self.deref().into_py(py)
    }
}

impl IntoPy<PyObject> for RecorderIndex {
    fn into_py(self, py: Python) -> PyObject {
        self.deref().into_py(py)
    }
}

// #[pymethods]
// impl PyModel {
//     #[new]
//     fn new() -> Self {
//         Self { model: Model::new() }
//     }
//
//     fn add_input_node(&mut self, name: &str, sub_name: Option<&str>) -> PyResult<NodeIndex> {
//         let idx = self.model.add_input_node(name, sub_name)?;
//         Ok(idx)
//     }
//
//     fn add_link_node(&mut self, name: &str, sub_name: Option<&str>) -> PyResult<NodeIndex> {
//         let idx = self.model.add_link_node(name, sub_name)?;
//         Ok(idx)
//     }
//
//     fn add_output_node(&mut self, name: &str, sub_name: Option<&str>) -> PyResult<NodeIndex> {
//         let idx = self.model.add_output_node(name, sub_name)?;
//         Ok(idx)
//     }
//
//     fn add_storage_node(&mut self, name: &str, sub_name: Option<&str>, initial_volume: f64) -> PyResult<NodeIndex> {
//         // TODO support proportional initial volume in Python API
//         let idx = self
//             .model
//             .add_storage_node(name, sub_name, StorageInitialVolume::Absolute(initial_volume))?;
//         Ok(idx)
//     }
//
//     fn add_aggregated_node(
//         &mut self,
//         name: &str,
//         sub_name: Option<&str>,
//         node_names: Vec<String>,
//     ) -> PyResult<AggregatedNodeIndex> {
//         let mut nodes = Vec::with_capacity(node_names.len());
//         for name in node_names {
//             nodes.push(self.model.get_node_index_by_name(&name, sub_name)?);
//         }
//
//         let idx = self.model.add_aggregated_node(name, sub_name, nodes)?;
//         Ok(idx)
//     }
//
//     fn add_virtual_storage_node(
//         &mut self,
//         name: &str,
//         sub_name: Option<&str>,
//         node_names: Vec<String>,
//         factors: Option<Vec<f64>>,
//     ) -> PyResult<VirtualStorageIndex> {
//         let mut nodes = Vec::with_capacity(node_names.len());
//         for name in node_names {
//             nodes.push(self.model.get_node_index_by_name(&name, sub_name)?)
//         }
//
//         let idx = self.model.add_virtual_storage_node(name, sub_name, nodes, factors)?;
//         Ok(idx)
//     }
//
//     fn connect_nodes(
//         &mut self,
//         from_node_name: &str,
//         from_node_sub_name: Option<&str>,
//         to_node_name: &str,
//         to_node_sub_name: Option<&str>,
//     ) -> PyResult<EdgeIndex> {
//         let from_node = self.model.get_node_index_by_name(from_node_name, from_node_sub_name)?;
//         let to_node = self.model.get_node_index_by_name(to_node_name, to_node_sub_name)?;
//
//         let edge = self.model.connect_nodes(from_node, to_node)?;
//         Ok(edge.index())
//     }
//
//     fn run(&mut self, solver_name: &str, start: &str, end: &str, timestep: i64) -> PyResult<()> {
//         let format = time::format_description::parse("[year]-[month]-[day]")
//             .map_err(|e| PywrError::InvalidDateFormatDescription(e))?;
//
//         let timestepper = Timestepper::new(
//             time::Date::parse(start, &format).map_err(|e| PywrError::DateParse(e))?,
//             time::Date::parse(end, &format).map_err(|e| PywrError::DateParse(e))?,
//             timestep,
//         );
//         let mut scenarios = ScenarioGroupCollection::new();
//         scenarios.add_group("test-scenario", 1);
//
//         let mut solver: Box<dyn Solver> = match solver_name {
//             //"glpk" => Box::new(GlpkSolver::new().unwrap()),
//             "clp" => Box::new(ClpSolver::new()),
//             _ => return Err(PyErr::from(PywrError::UnrecognisedSolver)),
//         };
//
//         self.model.run(timestepper, scenarios, &mut solver)?;
//
//         Ok(())
//     }
//
//     fn set_node_constraint(
//         &mut self,
//         node_name: &str,
//         node_sub_name: Option<&str>,
//         constraint_type: &str,
//         value: PyConstraintValue,
//     ) -> PyResult<()> {
//         let value = self.to_constraint_value(value)?;
//         let node = self.model.get_mut_node_by_name(node_name, node_sub_name)?;
//
//         let constraint = match constraint_type {
//             "max_flow" => Constraint::MaxFlow,
//             "min_flow" => Constraint::MinFlow,
//             "max_volume" => Constraint::MaxVolume,
//             "min_volume" => Constraint::MinVolume,
//             _ => {
//                 return Err(PyErr::from(PywrError::InvalidConstraintType(
//                     constraint_type.to_string(),
//                 )))
//             }
//         };
//         node.set_constraint(value, constraint)?;
//         Ok(())
//     }
//
//     fn set_aggregated_node_constraint(
//         &mut self,
//         node_name: &str,
//         node_sub_name: Option<&str>,
//         constraint_type: &str,
//         value: PyConstraintValue,
//     ) -> PyResult<()> {
//         let value = self.to_constraint_value(value)?;
//         let node = self.model.get_mut_aggregated_node_by_name(node_name, node_sub_name)?;
//
//         // TODO implemented FromStr for Constraint
//         let constraint = match constraint_type {
//             "max_flow" => Constraint::MaxFlow,
//             "min_flow" => Constraint::MinFlow,
//             "max_volume" => Constraint::MaxVolume,
//             "min_volume" => Constraint::MinVolume,
//             _ => {
//                 return Err(PyErr::from(PywrError::InvalidConstraintType(
//                     constraint_type.to_string(),
//                 )))
//             }
//         };
//         node.set_constraint(value, constraint)?;
//         Ok(())
//     }
//
//     fn set_node_cost(
//         &mut self,
//         node_name: &str,
//         node_sub_name: Option<&str>,
//         value: PyConstraintValue,
//     ) -> PyResult<()> {
//         let value = self.to_constraint_value(value)?;
//         let node = self.model.get_mut_node_by_name(node_name, node_sub_name)?;
//         node.set_cost(value);
//         Ok(())
//     }
//
//     /// Add a Python object as a parameter.
//     fn add_python_parameter(&mut self, name: &str, object: PyObject) -> PyResult<parameters::ParameterIndex> {
//         let parameter = parameters::py::PyParameter::new(name, object);
//         let idx = self.model.add_parameter(Box::new(parameter))?;
//         Ok(idx)
//     }
//
//     fn add_simple_wasm_parameter(
//         &mut self,
//         name: &str,
//         src: Vec<u8>,
//         parameter_names: Vec<String>,
//     ) -> PyResult<parameters::ParameterIndex> {
//         // Find all the parameters by name
//         let mut parameters = Vec::with_capacity(parameter_names.len());
//         for name in parameter_names {
//             parameters.push(self.model.get_parameter_index_by_name(&name)?);
//         }
//
//         let parameter = parameters::simple_wasm::SimpleWasmParameter::new(name, src, parameters);
//         let idx = self.model.add_parameter(Box::new(parameter))?;
//         Ok(idx)
//     }
//
//     fn add_constant(&mut self, name: &str, value: f64) -> PyResult<parameters::ParameterIndex> {
//         let parameter = parameters::ConstantParameter::new(name, value);
//         let idx = self.model.add_parameter(Box::new(parameter))?;
//         Ok(idx)
//     }
//
//     fn add_array(&mut self, name: &str, values: PyReadonlyArray1<f64>) -> PyResult<parameters::ParameterIndex> {
//         let parameter = parameters::Array1Parameter::new(name, values.to_owned_array());
//         let idx = self.model.add_parameter(Box::new(parameter))?;
//         Ok(idx)
//     }
//
//     fn add_aggregated_parameter(
//         &mut self,
//         name: &str,
//         parameter_names: Vec<String>,
//         agg_func: &str,
//     ) -> PyResult<parameters::ParameterIndex> {
//         // Find all the parameters by name
//         let mut parameters = Vec::with_capacity(parameter_names.len());
//         for name in parameter_names {
//             parameters.push(self.model.get_parameter_index_by_name(&name)?);
//         }
//
//         let agg_func = AggFunc::from_str(agg_func)?;
//         let parameter = parameters::AggregatedParameter::new(name, parameters, agg_func);
//
//         let idx = self.model.add_parameter(Box::new(parameter))?;
//
//         Ok(idx)
//     }
//
//     fn add_aggregated_index_parameter(
//         &mut self,
//         name: &str,
//         parameter_names: Vec<String>,
//         agg_func: &str,
//     ) -> PyResult<parameters::IndexParameterIndex> {
//         // Find all the parameters by name
//         let mut parameters = Vec::with_capacity(parameter_names.len());
//         for name in parameter_names {
//             parameters.push(self.model.get_index_parameter_index_by_name(&name)?);
//         }
//
//         let agg_func = AggIndexFunc::from_str(agg_func)?;
//         let parameter = parameters::AggregatedIndexParameter::new(name, parameters, agg_func);
//
//         let idx = self.model.add_index_parameter(Box::new(parameter))?;
//
//         Ok(idx)
//     }
//
//     fn add_piecewise_control_curve(
//         &mut self,
//         name: &str,
//         metric: PyMetric,
//         control_curve_names: Vec<String>,
//         values: Vec<(f64, f64)>,
//         maximum: f64,
//         minimum: f64,
//     ) -> PyResult<parameters::ParameterIndex> {
//         let metric = self.try_pymetric_into_metric(metric)?;
//
//         let mut control_curves = Vec::with_capacity(control_curve_names.len());
//         for name in control_curve_names {
//             control_curves.push(Metric::ParameterValue(self.model.get_parameter_index_by_name(&name)?));
//         }
//
//         let parameter = parameters::control_curves::PiecewiseInterpolatedParameter::new(
//             name,
//             metric,
//             control_curves,
//             values,
//             maximum,
//             minimum,
//         );
//         let idx = self.model.add_parameter(Box::new(parameter))?;
//         Ok(idx)
//     }
//
//     fn add_control_curve_index_parameter(
//         &mut self,
//         name: &str,
//         metric: PyMetric,
//         control_curve_names: Vec<String>,
//     ) -> PyResult<parameters::IndexParameterIndex> {
//         let metric = self.try_pymetric_into_metric(metric)?;
//
//         let mut control_curves = Vec::with_capacity(control_curve_names.len());
//         for name in control_curve_names {
//             control_curves.push(Metric::ParameterValue(self.model.get_parameter_index_by_name(&name)?));
//         }
//
//         let parameter = parameters::control_curves::ControlCurveIndexParameter::new(name, metric, control_curves);
//         let idx = self.model.add_index_parameter(Box::new(parameter))?;
//         Ok(idx)
//     }
//
//     fn add_control_curve_interpolated_parameter(
//         &mut self,
//         name: &str,
//         metric: PyMetric,
//         control_curve_names: Vec<String>,
//         values: Vec<f64>,
//     ) -> PyResult<parameters::ParameterIndex> {
//         let metric = self.try_pymetric_into_metric(metric)?;
//
//         let mut control_curves = Vec::with_capacity(control_curve_names.len());
//         for name in control_curve_names {
//             control_curves.push(Metric::ParameterValue(self.model.get_parameter_index_by_name(&name)?));
//         }
//
//         let parameter = parameters::control_curves::InterpolatedParameter::new(name, metric, control_curves, values);
//         let idx = self.model.add_parameter(Box::new(parameter))?;
//         Ok(idx)
//     }
//
//     fn add_control_curve_parameter(
//         &mut self,
//         name: &str,
//         metric: PyMetric,
//         control_curve_names: Vec<String>,
//         parameter_names: Vec<String>,
//     ) -> PyResult<parameters::ParameterIndex> {
//         let metric = self.try_pymetric_into_metric(metric)?;
//
//         let mut control_curves = Vec::with_capacity(control_curve_names.len());
//         for name in control_curve_names {
//             control_curves.push(Metric::ParameterValue(self.model.get_parameter_index_by_name(&name)?));
//         }
//
//         let mut parameters = Vec::with_capacity(parameter_names.len());
//         for name in parameter_names {
//             parameters.push(Metric::ParameterValue(self.model.get_parameter_index_by_name(&name)?));
//         }
//
//         let parameter =
//             parameters::control_curves::ControlCurveParameter::new(name, metric, control_curves, parameters);
//         let idx = self.model.add_parameter(Box::new(parameter))?;
//         Ok(idx)
//     }
//
//     fn add_max_parameter(
//         &mut self,
//         name: &str,
//         metric: PyMetric,
//         threshold: f64,
//     ) -> PyResult<parameters::ParameterIndex> {
//         let metric = self.try_pymetric_into_metric(metric)?;
//
//         let parameter = parameters::MaxParameter::new(name, metric, threshold);
//         let idx = self.model.add_parameter(Box::new(parameter))?;
//         Ok(idx)
//     }
//
//     fn add_negative_parameter(&mut self, name: &str, metric: PyMetric) -> PyResult<parameters::ParameterIndex> {
//         let metric = self.try_pymetric_into_metric(metric)?;
//
//         let parameter = parameters::NegativeParameter::new(name, metric);
//         let idx = self.model.add_parameter(Box::new(parameter))?;
//         Ok(idx)
//     }
//
//     fn add_asymmetric_index_parameter(
//         &mut self,
//         name: &str,
//         on_parameter_name: &str,
//         off_parameter_name: &str,
//     ) -> PyResult<parameters::IndexParameterIndex> {
//         let on_parameter = self.model.get_index_parameter_index_by_name(on_parameter_name)?;
//         let off_parameter = self.model.get_index_parameter_index_by_name(off_parameter_name)?;
//
//         let parameter = parameters::asymmetric::AsymmetricSwitchIndexParameter::new(name, on_parameter, off_parameter);
//         let idx = self.model.add_index_parameter(Box::new(parameter))?;
//         Ok(idx)
//     }
//
//     fn add_indexed_array_parameter(
//         &mut self,
//         name: &str,
//         index_parameter_name: &str,
//         parameter_names: Vec<String>,
//     ) -> PyResult<parameters::ParameterIndex> {
//         let index_parameter = self.model.get_index_parameter_index_by_name(index_parameter_name)?;
//
//         let mut parameters = Vec::with_capacity(parameter_names.len());
//         for name in parameter_names {
//             parameters.push(self.model.get_parameter_index_by_name(&name)?);
//         }
//
//         let parameter = parameters::indexed_array::IndexedArrayParameter::new(name, index_parameter, parameters);
//         let idx = self.model.add_parameter(Box::new(parameter))?;
//         Ok(idx)
//     }
//
//     fn add_threshold_parameter(
//         &mut self,
//         name: &str,
//         metric: PyMetric,
//         threshold: PyMetric,
//         predicate: &str,
//         ratchet: bool,
//     ) -> PyResult<parameters::IndexParameterIndex> {
//         let metric = self.try_pymetric_into_metric(metric)?;
//         let threshold = self.try_pymetric_into_metric(threshold)?;
//
//         let parameter = parameters::ThresholdParameter::new(
//             name,
//             metric,
//             threshold,
//             parameters::Predicate::from_str(predicate)?,
//             ratchet,
//         );
//         let idx = self.model.add_index_parameter(Box::new(parameter))?;
//         Ok(idx)
//     }
//
//     fn add_polynomial1d_parameter(
//         &mut self,
//         name: &str,
//         metric: PyMetric,
//         coefficients: Vec<f64>,
//         scale: f64,
//         offset: f64,
//     ) -> PyResult<parameters::ParameterIndex> {
//         let metric = self.try_pymetric_into_metric(metric)?;
//
//         let parameter = parameters::Polynomial1DParameter::new(name, metric, coefficients, scale, offset);
//         let idx = self.model.add_parameter(Box::new(parameter))?;
//         Ok(idx)
//     }
//
//     fn add_monthly_profile_parameter(&mut self, name: &str, values: [f64; 12]) -> PyResult<parameters::ParameterIndex> {
//         let parameter = parameters::MonthlyProfileParameter::new(name, values);
//         let idx = self.model.add_parameter(Box::new(parameter))?;
//         Ok(idx)
//     }
//
//     fn add_daily_profile_parameter(&mut self, name: &str, values: [f64; 366]) -> PyResult<parameters::ParameterIndex> {
//         let parameter = parameters::DailyProfileParameter::new(name, values);
//         let idx = self.model.add_parameter(Box::new(parameter))?;
//         Ok(idx)
//     }
//
//     fn add_uniform_drawdown_profile_parameter(
//         &mut self,
//         name: &str,
//         reset_day: u8,
//         reset_month: u8,
//         residual_days: u8,
//     ) -> PyResult<parameters::ParameterIndex> {
//         let reset_month = time::Month::try_from(reset_month).map_err(|e| PywrError::InvalidDateComponentRange(e))?;
//
//         let parameter = parameters::UniformDrawdownProfileParameter::new(name, reset_day, reset_month, residual_days);
//         let idx = self.model.add_parameter(Box::new(parameter))?;
//         Ok(idx)
//     }
//
//     fn add_python_recorder(
//         &mut self,
//         name: &str,
//         component: &str,
//         component_sub_name: Option<&str>,
//         metric: &str,
//         object: PyObject,
//     ) -> PyResult<recorders::RecorderIndex> {
//         let metric = match metric {
//             "node_inflow" => Metric::NodeInFlow(self.model.get_node_index_by_name(component, component_sub_name)?),
//             "node_outflow" => Metric::NodeOutFlow(self.model.get_node_index_by_name(component, component_sub_name)?),
//             "node_volume" => Metric::NodeVolume(self.model.get_node_index_by_name(component, component_sub_name)?),
//             // TODO implement edge_flow
//             "parameter" => Metric::ParameterValue(self.model.get_parameter_index_by_name(component)?),
//             _ => return Err(PyErr::from(PywrError::UnrecognisedMetric)),
//         };
//
//         let recorder = recorders::py::PyRecorder::new(name, object, metric);
//         let idx = self.model.add_recorder(Box::new(recorder))?;
//         Ok(idx)
//     }
//
//     fn add_hdf5_output(&mut self, name: &str, filename: &str) -> PyResult<()> {
//         let path = Path::new(filename);
//
//         let metrics = self
//             .model
//             .nodes
//             .iter()
//             .map(|n| {
//                 let metric = n.default_metric();
//                 let (name, sub_name) = n.full_name();
//                 (metric, (name.to_string(), sub_name.map(|sn| sn.to_string())))
//             })
//             .collect();
//
//         let rec = recorders::hdf::HDF5Recorder::new(name, path.to_path_buf(), metrics);
//
//         let _rec = self.model.add_recorder(Box::new(rec))?;
//         Ok(())
//     }
// }

#[pyfunction]
fn load_model(path: PathBuf) {
    let data = std::fs::read_to_string(path).unwrap();
    let _schema_v2: PywrModel = serde_json::from_str(data.as_str()).unwrap();
}

#[pyfunction]
fn load_model_from_string(data: String) {
    let _schema_v2: PywrModel = serde_json::from_str(data.as_str()).unwrap();
}

#[pyfunction]
fn run_model_from_path(
    py: Python<'_>,
    path: PathBuf,
    solver_name: String,
    data_path: Option<PathBuf>,
    num_threads: Option<usize>,
) -> PyResult<()> {
    let data = std::fs::read_to_string(path.clone()).unwrap();

    let data_path = match data_path {
        None => path.parent().map(|dp| dp.to_path_buf()),
        Some(dp) => Some(dp),
    };

    run_model_from_string(py, data, solver_name, data_path, num_threads)
}

#[pyfunction]
fn run_model_from_string(
    py: Python<'_>,
    data: String,
    solver_name: String,
    path: Option<PathBuf>,
    num_threads: Option<usize>,
) -> PyResult<()> {
    // TODO handle the serde error properly
    let schema_v2: PywrModel = serde_json::from_str(data.as_str()).unwrap();

    let (mut model, timestepper): (Model, Timestepper) = schema_v2.build_model(path.as_deref(), None)?;

    let nt = num_threads.unwrap_or(1);

    py.allow_threads(|| {
        match solver_name.as_str() {
            "clp" => model.run::<ClpSolver>(&timestepper, &ClpSolverSettings::default()),
            #[cfg(feature = "highs")]
            "highs" => model.run::<HighsSolver>(&timestepper, &HighsSolverSettings::default()),
            #[cfg(feature = "ipm-ocl")]
            "clipm-f32" => model.run_multi_scenario::<ClIpmF32Solver>(&timestepper, &ClIpmSolverSettings::default()),
            #[cfg(feature = "ipm-ocl")]
            "clipm-f64" => model.run_multi_scenario::<ClIpmF64Solver>(&timestepper, &ClIpmSolverSettings::default()),
            _ => panic!("Solver {solver_name} not recognised."),
        }
        .unwrap();
    });

    Ok(())
}

/// A Python module implemented in Rust.
#[pymodule]
fn pywr(py: Python, m: &PyModule) -> PyResult<()> {
    pyo3_log::init();

    m.add_function(wrap_pyfunction!(load_model, m)?)?;
    m.add_function(wrap_pyfunction!(load_model_from_string, m)?)?;
    m.add_function(wrap_pyfunction!(run_model_from_string, m)?)?;
    m.add_function(wrap_pyfunction!(run_model_from_path, m)?)?;

    // m.add_class::<recorders::py::PyRecorder>()?;
    m.add("ParameterNotFoundError", py.get_type::<ParameterNotFoundError>())?;

    Ok(())
}
