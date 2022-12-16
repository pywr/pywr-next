use crate::parameters::{Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;

pub struct DailyProfileParameter {
    meta: ParameterMeta,
    values: [f64; 366],
}

impl DailyProfileParameter {
    pub fn new(name: &str, values: [f64; 366]) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            values,
        }
    }
}

impl Parameter for DailyProfileParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _state: &State,
        _internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<f64, PywrError> {
        Ok(self.values[timestep.date.ordinal() as usize - 1])
    }
}
