use super::{NetworkState, Parameter, ParameterMeta, PywrError, Timestep};
use crate::scenario::ScenarioIndex;
use pyo3::prelude::*;

pub struct PyParameter {
    meta: ParameterMeta,
    object: PyObject,
}

impl PyParameter {
    pub fn new(name: &str, obj: PyObject) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            object: obj,
        }
    }
}

impl Parameter for PyParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _state: &NetworkState,
        _parameter_state: &[f64],
    ) -> Result<f64, PywrError> {
        let gil = Python::acquire_gil();
        let py = gil.python();

        let value: f64 = match self.object.call_method0(py, "compute") {
            Ok(py_value) => match py_value.extract(py) {
                Ok(v) => v,
                Err(_) => return Err(PywrError::PythonError),
            },
            Err(_) => return Err(PywrError::PythonError),
        };

        Ok(value)
    }
}
