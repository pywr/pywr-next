use crate::metric::{MetricF64, UnresolvedMetricF64};
use crate::network::ResolutionMaps;
use crate::parameters::errors::GeneralCalculationError;
use crate::parameters::interpolate::linear_interpolation;
use crate::parameters::{
    BuiltParameter, GeneralBeforeParameter, GeneralParameter, GeneralParameterContext, GeneralParameterEntry,
    MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterMeta, ParameterName,
    ParameterState,
};
use crate::resolve_metric_f64;

/// A parameter that interpolates a value to a function with given discrete data points.
#[derive(Debug)]
pub struct InterpolatedParameter {
    meta: ParameterMeta,
    x: MetricF64,
    points: Vec<(MetricF64, MetricF64)>,
    error_on_bounds: bool,
}

impl Parameter for InterpolatedParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}
impl GeneralParameter for InterpolatedParameter {
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralBeforeParameter<f64> for InterpolatedParameter {
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, GeneralCalculationError> {
        // Current value
        let x = self.x.get_value(ctx.network, ctx.state)?;

        let points = self
            .points
            .iter()
            .map(|(x, f)| {
                let xp = x.get_value(ctx.network, ctx.state)?;
                let fp = f.get_value(ctx.network, ctx.state)?;

                Ok::<(f64, f64), GeneralCalculationError>((xp, fp))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let f = linear_interpolation(x, &points, self.error_on_bounds)?;

        Ok(f)
    }
}

#[derive(Debug)]
pub struct InterpolatedParameterBuilder {
    meta: ParameterMeta,
    x: UnresolvedMetricF64,
    points: Vec<(UnresolvedMetricF64, UnresolvedMetricF64)>,
    error_on_bounds: bool,
}

impl InterpolatedParameterBuilder {
    pub fn new(
        name: ParameterName,
        x: UnresolvedMetricF64,
        points: Vec<(UnresolvedMetricF64, UnresolvedMetricF64)>,
    ) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            x,
            points,
            error_on_bounds: true,
        }
    }

    pub fn error_on_bounds(&mut self, value: bool) -> &mut Self {
        self.error_on_bounds = value;
        self
    }
}

impl ParameterBuilder<f64> for InterpolatedParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        let x = resolve_metric_f64!(self, self.x, resolution_maps, "x");

        let mut points = Vec::with_capacity(self.points.len());
        for (i, (uxp, ufp)) in self.points.iter().enumerate() {
            let xp = resolve_metric_f64!(self, uxp, resolution_maps, &format!("points[{i}].x"));
            let fp = resolve_metric_f64!(self, ufp, resolution_maps, &format!("points[{i}].f"));
            points.push((xp, fp));
        }

        let p = InterpolatedParameter {
            meta: self.meta,
            x,
            points,
            error_on_bounds: self.error_on_bounds,
        };

        Ok(BuiltParameter::General(GeneralParameterEntry::before(p)).into())
    }
}
