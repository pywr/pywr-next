use super::{IndexValue, Parameter, ParameterMeta, PywrError, Timestep};
use crate::metric::Metric;
use crate::model::Model;
use crate::scenario::ScenarioIndex;
use crate::state::State;
use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyDate, PyDict, PyTuple};
use std::any::Any;
use std::collections::HashMap;

pub struct PyParameter {
    meta: ParameterMeta,
    object: Py<PyAny>,
    args: Py<PyTuple>,
    kwargs: Py<PyDict>,
    metrics: HashMap<String, Metric>,
    indices: HashMap<String, IndexValue>,
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
    pub fn new(
        name: &str,
        object: Py<PyAny>,
        args: Py<PyTuple>,
        kwargs: Py<PyDict>,
        metrics: &HashMap<String, Metric>,
        indices: &HashMap<String, IndexValue>,
    ) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            object,
            args,
            kwargs,
            metrics: metrics.clone(),
            indices: indices.clone(),
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
        pyo3::prepare_freethreaded_python();

        let user_obj: PyObject = Python::with_gil(|py| -> PyResult<PyObject> {
            let args = self.args.as_ref(py);
            let kwargs = self.kwargs.as_ref(py);
            self.object.call(py, args, Some(kwargs))
        })
        .unwrap();

        let internal = Internal { user_obj };

        Ok(Some(internal.into_boxed_any()))
    }

    fn before(&self, internal_state: &mut Option<Box<dyn Any + Send>>) -> Result<(), PywrError> {
        let internal = match internal_state {
            Some(internal) => match internal.downcast_mut::<Internal>() {
                Some(pa) => pa,
                None => panic!("Internal state did not downcast to the correct type! :("),
            },
            None => panic!("No internal state defined when one was expected! :("),
        };

        Python::with_gil(|py| internal.user_obj.call_method0(py, "before"))
            .map_err(|e| PywrError::PythonError(e.to_string()))?;

        Ok(())
    }

    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Model,
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

            let metric_values: Vec<(&str, f64)> = self
                .metrics
                .iter()
                .map(|(k, value)| Ok((k.as_str(), value.get_value(model, state)?)))
                .collect::<Result<Vec<_>, PywrError>>()?;

            let metric_dict = metric_values.into_py_dict(py);

            let index_values: Vec<(&str, usize)> = self
                .indices
                .iter()
                .map(|(k, value)| Ok((k.as_str(), value.get_index(state)?)))
                .collect::<Result<Vec<_>, PywrError>>()?;

            let index_dict = index_values.into_py_dict(py);

            let args = PyTuple::new(py, [date, si.as_ref(py), metric_dict, index_dict]);

            internal.user_obj.call_method1(py, "calc", args)?.extract(py)
        })
        .map_err(|e| PywrError::PythonError(e.to_string()))?;

        Ok(value)
    }

    fn after(&self, internal_state: &mut Option<Box<dyn Any + Send>>) -> Result<(), PywrError> {
        let internal = match internal_state {
            Some(internal) => match internal.downcast_mut::<Internal>() {
                Some(pa) => pa,
                None => panic!("Internal state did not downcast to the correct type! :("),
            },
            None => panic!("No internal state defined when one was expected! :("),
        };

        Python::with_gil(|py| internal.user_obj.call_method0(py, "after"))
            .map_err(|e| PywrError::PythonError(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::default_timestepper;
    use float_cmp::assert_approx_eq;

    #[test]
    /// Test `PythonParameter` returns the correct value.
    fn test_counter_parameter() {
        // Init Python
        pyo3::prepare_freethreaded_python();

        let class = Python::with_gil(|py| {
            let test_module = PyModule::from_code(
                py,
                r#"
class MyParameter:
    def __init__(self, count, **kwargs):
        self.count = count

    def calc(self, ts, si, metrics, indices):
        self.count += si
        return float(self.count + ts.day)
"#,
                "",
                "",
            )
            .unwrap();

            test_module.getattr("MyParameter").unwrap().into()
        });

        let args = Python::with_gil(|py| PyTuple::new(py, [0]).into());
        let kwargs = Python::with_gil(|py| PyDict::new(py).into());

        let param = PyParameter::new("my-parameter", class, args, kwargs, &HashMap::new(), &HashMap::new());
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
            .map(|si| param.setup(&timesteps, si).expect("Could not setup the PyParameter"))
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
