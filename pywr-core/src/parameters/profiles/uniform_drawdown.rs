use crate::network::Network;
use crate::parameters::{Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{ParameterState, State};
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;
use time::{Date, Month};

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0) & ((year % 100 != 0) | (year % 400 == 0))
}

pub struct UniformDrawdownProfileParameter {
    meta: ParameterMeta,
    residual_days: u8,
    reset_doy: u16,
}

impl UniformDrawdownProfileParameter {
    pub fn new(name: &str, reset_day: u8, reset_month: Month, residual_days: u8) -> Self {
        // Calculate the reset day of year in a known leap year.
        let reset_doy = Date::from_calendar_date(2016, reset_month, reset_day)
            .expect("Invalid reset day")
            .ordinal();

        Self {
            meta: ParameterMeta::new(name),
            residual_days,
            reset_doy,
        }
    }
}

impl Parameter for UniformDrawdownProfileParameter {
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
        // Current calendar year (might be adjusted depending on position of reset day)
        let mut year = timestep.date.year();

        // Zero-base current day of the year.
        let current_doy = timestep.date.ordinal();
        let mut days_into_period: i32 = current_doy as i32 - self.reset_doy as i32;
        if days_into_period < 0 {
            // We're not past the reset day yet; use the previous year
            year -= 1
        }

        if self.reset_doy > 59 {
            year += 1
        }

        // Determine the number of days in the period based on whether there is a leap year
        // or not in the current period
        let total_days_in_period = if is_leap_year(year) { 366 } else { 365 };

        // Now determine number of days we're into the period if it has wrapped around to a new year
        if days_into_period < 0 {
            days_into_period += 366;
            // Need to adjust for post 29th Feb in non-leap years.
            // Recall `current_doy` was incremented by 1 if it is a non-leap already (hence comparison to 59)
            if !is_leap_year(timestep.date.year()) && current_doy > 59 {
                days_into_period -= 1;
            }
        }

        let residual_proportion = self.residual_days as f64 / total_days_in_period as f64;
        let slope = (residual_proportion - 1.0) / total_days_in_period as f64;

        Ok(1.0 + (slope * days_into_period as f64))
    }
}
