use crate::metric::MetricF64;
use crate::network::Network;
use crate::parameters::errors::{ParameterCalculationError, ParameterSetupError};
use crate::parameters::{
    GeneralParameter, Parameter, ParameterMeta, ParameterName, ParameterState, Predicate, downcast_internal_state_mut,
};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;

pub struct MultiThresholdParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    thresholds: Vec<MetricF64>,
    predicate: Predicate,
    ratchet: bool,
}

impl MultiThresholdParameter {
    pub fn new(
        name: ParameterName,
        metric: MetricF64,
        thresholds: &[MetricF64],
        predicate: Predicate,
        ratchet: bool,
    ) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            thresholds: thresholds.to_vec(),
            predicate,
            ratchet,
        }
    }
}

impl Parameter for MultiThresholdParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, ParameterSetupError> {
        // Internal state is just a u64 indicating the previous highest value.
        // Initially this is zero.
        Ok(Some(Box::new(0_u64)))
    }
}

impl GeneralParameter<u64> for MultiThresholdParameter {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, ParameterCalculationError> {
        // Downcast the internal state to the correct type
        let previous_max = downcast_internal_state_mut::<u64>(internal_state);

        let value = self.metric.get_value(model, state)?;

        // Determine the first threshold that is met
        let mut position: u64 = 0;
        for threshold in &self.thresholds {
            let t = threshold.get_value(model, state)?;
            let active = self.predicate.apply(value, t);

            if active {
                break;
            }
            position += 1;
        }

        if self.ratchet {
            // If ratchet is enabled, we only update if the new position is greater than the previous max
            if position > *previous_max {
                *previous_max = position;
            } else {
                return Ok(*previous_max);
            }
        }

        Ok(position)
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
    use super::MultiThresholdParameter;
    use crate::metric::MetricF64;
    use crate::parameters::{Array1Parameter, Predicate};
    use crate::test_utils::{run_and_assert_parameter_u64, simple_model};
    use ndarray::{Array1, Array2, Axis, concatenate};

    /// Basic functional test of the `MultiThresholdParameter` parameter.
    #[test]
    fn test_multi_threshold() {
        let mut model = simple_model(1, None);

        // Create an artificial volume series to use for the delay test
        let v1 = Array1::linspace(1.0, 0.0, 11);
        let v2 = Array1::linspace(0.1, 1.0, 10);

        let volumes = concatenate![Axis(0), v1, v2];

        let volume = Array1Parameter::new("test-x".into(), volumes.clone(), None);

        let volume_idx = model.network_mut().add_simple_parameter(Box::new(volume)).unwrap();

        let t1: MetricF64 = 0.75.into();
        let t2: MetricF64 = 0.5.into();

        let thresholds = vec![t1, t2];

        let parameter = MultiThresholdParameter::new(
            "test-parameter".into(),
            volume_idx.into(),
            &thresholds,
            Predicate::GreaterThan,
            false,
        );

        // The multi-threshold parameter should return the index of the first threshold that is met.
        // In this case, the thresholds are 0.75 and 0.5,
        let expected_values: Array1<u64> = vec![0, 0, 0, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 1, 1, 0, 0, 0].into();

        let expected_values: Array2<u64> = expected_values.insert_axis(Axis(1));

        run_and_assert_parameter_u64(&mut model, Box::new(parameter), expected_values);
    }

    /// Basic functional test of the `MultiThresholdParameter` parameter with ratchet enabled.
    #[test]
    fn test_multi_threshold_with_ratchet() {
        let mut model = simple_model(1, None);

        // Create an artificial volume series to use for the delay test
        let v1 = Array1::linspace(1.0, 0.0, 11);
        let v2 = Array1::linspace(0.1, 1.0, 10);

        let volumes = concatenate![Axis(0), v1, v2];

        let volume = Array1Parameter::new("test-x".into(), volumes.clone(), None);

        let volume_idx = model.network_mut().add_simple_parameter(Box::new(volume)).unwrap();

        let t1: MetricF64 = 0.75.into();
        let t2: MetricF64 = 0.5.into();

        let thresholds = vec![t1, t2];

        let parameter = MultiThresholdParameter::new(
            "test-parameter".into(),
            volume_idx.into(),
            &thresholds,
            Predicate::GreaterThan,
            true,
        );

        // The multi-threshold parameter should return the index of the first threshold that is met,
        // but with ratchet enabled, it should not decrease the value once it has been set.
        // In this case, the thresholds are 0.75 and 0.5,
        let expected_values: Array1<u64> = vec![0, 0, 0, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2].into();

        let expected_values: Array2<u64> = expected_values.insert_axis(Axis(1));

        run_and_assert_parameter_u64(&mut model, Box::new(parameter), expected_values);
    }
}
