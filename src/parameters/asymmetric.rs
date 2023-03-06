use crate::model::Model;
use crate::parameters::{downcast_internal_state, IndexParameter, IndexValue, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;

pub struct AsymmetricSwitchIndexParameter {
    meta: ParameterMeta,
    on_parameter: IndexValue,
    off_parameter: IndexValue,
}

impl AsymmetricSwitchIndexParameter {
    pub fn new(name: &str, on_parameter: IndexValue, off_parameter: IndexValue) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            on_parameter,
            off_parameter,
        }
    }
}

impl IndexParameter for AsymmetricSwitchIndexParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn Any + Send>>, PywrError> {
        Ok(Some(Box::new(0_usize)))
    }

    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _model: &Model,
        state: &State,
        internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<usize, PywrError> {
        let on_value = match self.on_parameter {
            IndexValue::Constant(idx) => idx,
            IndexValue::Dynamic(p) => state.get_parameter_index(p)?,
        };

        // Downcast the internal state to the correct type
        let current_state = downcast_internal_state::<usize>(internal_state);

        if *current_state > 0 {
            if on_value > 0 {
                // No change
            } else {
                let off_value = match self.off_parameter {
                    IndexValue::Constant(idx) => idx,
                    IndexValue::Dynamic(p) => state.get_parameter_index(p)?,
                };

                if off_value == 0 {
                    *current_state = 0;
                }
            }
        } else if on_value > 0 {
            *current_state = 1;
        }

        Ok(*current_state)
    }
}
