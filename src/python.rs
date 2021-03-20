use crate::metric::Metric;
use crate::model::Model;
use crate::node::Constraint;
use crate::scenario::ScenarioGroupCollection;
use crate::solvers::clp::ClpSolver;
use crate::solvers::Solver;
use crate::timestep::Timestepper;
use crate::{parameters, recorders};
use crate::{EdgeIndex, NodeIndex, PywrError};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::PyErr;

/// Python API
///
/// The following structures provide a Python API to access the core model structures.

impl std::convert::From<PywrError> for PyErr {
    fn from(err: PywrError) -> PyErr {
        PyRuntimeError::new_err(err.to_string())
    }
}

#[pyclass]
struct PyModel {
    model: Model,
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

    fn connect_nodes(&mut self, from_node_name: &str, to_node_name: &str) -> PyResult<EdgeIndex> {
        let from_node = self.model.get_node_by_name(from_node_name)?;
        let to_node = self.model.get_node_by_name(to_node_name)?;

        let edge = self.model.connect_nodes(&from_node, &to_node)?;
        Ok(edge.index())
    }

    fn run(&mut self, solver_name: &str) -> PyResult<()> {
        let timestepper = Timestepper::new("2020-01-01", "2020-01-31", "%Y-%m-%d", 1)?;
        let mut scenarios = ScenarioGroupCollection::new();
        scenarios.add_group("test-scenario", 5);

        let mut solver: Box<dyn Solver> = match solver_name {
            //"glpk" => Box::new(GlpkSolver::new().unwrap()),
            "clp" => Box::new(ClpSolver::new()),
            _ => return Err(PyErr::from(PywrError::UnrecognisedSolver)),
        };

        self.model.run(timestepper, scenarios, &mut solver)?;
        Ok(())
    }

    fn set_node_constraint(&mut self, node_name: &str, parameter_name: &str) -> PyResult<()> {
        let node = self.model.get_node_by_name(node_name)?;
        let parameter = self.model.get_parameter_by_name(parameter_name)?;
        // TODO support setting other constraints
        node.set_constraint(Some(parameter), Constraint::MaxFlow)?;
        Ok(())
    }

    fn set_node_cost(&mut self, node_name: &str, parameter_name: &str) -> PyResult<()> {
        let node = self.model.get_node_by_name(node_name)?;
        let parameter = self.model.get_parameter_by_name(parameter_name)?;
        node.set_cost(Some(parameter));
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

    fn add_python_recorder(
        &mut self,
        name: &str,
        metric: &str,
        index: usize,
        object: PyObject,
    ) -> PyResult<recorders::RecorderIndex> {
        let metric = match metric {
            "NodeInFlow" => Metric::NodeInFlow(index),
            _ => return Err(PyErr::from(PywrError::UnrecognisedMetric)),
        };

        let recorder = recorders::py::PyRecorder::new(name, object, metric);
        let idx = self.model.add_recorder(Box::new(recorder))?.index();
        Ok(idx)
    }
}

/// A Python module implemented in Rust.
#[pymodule]
fn pywr(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyModel>()?;
    // m.add_function(wrap_pyfunction!(sum_as_string, m)?)?;
    // m.add_class::<recorders::py::PyRecorder>()?;

    Ok(())
}
