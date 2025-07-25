use crate::metric::{MetricF64, MetricF64Error, MetricU64, MetricU64Error, SimpleMetricF64, SimpleMetricU64};
use crate::network::Network;
use crate::parameters::errors::{ParameterCalculationError, ParameterSetupError, SimpleCalculationError};
use crate::parameters::{
    GeneralParameter, Parameter, ParameterMeta, ParameterName, ParameterState, SimpleParameter,
    downcast_internal_state_mut,
};
use crate::scenario::ScenarioIndex;
use crate::state::{SimpleParameterValues, State};
use crate::timestep::Timestep;
use std::collections::VecDeque;

pub struct DelayParameter<M, T> {
    meta: ParameterMeta,
    metric: M,
    delay: u64,
    initial_value: T,
}

impl<M, T> DelayParameter<M, T> {
    pub fn new(name: ParameterName, metric: M, delay: u64, initial_value: T) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            delay,
            initial_value,
        }
    }
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
    M: Send + Sync,
    T: Send + Sync + Copy + 'static,
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

impl GeneralParameter<f64> for DelayParameter<MetricF64, f64> {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _model: &Network,
        _state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, ParameterCalculationError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_mut::<VecDeque<f64>>(internal_state);

        // Take the oldest value from the queue
        // It should be guaranteed that the internal memory/queue has self.delay number of values
        let value = memory.pop_front().ok_or_else(|| ParameterCalculationError::Internal {
            message: "Delay parameter queue did not contain any values. This internal error should not be possible!"
                .into(),
        })?;

        Ok(value)
    }

    fn after(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), ParameterCalculationError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_mut::<VecDeque<f64>>(internal_state);

        // Get today's value from the metric
        let value = self.metric.get_value(model, state)?;
        memory.push_back(value);

        Ok(())
    }

    fn try_into_simple(&self) -> Option<Box<dyn SimpleParameter<f64>>>
    where
        Self: Sized,
    {
        self.try_into()
            .ok()
            .map(|p: DelayParameter<SimpleMetricF64, f64>| Box::new(p) as Box<dyn SimpleParameter<f64>>)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl SimpleParameter<f64> for DelayParameter<SimpleMetricF64, f64> {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _values: &SimpleParameterValues,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, SimpleCalculationError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_mut::<VecDeque<f64>>(internal_state);

        // Take the oldest value from the queue
        // It should be guaranteed that the internal memory/queue has self.delay number of values
        let value = memory.pop_front().ok_or_else(|| SimpleCalculationError::Internal {
            message: "Delay parameter queue did not contain any values. This internal error should not be possible!"
                .into(),
        })?;

        Ok(value)
    }

    fn after(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        values: &SimpleParameterValues,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), SimpleCalculationError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_mut::<VecDeque<f64>>(internal_state);

        // Get today's value from the metric
        let value = self.metric.get_value(values)?;
        memory.push_back(value);

        Ok(())
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralParameter<u64> for DelayParameter<MetricU64, u64> {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _model: &Network,
        _state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, ParameterCalculationError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_mut::<VecDeque<u64>>(internal_state);

        // Take the oldest value from the queue
        // It should be guaranteed that the internal memory/queue has self.delay number of values
        let value = memory.pop_front().ok_or_else(|| ParameterCalculationError::Internal {
            message: "Delay parameter queue did not contain any values. This internal error should not be possible!"
                .into(),
        })?;

        Ok(value)
    }

    fn after(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), ParameterCalculationError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_mut::<VecDeque<u64>>(internal_state);

        // Get today's value from the metric
        let value = self.metric.get_value(model, state)?;
        memory.push_back(value);

        Ok(())
    }

    fn try_into_simple(&self) -> Option<Box<dyn SimpleParameter<u64>>>
    where
        Self: Sized,
    {
        self.try_into()
            .ok()
            .map(|p: DelayParameter<SimpleMetricU64, u64>| Box::new(p) as Box<dyn SimpleParameter<u64>>)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl SimpleParameter<u64> for DelayParameter<SimpleMetricU64, u64> {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _values: &SimpleParameterValues,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, SimpleCalculationError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_mut::<VecDeque<u64>>(internal_state);

        // Take the oldest value from the queue
        // It should be guaranteed that the internal memory/queue has self.delay number of values
        let value = memory.pop_front().ok_or_else(|| SimpleCalculationError::Internal {
            message: "Delay parameter queue did not contain any values. This internal error should not be possible!"
                .into(),
        })?;

        Ok(value)
    }

    fn after(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        values: &SimpleParameterValues,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), SimpleCalculationError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_mut::<VecDeque<u64>>(internal_state);

        // Get today's value from the metric
        let value = self.metric.get_value(values)?;
        memory.push_back(value);

        Ok(())
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

#[cfg(test)]
mod test {
    use crate::metric::MetricU64;
    use crate::parameters::{Array1Parameter, DelayParameter};
    use crate::test_utils::{run_and_assert_parameter, run_and_assert_parameter_u64, simple_model};
    use ndarray::{Array1, Array2, Axis, concatenate, s};

    /// Basic functional test of the delay parameter.
    #[test]
    fn test_delay_f64() {
        let mut model = simple_model(1, None);

        // Create an artificial volume series to use for the delay test
        let volumes = Array1::linspace(1.0, 0.0, 21);
        let volume = Array1Parameter::new("test-x".into(), volumes.clone(), None);

        let volume_idx = model.network_mut().add_simple_parameter(Box::new(volume)).unwrap();

        const DELAY: u64 = 3; // 3 time-step delay
        let parameter = DelayParameter::new("test-parameter".into(), volume_idx.into(), DELAY, 0.0);

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

        run_and_assert_parameter(&mut model, Box::new(parameter), expected_values, None, Some(1e-12));
    }

    /// Basic functional test of the delay parameter.
    #[test]
    fn test_delay_u64() {
        let mut model = simple_model(1, None);

        // Create an artificial volume series to use for the delay test
        let volumes: Array1<u64> = Array1::from(Array1::linspace(1.0, 0.0, 21).map(|x| *x as u64));
        let volume = Array1Parameter::new("test-x".into(), volumes.clone(), None);

        let metric: MetricU64 = model
            .network_mut()
            .add_simple_index_parameter(Box::new(volume))
            .unwrap()
            .into();

        const DELAY: u64 = 3; // 3 time-step delay
        let parameter = DelayParameter::new("test-parameter".into(), metric, DELAY, 0);

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

        run_and_assert_parameter_u64(&mut model, Box::new(parameter), expected_values);
    }
}
