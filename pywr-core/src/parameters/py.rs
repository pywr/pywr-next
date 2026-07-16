use super::{
    BuiltParameter, GeneralParameter, GeneralParameterContext, MaybeBuiltParameter, Parameter, ParameterBuildError,
    ParameterBuilder, ParameterMeta, ParameterName, ParameterState, Timestep,
};
use crate::metric::{MetricF64, MetricU64, UnresolvedMetricF64, UnresolvedMetricU64};
use crate::network::{Network, ResolutionMaps};
use crate::parameters::downcast_internal_state_mut;
use crate::parameters::errors::{GeneralCalculationError, ParameterSetupError};
use crate::scenario::ScenarioIndex;
use crate::state::{MultiValue, State};
use crate::{resolve_metric_f64_hashmap, resolve_metric_u64_hashmap};
use ahash::RandomState;
use pyo3::exceptions::PyKeyError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use std::collections::HashMap;

/// Provides data for a custom Pywr parameter.
///
/// This is a read-only object that provides information that can be used for custom parameters in Pywr. It
/// is passed as the first argument to the `before` and `after` methods of custom parameter objects.
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

#[derive(Debug)]
struct PyCommon {
    meta: ParameterMeta,
    args: Py<PyTuple>,
    kwargs: Py<PyDict>,
    metrics: HashMap<String, MetricF64>,
    indices: HashMap<String, MetricU64>,
}

/// Compare PyCommon instances using Python pointer equality for args and kwargs. This should
/// be roughly equivalent to use `id()` in Python.
impl PartialEq for PyCommon {
    fn eq(&self, other: &Self) -> bool {
        self.meta == other.meta
            && self.args.as_ptr().addr() == other.args.as_ptr().addr()
            && self.kwargs.as_ptr().addr() == other.kwargs.as_ptr().addr()
            && self.metrics == other.metrics
            && self.indices == other.indices
    }
}

impl PyCommon {
    fn update_metrics(
        &self,
        network: &Network,
        state: &State,
        values: &mut HashMap<String, f64, RandomState>,
    ) -> Result<(), GeneralCalculationError> {
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
    ) -> Result<(), GeneralCalculationError> {
        for (k, m) in self.indices.iter() {
            let value = m.get_value(network, state)?;
            values.insert(k.clone(), value);
        }

        Ok(())
    }
}

#[derive(Debug)]
struct PyCommonBuilder {
    meta: ParameterMeta,
    args: Py<PyTuple>,
    kwargs: Py<PyDict>,
    metrics: HashMap<String, UnresolvedMetricF64>,
    indices: HashMap<String, UnresolvedMetricU64>,
}

impl PyCommonBuilder {
    fn new(meta: ParameterMeta, args: Py<PyTuple>, kwargs: Py<PyDict>) -> Self {
        Self {
            meta,
            args,
            kwargs,
            metrics: HashMap::new(),
            indices: HashMap::new(),
        }
    }

    fn metric(&mut self, key: &str, value: UnresolvedMetricF64) -> &mut Self {
        self.metrics.insert(key.to_string(), value);
        self
    }

    fn index(&mut self, key: &str, value: UnresolvedMetricU64) -> &mut Self {
        self.indices.insert(key.to_string(), value);
        self
    }
}

/// A Python parameter that returns the value produced by a Python object.
///
/// This parameter allows you to define a Python class that implements a `before` and/or `after` methods,
/// which will be called to compute the parameter values.
#[derive(Debug)]
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

    fn call_method<T>(
        &self,
        method: &str,
        ctx: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<T>, GeneralCalculationError>
    where
        T: for<'a, 'py> FromPyObject<'a, 'py>,
    {
        let internal = downcast_internal_state_mut::<InternalObj>(internal_state);

        ensure_parameter_info(&mut internal.info_obj, ctx.timestep, ctx.scenario_index).map_err(|py_error| {
            GeneralCalculationError::PythonError {
                name: self.common.meta.name.to_string(),
                object: self.class.to_string(),
                py_error: Box::new(py_error),
            }
        })?;

        // Safe to unwrap as we just ensured it is Some.
        let info = internal.info_obj.as_ref().unwrap();

        let value: Option<T> = Python::attach(|py| {
            if internal.user_obj.getattr(py, method).is_ok() {
                let info_bind = info.bind(py);
                {
                    let mut info_mut = info_bind.borrow_mut();
                    info_mut.timestep = *ctx.timestep;
                    info_mut.scenario_index = ctx.scenario_index.clone();
                    self.common
                        .update_metrics(ctx.network, ctx.state, &mut info_mut.metric_values)?;

                    self.common
                        .update_indices(ctx.network, ctx.state, &mut info_mut.index_values)?;
                }

                let args = PyTuple::new(py, [info_bind]).map_err(|py_error| GeneralCalculationError::PythonError {
                    name: self.common.meta.name.to_string(),
                    object: self.class.to_string(),
                    py_error: Box::new(py_error),
                })?;

                internal
                    .user_obj
                    .call_method1(py, method, args)
                    .map_err(|py_error| GeneralCalculationError::PythonError {
                        name: self.common.meta.name.to_string(),
                        object: self.class.to_string(),
                        py_error: Box::new(py_error),
                    })?
                    .extract(py)
                    .map_err(Into::into)
                    .map_err(|py_error| GeneralCalculationError::PythonError {
                        name: self.common.meta.name.to_string(),
                        object: self.class.to_string(),
                        py_error: Box::new(py_error),
                    })
            } else {
                Ok(None)
            }
        })?;

        Ok(value)
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
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<f64>, GeneralCalculationError> {
        self.call_method("before", ctx, internal_state)
    }

    fn after(
        &self,
        ctx: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<f64>, GeneralCalculationError> {
        self.call_method("after", ctx, internal_state)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralParameter<u64> for PyClassParameter {
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<u64>, GeneralCalculationError> {
        self.call_method("before", ctx, internal_state)
    }

    fn after(
        &self,
        ctx: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<u64>, GeneralCalculationError> {
        self.call_method("after", ctx, internal_state)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralParameter<MultiValue> for PyClassParameter {
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<MultiValue>, GeneralCalculationError> {
        self.call_method("before", ctx, internal_state)
    }

    fn after(
        &self,
        ctx: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<MultiValue>, GeneralCalculationError> {
        self.call_method("after", ctx, internal_state)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

#[derive(Debug)]
pub struct PyClassParameterBuilder {
    class: Py<PyAny>,
    common: PyCommonBuilder,
}

impl PyClassParameterBuilder {
    pub fn new(name: ParameterName, object: Py<PyAny>, args: Py<PyTuple>, kwargs: Py<PyDict>) -> Self {
        Self {
            class: object,
            common: PyCommonBuilder::new(ParameterMeta::new(name), args, kwargs),
        }
    }

    pub fn metric(&mut self, key: &str, value: UnresolvedMetricF64) -> &mut Self {
        self.common.metric(key, value);
        self
    }

    pub fn index(&mut self, key: &str, value: UnresolvedMetricU64) -> &mut Self {
        self.common.index(key, value);
        self
    }
}

impl ParameterBuilder<f64> for PyClassParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.common.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        let metrics = resolve_metric_f64_hashmap!(self, &self.common.metrics, resolution_maps, "metrics");
        let indices = resolve_metric_u64_hashmap!(self, &self.common.indices, resolution_maps, "indices");

        let common = PyCommon {
            meta: self.common.meta,
            args: self.common.args,
            kwargs: self.common.kwargs,
            metrics,
            indices,
        };

        let p = PyClassParameter {
            class: self.class,
            common,
        };

        Ok(MaybeBuiltParameter::Built(BuiltParameter::General(Box::new(p))))
    }
}

impl ParameterBuilder<u64> for PyClassParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.common.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<u64>, ParameterBuildError> {
        let metrics = resolve_metric_f64_hashmap!(self, &self.common.metrics, resolution_maps, "metrics");
        let indices = resolve_metric_u64_hashmap!(self, &self.common.indices, resolution_maps, "indices");

        let common = PyCommon {
            meta: self.common.meta,
            args: self.common.args,
            kwargs: self.common.kwargs,
            metrics,
            indices,
        };

        let p = PyClassParameter {
            class: self.class,
            common,
        };

        Ok(MaybeBuiltParameter::Built(BuiltParameter::General(Box::new(p))))
    }
}

impl ParameterBuilder<MultiValue> for PyClassParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.common.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<MultiValue>, ParameterBuildError> {
        let metrics = resolve_metric_f64_hashmap!(self, &self.common.metrics, resolution_maps, "metrics");
        let indices = resolve_metric_u64_hashmap!(self, &self.common.indices, resolution_maps, "indices");

        let common = PyCommon {
            meta: self.common.meta,
            args: self.common.args,
            kwargs: self.common.kwargs,
            metrics,
            indices,
        };

        let p = PyClassParameter {
            class: self.class,
            common,
        };

        Ok(MaybeBuiltParameter::Built(BuiltParameter::General(Box::new(p))))
    }
}

/// A Python parameter that returns the value produced by a Python function.
///
/// This parameter allows you to define a Python function which takes a `ParameterInfo` object as its first argument,
/// and then the user defined `args` and `kwargs` as additional arguments.
#[derive(Debug)]
pub struct PyFuncParameter {
    /// This is the user's class that implements the parameter logic.
    function: Py<PyAny>,
    common: PyCommon,
}

impl PartialEq for PyFuncParameter {
    fn eq(&self, other: &Self) -> bool {
        self.function.as_ptr().addr() == other.function.as_ptr().addr() && self.common == other.common
    }
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
    fn setup(&self) -> Result<Option<Box<dyn ParameterState>>, ParameterSetupError> {
        Python::initialize();

        let internal = InternalInfo { info_obj: None };

        Ok(Some(internal.into_boxed_any()))
    }

    fn compute<T>(
        &self,
        ctx: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<T, GeneralCalculationError>
    where
        T: for<'a, 'py> FromPyObject<'a, 'py>,
    {
        let internal = downcast_internal_state_mut::<InternalInfo>(internal_state);

        ensure_parameter_info(&mut internal.info_obj, ctx.timestep, ctx.scenario_index).map_err(|py_error| {
            GeneralCalculationError::PythonError {
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
                info_mut.timestep = *ctx.timestep;
                info_mut.scenario_index = ctx.scenario_index.clone();
                self.common
                    .update_metrics(ctx.network, ctx.state, &mut info_mut.metric_values)?;

                self.common
                    .update_indices(ctx.network, ctx.state, &mut info_mut.index_values)?;
            }

            let args = PyTuple::new(py, [info_bind]).map_err(|py_error| GeneralCalculationError::PythonError {
                name: self.common.meta.name.to_string(),
                object: self.function.to_string(),
                py_error: Box::new(py_error),
            })?;

            // Concatenate the user defined args with the info arg.
            let args = args
                .into_sequence()
                .concat(self.common.args.bind(py).as_sequence())
                .map_err(|py_error| GeneralCalculationError::PythonError {
                    name: self.common.meta.name.to_string(),
                    object: self.function.to_string(),
                    py_error: Box::new(py_error),
                })?;
            let args = args
                .to_tuple()
                .map_err(|py_error| GeneralCalculationError::PythonError {
                    name: self.common.meta.name.to_string(),
                    object: self.function.to_string(),
                    py_error: Box::new(py_error),
                })?;

            let kwargs = self.common.kwargs.bind(py);

            self.function
                .call(py, args, Some(kwargs))
                .map_err(|py_error| GeneralCalculationError::PythonError {
                    name: self.common.meta.name.to_string(),
                    object: self.function.to_string(),
                    py_error: Box::new(py_error),
                })?
                .extract(py)
                .map_err(Into::into)
                .map_err(|py_error| GeneralCalculationError::PythonError {
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
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<f64>, GeneralCalculationError> {
        self.compute(ctx, internal_state)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralParameter<u64> for PyFuncParameter {
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<u64>, GeneralCalculationError> {
        self.compute(ctx, internal_state)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralParameter<MultiValue> for PyFuncParameter {
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<MultiValue>, GeneralCalculationError> {
        self.compute(ctx, internal_state)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

#[derive(Debug)]
pub struct PyFuncParameterBuilder {
    function: Py<PyAny>,
    common: PyCommonBuilder,
}

impl PyFuncParameterBuilder {
    pub fn new(name: ParameterName, object: Py<PyAny>, args: Py<PyTuple>, kwargs: Py<PyDict>) -> Self {
        Self {
            function: object,
            common: PyCommonBuilder::new(ParameterMeta::new(name), args, kwargs),
        }
    }

    pub fn metric(&mut self, key: &str, value: UnresolvedMetricF64) -> &mut Self {
        self.common.metric(key, value);
        self
    }

    pub fn index(&mut self, key: &str, value: UnresolvedMetricU64) -> &mut Self {
        self.common.index(key, value);
        self
    }
}

impl ParameterBuilder<f64> for PyFuncParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.common.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        let metrics = resolve_metric_f64_hashmap!(self, &self.common.metrics, resolution_maps, "metrics");
        let indices = resolve_metric_u64_hashmap!(self, &self.common.indices, resolution_maps, "indices");

        let common = PyCommon {
            meta: self.common.meta,
            args: self.common.args,
            kwargs: self.common.kwargs,
            metrics,
            indices,
        };

        let p = PyFuncParameter {
            function: self.function,
            common,
        };

        Ok(MaybeBuiltParameter::Built(BuiltParameter::General(Box::new(p))))
    }
}

impl ParameterBuilder<u64> for PyFuncParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.common.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<u64>, ParameterBuildError> {
        let metrics = resolve_metric_f64_hashmap!(self, &self.common.metrics, resolution_maps, "metrics");
        let indices = resolve_metric_u64_hashmap!(self, &self.common.indices, resolution_maps, "indices");

        let common = PyCommon {
            meta: self.common.meta,
            args: self.common.args,
            kwargs: self.common.kwargs,
            metrics,
            indices,
        };

        let p = PyFuncParameter {
            function: self.function,
            common,
        };

        Ok(MaybeBuiltParameter::Built(BuiltParameter::General(Box::new(p))))
    }
}

impl ParameterBuilder<MultiValue> for PyFuncParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.common.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<MultiValue>, ParameterBuildError> {
        let metrics = resolve_metric_f64_hashmap!(self, &self.common.metrics, resolution_maps, "metrics");
        let indices = resolve_metric_u64_hashmap!(self, &self.common.indices, resolution_maps, "indices");

        let common = PyCommon {
            meta: self.common.meta,
            args: self.common.args,
            kwargs: self.common.kwargs,
            metrics,
            indices,
        };

        let p = PyFuncParameter {
            function: self.function,
            common,
        };

        Ok(MaybeBuiltParameter::Built(BuiltParameter::General(Box::new(p))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::ScenarioIndexBuilder;
    use crate::state::StateBuilder;
    use crate::test_utils::default_time_domain_builder;
    use chrono::Datelike;
    use float_cmp::assert_approx_eq;
    use pyo3::ffi::c_str;
    use std::ffi::CStr;

    enum CounterParameterType {
        BeforeOnly,
        BeforeAfter,
        AfterOnly,
    }

    #[test]
    fn test_counter_parameter_before_only() {
        test_counter_parameter(
            CounterParameterType::BeforeOnly,
            c_str!(
                r#"
class MyParameter:
    def __init__(self, count, **kwargs):
        self.count = count

    def before(self, info):
        self.count += info.scenario_index.simulation_id
        return float(self.count + info.timestep.day)
"#
            ),
        )
    }

    #[test]
    fn test_counter_parameter_before_after() {
        test_counter_parameter(
            CounterParameterType::BeforeAfter,
            c_str!(
                r#"
class MyParameter:
    def __init__(self, count, **kwargs):
        self.count = count

    def before(self, info):
        self.count += info.scenario_index.simulation_id
        return float(self.count + info.timestep.day)

    def after(self, info):
        self.count += info.scenario_index.simulation_id
        return float(self.count + info.timestep.day)
"#
            ),
        )
    }

    #[test]
    fn test_counter_parameter_after_only() {
        test_counter_parameter(
            CounterParameterType::AfterOnly,
            c_str!(
                r#"
class MyParameter:
    def __init__(self, count, **kwargs):
        self.count = count

    def after(self, info):
        self.count += info.scenario_index.simulation_id
        return float(self.count + info.timestep.day)
"#
            ),
        )
    }

    /// Test `PyClassParameter` returns the correct value.
    fn test_counter_parameter(counter_parameter_type: CounterParameterType, counter_parameter_str: &'static CStr) {
        // Init Python
        Python::initialize();

        let class = Python::attach(|py| {
            let test_module = PyModule::from_code(py, counter_parameter_str, c_str!(""), c_str!("")).unwrap();

            test_module.getattr("MyParameter").unwrap().into()
        });

        let args = Python::attach(|py| PyTuple::new(py, [0]).unwrap().unbind());
        let kwargs = Python::attach(|py| PyDict::new(py).unbind());

        let common = PyCommon {
            meta: ParameterMeta::new("my-parameter".into()),
            args,
            kwargs,
            metrics: Default::default(),
            indices: Default::default(),
        };

        let param = PyClassParameter { class, common };

        let time = default_time_domain_builder().build().unwrap();
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
                let ctx = GeneralParameterContext {
                    timestep: ts,
                    scenario_index: si,
                    network: &model,
                    state: &state,
                };

                let before_value: Option<f64> = GeneralParameter::before(&param, ctx, internal).unwrap();

                let after_value: Option<f64> = GeneralParameter::after(&param, ctx, internal).unwrap();

                match counter_parameter_type {
                    CounterParameterType::BeforeOnly => {
                        assert_approx_eq!(
                            f64,
                            before_value.expect("Expected a value from before()"),
                            ((ts.index + 1) * si.simulation_id() + ts.date.day() as usize) as f64
                        );
                        assert!(after_value.is_none(), "Expected no value from after()");
                    }
                    CounterParameterType::BeforeAfter => {
                        assert_approx_eq!(
                            f64,
                            before_value.expect("Expected a value from before()"),
                            ((ts.index * 2 + 1) * si.simulation_id() + ts.date.day() as usize) as f64
                        );
                        assert_approx_eq!(
                            f64,
                            after_value.expect("Expected a value from after()"),
                            ((ts.index * 2 + 2) * si.simulation_id() + ts.date.day() as usize) as f64
                        );
                    }
                    CounterParameterType::AfterOnly => {
                        assert!(before_value.is_none(), "Expected no value from before()");
                        assert_approx_eq!(
                            f64,
                            after_value.expect("Expected a value from after()"),
                            ((ts.index + 1) * si.simulation_id() + ts.date.day() as usize) as f64
                        );
                    }
                }
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

    def before(self, info):
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

        let common = PyCommon {
            meta: ParameterMeta::new("my-parameter".into()),
            args,
            kwargs,
            metrics: Default::default(),
            indices: Default::default(),
        };

        let param = PyClassParameter { class, common };

        let time = default_time_domain_builder().build().unwrap();
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
                let ctx = GeneralParameterContext {
                    timestep: ts,
                    scenario_index: si,
                    network: &model,
                    state: &state,
                };

                let value = GeneralParameter::<MultiValue>::before(&param, ctx, internal)
                    .unwrap()
                    .unwrap();

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

        let common = PyCommon {
            meta: ParameterMeta::new("my-parameter".into()),
            args,
            kwargs,
            metrics: Default::default(),
            indices: Default::default(),
        };

        let param = PyFuncParameter { function, common };

        let time = default_time_domain_builder().build().unwrap();
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
                let ctx = GeneralParameterContext {
                    timestep: ts,
                    scenario_index: si,
                    network: &model,
                    state: &state,
                };

                let value = GeneralParameter::before(&param, ctx, internal).unwrap().unwrap();

                assert_approx_eq!(f64, value, (2 + si.simulation_id() + ts.date.day() as usize) as f64);
            }
        }
    }
}
