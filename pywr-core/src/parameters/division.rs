use super::{
    BuiltParameter, GeneralBeforeParameter, GeneralAfterParameter, GeneralParameterContext, GeneralParameterEntry, MaybeBuiltParameter,
    Parameter, ParameterBuildError, ParameterBuilder, ParameterName,
};
use crate::metric::{MetricConsumerPhase, MetricF64, UnresolvedMetricF64};
use crate::network::ResolutionMaps;
use crate::parameters::errors::GeneralCalculationError;
use crate::parameters::{GeneralParameter, ParameterMeta, ParameterState};
use crate::resolve_metric_f64;

#[derive(Debug)]
pub struct DivisionParameter {
    meta: ParameterMeta,
    numerator: MetricF64,
    denominator: MetricF64,
}

impl Parameter for DivisionParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}
impl GeneralParameter for DivisionParameter {
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralBeforeParameter<f64> for DivisionParameter {
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, GeneralCalculationError> {
        let denominator = self.denominator.get_value(ctx.network, ctx.state)?;

        if denominator == 0.0 {
            return Err(GeneralCalculationError::DivisionByZeroError);
        }

        let numerator = self.numerator.get_value(ctx.network, ctx.state)?;
        Ok(numerator / denominator)
    }
}

impl GeneralAfterParameter<f64> for DivisionParameter {
    fn after(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, GeneralCalculationError> {
        let denominator = self.denominator.get_value(ctx.network, ctx.state)?;

        if denominator == 0.0 {
            return Err(GeneralCalculationError::DivisionByZeroError);
        }

        let numerator = self.numerator.get_value(ctx.network, ctx.state)?;
        Ok(numerator / denominator)
    }
}

/// Builder for creating a [`DivisionParameter`].
#[derive(Debug)]
pub struct DivisionParameterBuilder {
    meta: ParameterMeta,
    numerator: UnresolvedMetricF64,
    denominator: UnresolvedMetricF64,
    phase: MetricConsumerPhase,
}

impl DivisionParameterBuilder {
    /// Create a new builder for [`DivisionParameter`] that is evaluated in the "before" phase.
    pub fn before(name: ParameterName, numerator: UnresolvedMetricF64, denominator: UnresolvedMetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            numerator,
            denominator,
            phase: MetricConsumerPhase::Before,
        }
    }

    /// Create a new builder for [`DivisionParameter`] that is evaluated in the "after" phase.
    pub fn after(name: ParameterName, numerator: UnresolvedMetricF64, denominator: UnresolvedMetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            numerator,
            denominator,
            phase: MetricConsumerPhase::After,
        }
    }

    /// Create a new builder for [`DivisionParameter`] that is evaluated in both "before" and "after" phases.
    pub fn both(name: ParameterName, numerator: UnresolvedMetricF64, denominator: UnresolvedMetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            numerator,
            denominator,
            phase: MetricConsumerPhase::Both,
        }
    }
}

impl ParameterBuilder<f64> for DivisionParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {

        let numerator = resolve_metric_f64!(self, self.numerator, resolution_maps, self.phase, "numerator");
        let denominator = resolve_metric_f64!(self, self.denominator, resolution_maps, self.phase, "denominator");

        let p = DivisionParameter {
            meta: self.meta,
            numerator,
            denominator,
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