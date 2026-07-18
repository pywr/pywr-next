use crate::metric::{MetricF64, UnresolvedMetricF64};
use crate::network::ResolutionMaps;
use crate::parameters::errors::{GeneralCalculationError, ParameterSetupError};
use crate::parameters::{
    BuiltParameter, GeneralBeforeParameter, GeneralParameter, GeneralParameterContext, GeneralParameterEntry,
    MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterMeta, ParameterName,
    ParameterState, Predicate, downcast_internal_state_mut,
};
use crate::scenario::ScenarioIndex;
use crate::timestep::Timestep;
use crate::{resolve_metric_f64, resolve_metric_f64_vec};

#[derive(Debug)]
pub struct MultiThresholdParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    thresholds: Vec<MetricF64>,
    predicate: Predicate,
    ratchet: bool,
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

impl GeneralParameter for MultiThresholdParameter {
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralBeforeParameter<u64> for MultiThresholdParameter {
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, GeneralCalculationError> {
        // Downcast the internal state to the correct type
        let previous_max = downcast_internal_state_mut::<u64>(internal_state);

        let value = self.metric.get_value(ctx.network, ctx.state)?;

        // Determine the first threshold that is met
        let mut position: u64 = 0;
        for threshold in &self.thresholds {
            let t = threshold.get_value(ctx.network, ctx.state)?;
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
}

#[derive(Debug)]
pub struct MultiThresholdParameterBuilder {
    meta: ParameterMeta,
    metric: UnresolvedMetricF64,
    thresholds: Vec<UnresolvedMetricF64>,
    predicate: Predicate,
    ratchet: bool,
}

impl MultiThresholdParameterBuilder {
    pub fn new(name: ParameterName, metric: UnresolvedMetricF64, predicate: Predicate) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            thresholds: Vec::new(),
            predicate,
            ratchet: false,
        }
    }

    /// Enable ratchet
    pub fn ratchet(&mut self) -> &mut Self {
        self.ratchet = true;
        self
    }

    /// Add a threshold to the builder. The thresholds should be added in order.
    pub fn threshold(&mut self, threshold: UnresolvedMetricF64) -> &mut Self {
        self.thresholds.push(threshold);
        self
    }
}

impl ParameterBuilder<u64> for MultiThresholdParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<u64>, ParameterBuildError> {
        let metric = resolve_metric_f64!(self, self.metric, resolution_maps, "metric");
        let thresholds = resolve_metric_f64_vec!(self, &self.thresholds, resolution_maps, "thresholds");

        let p = MultiThresholdParameter {
            meta: self.meta,
            metric,
            thresholds,
            predicate: self.predicate,
            ratchet: self.ratchet,
        };

        let bp = BuiltParameter::General(GeneralParameterEntry::before(p));
        Ok(bp.into())
    }
}

#[cfg(test)]
mod tests {
    use super::MultiThresholdParameterBuilder;
    use crate::metric::UnresolvedMetricF64;
    use crate::parameters::{Array1ParameterBuilder, Predicate};
    use crate::test_utils::{run_and_assert_parameter_u64, simple_model};
    use ndarray::{Array1, Array2, Axis, concatenate};

    /// Basic functional test of the `MultiThresholdParameter` parameter.
    #[test]
    fn test_multi_threshold() {
        let mut model_builder = simple_model(1, None);

        // Create an artificial volume series to use for the delay test
        let v1 = Array1::linspace(1.0, 0.0, 11);
        let v2 = Array1::linspace(0.1, 1.0, 10);

        let volumes = concatenate![Axis(0), v1, v2];

        let volume = Array1ParameterBuilder::new("test-x".into(), volumes.clone());

        model_builder.network_builder().parameters().f64(Box::new(volume));

        let mut parameter = MultiThresholdParameterBuilder::new(
            "test-parameter".into(),
            UnresolvedMetricF64::new_parameter_before("test-x"),
            Predicate::GreaterThan,
        );

        parameter.threshold(0.75.into()).threshold(0.5.into());

        // The multi-threshold parameter should return the index of the first threshold that is met.
        // In this case, the thresholds are 0.75 and 0.5,
        let expected_values: Array1<u64> = vec![0, 0, 0, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 1, 1, 0, 0, 0].into();

        let expected_values: Array2<u64> = expected_values.insert_axis(Axis(1));

        run_and_assert_parameter_u64(model_builder, Box::new(parameter), expected_values);
    }

    /// Basic functional test of the `MultiThresholdParameter` parameter with ratchet enabled.
    #[test]
    fn test_multi_threshold_with_ratchet() {
        let mut model_builder = simple_model(1, None);

        // Create an artificial volume series to use for the delay test
        let v1 = Array1::linspace(1.0, 0.0, 11);
        let v2 = Array1::linspace(0.1, 1.0, 10);

        let volumes = concatenate![Axis(0), v1, v2];

        let volume = Array1ParameterBuilder::new("test-x".into(), volumes.clone());

        model_builder.network_builder().parameters().f64(Box::new(volume));

        let mut parameter = MultiThresholdParameterBuilder::new(
            "test-parameter".into(),
            UnresolvedMetricF64::new_parameter_before("test-x"),
            Predicate::GreaterThan,
        );

        parameter.threshold(0.75.into()).threshold(0.5.into()).ratchet();

        // The multi-threshold parameter should return the index of the first threshold that is met,
        // but with ratchet enabled, it should not decrease the value once it has been set.
        // In this case, the thresholds are 0.75 and 0.5,
        let expected_values: Array1<u64> = vec![0, 0, 0, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2].into();

        let expected_values: Array2<u64> = expected_values.insert_axis(Axis(1));

        run_and_assert_parameter_u64(model_builder, Box::new(parameter), expected_values);
    }
}
