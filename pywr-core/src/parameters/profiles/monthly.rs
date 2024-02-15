use crate::network::Network;
use crate::parameters::{Parameter, ParameterIndex, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{ParameterState, State};
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;

use chrono::{Datelike, NaiveDateTime};

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

fn days_in_year_month(datetime: &NaiveDateTime) -> u32 {
    match datetime.month() {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if datetime.date().leap_year() => 29,
        2 => 28,
        _ => panic!("Invalid month"),
    }
}

/// Interpolate between first_value and last value based on the day of the month. The last
/// value is assumed to correspond to the first day of the next month.
fn interpolate_first(date: &NaiveDateTime, first_value: f64, last_value: f64) -> f64 {
    let days_in_month = days_in_year_month(date);

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
fn interpolate_last(date: &NaiveDateTime, first_value: f64, last_value: f64) -> f64 {
    let days_in_month = days_in_year_month(date);

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
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, PywrError> {
        let v = match &self.interp_day {
            Some(interp_day) => match interp_day {
                MonthlyInterpDay::First => {
                    let next_month = (timestep.date.month() % 12) + 1;
                    let first_value = self.values[timestep.date.month() as usize - 1];
                    let last_value = self.values[next_month as usize - 1];

                    interpolate_first(&timestep.date, first_value, last_value)
                }
                MonthlyInterpDay::Last => {
                    let current_month = timestep.date.month();
                    let last_month = if current_month == 1 { 12 } else { current_month - 1 };
                    let first_value = self.values[last_month as usize - 1];
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
