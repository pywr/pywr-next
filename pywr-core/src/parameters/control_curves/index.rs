use crate::metric::{MetricF64, UnresolvedMetricF64};
use crate::network::ResolutionMaps;
use crate::parameters::errors::GeneralCalculationError;
use crate::parameters::{
    BuiltParameter, GeneralBeforeParameter, GeneralParameter, GeneralParameterContext, GeneralParameterEntry,
    MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterMeta, ParameterName,
    ParameterState,
};
use crate::{resolve_metric_f64, resolve_metric_f64_vec};

#[derive(Debug)]
pub struct ControlCurveIndexParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    control_curves: Vec<MetricF64>,
}

impl Parameter for ControlCurveIndexParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter for ControlCurveIndexParameter {
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralBeforeParameter<u64> for ControlCurveIndexParameter {
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, GeneralCalculationError> {
        // Current value
        let x = self.metric.get_value(ctx.network, ctx.state)?;

        for (idx, control_curve) in self.control_curves.iter().enumerate() {
            let cc_value = control_curve.get_value(ctx.network, ctx.state)?;
            if x >= cc_value {
                return Ok(idx as u64);
            }
        }
        Ok(self.control_curves.len() as u64)
    }
}

#[derive(Debug)]
pub struct ControlCurveIndexParameterBuilder {
    meta: ParameterMeta,
    metric: UnresolvedMetricF64,
    control_curves: Vec<UnresolvedMetricF64>,
}

impl ControlCurveIndexParameterBuilder {
    pub fn new(name: ParameterName, metric: UnresolvedMetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves: Vec::new(),
        }
    }

    pub fn control_curve(&mut self, cc: UnresolvedMetricF64) -> &mut Self {
        self.control_curves.push(cc);
        self
    }
}

impl ParameterBuilder<u64> for ControlCurveIndexParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<u64>, ParameterBuildError> {
        let metric = resolve_metric_f64!(self, self.metric, resolution_maps, "metric");
        let control_curves = resolve_metric_f64_vec!(self, &self.control_curves, resolution_maps, "control_curves");

        let p = ControlCurveIndexParameter {
            meta: self.meta,
            metric,
            control_curves,
        };

        Ok(BuiltParameter::General(GeneralParameterEntry::before(p)).into())
    }
}
