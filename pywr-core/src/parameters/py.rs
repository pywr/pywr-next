use super::{GeneralParameter, Parameter, ParameterMeta, ParameterName, ParameterState, PywrError, Timestep};
use crate::metric::{MetricF64, MetricU64};
use crate::network::Network;
use crate::parameters::downcast_internal_state_mut;
use crate::scenario::ScenarioIndex;
use crate::state::{MultiValue, State};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyDict, PyFloat, PyInt, PyTuple};
use std::collections::HashMap;

pub struct PyParameter {
    meta: ParameterMeta,
    object: Py<PyAny>,
    args: Py<PyTuple>,
    kwargs: Py<PyDict>,
    metrics: HashMap<String, MetricF64>,
    indices: HashMap<String, MetricU64>,
}

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
        name: ParameterName,
        object: Py<PyAny>,
        args: Py<PyTuple>,
        kwargs: Py<PyDict>,
        metrics: &HashMap<String, MetricF64>,
        indices: &HashMap<String, MetricU64>,
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

    fn get_metrics_dict<'py>(
        &self,
        network: &Network,
        state: &State,
        py: Python<'py>,
    ) -> Result<Bound<'py, PyDict>, PywrError> {
        let metric_values: Vec<(&str, f64)> = self
            .metrics
            .iter()
            .map(|(k, value)| Ok((k.as_str(), value.get_value(network, state)?)))
            .collect::<Result<Vec<_>, PywrError>>()?;

        Ok(metric_values.into_py_dict(py)?)
    }

    fn get_indices_dict<'py>(
        &self,
        network: &Network,
        state: &State,
        py: Python<'py>,
    ) -> Result<Bound<'py, PyDict>, PywrError> {
        let index_values: Vec<(&str, u64)> = self
            .indices
            .iter()
            .map(|(k, value)| Ok((k.as_str(), value.get_value(network, state)?)))
            .collect::<Result<Vec<_>, PywrError>>()?;

        Ok(index_values.into_py_dict(py)?)
    }

    fn setup(&self) -> Result<Option<Box<dyn ParameterState>>, PywrError> {
        pyo3::prepare_freethreaded_python();

        let user_obj: PyObject = Python::with_gil(|py| -> PyResult<PyObject> {
            let args = self.args.bind(py);
            let kwargs = self.kwargs.bind(py);
            self.object.call(py, args, Some(kwargs))
        })?;

        let internal = Internal { user_obj };

        Ok(Some(internal.into_boxed_any()))
    }

    fn compute<T>(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<T, PywrError>
    where
        T: for<'a> FromPyObject<'a>,
    {
        let internal = downcast_internal_state_mut::<Internal>(internal_state);

        let value: T = Python::with_gil(|py| {
            let date = timestep.date.into_pyobject(py)?;

            let si = scenario_index.simulation_id().into_pyobject(py)?;

            let metric_dict = self.get_metrics_dict(network, state, py)?;
            let index_dict = self.get_indices_dict(network, state, py)?;

            let args = PyTuple::new(
                py,
                [date.as_any(), si.as_any(), metric_dict.as_any(), index_dict.as_any()],
            )?;

            internal.user_obj.call_method1(py, "calc", args)?.extract(py)
        })?;

        Ok(value)
    }

    fn after(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), PywrError> {
        let internal = downcast_internal_state_mut::<Internal>(internal_state);

        Python::with_gil(|py| {
            // Only do this if the object has an "after" method defined.
            if internal.user_obj.getattr(py, "after").is_ok() {
                let date = timestep.date.into_pyobject(py)?;
                let si = scenario_index.simulation_id().into_pyobject(py)?;

                let metric_dict = self.get_metrics_dict(network, state, py)?;
                let index_dict = self.get_indices_dict(network, state, py)?;

                let args = PyTuple::new(
                    py,
                    [date.as_any(), si.as_any(), metric_dict.as_any(), index_dict.as_any()],
                )?;

                internal.user_obj.call_method1(py, "after", args)?;
            }
            Ok::<(), PywrError>(())
        })?;

        Ok(())
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
    ) -> Result<Option<Box<dyn ParameterState>>, PywrError> {
        self.setup()
    }
}

impl GeneralParameter<f64> for PyParameter {
    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, PywrError> {
        self.compute(timestep, scenario_index, model, state, internal_state)
    }

    fn after(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), PywrError> {
        self.after(timestep, scenario_index, model, state, internal_state)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralParameter<u64> for PyParameter {
    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, PywrError> {
        self.compute(timestep, scenario_index, model, state, internal_state)
    }

    fn after(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), PywrError> {
        self.after(timestep, scenario_index, model, state, internal_state)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralParameter<MultiValue> for PyParameter {
    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<MultiValue, PywrError> {
        let internal = downcast_internal_state_mut::<Internal>(internal_state);

        let value: MultiValue = Python::with_gil(|py| {
            let date = timestep.date.into_pyobject(py)?;

            let si = scenario_index.simulation_id().into_pyobject(py)?;

            let metric_dict = self.get_metrics_dict(network, state, py)?;
            let index_dict = self.get_indices_dict(network, state, py)?;

            let args = PyTuple::new(
                py,
                [date.as_any(), si.as_any(), metric_dict.as_any(), index_dict.as_any()],
            )?;

            let py_values: HashMap<String, PyObject> = internal.user_obj.call_method1(py, "calc", args)?.extract(py)?;

            // Try to convert the floats
            let values: HashMap<String, f64> = py_values
                .iter()
                .filter_map(|(k, v)| match v.downcast_bound::<PyFloat>(py) {
                    Ok(v) => Some((k.clone(), v.extract().unwrap())),
                    Err(_) => None,
                })
                .collect();

            let indices: HashMap<String, u64> = py_values
                .iter()
                .filter_map(|(k, v)| match v.downcast_bound::<PyInt>(py) {
                    Ok(v) => Some((k.clone(), v.extract().unwrap())),
                    Err(_) => None,
                })
                .collect();

            if py_values.len() != values.len() + indices.len() {
                Err(PyValueError::new_err(
                    "Some returned values were not interpreted as floats or integers.",
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
        self.after(timestep, scenario_index, model, state, internal_state)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::ScenarioIndexBuilder;
    use crate::state::StateBuilder;
    use crate::test_utils::default_timestepper;
    use crate::timestep::TimeDomain;
    use chrono::Datelike;
    use float_cmp::assert_approx_eq;
    use pyo3::ffi::c_str;

    #[test]
    /// Test `PythonParameter` returns the correct value.
    fn test_counter_parameter() {
        // Init Python
        pyo3::prepare_freethreaded_python();

        let class = Python::with_gil(|py| {
            let test_module = PyModule::from_code(
                py,
                c_str!(
                    r#"
class MyParameter:
    def __init__(self, count, **kwargs):
        self.count = count

    def calc(self, ts, si, metrics, indices):
        self.count += si
        return float(self.count + ts.day)
"#
                ),
                c_str!(""),
                c_str!(""),
            )
            .unwrap();

            test_module.getattr("MyParameter").unwrap().into()
        });

        let args = Python::with_gil(|py| PyTuple::new(py, [0]).unwrap().unbind());
        let kwargs = Python::with_gil(|py| PyDict::new(py).unbind());

        let param = PyParameter::new(
            "my-parameter".into(),
            class,
            args,
            kwargs,
            &HashMap::new(),
            &HashMap::new(),
        );
        let timestepper = default_timestepper();
        let time: TimeDomain = TimeDomain::try_from(timestepper).unwrap();
        let timesteps = time.timesteps();

        let scenario_indices = [
            ScenarioIndexBuilder::new(0, vec![0], vec!["0"]).build(),
            ScenarioIndexBuilder::new(1, vec![1], vec!["1"]).build(),
        ];

        let state = StateBuilder::new(vec![], 0).build();

        let mut internal_p_states: Vec<_> = scenario_indices
            .iter()
            .map(|si| Parameter::setup(&param, timesteps, si).expect("Could not setup the PyParameter"))
            .collect();

        let model = Network::default();

        for ts in timesteps {
            for (si, internal) in scenario_indices.iter().zip(internal_p_states.iter_mut()) {
                let value = GeneralParameter::compute(&param, ts, si, &model, &state, internal).unwrap();

                assert_approx_eq!(
                    f64,
                    value,
                    ((ts.index + 1) * si.simulation_id() + ts.date.day() as usize) as f64
                );
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
                c_str!(
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
"#
                ),
                c_str!(""),
                c_str!(""),
            )
            .unwrap();

            test_module.getattr("MyParameter").unwrap().into()
        });

        let args = Python::with_gil(|py| PyTuple::new(py, [0]).unwrap().unbind());
        let kwargs = Python::with_gil(|py| PyDict::new(py).unbind());

        let param = PyParameter::new(
            "my-parameter".into(),
            class,
            args,
            kwargs,
            &HashMap::new(),
            &HashMap::new(),
        );
        let timestepper = default_timestepper();
        let time: TimeDomain = TimeDomain::try_from(timestepper).unwrap();
        let timesteps = time.timesteps();

        let scenario_indices = [
            ScenarioIndexBuilder::new(0, vec![0], vec!["0"]).build(),
            ScenarioIndexBuilder::new(1, vec![1], vec!["1"]).build(),
        ];

        let state = StateBuilder::new(vec![], 0).build();

        let mut internal_p_states: Vec<_> = scenario_indices
            .iter()
            .map(|si| Parameter::setup(&param, timesteps, si).expect("Could not setup the PyParameter"))
            .collect();

        let model = Network::default();

        for ts in timesteps {
            for (si, internal) in scenario_indices.iter().zip(internal_p_states.iter_mut()) {
                let value = GeneralParameter::<MultiValue>::compute(&param, ts, si, &model, &state, internal).unwrap();

                assert_approx_eq!(f64, *value.get_value("a-float").unwrap(), std::f64::consts::PI);

                assert_eq!(
                    *value.get_index("count").unwrap() as usize,
                    ((ts.index + 1) * si.simulation_id() + ts.date.day() as usize)
                );
            }
        }
    }
}
