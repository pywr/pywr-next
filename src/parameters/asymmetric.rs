use crate::model::Model;
use crate::parameters::{IndexParameter, ParameterMeta, _IndexParameter};
use crate::scenario::ScenarioIndex;
use crate::state::{NetworkState, ParameterState};
use crate::timestep::Timestep;
use crate::PywrError;

pub struct AsymmetricSwitchIndexParameter {
    meta: ParameterMeta,
    on_parameter: IndexParameter,
    off_parameter: IndexParameter,
    current_state: Vec<usize>,
}

impl AsymmetricSwitchIndexParameter {
    pub fn new(name: &str, on_parameter: IndexParameter, off_parameter: IndexParameter) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            on_parameter,
            off_parameter,
            current_state: Vec::new(),
        }
    }
}

impl _IndexParameter for AsymmetricSwitchIndexParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &mut self,
        _timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        _model: &Model,
        _network_state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<usize, PywrError> {
        let on_value = parameter_state.get_index(self.on_parameter.index())?;
        let current_state = *self
            .current_state
            .get(scenario_index.index)
            .ok_or_else(|| PywrError::InternalParameterError("State not found.".to_string()))?;

        if current_state > 0 {
            if on_value > 0 {
                // No change
            } else {
                let off_value = parameter_state.get_index(self.off_parameter.index())?;
                if off_value == 0 {
                    self.current_state[scenario_index.index] = 0;
                }
            }
        } else if on_value > 0 {
            self.current_state[scenario_index.index] = 1;
        }

        Ok(self.current_state[scenario_index.index])
    }
}
