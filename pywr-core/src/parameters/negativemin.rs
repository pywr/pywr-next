use crate::metric::{MetricConsumerPhase, MetricF64, UnresolvedMetricF64};
use crate::network::ResolutionMaps;
use crate::parameters::errors::GeneralCalculationError;
use crate::parameters::{
    BuiltParameter, GeneralBeforeParameter, GeneralAfterParameter, GeneralParameter, GeneralParameterContext, GeneralParameterEntry,
    MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterMeta, ParameterName,
    ParameterState,
};
use crate::resolve_metric_f64;

#[derive(Debug)]
pub struct NegativeMinParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    threshold: f64,
}

impl Parameter for NegativeMinParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}
impl GeneralParameter for NegativeMinParameter {
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralBeforeParameter<f64> for NegativeMinParameter {
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, GeneralCalculationError> {
        let x = -self.metric.get_value(ctx.network, ctx.state)?;
        Ok(x.min(self.threshold))
    }
}

impl GeneralAfterParameter<f64> for NegativeMinParameter {
    fn after(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, GeneralCalculationError> {
        let x = -self.metric.get_value(ctx.network, ctx.state)?;
        Ok(x.min(self.threshold))
    }
}


/// Builder for creating a [`NegativeMinParameter`].
#[derive(Debug)]
pub struct NegativeMinParameterBuilder {
    meta: ParameterMeta,
    metric: UnresolvedMetricF64,
    threshold: f64,
    phase: MetricConsumerPhase,
}

impl NegativeMinParameterBuilder {
    /// Create a new builder for [`NegativeMinParameter`] that is evaluated in "before" phase.
    pub fn before(name: ParameterName, metric: UnresolvedMetricF64, threshold: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            threshold,
            phase: MetricConsumerPhase::Before,
        }
    }

    /// Create a new builder for [`NegativeMinParameter`] that is evaluated in "after" phase.
    pub fn after(name: ParameterName, metric: UnresolvedMetricF64, threshold: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            threshold,
            phase: MetricConsumerPhase::After,
        }
    }

    /// Create a new builder for [`NegativeMinParameter`] that is evaluated in both "before" and "after" phases.
    pub fn both(name: ParameterName, metric: UnresolvedMetricF64, threshold: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            threshold,
            phase: MetricConsumerPhase::Both,
        }
    }
}

impl ParameterBuilder<f64> for NegativeMinParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {

        let metric = resolve_metric_f64!(self, self.metric, resolution_maps, self.phase, "metric");

        let p = NegativeMinParameter {
            meta: self.meta,
            metric,
            threshold: self.threshold,
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
