use crate::parameters::errors::SimpleCalculationError;
use crate::parameters::{Parameter, ParameterMeta, ParameterName, ParameterState, SimpleParameter};
use crate::scenario::ScenarioIndex;
use crate::state::SimpleParameterValues;
use crate::timestep::Timestep;
use chrono::{Datelike, NaiveDateTime, Timelike};

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
    pub fn new(name: ParameterName, values: [f64; 12], interp_day: Option<MonthlyInterpDay>) -> Self {
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
        first_value
            + (last_value - first_value) * (date.day() as f64 + date.num_seconds_from_midnight() as f64 / 86400.0 - 1.0)
                / days_in_month as f64
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
        first_value
            + (last_value - first_value) * (date.day() as f64 + date.num_seconds_from_midnight() as f64 / 86400.0)
                / days_in_month as f64
    }
}

impl Parameter for MonthlyProfileParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}
impl SimpleParameter<f64> for MonthlyProfileParameter {
    fn compute(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _values: &SimpleParameterValues,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, SimpleCalculationError> {
        let v = match &self.interp_day {
            Some(interp_day) => match interp_day {
                MonthlyInterpDay::First => {
                    let next_month0 = (timestep.date.month0() + 1) % 12;
                    let first_value = self.values[timestep.date.month0() as usize];
                    let last_value = self.values[next_month0 as usize];

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

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}
