use crate::parameters::errors::SimpleCalculationError;
use crate::parameters::{Parameter, ParameterMeta, ParameterName, ParameterState, SimpleParameter};
use crate::scenario::ScenarioIndex;
use crate::state::SimpleParameterValues;
use crate::timestep::Timestep;
use chrono::Timelike;

/// A parameter that defines a profile over a 24-hour period.
///
/// The values array should contain 24 values, one for each hour of the day.
pub struct DiurnalProfileParameter {
    meta: ParameterMeta,
    values: [f64; 24],
}

impl DiurnalProfileParameter {
    pub fn new(name: ParameterName, values: [f64; 24]) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            values,
        }
    }
}

impl Parameter for DiurnalProfileParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl SimpleParameter<f64> for DiurnalProfileParameter {
    fn compute(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _values: &SimpleParameterValues,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, SimpleCalculationError> {
        Ok(self.values[timestep.date.time().hour() as usize])
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}
