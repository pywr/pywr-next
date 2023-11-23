use crate::model::Model;
use crate::parameters::{Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;

// A weekly profile can be 52 or 53 week long
enum WeeklyProfileSize {
    FiftyTwo([f64; 52]),
    FiftyThree([f64; 53]),
}

impl WeeklyProfileSize {
    // Calculate the value for the current week
    fn value(&self, date: &time::Date) -> f64 {
        let current_day = date.ordinal() as usize;
        let current_day_index = current_day - 1;

        match self {
            Self::FiftyTwo(values) => {
                if current_day >= 364 {
                    values[51]
                } else {
                    values[current_day_index / 7]
                }
            }
            Self::FiftyThree(values) => {
                if current_day >= 364 {
                    values[52]
                } else {
                    values[current_day_index / 7]
                }
            }
        }
    }
}

pub struct WeeklyProfileParameter {
    meta: ParameterMeta,
    values: WeeklyProfileSize,
}

impl WeeklyProfileParameter {
    pub fn new(name: &str, values: WeeklyProfileSize) -> Self {
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
        Ok(self.values.value(&timestep.date))
    }
}
