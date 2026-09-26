use crate::metric::{MetricConsumerPhase, MetricF64, UnresolvedMetricF64};
use crate::network::ResolutionMaps;
use crate::parameters::errors::GeneralCalculationError;
use crate::parameters::interpolate::interpolate;
use crate::parameters::{
    BuiltParameter, GeneralAfterParameter, GeneralBeforeParameter, GeneralParameter, GeneralParameterContext,
    GeneralParameterEntry, MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterMeta,
    ParameterName, ParameterState,
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
            .map(|cc| cc.get_value(ctx.network, ctx.state));

        let values = self.values.windows(2).map(|w| {
            let v0 = w[0].get_value(ctx.network, ctx.state)?;
            let v1 = w[1].get_value(ctx.network, ctx.state)?;
            Ok((v0, v1))
        });

        control_curve_interpolated(x, control_curves, values)
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
            .map(|cc| cc.get_value(ctx.network, ctx.state));

        let values = self.values.windows(2).map(|w| {
            let v0 = w[0].get_value(ctx.network, ctx.state)?;
            let v1 = w[1].get_value(ctx.network, ctx.state)?;
            Ok((v0, v1))
        });

        control_curve_interpolated(x, control_curves, values)
    }
}

/// Interpolate between control curves and values
fn control_curve_interpolated<E>(
    x: f64,
    control_curves: impl IntoIterator<Item = Result<f64, E>>,
    values: impl IntoIterator<Item = Result<(f64, f64), E>>,
) -> Result<f64, GeneralCalculationError>
where
    GeneralCalculationError: From<E>,
{
    let mut cc_prev = 1.0;

    let mut index = 0;
    for control_curve in control_curves {
        let cc_value = control_curve?;
        if x >= cc_value {
            let (upper_value, lower_value) = upper_lower(index, values)?;
            return Ok(interpolate(x, cc_value, cc_prev, lower_value, upper_value));
        }

        cc_prev = cc_value;
        index += 1;
    }

    let cc_value = 0.0;

    let (upper_value, lower_value) = upper_lower(index, values)?;

    Ok(interpolate(x, cc_value, cc_prev, lower_value, upper_value))
}

fn upper_lower<E>(
    index: usize,
    values: impl IntoIterator<Item = Result<(f64, f64), E>>,
) -> Result<(f64, f64), GeneralCalculationError>
where
    GeneralCalculationError: From<E>,
{
    let mut length = 0;
    for value in values.into_iter() {
        if length == index {
            return Ok(value?);
        }
        length += 1;
    }

    Err(GeneralCalculationError::OutOfBoundsError { axis: 0, index, length })
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
        let control_curves = resolve_metric_f64_vec!(
            self,
            &self.control_curves,
            resolution_maps,
            self.phase,
            "control_curves"
        );
        let values = resolve_metric_f64_vec!(self, &self.values, resolution_maps, self.phase, "values");

        if values.len() != control_curves.len() + 2 {
            return Err(ParameterBuildError::ControlCurveValuesInterpLengthMismatch {
                values: values.len(),
                control_curves: control_curves.len(),
            });
        }

        let p = ControlCurveInterpolatedParameter {
            meta: self.meta,
            metric,
            control_curves,
            values,
        };

        let built = match self.phase {
            MetricConsumerPhase::Before => BuiltParameter::General(GeneralParameterEntry::before(p)),
            MetricConsumerPhase::After => BuiltParameter::General(GeneralParameterEntry::after(p)),
            MetricConsumerPhase::Both => BuiltParameter::General(GeneralParameterEntry::both(p)),
        };

        Ok(built.into())
    }
}

#[cfg(test)]
mod tests {
    use super::control_curve_interpolated;
    use crate::parameters::GeneralCalculationError;

    #[test]
    fn test_control_curve_interpolated() {
        let control_curves = [0.8, 0.5, 0.2];
        let values = [10.0, 20.0, 30.0, 40.0, 50.0];

        let value = |x| {
            control_curve_interpolated(
                x,
                control_curves.iter().copied().map(Ok::<_, GeneralCalculationError>),
                values
                    .windows(2)
                    .map(|w| Ok::<_, GeneralCalculationError>((w[0], w[1]))),
            )
            .unwrap()
        };

        assert_eq!(value(0.9), 15.0);
        assert_eq!(value(0.8), 20.0);
        assert_eq!(value(0.65), 25.0);
        assert_eq!(value(0.5), 30.0);
        assert_eq!(value(0.35), 35.0);
        assert_eq!(value(0.1), 45.0);
    }

    #[test]
    fn test_control_curve_empty_interpolated() {
        let control_curves = [];
        let values = [10.0, 20.0];

        let value = |x| {
            control_curve_interpolated(
                x,
                control_curves.iter().copied().map(Ok::<_, GeneralCalculationError>),
                values
                    .windows(2)
                    .map(|w| Ok::<_, GeneralCalculationError>((w[0], w[1]))),
            )
            .unwrap()
        };

        assert_eq!(value(0.9), 11.0);
        assert_eq!(value(0.8), 12.0);
        assert_eq!(value(0.5), 15.0);
        assert_eq!(value(0.1), 19.0);
    }
}
