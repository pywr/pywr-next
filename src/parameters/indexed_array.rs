use crate::model::Model;
use crate::parameters::{IndexParameter, Parameter, ParameterMeta, _Parameter};
use crate::scenario::ScenarioIndex;
use crate::state::{NetworkState, ParameterState};
use crate::timestep::Timestep;
use crate::PywrError;

pub struct IndexedArrayParameter {
    meta: ParameterMeta,
    index_parameter: IndexParameter,
    parameters: Vec<Parameter>,
}

impl IndexedArrayParameter {
    pub fn new(name: &str, index_parameter: IndexParameter, parameters: Vec<Parameter>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            index_parameter,
            parameters,
        }
    }
}

impl _Parameter for IndexedArrayParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &mut self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _model: &Model,
        _network_state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<f64, PywrError> {
        let index = parameter_state.get_index(self.index_parameter.index())?;

        let parameter = self.parameters.get(index).ok_or(PywrError::DataOutOfRange)?;

        parameter_state.get_value(parameter.index())
    }
}
