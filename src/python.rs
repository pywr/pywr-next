use crate::metric::Metric;
use crate::model::Model;
use crate::node::{Constraint, ConstraintValue};
use crate::parameters::AggFunc;
use crate::scenario::ScenarioGroupCollection;
use crate::solvers::clp::ClpSolver;
use crate::solvers::Solver;
use crate::timestep::Timestepper;
use crate::{parameters, recorders};
use crate::{EdgeIndex, NodeIndex, PywrError};
use ndarray::ArrayView1;
use numpy::{PyArrayDyn, PyReadonlyArray1, PyReadonlyArrayDyn};
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::PyErr;
use std::path::Path;
use std::str::FromStr;

/// Python API
///
/// The following structures provide a Python API to access the core model structures.

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

#[pyclass]
struct PyModel {
    model: Model,
}

impl PyModel {
    fn to_constraint_value(&self, value: PyConstraintValue) -> Result<ConstraintValue, PywrError> {
        match value {
            PyConstraintValue::Scalar(v) => Ok(ConstraintValue::Scalar(v)),
            PyConstraintValue::Parameter(name) => {
                let parameter = self.model.get_parameter_by_name(&name)?;
                Ok(ConstraintValue::Parameter(parameter))
            }
            PyConstraintValue::CatchAll(obj) => {
                if obj.is_none() {
                    Ok(ConstraintValue::None)
                } else {
                    Err(PywrError::InvalidConstraintValue(obj.to_string()))
                }
            }
        }
    }
}

#[pymethods]
impl PyModel {
    #[new]
    fn new() -> Self {
        Self { model: Model::new() }
    }

    fn add_input_node(&mut self, name: &str) -> PyResult<NodeIndex> {
        let idx = self.model.add_input_node(name)?.index();
        Ok(idx)
    }

    fn add_link_node(&mut self, name: &str) -> PyResult<NodeIndex> {
        let idx = self.model.add_link_node(name)?.index();
        Ok(idx)
    }

    fn add_output_node(&mut self, name: &str) -> PyResult<NodeIndex> {
        let idx = self.model.add_output_node(name)?.index();
        Ok(idx)
    }

    fn add_storage_node(&mut self, name: &str, initial_volume: f64) -> PyResult<NodeIndex> {
        let idx = self.model.add_storage_node(name, initial_volume)?.index();
        Ok(idx)
    }

    fn connect_nodes(&mut self, from_node_name: &str, to_node_name: &str) -> PyResult<EdgeIndex> {
        let from_node = self.model.get_node_by_name(from_node_name)?;
        let to_node = self.model.get_node_by_name(to_node_name)?;

        let edge = self.model.connect_nodes(&from_node, &to_node)?;
        Ok(edge.index())
    }

    fn run(&mut self, solver_name: &str, start: &str, end: &str, timestep: i64) -> PyResult<()> {
        let timestepper = Timestepper::new(start, end, "%Y-%m-%d", timestep)?;
        let mut scenarios = ScenarioGroupCollection::new();
        scenarios.add_group("test-scenario", 1);

        let mut solver: Box<dyn Solver> = match solver_name {
            //"glpk" => Box::new(GlpkSolver::new().unwrap()),
            "clp" => Box::new(ClpSolver::new()),
            _ => return Err(PyErr::from(PywrError::UnrecognisedSolver)),
        };

        self.model.run(timestepper, scenarios, &mut solver)?;
        Ok(())
    }

    fn set_node_constraint(
        &mut self,
        node_name: &str,
        constraint_type: &str,
        value: PyConstraintValue,
    ) -> PyResult<()> {
        let node = self.model.get_node_by_name(node_name)?;
        let value = self.to_constraint_value(value)?;

        let constraint = match constraint_type {
            "max_flow" => Constraint::MaxFlow,
            "min_flow" => Constraint::MinFlow,
            "max_volume" => Constraint::MaxVolume,
            "min_volume" => Constraint::MinVolume,
            _ => {
                return Err(PyErr::from(PywrError::InvalidConstraintType(
                    constraint_type.to_string(),
                )))
            }
        };
        node.set_constraint(value, constraint)?;
        Ok(())
    }

    fn set_node_cost(&mut self, node_name: &str, value: PyConstraintValue) -> PyResult<()> {
        let node = self.model.get_node_by_name(node_name)?;
        let value = self.to_constraint_value(value)?;
        node.set_cost(value);
        Ok(())
    }

    /// Add a Python object as a parameter.
    fn add_python_parameter(&mut self, name: &str, object: PyObject) -> PyResult<parameters::ParameterIndex> {
        let parameter = parameters::py::PyParameter::new(name, object);
        let idx = self.model.add_parameter(Box::new(parameter))?.index();
        Ok(idx)
    }

    fn add_constant(&mut self, name: &str, value: f64) -> PyResult<parameters::ParameterIndex> {
        let parameter = parameters::ConstantParameter::new(name, value);
        let idx = self.model.add_parameter(Box::new(parameter))?.index();
        Ok(idx)
    }

    fn add_array(&mut self, name: &str, values: PyReadonlyArray1<f64>) -> PyResult<parameters::ParameterIndex> {
        let parameter = parameters::Array1Parameter::new(name, values.to_owned_array());
        let idx = self.model.add_parameter(Box::new(parameter))?.index();
        Ok(idx)
    }

    fn add_aggregated_parameter(
        &mut self,
        name: &str,
        parameter_names: Vec<String>,
        agg_func: &str,
    ) -> PyResult<parameters::ParameterIndex> {
        // Find all the parameters by name
        let mut parameters = Vec::with_capacity(parameter_names.len());
        for name in parameter_names {
            parameters.push(self.model.get_parameter_by_name(&name)?);
        }

        let agg_func = AggFunc::from_str(agg_func)?;
        let parameter = parameters::AggregatedParameter::new(name, parameters, agg_func);

        let idx = self.model.add_parameter(Box::new(parameter))?.index();

        Ok(idx)
    }

    fn add_python_recorder(
        &mut self,
        name: &str,
        component: &str,
        metric: &str,
        object: PyObject,
    ) -> PyResult<recorders::RecorderIndex> {
        let metric = match metric {
            "node_inflow" => Metric::NodeInFlow(self.model.get_node_by_name(component)?.index()),
            "node_outflow" => Metric::NodeOutFlow(self.model.get_node_by_name(component)?.index()),
            "node_volume" => Metric::NodeVolume(self.model.get_node_by_name(component)?.index()),
            // TODO implement edge_flow
            "parameter" => Metric::ParameterValue(self.model.get_parameter_by_name(component)?.index()),
            _ => return Err(PyErr::from(PywrError::UnrecognisedMetric)),
        };

        let recorder = recorders::py::PyRecorder::new(name, object, metric);
        let idx = self.model.add_recorder(Box::new(recorder))?.index();
        Ok(idx)
    }

    fn add_hdf5_output(&mut self, name: &str, filename: &str) -> PyResult<()> {
        let path = Path::new(filename);
        let rec = recorders::hdf::HDF5Recorder::new(name, path.to_path_buf());

        let rec = self.model.add_recorder(Box::new(rec))?;
        Ok(())
    }
}

/// A Python module implemented in Rust.
#[pymodule]
fn pywr(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyModel>()?;
    // m.add_function(wrap_pyfunction!(sum_as_string, m)?)?;
    // m.add_class::<recorders::py::PyRecorder>()?;
    m.add("ParameterNotFoundError", py.get_type::<ParameterNotFoundError>())?;

    Ok(())
}
