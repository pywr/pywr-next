use super::{NetworkState, PywrError, Recorder, RecorderMeta, Timestep};
use crate::metric::Metric;
use crate::scenario::ScenarioIndex;
use crate::state::ParameterState;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

#[derive(Clone, Debug)]
pub struct PyRecorder {
    meta: RecorderMeta,
    object: PyObject,
    metric: Metric,
}

impl PyRecorder {
    pub fn new(name: &str, obj: PyObject, metric: Metric) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            object: obj,
            metric,
        }
    }
}

impl Recorder for PyRecorder {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }

    fn save(
        &mut self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network_state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<(), PywrError> {
        let gil = Python::acquire_gil();
        let py = gil.python();

        let args = PyTuple::new(py, self.metric.get_value(network_state, parameter_state));
        match self.object.call_method1(py, "save", args) {
            Ok(_) => Ok(()),
            Err(_) => Err(PywrError::PythonError),
        }
    }
}
