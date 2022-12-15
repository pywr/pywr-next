use crate::parameters::{IndexParameter, IndexValue, ParameterMeta};
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
    ) -> Result<Option<Box<dyn Any>>, PywrError> {
        Ok(Some(Box::new(0_usize)))
    }

    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        state: &State,
        internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<usize, PywrError> {
        let on_value = match self.on_parameter {
            IndexValue::Constant(idx) => idx,
            IndexValue::Dynamic(p) => state.get_parameter_index(p)?,
        };

        // Downcast the internal state to the correct type
        let current_state = match internal_state {
            Some(internal) => match internal.downcast_mut::<usize>() {
                Some(pa) => pa,
                None => panic!("Internal state did not downcast to the correct type! :("),
            },
            None => panic!("No internal state defined when one was expected! :("),
        };

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
