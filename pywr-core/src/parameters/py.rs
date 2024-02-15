use super::{IndexValue, Parameter, ParameterMeta, PywrError, Timestep};
use crate::metric::Metric;
use crate::network::Network;
use crate::parameters::{downcast_internal_state, MultiValueParameter};
use crate::scenario::ScenarioIndex;
use crate::state::{MultiValue, ParameterState, State};
use chrono::Datelike;
use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyDate, PyDict, PyFloat, PyLong, PyTuple};
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

#[derive(Clone)]
struct Internal {
    user_obj: PyObject,
}

impl Internal {
    fn into_boxed_any(self) -> Box<dyn ParameterState> {
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

    fn get_metrics_dict<'py>(&self, model: &Network, state: &State, py: Python<'py>) -> Result<&'py PyDict, PywrError> {
        let metric_values: Vec<(&str, f64)> = self
            .metrics
            .iter()
            .map(|(k, value)| Ok((k.as_str(), value.get_value(model, state)?)))
            .collect::<Result<Vec<_>, PywrError>>()?;

        Ok(metric_values.into_py_dict(py))
    }

    fn get_indices_dict<'py>(&self, state: &State, py: Python<'py>) -> Result<&'py PyDict, PywrError> {
        let index_values: Vec<(&str, usize)> = self
            .indices
            .iter()
            .map(|(k, value)| Ok((k.as_str(), value.get_index(state)?)))
            .collect::<Result<Vec<_>, PywrError>>()?;

        Ok(index_values.into_py_dict(py))
    }
}

impl Parameter for PyParameter {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, PywrError> {
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

    // fn before(&self, internal_state: &mut Option<Box<dyn ParameterState>>) -> Result<(), PywrError> {
    //     let internal = downcast_internal_state::<Internal>(internal_state);
    //
    //     Python::with_gil(|py| internal.user_obj.call_method0(py, "before"))
    //         .map_err(|e| PywrError::PythonError(e.to_string()))?;
    //
    //     Ok(())
    // }

    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, PywrError> {
        let internal = downcast_internal_state::<Internal>(internal_state);

        let value: f64 = Python::with_gil(|py| {
            let date = PyDate::new(
                py,
                timestep.date.year(),
                timestep.date.month() as u8,
                timestep.date.day() as u8,
            )?;

            let si = scenario_index.index.into_py(py);

            let metric_dict = self.get_metrics_dict(model, state, py)?;
            let index_dict = self.get_indices_dict(state, py)?;

            let args = PyTuple::new(py, [date, si.as_ref(py), metric_dict, index_dict]);

            internal.user_obj.call_method1(py, "calc", args)?.extract(py)
        })
        .map_err(|e| PywrError::PythonError(e.to_string()))?;

        Ok(value)
    }

    fn after(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), PywrError> {
        let internal = downcast_internal_state::<Internal>(internal_state);

        Python::with_gil(|py| {
            // Only do this if the object has an "after" method defined.
            if internal.user_obj.getattr(py, "after").is_ok() {
                let date = PyDate::new(
                    py,
                    timestep.date.year(),
                    timestep.date.month() as u8,
                    timestep.date.day() as u8,
                )?;

                let si = scenario_index.index.into_py(py);

                let metric_dict = self.get_metrics_dict(model, state, py)?;
                let index_dict = self.get_indices_dict(state, py)?;

                let args = PyTuple::new(py, [date, si.as_ref(py), metric_dict, index_dict]);

                internal.user_obj.call_method1(py, "after", args)?;
            }
            Ok(())
        })
        .map_err(|e: PyErr| PywrError::PythonError(e.to_string()))?;

        Ok(())
    }
}

impl MultiValueParameter for PyParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, PywrError> {
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

    // fn before(&self, internal_state: &mut Option<Box<dyn ParameterState>>) -> Result<(), PywrError> {
    //     let internal = downcast_internal_state::<Internal>(internal_state);
    //
    //     Python::with_gil(|py| internal.user_obj.call_method0(py, "before"))
    //         .map_err(|e| PywrError::PythonError(e.to_string()))?;
    //
    //     Ok(())
    // }

    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<MultiValue, PywrError> {
        let internal = downcast_internal_state::<Internal>(internal_state);

        let value: MultiValue = Python::with_gil(|py| {
            let date = PyDate::new(
                py,
                timestep.date.year(),
                timestep.date.month() as u8,
                timestep.date.day() as u8,
            )
            .map_err(|e: PyErr| PywrError::PythonError(e.to_string()))?;

            let si = scenario_index.index.into_py(py);

            let metric_dict = self.get_metrics_dict(model, state, py)?;
            let index_dict = self.get_indices_dict(state, py)?;

            let args = PyTuple::new(py, [date, si.as_ref(py), metric_dict, index_dict]);

            let py_values: HashMap<String, PyObject> = internal
                .user_obj
                .call_method1(py, "calc", args)
                .map_err(|e: PyErr| PywrError::PythonError(e.to_string()))?
                .extract(py)
                .map_err(|e: PyErr| PywrError::PythonError(e.to_string()))?;

            // Try to convert the floats
            let values: HashMap<String, f64> = py_values
                .iter()
                .filter_map(|(k, v)| match v.downcast::<PyFloat>(py) {
                    Ok(v) => Some((k.clone(), v.extract().unwrap())),
                    Err(_) => None,
                })
                .collect();

            let indices: HashMap<String, usize> = py_values
                .iter()
                .filter_map(|(k, v)| match v.downcast::<PyLong>(py) {
                    Ok(v) => Some((k.clone(), v.extract().unwrap())),
                    Err(_) => None,
                })
                .collect();

            if py_values.len() != values.len() + indices.len() {
                Err(PywrError::PythonError(
                    "Some returned values were not interpreted as floats or integers.".to_string(),
                ))
            } else {
                Ok(MultiValue::new(values, indices))
            }
        })?;

        Ok(value)
    }

    fn after(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), PywrError> {
        let internal = downcast_internal_state::<Internal>(internal_state);

        Python::with_gil(|py| {
            // Only do this if the object has an "after" method defined.
            if internal.user_obj.getattr(py, "after").is_ok() {
                let date = PyDate::new(
                    py,
                    timestep.date.year(),
                    timestep.date.month() as u8,
                    timestep.date.day() as u8,
                )?;

                let si = scenario_index.index.into_py(py);

                let metric_dict = self.get_metrics_dict(model, state, py)?;
                let index_dict = self.get_indices_dict(state, py)?;

                let args = PyTuple::new(py, [date, si.as_ref(py), metric_dict, index_dict]);

                internal.user_obj.call_method1(py, "after", args)?;
            }
            Ok(())
        })
        .map_err(|e: PyErr| PywrError::PythonError(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::default_timestepper;
    use crate::timestep::TimeDomain;
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
        let time: TimeDomain = timestepper.into();
        let timesteps = time.timesteps();

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

        let state = State::new(vec![], 0, vec![], 1, 0, 0, 0, 0);

        let mut internal_p_states: Vec<_> = scenario_indices
            .iter()
            .map(|si| Parameter::setup(&param, &timesteps, si).expect("Could not setup the PyParameter"))
            .collect();

        let model = Network::default();

        for ts in timesteps {
            for (si, internal) in scenario_indices.iter().zip(internal_p_states.iter_mut()) {
                let value = Parameter::compute(&param, ts, si, &model, &state, internal).unwrap();

                assert_approx_eq!(f64, value, ((ts.index + 1) * si.index + ts.date.day() as usize) as f64);
            }
        }
    }

    #[test]
    /// Test `PythonParameter` returns the correct value.
    fn test_multi_valued_parameter() {
        // Init Python
        pyo3::prepare_freethreaded_python();

        let class = Python::with_gil(|py| {
            let test_module = PyModule::from_code(
                py,
                r#"
import math


class MyParameter:
    def __init__(self, count, **kwargs):
        self.count = count

    def calc(self, ts, si, metrics, indices):
        self.count += si
        return {
            'a-float': math.pi,  # This is a float
            'count': self.count + ts.day  # This is an integer
        }
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
        let time: TimeDomain = timestepper.into();
        let timesteps = time.timesteps();

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

        let state = State::new(vec![], 0, vec![], 1, 0, 0, 0, 0);

        let mut internal_p_states: Vec<_> = scenario_indices
            .iter()
            .map(|si| MultiValueParameter::setup(&param, &timesteps, si).expect("Could not setup the PyParameter"))
            .collect();

        let model = Network::default();

        for ts in timesteps {
            for (si, internal) in scenario_indices.iter().zip(internal_p_states.iter_mut()) {
                let value = MultiValueParameter::compute(&param, ts, si, &model, &state, internal).unwrap();

                assert_approx_eq!(f64, *value.get_value("a-float").unwrap(), std::f64::consts::PI);

                assert_eq!(
                    *value.get_index("count").unwrap(),
                    ((ts.index + 1) * si.index + ts.date.day() as usize)
                );
            }
        }
    }
}
