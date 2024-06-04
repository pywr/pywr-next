use crate::parameters::{Parameter, ParameterMeta, ParameterState, SimpleParameter};
use crate::scenario::ScenarioIndex;
use crate::state::SimpleParameterValues;
use crate::timestep::Timestep;
use crate::PywrError;
use chrono::Datelike;

pub struct DailyProfileParameter {
    meta: ParameterMeta,
    values: [f64; 366],
}

impl DailyProfileParameter {
    pub fn new(name: &str, values: [f64; 366]) -> Self {
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
    ) -> Result<f64, PywrError> {
        Ok(self.values[timestep.date.ordinal() as usize - 1])
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}
