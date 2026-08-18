use crate::network::ResolutionMaps;
use crate::parameters::errors::SimpleCalculationError;
use crate::parameters::{
    BuiltParameter, MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterMeta,
    ParameterName, ParameterState, SimpleParameter, SimpleParameterContext,
};
use jiff::ToSpan;
use jiff::civil::DateTime;

#[derive(Debug, Copy, Clone)]
pub enum MonthlyInterpDay {
    First,
    Last,
}

#[derive(Debug)]
pub struct MonthlyProfileParameter {
    meta: ParameterMeta,
    values: [f64; 12],
    interp_day: Option<MonthlyInterpDay>,
}

/// Interpolate between `first_value` and `last_value` based on the day of the month. The last
/// value is assumed to correspond to the first day of the next month.
fn interpolate_first(date: &DateTime, first_value: f64, last_value: f64) -> f64 {
    let start_of_month = date.first_of_month().start_of_day();
    let start_of_next_month = date
        .checked_add(1.months())
        .expect("Datetime overflowed!")
        .first_of_month()
        .start_of_day();

    let duration_of_month = start_of_next_month.duration_since(start_of_month);
    let since_start_of_month = date.duration_since(start_of_month);
    let fraction_of_month = since_start_of_month.as_secs_f64() / duration_of_month.as_secs_f64();

    first_value + (last_value - first_value) * fraction_of_month
}

/// Interpolate between `first_value` and `last_value` based on the day of the month. The first
/// value is assumed to correspond to the last day of the previous month.
fn interpolate_last(date: &DateTime, first_value: f64, last_value: f64) -> f64 {
    let end_of_last_month = date
        .checked_add(-1.months())
        .expect("Datetime overflowed!")
        .last_of_month()
        .start_of_day();
    let end_of_month = date.last_of_month().start_of_day();

    let duration_of_month = end_of_month.duration_since(end_of_last_month);
    let since_end_of_last_month = date.duration_since(end_of_last_month);
    let fraction_of_month = since_end_of_last_month.as_secs_f64() / duration_of_month.as_secs_f64();

    first_value + (last_value - first_value) * fraction_of_month
}

impl Parameter for MonthlyProfileParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}
impl SimpleParameter<f64> for MonthlyProfileParameter {
    fn compute(
        &self,
        ctx: SimpleParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, SimpleCalculationError> {
        let v = match &self.interp_day {
            Some(interp_day) => match interp_day {
                MonthlyInterpDay::First => {
                    let next_month0 = ctx.timestep.date.month() % 12;
                    let first_value = self.values[(ctx.timestep.date.month() - 1) as usize];
                    let last_value = self.values[next_month0 as usize];

                    interpolate_first(&ctx.timestep.date, first_value, last_value)
                }
                MonthlyInterpDay::Last => {
                    let current_month = ctx.timestep.date.month();
                    let last_month = if current_month == 1 { 12 } else { current_month - 1 };
                    let first_value = self.values[last_month as usize - 1];
                    let last_value = self.values[ctx.timestep.date.month() as usize - 1];

                    interpolate_last(&ctx.timestep.date, first_value, last_value)
                }
            },
            None => self.values[ctx.timestep.date.month() as usize - 1],
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

#[derive(Debug)]
pub struct MonthlyProfileParameterBuilder {
    meta: ParameterMeta,
    values: [f64; 12],
    interp_day: Option<MonthlyInterpDay>,
}

impl MonthlyProfileParameterBuilder {
    pub fn new(name: ParameterName, values: [f64; 12]) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            values,
            interp_day: None,
        }
    }

    pub fn interp_day(&mut self, interp_day: MonthlyInterpDay) -> &mut Self {
        self.interp_day = Some(interp_day);
        self
    }
}

impl ParameterBuilder<f64> for MonthlyProfileParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        _resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        let p = MonthlyProfileParameter {
            meta: self.meta,
            values: self.values,
            interp_day: self.interp_day,
        };
        Ok(BuiltParameter::Simple(Box::new(p)).into())
    }
}
