use crate::model::Model;
use crate::parameters::{ParameterMeta, _Parameter};
use crate::scenario::ScenarioIndex;
use crate::state::ParameterState;
use crate::timestep::Timestep;
use crate::{NetworkState, PywrError};

pub struct MonthlyProfileParameter {
    meta: ParameterMeta,
    values: [f64; 12],
}

impl MonthlyProfileParameter {
    pub fn new(name: &str, values: [f64; 12]) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            values,
        }
    }
}

impl _Parameter for MonthlyProfileParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &mut self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _model: &Model,
        _state: &NetworkState,
        _parameter_state: &ParameterState,
    ) -> Result<f64, PywrError> {
        Ok(self.values[timestep.date.month() as usize])
    }
}
