use crate::parameters::{IndexParameter, IndexParameterIndex, InternalParameterState, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{NetworkState, ParameterState};
use crate::timestep::Timestep;
use crate::PywrError;

pub struct AsymmetricSwitchIndexParameter {
    meta: ParameterMeta,
    on_parameter: IndexParameterIndex,
    off_parameter: IndexParameterIndex,
    current_state: InternalParameterState<usize>,
}

impl AsymmetricSwitchIndexParameter {
    pub fn new(name: &str, on_parameter: IndexParameterIndex, off_parameter: IndexParameterIndex) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            on_parameter,
            off_parameter,
            current_state: InternalParameterState::new(),
        }
    }
}

impl IndexParameter for AsymmetricSwitchIndexParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn setup(&mut self, _timesteps: &Vec<Timestep>, scenario_indices: &Vec<ScenarioIndex>) -> Result<(), PywrError> {
        self.current_state.setup(scenario_indices.len(), 0);
        Ok(())
    }

    fn compute(
        &mut self,
        _timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        _network_state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<usize, PywrError> {
        let on_value = parameter_state.get_index(self.on_parameter)?;
        let current_state = *self.current_state.get(scenario_index.index);

        if current_state > 0 {
            if on_value > 0 {
                // No change
            } else {
                let off_value = parameter_state.get_index(self.off_parameter)?;
                if off_value == 0 {
                    self.current_state.set(scenario_index.index, 0);
                }
            }
        } else if on_value > 0 {
            self.current_state.set(scenario_index.index, 1);
        }

        Ok(*self.current_state.get(scenario_index.index))
    }
}
