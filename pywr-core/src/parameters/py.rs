use super::{GeneralParameter, Parameter, ParameterMeta, ParameterName, ParameterState, Timestep};
use crate::metric::{MetricF64, MetricU64};
use crate::network::Network;
use crate::parameters::downcast_internal_state_mut;
use crate::parameters::errors::{ParameterCalculationError, ParameterSetupError};
use crate::scenario::ScenarioIndex;
use crate::state::{MultiValue, State};
use ahash::RandomState;
use pyo3::exceptions::PyKeyError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use std::collections::HashMap;

/// Provides data for a custom Pywr parameter.
///
/// This is a read-only object that provides information that can be used for custom parameters in Pywr. It
/// is passed as the first argument to the `calc` and `after` methods of custom parameter objects.
#[pyclass]
pub struct ParameterInfo {
    /// The timestep for which the parameter is being calculated.
    #[pyo3(get)]
    timestep: Timestep,

    /// The scenario index for which the parameter is being calculated.
    #[pyo3(get)]
    scenario_index: ScenarioIndex,

    /// The metric values available for the parameter calculation.
    metric_values: HashMap<String, f64, RandomState>,

    /// The index values available for the parameter calculation.
    index_values: HashMap<String, u64, RandomState>,
}

#[pymethods]
impl ParameterInfo {
    pub fn get_metric(&self, key: &str) -> PyResult<f64> {
        self.metric_values
            .get(key)
            .ok_or_else(|| PyKeyError::new_err(format!("Metric `{key}` not found")))
            .cloned()
    }

    pub fn get_index(&self, key: &str) -> PyResult<u64> {
        self.index_values
            .get(key)
            .ok_or_else(|| PyKeyError::new_err(format!("Index `{key}` not found")))
            .cloned()
    }
}

struct PyCommon {
    meta: ParameterMeta,
    args: Py<PyTuple>,
    kwargs: Py<PyDict>,
    metrics: HashMap<String, MetricF64>,
    indices: HashMap<String, MetricU64>,
}

impl PyCommon {
    fn new(
        meta: ParameterMeta,
        args: Py<PyTuple>,
        kwargs: Py<PyDict>,
        metrics: &HashMap<String, MetricF64>,
        indices: &HashMap<String, MetricU64>,
    ) -> Self {
        Self {
            meta,
            args,
            kwargs,
            metrics: metrics.clone(),
            indices: indices.clone(),
        }
    }

    fn update_metrics(
        &self,
        network: &Network,
        state: &State,
        values: &mut HashMap<String, f64, RandomState>,
    ) -> Result<(), ParameterCalculationError> {
        for (k, m) in self.metrics.iter() {
            let value = m.get_value(network, state)?;
            values.insert(k.clone(), value);
        }

        Ok(())
    }

    fn update_indices(
        &self,
        network: &Network,
        state: &State,
        values: &mut HashMap<String, u64, RandomState>,
    ) -> Result<(), ParameterCalculationError> {
        for (k, m) in self.indices.iter() {
            let value = m.get_value(network, state)?;
            values.insert(k.clone(), value);
        }

        Ok(())
    }
}

/// A Python parameter that returns the value produced by a Python object.
///
/// This parameter allows you to define a Python class that implements a `calc` method,
/// which will be called to compute the parameter value. An optional `after` method can also be defined
/// to perform any additional actions during the "after" phase of a time-step.
pub struct PyClassParameter {
    /// This is the user's class that implements the parameter logic.
    class: Py<PyAny>,
    common: PyCommon,
}

struct InternalObj {
    /// The user-defined Python object that implements the parameter logic.
    user_obj: Py<PyAny>,
    info_obj: Option<Py<ParameterInfo>>,
}

impl InternalObj {
    fn into_boxed_any(self) -> Box<dyn ParameterState> {
        Box::new(self)
    }
}

/// Ensure that `info_obj` is populated with a `ParameterInfo`.
fn ensure_parameter_info(
    info_obj: &mut Option<Py<ParameterInfo>>,
    timestep: &Timestep,
    scenario_index: &ScenarioIndex,
) -> Result<(), PyErr> {
    if info_obj.is_none() {
        let obj = Python::attach(|py| {
            Py::new(
                py,
                ParameterInfo {
                    timestep: *timestep,
                    scenario_index: scenario_index.clone(),
                    metric_values: HashMap::default(),
                    index_values: HashMap::default(),
                },
            )
        })?;

        *info_obj = Some(obj);
    }

    Ok(())
}

impl PyClassParameter {
    pub fn new(
        name: ParameterName,
        object: Py<PyAny>,
        args: Py<PyTuple>,
        kwargs: Py<PyDict>,
        metrics: &HashMap<String, MetricF64>,
        indices: &HashMap<String, MetricU64>,
    ) -> Self {
        Self {
            class: object,
            common: PyCommon::new(ParameterMeta::new(name), args, kwargs, metrics, indices),
        }
    }

    fn setup(&self) -> Result<Option<Box<dyn ParameterState>>, ParameterSetupError> {
        Python::initialize();

        let user_obj: Py<PyAny> = Python::attach(|py| -> PyResult<_> {
            let args = self.common.args.bind(py);
            let kwargs = self.common.kwargs.bind(py);
            self.class.call(py, args, Some(kwargs))
        })
        .map_err(|py_error| ParameterSetupError::PythonError {
            name: self.common.meta.name.to_string(),
            object: self.class.to_string(),
            py_error: Box::new(py_error),
        })?;

        let internal = InternalObj {
            user_obj,
            info_obj: None,
        };

        Ok(Some(internal.into_boxed_any()))
    }

    fn compute<T>(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<T, ParameterCalculationError>
    where
        T: for<'a> FromPyObject<'a>,
    {
        let internal = downcast_internal_state_mut::<InternalObj>(internal_state);

        ensure_parameter_info(&mut internal.info_obj, timestep, scenario_index).map_err(|py_error| {
            ParameterCalculationError::PythonError {
                name: self.common.meta.name.to_string(),
                object: self.class.to_string(),
                py_error: Box::new(py_error),
            }
        })?;

        // Safe to unwrap as we just ensured it is Some.
        let info = internal.info_obj.as_ref().unwrap();

        let value: T = Python::attach(|py| {
            let info_bind = info.bind(py);
            {
                let mut info_mut = info_bind.borrow_mut();
                info_mut.timestep = *timestep;
                info_mut.scenario_index = scenario_index.clone();
                self.common
                    .update_metrics(network, state, &mut info_mut.metric_values)?;

                self.common.update_indices(network, state, &mut info_mut.index_values)?;
            }

            let args = PyTuple::new(py, [info_bind]).map_err(|py_error| ParameterCalculationError::PythonError {
                name: self.common.meta.name.to_string(),
                object: self.class.to_string(),
                py_error: Box::new(py_error),
            })?;

            internal
                .user_obj
                .call_method1(py, "calc", args)
                .map_err(|py_error| ParameterCalculationError::PythonError {
                    name: self.common.meta.name.to_string(),
                    object: self.class.to_string(),
                    py_error: Box::new(py_error),
                })?
                .extract(py)
                .map_err(|py_error| ParameterCalculationError::PythonError {
                    name: self.common.meta.name.to_string(),
                    object: self.class.to_string(),
                    py_error: Box::new(py_error),
                })
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
    ) -> Result<(), ParameterCalculationError> {
        let internal = downcast_internal_state_mut::<InternalObj>(internal_state);

        ensure_parameter_info(&mut internal.info_obj, timestep, scenario_index).map_err(|py_error| {
            ParameterCalculationError::PythonError {
                name: self.common.meta.name.to_string(),
                object: self.class.to_string(),
                py_error: Box::new(py_error),
            }
        })?;

        // Safe to unwrap as we just ensured it is Some.
        let info = internal.info_obj.as_ref().unwrap();

        Python::attach(|py| {
            // Only do this if the object has an "after" method defined.
            if internal.user_obj.getattr(py, "after").is_ok() {
                let info_bind = info.bind(py);
                {
                    let mut info_mut = info_bind.borrow_mut();
                    info_mut.timestep = *timestep;
                    info_mut.scenario_index = scenario_index.clone();
                    self.common
                        .update_metrics(network, state, &mut info_mut.metric_values)?;

                    self.common.update_indices(network, state, &mut info_mut.index_values)?;
                }

                let args =
                    PyTuple::new(py, [info_bind]).map_err(|py_error| ParameterCalculationError::PythonError {
                        name: self.common.meta.name.to_string(),
                        object: self.class.to_string(),
                        py_error: Box::new(py_error),
                    })?;

                internal.user_obj.call_method1(py, "after", args).map_err(|py_error| {
                    ParameterCalculationError::PythonError {
                        name: self.common.meta.name.to_string(),
                        object: self.class.to_string(),
                        py_error: Box::new(py_error),
                    }
                })?;
            }
            Ok::<(), ParameterCalculationError>(())
        })?;

        Ok(())
    }
}

impl Parameter for PyClassParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.common.meta
    }

    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, ParameterSetupError> {
        self.setup()
    }
}

impl GeneralParameter<f64> for PyClassParameter {
    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, ParameterCalculationError> {
        self.compute(timestep, scenario_index, network, state, internal_state)
    }

    fn after(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), ParameterCalculationError> {
        self.after(timestep, scenario_index, network, state, internal_state)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralParameter<u64> for PyClassParameter {
    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, ParameterCalculationError> {
        self.compute(timestep, scenario_index, network, state, internal_state)
    }

    fn after(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), ParameterCalculationError> {
        self.after(timestep, scenario_index, network, state, internal_state)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralParameter<MultiValue> for PyClassParameter {
    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<MultiValue, ParameterCalculationError> {
        self.compute(timestep, scenario_index, network, state, internal_state)
    }

    fn after(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), ParameterCalculationError> {
        self.after(timestep, scenario_index, model, state, internal_state)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

/// A Python parameter that returns the value produced by a Python function.
///
/// This parameter allows you to define a Python function which takes a `ParameterInfo` object as its first argument,
/// and then the user defined `args` and `kwargs` as additional arguments.
pub struct PyFuncParameter {
    /// This is the user's class that implements the parameter logic.
    function: Py<PyAny>,
    common: PyCommon,
}

struct InternalInfo {
    info_obj: Option<Py<ParameterInfo>>,
}

impl InternalInfo {
    fn into_boxed_any(self) -> Box<dyn ParameterState> {
        Box::new(self)
    }
}

impl PyFuncParameter {
    pub fn new(
        name: ParameterName,
        function: Py<PyAny>,
        args: Py<PyTuple>,
        kwargs: Py<PyDict>,
        metrics: &HashMap<String, MetricF64>,
        indices: &HashMap<String, MetricU64>,
    ) -> Self {
        Self {
            function,
            common: PyCommon::new(ParameterMeta::new(name), args, kwargs, metrics, indices),
        }
    }

    fn setup(&self) -> Result<Option<Box<dyn ParameterState>>, ParameterSetupError> {
        Python::initialize();

        let internal = InternalInfo { info_obj: None };

        Ok(Some(internal.into_boxed_any()))
    }

    fn compute<T>(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<T, ParameterCalculationError>
    where
        T: for<'a> FromPyObject<'a>,
    {
        let internal = downcast_internal_state_mut::<InternalInfo>(internal_state);

        ensure_parameter_info(&mut internal.info_obj, timestep, scenario_index).map_err(|py_error| {
            ParameterCalculationError::PythonError {
                name: self.common.meta.name.to_string(),
                object: self.function.to_string(),
                py_error: Box::new(py_error),
            }
        })?;

        // Safe to unwrap as we just ensured it is Some.
        let info = internal.info_obj.as_ref().unwrap();

        let value: T = Python::attach(|py| {
            let info_bind = info.bind(py);
            {
                let mut info_mut = info_bind.borrow_mut();
                info_mut.timestep = *timestep;
                info_mut.scenario_index = scenario_index.clone();
                self.common
                    .update_metrics(network, state, &mut info_mut.metric_values)?;

                self.common.update_indices(network, state, &mut info_mut.index_values)?;
            }

            let args = PyTuple::new(py, [info_bind]).map_err(|py_error| ParameterCalculationError::PythonError {
                name: self.common.meta.name.to_string(),
                object: self.function.to_string(),
                py_error: Box::new(py_error),
            })?;

            // Concatenate the user defined args with the info arg.
            let args = args
                .into_sequence()
                .concat(self.common.args.bind(py).as_sequence())
                .map_err(|py_error| ParameterCalculationError::PythonError {
                    name: self.common.meta.name.to_string(),
                    object: self.function.to_string(),
                    py_error: Box::new(py_error),
                })?;
            let args = args
                .to_tuple()
                .map_err(|py_error| ParameterCalculationError::PythonError {
                    name: self.common.meta.name.to_string(),
                    object: self.function.to_string(),
                    py_error: Box::new(py_error),
                })?;

            let kwargs = self.common.kwargs.bind(py);

            self.function
                .call(py, args, Some(kwargs))
                .map_err(|py_error| ParameterCalculationError::PythonError {
                    name: self.common.meta.name.to_string(),
                    object: self.function.to_string(),
                    py_error: Box::new(py_error),
                })?
                .extract(py)
                .map_err(|py_error| ParameterCalculationError::PythonError {
                    name: self.common.meta.name.to_string(),
                    object: self.function.to_string(),
                    py_error: Box::new(py_error),
                })
        })?;

        Ok(value)
    }
}

impl Parameter for PyFuncParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.common.meta
    }

    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, ParameterSetupError> {
        self.setup()
    }
}
impl GeneralParameter<f64> for PyFuncParameter {
    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, ParameterCalculationError> {
        self.compute(timestep, scenario_index, network, state, internal_state)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralParameter<u64> for PyFuncParameter {
    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, ParameterCalculationError> {
        self.compute(timestep, scenario_index, network, state, internal_state)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralParameter<MultiValue> for PyFuncParameter {
    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<MultiValue, ParameterCalculationError> {
        self.compute(timestep, scenario_index, network, state, internal_state)
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
    /// Test `PyClassParameter` returns the correct value.
    fn test_counter_parameter() {
        // Init Python
        Python::initialize();

        let class = Python::attach(|py| {
            let test_module = PyModule::from_code(
                py,
                c_str!(
                    r#"
class MyParameter:
    def __init__(self, count, **kwargs):
        self.count = count

    def calc(self, info):
        self.count += info.scenario_index.simulation_id
        return float(self.count + info.timestep.day)
"#
                ),
                c_str!(""),
                c_str!(""),
            )
            .unwrap();

            test_module.getattr("MyParameter").unwrap().into()
        });

        let args = Python::attach(|py| PyTuple::new(py, [0]).unwrap().unbind());
        let kwargs = Python::attach(|py| PyDict::new(py).unbind());

        let param = PyClassParameter::new(
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
    /// Test `PyClassParameter` returns the correct value.
    fn test_multi_valued_parameter() {
        // Init Python
        Python::initialize();

        let class = Python::attach(|py| {
            let test_module = PyModule::from_code(
                py,
                c_str!(
                    r#"
import math


class MyParameter:
    def __init__(self, count, **kwargs):
        self.count = count

    def calc(self, info):
        self.count += info.scenario_index.simulation_id
        return {
            'a-float': math.pi,  # This is a float
            'count': self.count + info.timestep.day  # This is an integer
        }
"#
                ),
                c_str!(""),
                c_str!(""),
            )
            .unwrap();

            test_module.getattr("MyParameter").unwrap().into()
        });

        let args = Python::attach(|py| PyTuple::new(py, [0]).unwrap().unbind());
        let kwargs = Python::attach(|py| PyDict::new(py).unbind());

        let param = PyClassParameter::new(
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

    #[test]
    /// Test `PythonParameter` returns the correct value.
    fn test_function_parameter() {
        // Init Python
        Python::initialize();

        let function = Python::attach(|py| {
            let test_module = PyModule::from_code(
                py,
                c_str!(
                    r#"
def my_function(info, count, **kwargs):
    return float(count + info.timestep.day + info.scenario_index.simulation_id)
"#
                ),
                c_str!(""),
                c_str!(""),
            )
            .unwrap();

            test_module.getattr("my_function").unwrap().into()
        });

        let args = Python::attach(|py| PyTuple::new(py, [2]).unwrap().unbind());
        let kwargs = Python::attach(|py| PyDict::new(py).unbind());

        let param = PyFuncParameter::new(
            "MyParameter".into(),
            function,
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
            .map(|si| Parameter::setup(&param, timesteps, si).expect("Could not setup the PyFuncParameter"))
            .collect();

        let model = Network::default();

        for ts in timesteps {
            for (si, internal) in scenario_indices.iter().zip(internal_p_states.iter_mut()) {
                let value = GeneralParameter::compute(&param, ts, si, &model, &state, internal).unwrap();

                assert_approx_eq!(f64, value, (2 + si.simulation_id() + ts.date.day() as usize) as f64);
            }
        }
    }
}
