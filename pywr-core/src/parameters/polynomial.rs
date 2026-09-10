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
pub struct Polynomial1DParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    coefficients: Vec<f64>,
    scale: f64,
    offset: f64,
}

impl Parameter for Polynomial1DParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter for Polynomial1DParameter {
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralBeforeParameter<f64> for Polynomial1DParameter {
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, GeneralCalculationError> {
        // Current value
        let x = self.metric.get_value(ctx.network, ctx.state)?;
        let x = x * self.scale + self.offset;
        // Calculate the polynomial value
        Ok(polynomial(x, self.coefficients.clone()))
    }
}

impl GeneralAfterParameter<f64> for Polynomial1DParameter {
    fn after(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, GeneralCalculationError> {
        // Current value
        let x = self.metric.get_value(ctx.network, ctx.state)?;
        let x = x * self.scale + self.offset;
        // Calculate the polynomial value
        Ok(polynomial(x, self.coefficients.clone()))
    }
}


fn polynomial(x: f64, coefficients: Vec<f64>) -> f64 {

    coefficients
        .iter()
        .enumerate()
        .fold(0.0, |y, (i, c)| y + c * x.powi(i as i32))
}



/// Builder for creating a [`Polynomial1DParameter`].
#[derive(Debug)]
pub struct Polynomial1DParameterBuilder {
    meta: ParameterMeta,
    metric: UnresolvedMetricF64,
    coefficients: Vec<f64>,
    scale: f64,
    offset: f64,
    phase: MetricConsumerPhase,
}

impl Polynomial1DParameterBuilder {
    /// Create a new builder for [`Polynomial1DParameter`] that is evaluated in the "before" phase.
    pub fn before(name: ParameterName, metric: UnresolvedMetricF64, coefficients: Vec<f64>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            coefficients,
            scale: 1.0,
            offset: 0.0,
            phase: MetricConsumerPhase::Before,
        }
    }

    /// Create a new builder for [`Polynomial1DParameter`] that is evaluated in the "after" phase.
    pub fn after(name: ParameterName, metric: UnresolvedMetricF64, coefficients: Vec<f64>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            coefficients,
            scale: 1.0,
            offset: 0.0,
            phase: MetricConsumerPhase::After,
        }
    }

    /// Create a new builder for [`Polynomial1DParameter`] that is evaluated in "before" and "after" phases.
    pub fn both(name: ParameterName, metric: UnresolvedMetricF64, coefficients: Vec<f64>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            coefficients,
            scale: 1.0,
            offset: 0.0,
            phase: MetricConsumerPhase::Both,
        }
    }

    pub fn scale(&mut self, scale: f64) -> &mut Self {
        self.scale = scale;
        self
    }

    pub fn offset(&mut self, offset: f64) -> &mut Self {
        self.offset = offset;
        self
    }
}

impl ParameterBuilder<f64> for Polynomial1DParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {

        let metric = resolve_metric_f64!(self, self.metric, resolution_maps, self.phase, "metric");

        let p = Polynomial1DParameter {
            meta: self.meta,
            metric,
            coefficients: self.coefficients,
            scale: self.scale,
            offset: self.offset,
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
