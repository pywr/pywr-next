use crate::metric::{MetricF64, UnresolvedMetricF64};
use crate::network::{Network, ResolutionMaps};
use crate::parameters::errors::GeneralCalculationError;
use crate::parameters::{
    BuiltParameter, GeneralParameter, MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder,
    ParameterMeta, ParameterName, ParameterState,
};
use crate::resolve_metric_f64;
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;

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

impl GeneralParameter<f64> for Polynomial1DParameter {
    fn before(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<f64>, GeneralCalculationError> {
        // Current value
        let x = self.metric.get_value(model, state)?;
        let x = x * self.scale + self.offset;
        // Calculate the polynomial value
        let y = self
            .coefficients
            .iter()
            .enumerate()
            .fold(0.0, |y, (i, c)| y + c * x.powi(i as i32));
        Ok(Some(y))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

#[derive(Debug)]
pub struct Polynomial1DParameterBuilder {
    meta: ParameterMeta,
    metric: UnresolvedMetricF64,
    coefficients: Vec<f64>,
    scale: f64,
    offset: f64,
}

impl Polynomial1DParameterBuilder {
    pub fn new(name: ParameterName, metric: UnresolvedMetricF64, coefficients: Vec<f64>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            coefficients,
            scale: 1.0,
            offset: 0.0,
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
        let metric = resolve_metric_f64!(self, self.metric, resolution_maps, "metric");

        let p = Polynomial1DParameter {
            meta: self.meta,
            metric,
            coefficients: self.coefficients,
            scale: self.scale,
            offset: self.offset,
        };

        Ok(MaybeBuiltParameter::Built(BuiltParameter::General(Box::new(p))))
    }
}
