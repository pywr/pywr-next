use crate::model::Model;
use crate::parameters::{Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;

pub struct WeeklyProfileParameter {
    meta: ParameterMeta,
    values: Vec<f64>,
}

impl WeeklyProfileParameter {
    pub fn new(name: &str, values: Vec<f64>) -> Self {
        if values.len() != 52 || values.len() != 53 {
            panic!("52 or 53 values must be given for the weekly profile named {name}");
        }

        Self {
            meta: ParameterMeta::new(name),
            values,
        }
    }
}

impl Parameter for WeeklyProfileParameter {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _model: &Model,
        _state: &State,
        _internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<f64, PywrError> {
        let current_day = timestep.date.ordinal();
        let current_day_index = current_day - 1;

        let week_index: u16;
        if current_day >= 364 {
            if self.values.len() == 52 {
                week_index = 51
            } else {
                week_index = 52
            }
        } else {
            week_index = current_day_index / 7
        }

        Ok(self.values[week_index as usize])
    }
}
