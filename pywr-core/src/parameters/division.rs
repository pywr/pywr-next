use super::{BuiltParameter, MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterName};
use crate::metric::{MetricF64, UnresolvedMetricF64};
use crate::network::{Network, ResolutionMaps};
use crate::parameters::errors::GeneralCalculationError;
use crate::parameters::{GeneralParameter, ParameterMeta, ParameterState};
use crate::resolve_metric_f64;
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;

#[derive(Debug)]
pub struct DivisionParameter {
    meta: ParameterMeta,
    numerator: MetricF64,
    denominator: MetricF64,
}

impl Parameter for DivisionParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}
impl GeneralParameter<f64> for DivisionParameter {
    fn before(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<f64>, GeneralCalculationError> {
        let denominator = self.denominator.get_value(model, state)?;

        if denominator == 0.0 {
            return Err(GeneralCalculationError::DivisionByZeroError);
        }

        let numerator = self.numerator.get_value(model, state)?;
        Ok(Some(numerator / denominator))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

#[derive(Debug)]
pub struct DivisionParameterBuilder {
    meta: ParameterMeta,
    numerator: UnresolvedMetricF64,
    denominator: UnresolvedMetricF64,
}

impl DivisionParameterBuilder {
    pub fn new(name: ParameterName, numerator: UnresolvedMetricF64, denominator: UnresolvedMetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            numerator,
            denominator,
        }
    }
}

impl ParameterBuilder<f64> for DivisionParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        let numerator = resolve_metric_f64!(self, self.numerator, resolution_maps, "numerator");
        let denominator = resolve_metric_f64!(self, self.denominator, resolution_maps, "denominator");

        let p = DivisionParameter {
            meta: self.meta,
            numerator,
            denominator,
        };

        Ok(MaybeBuiltParameter::Built(BuiltParameter::General(Box::new(p))))
    }
}
