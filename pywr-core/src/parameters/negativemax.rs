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
pub struct NegativeMaxParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    threshold: f64,
}

impl Parameter for NegativeMaxParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}
impl GeneralParameter<f64> for NegativeMaxParameter {
    fn before(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<f64>, GeneralCalculationError> {
        let x = -self.metric.get_value(network, state)?;
        Ok(Some(x.max(self.threshold)))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

#[derive(Debug)]
pub struct NegativeMaxParameterBuilder {
    meta: ParameterMeta,
    metric: UnresolvedMetricF64,
    threshold: f64,
}

impl NegativeMaxParameterBuilder {
    pub fn new(name: ParameterName, metric: UnresolvedMetricF64, threshold: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            threshold,
        }
    }
}

impl ParameterBuilder<f64> for NegativeMaxParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        let metric = resolve_metric_f64!(self, self.metric, resolution_maps, "metric");

        let p = NegativeMaxParameter {
            meta: self.meta,
            metric,
            threshold: self.threshold,
        };

        Ok(MaybeBuiltParameter::Built(BuiltParameter::General(Box::new(p))))
    }
}
