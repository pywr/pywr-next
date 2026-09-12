use crate::metric::{MetricConsumerPhase, MetricF64, UnresolvedMetricF64};
use crate::network::ResolutionMaps;
use crate::parameters::errors::GeneralCalculationError;
use crate::parameters::{
    BuiltParameter, GeneralBeforeParameter, GeneralAfterParameter, GeneralParameter, GeneralParameterContext, GeneralParameterEntry,
    MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterMeta, ParameterName,
    ParameterState,
};
use crate::resolve_metric_f64;
use crate::state::MultiValue;
use std::collections::HashMap;

/// A parameter which apportions a metric to an upper and lower amount based
/// on the current value of a control curve.
///
/// The control curve is expected to produce values between 0.0 and 1.0. If the control curve
/// returns a value outside of this range it is "clamped" to it. The upper amount
/// is equal to `(1.0 - control_curve) * metric` and the lower amount is equal to
/// `control_curve * metric`.
///
#[derive(Debug)]
pub struct ApportionParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    control_curve: MetricF64,
}

impl Parameter for ApportionParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter for ApportionParameter {
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralBeforeParameter<MultiValue> for ApportionParameter {
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<MultiValue, GeneralCalculationError> {
        // Current value
        let x = self.metric.get_value(ctx.network, ctx.state)?;

        // Get the control curve value and force it
        let control_curve = self.control_curve.get_value(ctx.network, ctx.state)?.clamp(0.0, 1.0);

        Ok(apportion(x, control_curve))
    }
}

impl GeneralAfterParameter<MultiValue> for ApportionParameter {
    fn after(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<MultiValue, GeneralCalculationError> {
        // Current value
        let x = self.metric.get_value(ctx.network, ctx.state)?;

        // Get the control curve value and force it
        let control_curve = self.control_curve.get_value(ctx.network, ctx.state)?.clamp(0.0, 1.0);

        Ok(apportion(x, control_curve))
    }
}


fn apportion(x: f64, control_curve: f64) -> MultiValue {
    let upper = (1.0 - control_curve) * x;
    let lower = control_curve * x;

    let values = HashMap::from([("upper".to_string(), upper), ("lower".to_string(), lower)]);
    MultiValue::new(values, HashMap::new())
}

#[derive(Debug)]
pub struct ApportionParameterBuilder {
    meta: ParameterMeta,
    metric: UnresolvedMetricF64,
    control_curve: UnresolvedMetricF64,
    phase: MetricConsumerPhase,
}

impl ApportionParameterBuilder {
    /// Create a new builder for [`ApportionParameter`] that is evaluated in the "before" phase.
    pub fn before(name: ParameterName, metric: UnresolvedMetricF64, control_curve: UnresolvedMetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curve,
            phase: MetricConsumerPhase::Before,
        }
    }

    /// Create a new builder for [`ApportionParameter`] that is evaluated in the "after" phase.
    pub fn after(name: ParameterName, metric: UnresolvedMetricF64, control_curve: UnresolvedMetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curve,
            phase: MetricConsumerPhase::After,
        }
    }

    /// Create a new builder for [`ApportionParameter`] that is evaluated in both "before" and "after" phases.
    pub fn both(name: ParameterName, metric: UnresolvedMetricF64, control_curve: UnresolvedMetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curve,
            phase: MetricConsumerPhase::Both,
        }
    }
}

impl ParameterBuilder<MultiValue> for ApportionParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<MultiValue>, ParameterBuildError> {
        let metric = resolve_metric_f64!(self, self.metric, resolution_maps, self.phase, "metric");
        let control_curve = resolve_metric_f64!(self, self.control_curve, resolution_maps, self.phase, "control_curve");

        let p = ApportionParameter {
            meta: self.meta,
            metric,
            control_curve,
        };

        let built = match self.phase {
            MetricConsumerPhase::Before => {
                BuiltParameter::General(GeneralParameterEntry::before(p))
            },
            MetricConsumerPhase::After => {
                BuiltParameter::General(GeneralParameterEntry::after(p))
            },
            MetricConsumerPhase::Both => {
                BuiltParameter::General(GeneralParameterEntry::both(p))
            }
        };

        Ok(built.into())
    }
}

#[cfg(test)]
mod tests {
    use super::apportion;

    #[test]
    fn test_apportion() {
        let x = 100.0;
        let control_curve = 0.3;
        let result = apportion(x, control_curve);

        assert_eq!(*result.get_value("upper").unwrap(), (1.0 - control_curve) * x);
        assert_eq!(*result.get_value("lower").unwrap(), control_curve * x);
    }
}