use crate::metric::{MetricConsumerPhase, MetricF64, UnresolvedMetricF64};
use crate::network::ResolutionMaps;
use crate::parameters::errors::GeneralCalculationError;
use crate::parameters::{
    BuiltParameter, GeneralBeforeParameter, GeneralParameter, GeneralParameterContext, GeneralParameterEntry,
    MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterMeta, ParameterName,
    ParameterState,
};
use crate::resolve_metric_f64;

#[derive(Debug)]
pub struct MinParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    threshold: f64,
}

impl Parameter for MinParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}
impl GeneralParameter for MinParameter {
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralBeforeParameter<f64> for MinParameter {
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, GeneralCalculationError> {
        let x = self.metric.get_value(ctx.network, ctx.state)?;
        Ok(x.min(self.threshold))
    }
}

#[derive(Debug)]
pub struct MinParameterBuilder {
    meta: ParameterMeta,
    metric: UnresolvedMetricF64,
    threshold: f64,
}

impl MinParameterBuilder {
    pub fn new(name: ParameterName, metric: UnresolvedMetricF64, threshold: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            threshold,
        }
    }
}

impl ParameterBuilder<f64> for MinParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        // Phase is hardcoded to "before" for this parameter, as it only implements the `GeneralBeforeParameter` trait.
        let phase = MetricConsumerPhase::Before;
        let metric = resolve_metric_f64!(self, self.metric, resolution_maps, phase, "metric");

        let p = MinParameter {
            meta: self.meta,
            metric,
            threshold: self.threshold,
        };

        Ok(BuiltParameter::General(GeneralParameterEntry::before(p)).into())
    }
}
