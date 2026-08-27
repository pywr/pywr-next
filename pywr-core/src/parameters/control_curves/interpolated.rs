use crate::metric::{MetricConsumerPhase, MetricF64, UnresolvedMetricF64};
use crate::network::ResolutionMaps;
use crate::parameters::errors::GeneralCalculationError;
use crate::parameters::interpolate::interpolate;
use crate::parameters::{
    BuiltParameter, GeneralBeforeParameter, GeneralAfterParameter, GeneralParameter, GeneralParameterContext, GeneralParameterEntry,
    MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterMeta, ParameterName,
    ParameterState,
};
use crate::{resolve_metric_f64, resolve_metric_f64_vec};


/// A control curve parameter that interpolates between three or more values.
///
/// Return values are linearly interpolated between the control curves, with the first and last
/// value being 100% and 0% respectively.
///
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
        let control_curves = self
            .control_curves
            .iter()
            .map(|cc| cc.get_value(ctx.network, ctx.state))
            .collect::<Result<Vec<_>, _>>()?;
        let values = self
            .values
            .iter()
            .map(|v| v.get_value(ctx.network, ctx.state))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(control_curve_interpolated(x, &control_curves, &values))
    }
}


impl GeneralAfterParameter<f64> for ControlCurveInterpolatedParameter {
    fn after(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, GeneralCalculationError> {
        // Current value
        let x = self.metric.get_value(ctx.network, ctx.state)?;
        let control_curves = self
            .control_curves
            .iter()
            .map(|cc| cc.get_value(ctx.network, ctx.state))
            .collect::<Result<Vec<_>, _>>()?;
        let values = self
            .values
            .iter()
            .map(|v| v.get_value(ctx.network, ctx.state))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(control_curve_interpolated(x, &control_curves, &values))
    }
}


fn control_curve_interpolated(x: f64, control_curves: &[f64], values: &[f64]) -> f64 {
    let mut cc_prev = 1.0;

    for (idx, &cc_value) in control_curves.iter().enumerate() {
        if x >= cc_value {
            let lower_value = values[idx + 1];
            let upper_value = values[idx];

            return interpolate(x, cc_value, cc_prev, lower_value, upper_value);
        }

        cc_prev = cc_value;
    }

    let cc_value = 0.0;
    let n = values.len();

    let lower_value = values[n - 1];
    let upper_value = values[n - 2];

    interpolate(x, cc_value, cc_prev, lower_value, upper_value)
}


#[derive(Debug)]
pub struct ControlCurveInterpolatedParameterBuilder {
    meta: ParameterMeta,
    metric: UnresolvedMetricF64,
    control_curves: Vec<UnresolvedMetricF64>,
    values: Vec<UnresolvedMetricF64>,
    phase: MetricConsumerPhase,
}

impl ControlCurveInterpolatedParameterBuilder {
    /// Create a new builder for [`ControlCurveInterpolatedParameter`] that is evaluated in the "before" phase.
    pub fn before(name: ParameterName, metric: UnresolvedMetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves: Vec::new(),
            values: Vec::new(),
            phase: MetricConsumerPhase::Before,
        }
    }

    /// Create a new builder for [`ControlCurveInterpolatedParameter`] that is evaluated in the "after" phase.
    pub fn after(name: ParameterName, metric: UnresolvedMetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves: Vec::new(),
            values: Vec::new(),
            phase: MetricConsumerPhase::After,
        }
    }

    /// Create a new builder for [`ControlCurveInterpolatedParameter`] that is evaluated in "before" and "after" phases.
    pub fn both(name: ParameterName, metric: UnresolvedMetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves: Vec::new(),
            values: Vec::new(),
            phase: MetricConsumerPhase::Both,
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
        let metric = resolve_metric_f64!(self, self.metric, resolution_maps, self.phase, "metric");
        let control_curves =
            resolve_metric_f64_vec!(self, &self.control_curves, resolution_maps, self.phase, "control_curves");
        let values = resolve_metric_f64_vec!(self, &self.values, resolution_maps, self.phase, "values");

        let p = ControlCurveInterpolatedParameter {
            meta: self.meta,
            metric,
            control_curves,
            values,
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
    use super::control_curve_interpolated;

    #[test]
    fn test_control_curve_interpolated() {
        let control_curves = vec![0.8, 0.5, 0.2];
        let values = vec![10.0, 20.0, 30.0, 40.0, 50.0];

        assert_eq!(control_curve_interpolated(0.9, &control_curves, &values), 15.0);
        assert_eq!(control_curve_interpolated(0.8, &control_curves, &values), 20.0);
        assert_eq!(control_curve_interpolated(0.65, &control_curves, &values), 25.0);
        assert_eq!(control_curve_interpolated(0.5, &control_curves, &values), 30.0);
        assert_eq!(control_curve_interpolated(0.35, &control_curves, &values), 35.0);
        assert_eq!(control_curve_interpolated(0.1, &control_curves, &values), 45.0);

    }
}