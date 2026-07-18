use crate::metric::{MetricF64, UnresolvedMetricF64};
use crate::network::ResolutionMaps;
use crate::parameters::errors::GeneralCalculationError;
use crate::parameters::interpolate::interpolate;
use crate::parameters::{
    BuiltParameter, GeneralBeforeParameter, GeneralParameter, GeneralParameterContext, GeneralParameterEntry,
    MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterMeta, ParameterName,
    ParameterState,
};
use crate::{resolve_metric_f64, resolve_metric_f64_vec};

#[derive(Debug)]
pub struct ControlCurveInterpolatedParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    control_curves: Vec<MetricF64>,
    values: Vec<MetricF64>,
}

impl Parameter for ControlCurveInterpolatedParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter for ControlCurveInterpolatedParameter {
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralBeforeParameter<f64> for ControlCurveInterpolatedParameter {
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, GeneralCalculationError> {
        // Current value
        let x = self.metric.get_value(ctx.network, ctx.state)?;

        let mut cc_prev = 1.0;
        for (idx, control_curve) in self.control_curves.iter().enumerate() {
            let cc_value = control_curve.get_value(ctx.network, ctx.state)?;

            if x >= cc_value {
                let lower_value = self.values[idx + 1].get_value(ctx.network, ctx.state)?;
                let upper_value = self.values[idx].get_value(ctx.network, ctx.state)?;

                return Ok(interpolate(x, cc_value, cc_prev, lower_value, upper_value));
            }

            cc_prev = cc_value
        }

        let cc_value = 0.0;
        let n = self.values.len();

        let lower_value = self.values[n - 1].get_value(ctx.network, ctx.state)?;
        let upper_value = self.values[n - 2].get_value(ctx.network, ctx.state)?;

        Ok(interpolate(x, cc_value, cc_prev, lower_value, upper_value))
    }
}

#[derive(Debug)]
pub struct ControlCurveInterpolatedParameterBuilder {
    meta: ParameterMeta,
    metric: UnresolvedMetricF64,
    control_curves: Vec<UnresolvedMetricF64>,
    values: Vec<UnresolvedMetricF64>,
}

impl ControlCurveInterpolatedParameterBuilder {
    pub fn new(name: ParameterName, metric: UnresolvedMetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves: Vec::new(),
            values: Vec::new(),
        }
    }

    pub fn control_curve(&mut self, control_curve: UnresolvedMetricF64) -> &mut Self {
        self.control_curves.push(control_curve);
        self
    }

    pub fn value(&mut self, value: UnresolvedMetricF64) -> &mut Self {
        self.values.push(value);
        self
    }
}

impl ParameterBuilder<f64> for ControlCurveInterpolatedParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        let metric = resolve_metric_f64!(self, self.metric, resolution_maps, "metric");
        let control_curves = resolve_metric_f64_vec!(self, &self.control_curves, resolution_maps, "control_curves");
        let values = resolve_metric_f64_vec!(self, &self.values, resolution_maps, "values");

        let p = ControlCurveInterpolatedParameter {
            meta: self.meta,
            metric,
            control_curves,
            values,
        };

        let bp = BuiltParameter::General(GeneralParameterEntry::before(p));
        Ok(bp.into())
    }
}
