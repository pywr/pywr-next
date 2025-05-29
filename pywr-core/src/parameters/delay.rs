use crate::PywrError;
use crate::metric::{MetricF64, SimpleMetricF64};
use crate::network::Network;
use crate::parameters::{
    GeneralParameter, Parameter, ParameterMeta, ParameterName, ParameterState, SimpleParameter,
    downcast_internal_state_mut,
};
use crate::scenario::ScenarioIndex;
use crate::state::{SimpleParameterValues, State};
use crate::timestep::Timestep;
use std::collections::VecDeque;

pub struct DelayParameter<M> {
    meta: ParameterMeta,
    metric: M,
    delay: u64,
    initial_value: f64,
}

impl<M> DelayParameter<M> {
    pub fn new(name: ParameterName, metric: M, delay: u64, initial_value: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            delay,
            initial_value,
        }
    }
}

impl TryInto<DelayParameter<SimpleMetricF64>> for &DelayParameter<MetricF64> {
    type Error = PywrError;

    fn try_into(self) -> Result<DelayParameter<SimpleMetricF64>, Self::Error> {
        Ok(DelayParameter {
            meta: self.meta.clone(),
            metric: self.metric.clone().try_into()?,
            delay: self.delay,
            initial_value: self.initial_value,
        })
    }
}

impl<M> Parameter for DelayParameter<M>
where
    M: Send + Sync,
{
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, PywrError> {
        // Internally we need to store a history of previous values
        let memory: VecDeque<f64> = (0..self.delay).map(|_| self.initial_value).collect();
        Ok(Some(Box::new(memory)))
    }
}

impl GeneralParameter<f64> for DelayParameter<MetricF64> {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _model: &Network,
        _state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, PywrError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_mut::<VecDeque<f64>>(internal_state);

        // Take the oldest value from the queue
        // It should be guaranteed that the internal memory/queue has self.delay number of values
        let value = memory
            .pop_front()
            .expect("Delay parameter queue did not contain any values. This internal error should not be possible!");

        Ok(value)
    }

    fn after(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), PywrError> {
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
            .map(|p: DelayParameter<SimpleMetricF64>| Box::new(p) as Box<dyn SimpleParameter<f64>>)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl SimpleParameter<f64> for DelayParameter<SimpleMetricF64> {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _values: &SimpleParameterValues,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, PywrError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_mut::<VecDeque<f64>>(internal_state);

        // Take the oldest value from the queue
        // It should be guaranteed that the internal memory/queue has self.delay number of values
        let value = memory
            .pop_front()
            .expect("Delay parameter queue did not contain any values. This internal error should not be possible!");

        Ok(value)
    }

    fn after(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        values: &SimpleParameterValues,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), PywrError> {
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

#[cfg(test)]
mod test {
    use crate::parameters::{Array1Parameter, DelayParameter};
    use crate::test_utils::{run_and_assert_parameter, simple_model};
    use ndarray::{Array1, Array2, Axis, concatenate, s};

    /// Basic functional test of the delay parameter.
    #[test]
    fn test_basic() {
        let mut model = simple_model(1, None);

        // Create an artificial volume series to use for the delay test
        let volumes = Array1::linspace(1.0, 0.0, 21);
        let volume = Array1Parameter::new("test-x".into(), volumes.clone(), None);

        let volume_idx = model.network_mut().add_simple_parameter(Box::new(volume)).unwrap();

        const DELAY: u64 = 3; // 3 time-step delay
        let parameter = DelayParameter::new(
            "test-parameter".into(),
            volume_idx.into(), // Interpolate with the parameter based values
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

        run_and_assert_parameter(&mut model, Box::new(parameter), expected_values, None, Some(1e-12));
    }
}
