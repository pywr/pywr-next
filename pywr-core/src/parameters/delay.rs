use crate::metric::{
    MetricF64, MetricF64Error, MetricU64, MetricU64Error, SimpleMetricF64, SimpleMetricU64, UnresolvedMetricF64,
    UnresolvedMetricU64,
};
use crate::network::ResolutionMaps;
use crate::parameters::errors::{GeneralCalculationError, ParameterSetupError, SimpleCalculationError};
use crate::parameters::{
    BuiltParameter, GeneralAfterParameterHook, GeneralBeforeParameter, GeneralParameter, GeneralParameterContext,
    GeneralParameterEntry, MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterMeta,
    ParameterName, ParameterState, SimpleParameter, SimpleParameterContext, downcast_internal_state_mut,
};
use crate::scenario::ScenarioIndex;
use crate::timestep::Timestep;
use crate::{resolve_metric_f64, resolve_metric_u64};
use std::collections::VecDeque;
use std::fmt::Debug;

#[derive(Debug)]
pub struct DelayParameter<M, T> {
    meta: ParameterMeta,
    metric: M,
    delay: u64,
    initial_value: T,
}

impl<T> TryInto<DelayParameter<SimpleMetricF64, T>> for &DelayParameter<MetricF64, T>
where
    T: Copy,
{
    type Error = MetricF64Error;

    fn try_into(self) -> Result<DelayParameter<SimpleMetricF64, T>, Self::Error> {
        Ok(DelayParameter {
            meta: self.meta.clone(),
            metric: self.metric.clone().try_into()?,
            delay: self.delay,
            initial_value: self.initial_value,
        })
    }
}

impl<T> TryInto<DelayParameter<SimpleMetricU64, T>> for &DelayParameter<MetricU64, T>
where
    T: Copy,
{
    type Error = MetricU64Error;

    fn try_into(self) -> Result<DelayParameter<SimpleMetricU64, T>, Self::Error> {
        Ok(DelayParameter {
            meta: self.meta.clone(),
            metric: self.metric.clone().try_into()?,
            delay: self.delay,
            initial_value: self.initial_value,
        })
    }
}

impl<M, T> Parameter for DelayParameter<M, T>
where
    M: Send + Sync + Debug,
    T: Send + Sync + Copy + Debug + 'static,
{
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, ParameterSetupError> {
        // Internally we need to store a history of previous values
        let memory: VecDeque<T> = (0..self.delay).map(|_| self.initial_value).collect();
        Ok(Some(Box::new(memory)))
    }
}

impl GeneralParameter for DelayParameter<MetricF64, f64> {
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralBeforeParameter<f64> for DelayParameter<MetricF64, f64> {
    fn before(
        &self,
        _ctx: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, GeneralCalculationError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_mut::<VecDeque<f64>>(internal_state);

        // Take the oldest value from the queue
        // It should be guaranteed that the internal memory/queue has self.delay number of values
        let value = memory.pop_front().ok_or_else(|| GeneralCalculationError::Internal {
            message: "Delay parameter queue did not contain any values. This internal error should not be possible!"
                .into(),
        })?;

        Ok(value)
    }
}
impl GeneralAfterParameterHook<f64> for DelayParameter<MetricF64, f64> {
    fn after(
        &self,
        ctx: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), GeneralCalculationError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_mut::<VecDeque<f64>>(internal_state);

        // Get today's value from the metric
        let value = self.metric.get_value(ctx.network, ctx.state)?;
        memory.push_back(value);

        Ok(())
    }
}

impl SimpleParameter<f64> for DelayParameter<SimpleMetricF64, f64> {
    fn compute(
        &self,
        ctx: SimpleParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, SimpleCalculationError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_mut::<VecDeque<f64>>(internal_state);

        // Get today's value from the metric
        let value = self.metric.get_value(ctx.values)?;
        memory.push_back(value);

        // Take the oldest value from the queue
        // It should be guaranteed that the internal memory/queue has self.delay number of values
        let value = memory.pop_front().ok_or_else(|| SimpleCalculationError::Internal {
            message: "Delay parameter queue did not contain any values. This internal error should not be possible!"
                .into(),
        })?;

        Ok(value)
    }
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralParameter for DelayParameter<MetricU64, u64> {
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralBeforeParameter<u64> for DelayParameter<MetricU64, u64> {
    fn before(
        &self,
        _ctx: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, GeneralCalculationError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_mut::<VecDeque<u64>>(internal_state);

        // Take the oldest value from the queue
        // It should be guaranteed that the internal memory/queue has self.delay number of values
        let value = memory.pop_front().ok_or_else(|| GeneralCalculationError::Internal {
            message: "Delay parameter queue did not contain any values. This internal error should not be possible!"
                .into(),
        })?;

        Ok(value)
    }
}
impl GeneralAfterParameterHook<u64> for DelayParameter<MetricU64, u64> {
    fn after(
        &self,
        ctx: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), GeneralCalculationError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_mut::<VecDeque<u64>>(internal_state);

        // Get today's value from the metric
        let value = self.metric.get_value(ctx.network, ctx.state)?;
        memory.push_back(value);

        Ok(())
    }
}

impl SimpleParameter<u64> for DelayParameter<SimpleMetricU64, u64> {
    fn compute(
        &self,
        ctx: SimpleParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, SimpleCalculationError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_mut::<VecDeque<u64>>(internal_state);

        // Get today's value from the metric
        let value = self.metric.get_value(ctx.values)?;
        memory.push_back(value);

        // Take the oldest value from the queue
        // It should be guaranteed that the internal memory/queue has self.delay number of values
        let value = memory.pop_front().ok_or_else(|| SimpleCalculationError::Internal {
            message: "Delay parameter queue did not contain any values. This internal error should not be possible!"
                .into(),
        })?;

        Ok(value)
    }
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

#[derive(Debug)]
pub struct DelayParameterBuilder<M, T> {
    meta: ParameterMeta,
    metric: M,
    delay: u64,
    initial_value: T,
}

impl<M, T> DelayParameterBuilder<M, T> {
    pub fn new(name: ParameterName, metric: M, delay: u64, initial_value: T) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            delay,
            initial_value,
        }
    }
}

impl ParameterBuilder<f64> for DelayParameterBuilder<UnresolvedMetricF64, f64> {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        let metric = resolve_metric_f64!(self, self.metric, resolution_maps, "metric");

        let simple_metric: Result<SimpleMetricF64, _> = metric.clone().try_into();
        if let Ok(simple_metric) = simple_metric {
            let p = DelayParameter {
                meta: self.meta,
                metric: simple_metric,
                delay: self.delay,
                initial_value: self.initial_value,
            };

            let bp = BuiltParameter::Simple(Box::new(p));
            return Ok(bp.into());
        }

        let p = DelayParameter {
            meta: self.meta,
            metric,
            delay: self.delay,
            initial_value: self.initial_value,
        };

        let bp = BuiltParameter::General(GeneralParameterEntry::before_with_after_hook(p));
        Ok(bp.into())
    }
}

impl ParameterBuilder<u64> for DelayParameterBuilder<UnresolvedMetricU64, u64> {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<u64>, ParameterBuildError> {
        let metric = resolve_metric_u64!(self, self.metric, resolution_maps, "metric");

        let simple_metric: Result<SimpleMetricU64, _> = metric.clone().try_into();
        if let Ok(simple_metric) = simple_metric {
            let p = DelayParameter {
                meta: self.meta,
                metric: simple_metric,
                delay: self.delay,
                initial_value: self.initial_value,
            };

            let bp = BuiltParameter::Simple(Box::new(p));
            return Ok(bp.into());
        }

        let p = DelayParameter {
            meta: self.meta,
            metric,
            delay: self.delay,
            initial_value: self.initial_value,
        };

        let bp = BuiltParameter::General(GeneralParameterEntry::before_with_after_hook(p));
        Ok(bp.into())
    }
}

#[cfg(test)]
mod test {
    use crate::metric::{UnresolvedMetricF64, UnresolvedMetricU64};
    use crate::parameters::Array1ParameterBuilder;
    use crate::parameters::delay::DelayParameterBuilder;
    use crate::test_utils::{run_and_assert_parameter, run_and_assert_parameter_u64, simple_model};
    use ndarray::{Array1, Array2, Axis, concatenate, s};

    /// Basic functional test of the delay parameter.
    #[test]
    fn test_delay_f64() {
        let mut model_builder = simple_model(1, None);

        // Create an artificial volume series to use for the delay test
        let volumes = Array1::linspace(1.0, 0.0, 21);
        let volume = Array1ParameterBuilder::new("test-x".into(), volumes.clone());

        model_builder.network_builder().parameters().f64(Box::new(volume));

        const DELAY: u64 = 3; // 3 time-step delay
        let parameter = DelayParameterBuilder::new(
            "test-parameter".into(),
            UnresolvedMetricF64::new_parameter_before("test-x"),
            DELAY,
            0.0,
        );

        // We should have DELAY number of initial values to start with, and then follow the
        // values in the `volumes` array.
        let expected_values: Array1<f64> = [
            0.0; DELAY as usize // initial values
        ]
            .to_vec()
            .into();

        let expected_values = concatenate![
            Axis(0),
            expected_values,
            volumes.slice(s![..volumes.len() - DELAY as usize])
        ];

        let expected_values: Array2<f64> = expected_values.insert_axis(Axis(1));

        run_and_assert_parameter(model_builder, Box::new(parameter), expected_values, None, Some(1e-12));
    }

    /// Basic functional test of the delay parameter.
    #[test]
    fn test_delay_u64() {
        let mut model_builder = simple_model(1, None);

        // Create an artificial volume series to use for the delay test
        let volumes: Array1<u64> = Array1::from(Array1::linspace(1.0, 0.0, 21).map(|x| *x as u64));
        let volume = Array1ParameterBuilder::new("test-x".into(), volumes.clone());

        model_builder.network_builder().parameters().u64(Box::new(volume));

        const DELAY: u64 = 3; // 3 time-step delay
        let parameter = DelayParameterBuilder::new(
            "test-parameter".into(),
            UnresolvedMetricU64::new_parameter_before("test-x"),
            DELAY,
            0,
        );

        // We should have DELAY number of initial values to start with, and then follow the
        // values in the `volumes` array.
        let expected_values: Array1<u64> = [
            0; DELAY as usize // initial values
        ]
            .to_vec()
            .into();

        let expected_values = concatenate![
            Axis(0),
            expected_values,
            volumes.slice(s![..volumes.len() - DELAY as usize])
        ];

        let expected_values: Array2<u64> = expected_values.insert_axis(Axis(1));

        run_and_assert_parameter_u64(model_builder, Box::new(parameter), expected_values);
    }
}
