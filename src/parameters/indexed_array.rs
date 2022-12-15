use crate::parameters::{FloatValue, IndexValue, Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;

pub struct IndexedArrayParameter {
    meta: ParameterMeta,
    index_parameter: IndexValue,
    parameters: Vec<FloatValue>,
}

impl IndexedArrayParameter {
    pub fn new(name: &str, index_parameter: IndexValue, parameters: Vec<FloatValue>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            index_parameter,
            parameters,
        }
    }
}

impl Parameter for IndexedArrayParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        state: &State,
        _internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<f64, PywrError> {
        let index = match self.index_parameter {
            IndexValue::Constant(idx) => idx,
            IndexValue::Dynamic(idx) => state.get_parameter_index(idx)?,
        };

        let value = self.parameters.get(index).ok_or(PywrError::DataOutOfRange)?;

        let value = match value {
            FloatValue::Constant(c) => *c,
            FloatValue::Dynamic(idx) => state.get_parameter_value(*idx)?,
        };
        Ok(value)
    }
}
