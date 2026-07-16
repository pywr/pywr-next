use crate::FLOAT_EQ_TOLERANCE;
use crate::metric::{MetricF64, UnresolvedMetricF64};
use crate::network::ResolutionMaps;
use crate::parameters::errors::{GeneralCalculationError, ParameterSetupError};
use crate::parameters::{
    BuiltParameter, GeneralParameter, GeneralParameterContext, MaybeBuiltParameter, Parameter, ParameterBuildError,
    ParameterBuilder, ParameterMeta, ParameterName, ParameterState, downcast_internal_state_mut,
};
use crate::resolve_metric_f64;
use crate::scenario::ScenarioIndex;
use crate::timestep::Timestep;

#[derive(Debug)]
pub enum Predicate {
    LessThan,
    GreaterThan,
    EqualTo,
    LessThanOrEqualTo,
    GreaterThanOrEqualTo,
}

impl Predicate {
    /// Applies the predicate to a `value` against a `threshold`.
    ///
    /// Equality comparison uses an absolute tolerance equal to [`FLOAT_EQ_TOLERANCE`].
    ///
    /// Note: Comparisons involving `NaN` will always return `false`.
    pub fn apply(&self, value: f64, threshold: f64) -> bool {
        match self {
            Predicate::LessThan => value < threshold,
            Predicate::GreaterThan => value > threshold,
            Predicate::EqualTo => (value - threshold).abs() <= FLOAT_EQ_TOLERANCE,
            Predicate::LessThanOrEqualTo => value <= threshold,
            Predicate::GreaterThanOrEqualTo => value >= threshold,
        }
    }
}

#[derive(Debug)]
pub struct ThresholdParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    threshold: MetricF64,
    predicate: Predicate,
    ratchet: bool,
}

impl Parameter for ThresholdParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, ParameterSetupError> {
        // Internal state is just a boolean indicating if the threshold was triggered previously.
        // Initially this is false.
        Ok(Some(Box::new(false)))
    }
}

impl GeneralParameter<u64> for ThresholdParameter {
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<u64>, GeneralCalculationError> {
        // Downcast the internal state to the correct type
        let previously_activated = downcast_internal_state_mut::<bool>(internal_state);

        // Return early if ratchet has been hit
        if self.ratchet & *previously_activated {
            return Ok(Some(1));
        }

        let threshold = self.threshold.get_value(ctx.network, ctx.state)?;
        let value = self.metric.get_value(ctx.network, ctx.state)?;
        let active = self.predicate.apply(value, threshold);

        if active {
            // Update the internal state to remember we've been triggered!
            *previously_activated = true;
            Ok(Some(1))
        } else {
            Ok(Some(0))
        }
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

#[derive(Debug)]
pub struct ThresholdParameterBuilder {
    meta: ParameterMeta,
    metric: UnresolvedMetricF64,
    threshold: UnresolvedMetricF64,
    predicate: Predicate,
    ratchet: bool,
}

impl ThresholdParameterBuilder {
    pub fn new(
        name: ParameterName,
        metric: UnresolvedMetricF64,
        threshold: UnresolvedMetricF64,
        predicate: Predicate,
    ) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            threshold,
            predicate,
            ratchet: false,
        }
    }

    /// Enable ratchet
    pub fn ratchet(&mut self) -> &mut Self {
        self.ratchet = true;
        self
    }
}

impl ParameterBuilder<u64> for ThresholdParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<u64>, ParameterBuildError> {
        let metric = resolve_metric_f64!(self, self.metric, resolution_maps, "metric");
        let threshold = resolve_metric_f64!(self, self.threshold, resolution_maps, "threshold");

        let p = ThresholdParameter {
            meta: self.meta,
            metric,
            threshold,
            predicate: self.predicate,
            ratchet: self.ratchet,
        };

        Ok(MaybeBuiltParameter::Built(BuiltParameter::General(Box::new(p))))
    }
}
