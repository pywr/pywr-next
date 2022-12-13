use crate::parameters::{FloatValue, IndexValue, Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{NetworkState, ParameterState};
use crate::timestep::Timestep;
use crate::PywrError;

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
        &mut self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _network_state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<f64, PywrError> {
        let index = match self.index_parameter {
            IndexValue::Constant(idx) => idx,
            IndexValue::Dynamic(idx) => parameter_state.get_index(idx)?,
        };

        let value = self.parameters.get(index).ok_or(PywrError::DataOutOfRange)?;

        let value = match value {
            FloatValue::Constant(c) => *c,
            FloatValue::Dynamic(idx) => parameter_state.get_value(*idx)?,
        };
        Ok(value)
    }
}
