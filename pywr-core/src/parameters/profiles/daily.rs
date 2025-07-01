use crate::parameters::errors::SimpleCalculationError;
use crate::parameters::{Parameter, ParameterMeta, ParameterName, ParameterState, SimpleParameter};
use crate::scenario::ScenarioIndex;
use crate::state::SimpleParameterValues;
use crate::timestep::Timestep;

pub struct DailyProfileParameter {
    meta: ParameterMeta,
    values: [f64; 366],
}

impl DailyProfileParameter {
    pub fn new(name: ParameterName, values: [f64; 366]) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            values,
        }
    }
}

impl Parameter for DailyProfileParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl SimpleParameter<f64> for DailyProfileParameter {
    fn compute(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _values: &SimpleParameterValues,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, SimpleCalculationError> {
        Ok(self.values[timestep.day_of_year_index()])
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}
