use super::{Parameter, ParameterMeta, PywrError, Timestep};
use crate::model::Model;
use crate::parameters::FloatValue;
use crate::scenario::ScenarioIndex;
use crate::state::State;
use pyo3::prelude::*;
use pyo3::types::{PyDate, PyTuple};
use std::any::Any;

pub struct PyParameter {
    meta: ParameterMeta,
    object: Py<PyAny>,
    parameters: Vec<FloatValue>,
}

struct Internal {
    user_obj: PyObject,
}

impl Internal {
    fn into_boxed_any(self) -> Box<dyn Any + Send> {
        Box::new(self)
    }
}

impl PyParameter {
    pub fn new(name: &str, object: Py<PyAny>, parameters: &[FloatValue]) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            object,
            parameters: parameters.to_vec(),
        }
    }
}

impl Parameter for PyParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn Any + Send>>, PywrError> {
        let user_obj: PyObject = Python::with_gil(|py| -> PyResult<PyObject> { self.object.call0(py) }).unwrap();

        let internal = Internal { user_obj };

        Ok(Some(internal.into_boxed_any()))
    }

    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        _model: &Model,
        state: &State,
        internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<f64, PywrError> {
        let internal = match internal_state {
            Some(internal) => match internal.downcast_mut::<Internal>() {
                Some(pa) => pa,
                None => panic!("Internal state did not downcast to the correct type! :("),
            },
            None => panic!("No internal state defined when one was expected! :("),
        };

        let value: f64 = Python::with_gil(|py| {
            let date = PyDate::new(
                py,
                timestep.date.year(),
                timestep.date.month() as u8,
                timestep.date.day(),
            )?;

            let si = scenario_index.index.into_py(py);

            let p_states = self
                .parameters
                .iter()
                .map(|value| value.get_value(state))
                .collect::<Result<Vec<f64>, _>>()?
                .into_py(py);

            let args = PyTuple::new(py, [date, si.as_ref(py), p_states.as_ref(py)]);

            internal.user_obj.call_method1(py, "calc", args)?.extract(py)
        })
        .map_err(|e| PywrError::PythonError(e.to_string()))?;

        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::default_timestepper;
    use float_cmp::assert_approx_eq;

    #[test]
    /// Test `PythonParameter` returns the correct value.
    fn test_constant_parameter() {
        // Init Python
        pyo3::prepare_freethreaded_python();

        let class = Python::with_gil(|py| {
            let test_module = PyModule::from_code(
                py,
                r#"
class MyParameter:
    def __init__(self):
        self.count = 0

    def calc(self, ts, si, p_values):
        self.count += si
        return float(self.count + ts.day)
"#,
                "",
                "",
            )
            .unwrap();

            test_module.getattr("MyParameter").unwrap().into()
        });

        let param = PyParameter::new("my-parameter", class, &[]);
        let timestepper = default_timestepper();
        let timesteps = timestepper.timesteps();

        let scenario_indices = [
            ScenarioIndex {
                index: 0,
                indices: vec![0],
            },
            ScenarioIndex {
                index: 1,
                indices: vec![1],
            },
        ];

        let state = State::new(vec![], 0, vec![], 1, 0);

        let mut internal_p_states: Vec<_> = scenario_indices
            .iter()
            .map(|si| param.setup(&timesteps, &si).expect("Could not setup the PyParameter"))
            .collect();

        let model = Model::default();

        for ts in &timesteps {
            for (si, internal) in scenario_indices.iter().zip(internal_p_states.iter_mut()) {
                let value = param.compute(ts, si, &model, &state, internal).unwrap();

                assert_approx_eq!(f64, value, ((ts.index + 1) * si.index + ts.date.day() as usize) as f64);
            }
        }
    }
}
