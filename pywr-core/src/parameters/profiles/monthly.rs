use crate::parameters::errors::SimpleCalculationError;
use crate::parameters::{
    Parameter, ParameterMeta, ParameterName, ParameterSetupError, ParameterState, SimpleParameter, VariableConfig,
    VariableParameter, VariableParameterError, VariableParameterValues, downcast_internal_state_mut,
    downcast_internal_state_ref, downcast_variable_config_ref,
};
use crate::scenario::ScenarioIndex;
use crate::state::SimpleParameterValues;
use crate::timestep::Timestep;
use chrono::{Datelike, NaiveDateTime, Timelike};
use std::any::Any;

#[derive(Copy, Clone)]
pub enum MonthlyInterpDay {
    First,
    Last,
}

// We store this internal value as an Option<f64> so that it can be updated by the variable API
type InternalValue = Option<[f64; 12]>;

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

    /// Return the current value for a given month0 (0-based index).
    ///
    /// If the internal state is None, the value is returned directly. Otherwise, the value is
    /// taken from the internal state.
    fn value_for_month0(&self, month0: usize, internal_state: &Option<Box<dyn ParameterState>>) -> f64 {
        match downcast_internal_state_ref::<InternalValue>(internal_state) {
            Some(value) => value[month0],
            None => self.values[month0],
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
    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, ParameterSetupError> {
        let value: Option<[f64; 12]> = None;
        Ok(Some(Box::new(value)))
    }
    fn as_variable(&self) -> Option<&dyn VariableParameter> {
        Some(self)
    }
}
impl SimpleParameter<f64> for MonthlyProfileParameter {
    fn compute(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _values: &SimpleParameterValues,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, SimpleCalculationError> {
        let v = match &self.interp_day {
            Some(interp_day) => match interp_day {
                MonthlyInterpDay::First => {
                    let next_month0 = (timestep.date.month0() + 1) % 12;
                    let first_value = self.value_for_month0(timestep.date.month0() as usize, internal_state);
                    let last_value = self.value_for_month0(next_month0 as usize, internal_state);

                    interpolate_first(&timestep.date, first_value, last_value)
                }
                MonthlyInterpDay::Last => {
                    let current_month = timestep.date.month();
                    let last_month = if current_month == 1 { 12 } else { current_month - 1 };
                    let first_value = self.value_for_month0(last_month as usize - 1, internal_state);
                    let last_value = self.value_for_month0(timestep.date.month() as usize - 1, internal_state);

                    interpolate_last(&timestep.date, first_value, last_value)
                }
            },
            None => self.value_for_month0(timestep.date.month() as usize - 1, internal_state),
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

impl VariableParameter for MonthlyProfileParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn size(&self, _variable_config: &dyn VariableConfig) -> (usize, usize) {
        (12, 0)
    }

    fn set_variables(
        &self,
        values_f64: &[f64],
        values_u64: &[u64],
        variable_config: &dyn VariableConfig,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), VariableParameterError> {
        let monthly_profile_config = downcast_variable_config_ref::<MonthlyProfileVariableConfig>(variable_config);

        if values_f64.len() != 12 {
            return Err(VariableParameterError::IncorrectNumberOfValues {
                expected: 12,
                received: values_f64.len(),
            });
        }

        if !values_u64.is_empty() {
            return Err(VariableParameterError::IncorrectNumberOfValues {
                expected: 0,
                received: values_u64.len(),
            });
        }

        let value = downcast_internal_state_mut::<InternalValue>(internal_state);

        let new_values: [f64; 12] = (0..12)
            .map(|i| {
                values_f64[i].clamp(
                    monthly_profile_config.lower_bounds[i],
                    monthly_profile_config.upper_bounds[i],
                )
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();

        *value = Some(new_values);

        Ok(())
    }

    fn get_variables(&self, internal_state: &Option<Box<dyn ParameterState>>) -> Option<VariableParameterValues> {
        downcast_internal_state_ref::<InternalValue>(internal_state)
            .as_ref()
            .map(|values| VariableParameterValues {
                f64: values.to_vec(),
                u64: vec![],
            })
    }

    fn get_lower_bounds(&self, variable_config: &dyn VariableConfig) -> Option<VariableParameterValues> {
        let monthly_profile_config = downcast_variable_config_ref::<MonthlyProfileVariableConfig>(variable_config);
        Some(VariableParameterValues {
            f64: monthly_profile_config.lower_bounds.to_vec(),
            u64: vec![],
        })
    }

    fn get_upper_bounds(&self, variable_config: &dyn VariableConfig) -> Option<VariableParameterValues> {
        let monthly_profile_config = downcast_variable_config_ref::<MonthlyProfileVariableConfig>(variable_config);
        Some(VariableParameterValues {
            f64: monthly_profile_config.upper_bounds.to_vec(),
            u64: vec![],
        })
    }
}

pub struct MonthlyProfileVariableConfig {
    upper_bounds: [f64; 12],
    lower_bounds: [f64; 12],
}

impl MonthlyProfileVariableConfig {
    pub fn new(upper_bounds: [f64; 12], lower_bounds: [f64; 12]) -> Self {
        Self {
            upper_bounds,
            lower_bounds,
        }
    }
}

impl VariableConfig for MonthlyProfileVariableConfig {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
