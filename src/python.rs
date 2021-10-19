use crate::metric::Metric;
use crate::model::Model;
use crate::node::{Constraint, ConstraintValue};
use crate::parameters::{AggFunc, AggIndexFunc};
use crate::scenario::ScenarioGroupCollection;
use crate::solvers::clp::ClpSolver;
use crate::solvers::Solver;
use crate::timestep::Timestepper;
use crate::{parameters, recorders};
use crate::{EdgeIndex, NodeIndex, PywrError};

use numpy::PyReadonlyArray1;
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

    fn add_simple_wasm_parameter(
        &mut self,
        name: &str,
        src: Vec<u8>,
        parameter_names: Vec<String>,
    ) -> PyResult<parameters::ParameterIndex> {
        // Find all the parameters by name
        let mut parameters = Vec::with_capacity(parameter_names.len());
        for name in parameter_names {
            parameters.push(self.model.get_parameter_by_name(&name)?);
        }

        let parameter = parameters::simple_wasm::SimpleWasmParameter::new(name, src, parameters);
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

    fn add_aggregated_index_parameter(
        &mut self,
        name: &str,
        parameter_names: Vec<String>,
        agg_func: &str,
    ) -> PyResult<parameters::IndexParameterIndex> {
        // Find all the parameters by name
        let mut parameters = Vec::with_capacity(parameter_names.len());
        for name in parameter_names {
            parameters.push(self.model.get_index_parameter_by_name(&name)?);
        }

        let agg_func = AggIndexFunc::from_str(agg_func)?;
        let parameter = parameters::AggregatedIndexParameter::new(name, parameters, agg_func);

        let idx = self.model.add_index_parameter(Box::new(parameter))?.index();

        Ok(idx)
    }

    fn add_piecewise_control_curve(
        &mut self,
        name: &str,
        storage_node: &str,
        control_curve_names: Vec<String>,
        values: Vec<(f64, f64)>,
        maximum: f64,
        minimum: f64,
    ) -> PyResult<parameters::ParameterIndex> {
        let metric = Metric::NodeProportionalVolume(self.model.get_node_by_name(storage_node)?.index());

        let mut control_curves = Vec::with_capacity(control_curve_names.len());
        for name in control_curve_names {
            control_curves.push(Metric::ParameterValue(self.model.get_parameter_by_name(&name)?.index()));
        }

        let parameter = parameters::control_curves::PiecewiseInterpolatedParameter::new(
            name,
            metric,
            control_curves,
            values,
            maximum,
            minimum,
        );
        let idx = self.model.add_parameter(Box::new(parameter))?.index();
        Ok(idx)
    }

    fn add_control_curve_index_parameter(
        &mut self,
        name: &str,
        storage_node: &str,
        control_curve_names: Vec<String>,
    ) -> PyResult<parameters::IndexParameterIndex> {
        let metric = Metric::NodeProportionalVolume(self.model.get_node_by_name(storage_node)?.index());

        let mut control_curves = Vec::with_capacity(control_curve_names.len());
        for name in control_curve_names {
            control_curves.push(Metric::ParameterValue(self.model.get_parameter_by_name(&name)?.index()));
        }

        let parameter = parameters::control_curves::ControlCurveIndexParameter::new(name, metric, control_curves);
        let idx = self.model.add_index_parameter(Box::new(parameter))?.index();
        Ok(idx)
    }

    fn add_asymmetric_index_parameter(
        &mut self,
        name: &str,
        on_parameter_name: &str,
        off_parameter_name: &str,
    ) -> PyResult<parameters::IndexParameterIndex> {
        let on_parameter = self.model.get_index_parameter_by_name(on_parameter_name)?;
        let off_parameter = self.model.get_index_parameter_by_name(off_parameter_name)?;

        let parameter = parameters::asymmetric::AsymmetricSwitchIndexParameter::new(name, on_parameter, off_parameter);
        let idx = self.model.add_index_parameter(Box::new(parameter))?.index();
        Ok(idx)
    }

    fn add_indexed_array_parameter(
        &mut self,
        name: &str,
        index_parameter_name: &str,
        parameter_names: Vec<String>,
    ) -> PyResult<parameters::ParameterIndex> {
        let index_parameter = self.model.get_index_parameter_by_name(index_parameter_name)?;

        let mut parameters = Vec::with_capacity(parameter_names.len());
        for name in parameter_names {
            parameters.push(self.model.get_parameter_by_name(&name)?);
        }

        let parameter = parameters::indexed_array::IndexedArrayParameter::new(name, index_parameter, parameters);
        let idx = self.model.add_parameter(Box::new(parameter))?.index();
        Ok(idx)
    }

    fn add_parameter_threshold_parameter(
        &mut self,
        name: &str,
        parameter_name: &str,
        threshold_name: &str,
        predicate: &str,
        ratchet: bool,
    ) -> PyResult<parameters::IndexParameterIndex> {
        let metric = Metric::ParameterValue(self.model.get_parameter_by_name(parameter_name)?.index());
        let threshold = self.model.get_parameter_by_name(threshold_name)?;

        let parameter = parameters::ThresholdParameter::new(
            name,
            metric,
            threshold,
            parameters::Predicate::from_str(predicate)?,
            ratchet,
        );
        let idx = self.model.add_index_parameter(Box::new(parameter))?.index();
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

        let _rec = self.model.add_recorder(Box::new(rec))?;
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
