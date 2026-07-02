use crate::network::ResolutionMaps;
use crate::parameters::errors::SimpleCalculationError;
use crate::parameters::{
    BuiltParameter, MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterMeta,
    ParameterName, ParameterState, SimpleParameter,
};
use crate::scenario::ScenarioIndex;
use crate::state::SimpleParameterValues;
use crate::timestep::Timestep;
use chrono::Timelike;

/// A parameter that defines a profile over a 24-hour period.
///
/// The values array should contain 24 values, one for each hour of the day.
pub struct DiurnalProfileParameter {
    meta: ParameterMeta,
    values: [f64; 24],
}

impl Parameter for DiurnalProfileParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl SimpleParameter<f64> for DiurnalProfileParameter {
    fn before(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _values: &SimpleParameterValues,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<f64>, SimpleCalculationError> {
        Ok(Some(self.values[timestep.date.time().hour() as usize]))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

pub struct DiurnalProfileParameterBuilder {
    meta: ParameterMeta,
    values: [f64; 24],
}

impl DiurnalProfileParameterBuilder {
    pub fn new(name: ParameterName, values: [f64; 24]) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            values,
        }
    }
}

impl ParameterBuilder<f64> for DiurnalProfileParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        _resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        let p = DiurnalProfileParameter {
            meta: self.meta,
            values: self.values,
        };

        Ok(MaybeBuiltParameter::Built(BuiltParameter::Simple(Box::new(p))))
    }
}
