use crate::parameters::{IndexParameter, Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{NetworkState, ParameterState};
use crate::timestep::Timestep;
use crate::{IndexParameterIndex, ParameterIndex, PywrError};

pub struct IndexedArrayParameter {
    meta: ParameterMeta,
    index_parameter: IndexParameterIndex,
    parameters: Vec<ParameterIndex>,
}

impl IndexedArrayParameter {
    pub fn new(name: &str, index_parameter: IndexParameterIndex, parameters: Vec<ParameterIndex>) -> Self {
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
        let index = parameter_state.get_index(self.index_parameter)?;

        let parameter = self.parameters.get(index).ok_or(PywrError::DataOutOfRange)?;

        parameter_state.get_value(*parameter)
    }
}
