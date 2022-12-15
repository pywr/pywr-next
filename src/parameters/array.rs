use crate::parameters::{Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;

pub struct Array1Parameter {
    meta: ParameterMeta,
    array: ndarray::Array1<f64>,
}

impl Array1Parameter {
    pub fn new(name: &str, array: ndarray::Array1<f64>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            array,
        }
    }
}

impl Parameter for Array1Parameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _state: &State,
        _internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<f64, PywrError> {
        Ok(self.array[timestep.index])
    }
}
