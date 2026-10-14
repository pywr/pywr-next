use crate::network::ResolutionMaps;
use crate::parameters::errors::SimpleCalculationError;
use crate::parameters::{
    BuiltParameter, MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterMeta,
    ParameterName, ParameterSetupError, ParameterState, SimpleParameter, SimpleParameterContext, VariableConfig,
    VariableParameter, VariableParameterError, VariableParameterValues, downcast_internal_state_mut,
    downcast_internal_state_ref, downcast_variable_config_ref,
};
use crate::scenario::ScenarioIndex;
use crate::timestep::Timestep;
use jiff::ToSpan;
use jiff::civil::DateTime;
use std::any::Any;

#[derive(Debug, Copy, Clone)]
pub enum MonthlyInterpDay {
    First,
    Last,
}

// We store this internal value as an Option<f64> so that it can be updated by the variable API
type InternalValue = Option<[f64; 12]>;

#[derive(Debug)]
pub struct MonthlyProfileParameter {
    meta: ParameterMeta,
    values: [f64; 12],
    interp_day: Option<MonthlyInterpDay>,
}

impl MonthlyProfileParameter {
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
        ctx: SimpleParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, SimpleCalculationError> {
        let v = match &self.interp_day {
            Some(interp_day) => match interp_day {
                MonthlyInterpDay::First => {
                    let next_month0 = ctx.timestep.date.month() % 12;
                    let first_value = self.value_for_month0((ctx.timestep.date.month() - 1) as usize, internal_state);
                    let last_value = self.value_for_month0(next_month0 as usize, internal_state);

                    interpolate_first(&ctx.timestep.date, first_value, last_value)
                }
                MonthlyInterpDay::Last => {
                    let current_month = ctx.timestep.date.month();
                    let last_month = if current_month == 1 { 12 } else { current_month - 1 };
                    let first_value = self.value_for_month0(last_month as usize - 1, internal_state);
                    let last_value = self.value_for_month0(ctx.timestep.date.month() as usize - 1, internal_state);

                    interpolate_last(&ctx.timestep.date, first_value, last_value)
                }
            },
            None => self.value_for_month0(ctx.timestep.date.month() as usize - 1, internal_state),
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
