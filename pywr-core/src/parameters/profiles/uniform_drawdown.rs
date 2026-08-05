use crate::network::ResolutionMaps;
use crate::parameters::errors::SimpleCalculationError;
use crate::parameters::{
    BuiltParameter, MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterMeta,
    ParameterName, ParameterState, SimpleParameter, SimpleParameterContext,
};
use jiff::civil::Date;

fn is_leap_year(year: i16) -> bool {
    (year % 4 == 0) & ((year % 100 != 0) | (year % 400 == 0))
}

#[derive(Debug)]
pub struct UniformDrawdownProfileParameter {
    meta: ParameterMeta,
    residual_days: u8,
    reset_doy: i16,
}

impl Parameter for UniformDrawdownProfileParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}
impl SimpleParameter<f64> for UniformDrawdownProfileParameter {
    fn compute(
        &self,
        ctx: SimpleParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, SimpleCalculationError> {
        // Current calendar year (might be adjusted depending on position of reset day)
        let mut year = ctx.timestep.date.year();

        // Current day of the year.
        let current_doy = ctx.timestep.day_of_year_index() + 1;
        let mut days_into_period: i32 = current_doy as i32 - self.reset_doy as i32;
        if days_into_period < 0 {
            // We're not past the reset day yet; use the previous year
            year -= 1
        }

        if self.reset_doy > 60 {
            year += 1
        }

        // Determine the number of days in the period based on whether there is a leap year
        // or not in the current period
        let total_days_in_period = if is_leap_year(year) { 366 } else { 365 };

        // Now determine number of days we're into the period if it has wrapped around to a new year
        if days_into_period < 0 {
            days_into_period += 366;
            // Need to adjust for post 29th Feb in non-leap years.
            // Recall `current_doy` was incremented by 1 if it is a non-leap already (hence comparison to 60)
            if !is_leap_year(ctx.timestep.date.year()) && current_doy > 60 {
                days_into_period -= 1;
            }
        }

        let residual_proportion = self.residual_days as f64 / total_days_in_period as f64;
        let slope = (residual_proportion - 1.0) / total_days_in_period as f64;

        Ok(1.0 + (slope * days_into_period as f64))
    }
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

#[derive(Debug)]
pub struct UniformDrawdownProfileParameterBuilder {
    meta: ParameterMeta,
    residual_days: u8,
    reset_day: i8,
    reset_month: i8,
}

impl UniformDrawdownProfileParameterBuilder {
    pub fn new(name: ParameterName, reset_day: i8, reset_month: i8) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            residual_days: 0,
            reset_month,
            reset_day,
        }
    }

    pub fn residual_days(&mut self, residual_days: u8) -> &mut Self {
        self.residual_days = residual_days;
        self
    }
}

impl ParameterBuilder<f64> for UniformDrawdownProfileParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        _resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        // Calculate the reset day of year in a known leap year.
        let reset_doy = Date::new(2016, self.reset_month, self.reset_day)
            .map_err(|_| ParameterBuildError::InvalidDayOfYear {
                day: self.reset_day,
                month: self.reset_month,
            })?
            .day_of_year();

        let p = UniformDrawdownProfileParameter {
            meta: self.meta,
            residual_days: self.residual_days,
            reset_doy,
        };

        Ok(BuiltParameter::Simple(Box::new(p)).into())
    }
}
