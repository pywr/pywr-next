use crate::network::Network;
use crate::parameters::{Parameter, ParameterIndex, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;
use time::util::days_in_year_month;
use time::Date;

#[derive(Copy, Clone)]
pub enum MonthlyInterpDay {
    First,
    Last,
}

pub struct MonthlyProfileParameter {
    meta: ParameterMeta,
    values: [f64; 12],
    interp_day: Option<MonthlyInterpDay>,
}

impl MonthlyProfileParameter {
    pub fn new(name: &str, values: [f64; 12], interp_day: Option<MonthlyInterpDay>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            values,
            interp_day,
        }
    }
}

/// Interpolate between first_value and last value based on the day of the month. The last
/// value is assumed to correspond to the first day of the next month.
fn interpolate_first(date: &Date, first_value: f64, last_value: f64) -> f64 {
    let days_in_month = days_in_year_month(date.year(), date.month());

    if date.day() <= 1 {
        first_value
    } else if date.day() > days_in_month {
        last_value
    } else {
        first_value + (last_value - first_value) * (date.day() - 1) as f64 / days_in_month as f64
    }
}

/// Interpolate between first_value and last value based on the day of the month. The first
/// value is assumed to correspond to the last day of the previous month.
fn interpolate_last(date: &Date, first_value: f64, last_value: f64) -> f64 {
    let days_in_month = days_in_year_month(date.year(), date.month());

    if date.day() < 1 {
        first_value
    } else if date.day() >= days_in_month {
        last_value
    } else {
        first_value + (last_value - first_value) * date.day() as f64 / days_in_month as f64
    }
}

impl Parameter for MonthlyProfileParameter {
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
        _model: &Network,
        _state: &State,
        _internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<f64, PywrError> {
        let v = match &self.interp_day {
            Some(interp_day) => match interp_day {
                MonthlyInterpDay::First => {
                    let first_value = self.values[timestep.date.month() as usize - 1];
                    let last_value = self.values[timestep.date.month().next() as usize - 1];

                    interpolate_first(&timestep.date, first_value, last_value)
                }
                MonthlyInterpDay::Last => {
                    let first_value = self.values[timestep.date.month().previous() as usize - 1];
                    let last_value = self.values[timestep.date.month() as usize - 1];

                    interpolate_last(&timestep.date, first_value, last_value)
                }
            },
            None => self.values[timestep.date.month() as usize - 1],
        };
        Ok(v)
    }
}

// TODO this is a proof-of-concept of a external "variable"
#[allow(dead_code)]
pub struct MonthlyProfileVariable {
    index: ParameterIndex,
}

#[allow(dead_code)]
impl MonthlyProfileVariable {
    fn update(&self, model: &mut Network, new_values: &[f64]) {
        let p = model.get_mut_parameter(&self.index).unwrap();

        let profile = p.as_any_mut().downcast_mut::<MonthlyProfileParameter>().unwrap();

        // This panics if the slices are different lengths!
        profile.values.copy_from_slice(new_values);
    }
}
