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

#[derive(Debug)]
pub struct PiecewiseInterpolatedParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    control_curves: Vec<MetricF64>,
    values: Vec<[f64; 2]>,
    maximum: f64,
    minimum: f64,
}

impl Parameter for PiecewiseInterpolatedParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}
impl GeneralParameter for PiecewiseInterpolatedParameter {
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralBeforeParameter<f64> for PiecewiseInterpolatedParameter {
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

        piecewise_interpolated(x, &control_curves, &self.values, self.maximum, self.minimum)
    }
}


impl GeneralAfterParameter<f64> for PiecewiseInterpolatedParameter {
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

        piecewise_interpolated(x, &control_curves, &self.values, self.maximum, self.minimum)
    }
}


fn piecewise_interpolated(x: f64, control_curves: &[f64], values: &[[f64; 2]], maximum: f64, minimum: f64) -> Result<f64, GeneralCalculationError> {
    let mut cc_previous_value = maximum;
    for (idx, &control_curve) in control_curves.iter().enumerate() {
        if x >= control_curve {
            let v = values
                .get(idx)
                .ok_or_else(|| GeneralCalculationError::OutOfBoundsError {
                    axis: 0,
                    index: idx,
                    length: values.len(),
                })?;
            return Ok(interpolate(x, control_curve, cc_previous_value, v[1], v[0]));
        }
        cc_previous_value = control_curve;
    }
    let v = values.last().ok_or_else(|| GeneralCalculationError::OutOfBoundsError {
        axis: 0,
        index: 0,
        length: values.len(),
    })?;
    Ok(interpolate(x, minimum, cc_previous_value, v[1], v[0]))
}

#[derive(Debug)]
pub struct PiecewiseInterpolatedParameterBuilder {
    meta: ParameterMeta,
    metric: UnresolvedMetricF64,
    control_curves: Vec<UnresolvedMetricF64>,
    values: Vec<[f64; 2]>,
    maximum: f64,
    minimum: f64,
    phase: MetricConsumerPhase,
}

impl PiecewiseInterpolatedParameterBuilder {
    /// Create a new builder for [`PiecewiseInterpolatedParameter`] that is evaluated in the "before" phase.
    pub fn before(name: ParameterName, metric: UnresolvedMetricF64, maximum: f64, minimum: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves: Vec::new(),
            values: Vec::new(),
            maximum,
            minimum,
            phase: MetricConsumerPhase::Before,
        }
    }

    /// Create a new builder for [`PiecewiseInterpolatedParameter`] that is evaluated in the "after" phase.
    pub fn after(name: ParameterName, metric: UnresolvedMetricF64, maximum: f64, minimum: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves: Vec::new(),
            values: Vec::new(),
            maximum,
            minimum,
            phase: MetricConsumerPhase::After,
        }
    }

    /// Create a new builder for [`PiecewiseInterpolatedParameter`] that is evaluated in both "before" and "after" phases.
    pub fn both(name: ParameterName, metric: UnresolvedMetricF64, maximum: f64, minimum: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves: Vec::new(),
            values: Vec::new(),
            maximum,
            minimum,
            phase: MetricConsumerPhase::Both,
        }
    }

    /// Add a control curve to the builder. Control curves should be added in descending order,
    /// i.e. the first control curve should be the one with the highest value, and the last control
    /// curve should be the one with the lowest value.
    pub fn control_curve(&mut self, control_curve: UnresolvedMetricF64) -> &mut Self {
        self.control_curves.push(control_curve);
        self
    }

    /// Add a piecewise-pair to the builder.
    pub fn value(&mut self, value: [f64; 2]) -> &mut Self {
        self.values.push(value);
        self
    }
}

impl ParameterBuilder<f64> for PiecewiseInterpolatedParameterBuilder {
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

        let p = PiecewiseInterpolatedParameter {
            meta: self.meta,
            metric,
            control_curves,
            values: self.values,
            maximum: self.maximum,
            minimum: self.minimum,
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
mod test {
    use super::piecewise_interpolated;
    use crate::metric::UnresolvedMetricF64;
    use crate::parameters::PiecewiseInterpolatedParameterBuilder;
    use crate::parameters::array::Array1ParameterBuilder;
    use crate::test_utils::{run_and_assert_parameter, simple_model};
    use ndarray::{Array1, Array2, Axis};

    #[test]
    fn test_piecewise_interpolated() {
        let control_curves = vec![0.8, 0.5];
        let values = vec![[10.0, 1.0], [0.0, -10.0], [-10.0, -100.0]];
        let maximum = 1.0;
        let minimum = 0.0;

        assert_eq!(piecewise_interpolated(0.9, &control_curves, &values, maximum, minimum).unwrap(), 5.5);
        assert_eq!(piecewise_interpolated(0.65, &control_curves, &values, maximum, minimum).unwrap(), -5.0);
        assert_eq!(piecewise_interpolated(0.25, &control_curves, &values, maximum, minimum).unwrap(), -55.0);
    }

    /// Basic functional test of the piecewise interpolation.
    #[test]
    fn test_basic() {
        let mut model_builder = simple_model(1, None);

        // Create an artificial volume series to use for the interpolation test
        let volume = Array1ParameterBuilder::new("test-x".into(), Array1::linspace(1.0, 0.0, 21));
        model_builder.network_builder().parameters().f64(Box::new(volume));

        let mut parameter = PiecewiseInterpolatedParameterBuilder::before(
            "test-parameter".into(),
            UnresolvedMetricF64::new_parameter_before("test-x"), // Interpolate with the parameter based values
            1.0,
            0.0,
        );

        parameter
            .control_curve(0.8.into())
            .control_curve(0.5.into())
            .value([10.0, 1.0])
            .value([0.0, 0.0])
            .value([-1.0, -10.0]);

        let expected_values: Array1<f64> = [
            10.0,                    // full
            1.0 + 9.0 * 0.15 / 0.2,  // 95%
            1.0 + 9.0 * 0.10 / 0.2,  // 90%
            1.0 + 9.0 * 0.05 / 0.2,  // 85%
            1.0,                     // 80%
            0.0,                     // 75%
            0.0,                     // 70%
            0.0,                     // 65%
            0.0,                     // 60%
            0.0,                     // 55%
            0.0,                     // 50%
            -1.0 - 9.0 * 0.05 / 0.5, // 45%
            -1.0 - 9.0 * 0.10 / 0.5, // 40%
            -1.0 - 9.0 * 0.15 / 0.5, // 35%
            -1.0 - 9.0 * 0.20 / 0.5, // 30%
            -1.0 - 9.0 * 0.25 / 0.5, // 25%
            -1.0 - 9.0 * 0.30 / 0.5, // 20%
            -1.0 - 9.0 * 0.35 / 0.5, // 15%
            -1.0 - 9.0 * 0.40 / 0.5, // 10%
            -1.0 - 9.0 * 0.45 / 0.5, // 05%
            -10.0,                   // 00%
        ]
        .to_vec()
        .into();
        let expected_values: Array2<f64> = expected_values.insert_axis(Axis(1));

        run_and_assert_parameter(model_builder, Box::new(parameter), expected_values, None, Some(1e-12));
    }
}
