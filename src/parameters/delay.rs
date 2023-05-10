use crate::metric::Metric;
use crate::model::Model;
use crate::parameters::{downcast_internal_state, Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;
use std::collections::VecDeque;

pub struct DelayParameter {
    meta: ParameterMeta,
    metric: Metric,
    delay: usize,
    initial_value: f64,
}

impl DelayParameter {
    pub fn new(name: &str, metric: Metric, delay: usize, initial_value: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            delay,
            initial_value,
        }
    }
}

impl Parameter for DelayParameter {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn setup(
        &self,
        timesteps: &[Timestep],
        scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn Any + Send>>, PywrError> {
        // Internally we need to store a history of previous values
        let memory: VecDeque<f64> = (0..self.delay).map(|_| self.initial_value).collect();
        Ok(Some(Box::new(memory)))
    }

    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Model,
        state: &State,
        internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<f64, PywrError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state::<VecDeque<f64>>(internal_state);

        // Take the oldest value from the queue
        // It should be guaranteed that the internal memory/queue has self.delay number of values
        let value = memory
            .pop_front()
            .expect("Delay parameter queue did not contain any values. This internal error should not be possible!");

        Ok(value)
    }

    fn after(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Model,
        state: &State,
        internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<(), PywrError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state::<VecDeque<f64>>(internal_state);

        // Get today's value from the metric
        let value = self.metric.get_value(model, state)?;
        memory.push_back(value);

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::metric::Metric;
    use crate::parameters::{Array1Parameter, DelayParameter};
    use crate::test_utils::{run_and_assert_parameter, simple_model};
    use ndarray::{concatenate, s, Array1, Array2, Axis};

    /// Basic functional test of the delay parameter.
    #[test]
    fn test_basic() {
        let mut model = simple_model(1);

        // Create an artificial volume series to use for the delay test
        let volumes = Array1::linspace(1.0, 0.0, 21);
        let volume = Array1Parameter::new("test-x", volumes.clone());

        let volume_idx = model.add_parameter(Box::new(volume)).unwrap();

        const DELAY: usize = 3; // 3 time-step delay
        let parameter = DelayParameter::new(
            "test-parameter",
            Metric::ParameterValue(volume_idx), // Interpolate with the parameter based values
            DELAY,
            0.0,
        );

        // We should have DELAY number of initial values to start with, and then follow the
        // values in the `volumes` array.
        let expected_values: Array1<f64> = [
            0.0; DELAY // initial values
        ]
            .to_vec()
            .into();

        let expected_values = concatenate![Axis(0), expected_values, volumes.slice(s![..volumes.len() - DELAY])];

        let expected_values: Array2<f64> = expected_values.insert_axis(Axis(1));

        run_and_assert_parameter(&mut model, Box::new(parameter), expected_values, None, Some(1e-12));
    }
}