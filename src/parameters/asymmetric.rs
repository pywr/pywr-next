use crate::parameters::{IndexParameter, IndexValue, InternalParameterState, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{NetworkState, ParameterState};
use crate::timestep::Timestep;
use crate::PywrError;

pub struct AsymmetricSwitchIndexParameter {
    meta: ParameterMeta,
    on_parameter: IndexValue,
    off_parameter: IndexValue,
    current_state: InternalParameterState<usize>,
}

impl AsymmetricSwitchIndexParameter {
    pub fn new(name: &str, on_parameter: IndexValue, off_parameter: IndexValue) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            on_parameter,
            off_parameter,
            current_state: InternalParameterState::default(),
        }
    }
}

impl IndexParameter for AsymmetricSwitchIndexParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn setup(&mut self, _timesteps: &[Timestep], scenario_indices: &[ScenarioIndex]) -> Result<(), PywrError> {
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
        let on_value = match self.on_parameter {
            IndexValue::Constant(idx) => idx,
            IndexValue::Dynamic(p) => parameter_state.get_index(p)?,
        };

        let current_state = *self.current_state.get(scenario_index.index);

        if current_state > 0 {
            if on_value > 0 {
                // No change
            } else {
                let off_value = match self.off_parameter {
                    IndexValue::Constant(idx) => idx,
                    IndexValue::Dynamic(p) => parameter_state.get_index(p)?,
                };

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
