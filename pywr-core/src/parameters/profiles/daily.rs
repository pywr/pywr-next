use crate::network::ResolutionMaps;
use crate::parameters::errors::SimpleCalculationError;
use crate::parameters::{
    BuiltParameter, MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterMeta,
    ParameterName, ParameterState, SimpleParameter,
};
use crate::scenario::ScenarioIndex;
use crate::state::SimpleParameterValues;
use crate::timestep::Timestep;

pub struct DailyProfileParameter {
    meta: ParameterMeta,
    values: [f64; 366],
}

impl Parameter for DailyProfileParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl SimpleParameter<f64> for DailyProfileParameter {
    fn before(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _values: &SimpleParameterValues,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<f64>, SimpleCalculationError> {
        Ok(Some(self.values[timestep.day_of_year_index()]))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

pub struct DailyProfileParameterBuilder {
    meta: ParameterMeta,
    values: [f64; 366],
}

impl DailyProfileParameterBuilder {
    pub fn new(name: ParameterName, values: [f64; 366]) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            values,
        }
    }
}

impl ParameterBuilder<f64> for DailyProfileParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        _resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        let p = DailyProfileParameter {
            meta: self.meta,
            values: self.values,
        };

        Ok(MaybeBuiltParameter::Built(BuiltParameter::Simple(Box::new(p))))
    }
}
