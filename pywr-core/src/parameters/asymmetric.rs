use crate::metric::MetricUsize;
use crate::network::Network;
use crate::parameters::{downcast_internal_state_mut, Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{ParameterState, State};
use crate::timestep::Timestep;
use crate::PywrError;

pub struct AsymmetricSwitchIndexParameter {
    meta: ParameterMeta,
    on_parameter: MetricUsize,
    off_parameter: MetricUsize,
}

impl AsymmetricSwitchIndexParameter {
    pub fn new(name: &str, on_parameter: MetricUsize, off_parameter: MetricUsize) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            on_parameter,
            off_parameter,
        }
    }
}

impl Parameter<usize> for AsymmetricSwitchIndexParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, PywrError> {
        Ok(Some(Box::new(0_usize)))
    }

    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<usize, PywrError> {
        let on_value = self.on_parameter.get_value(network, state)?;

        // Downcast the internal state to the correct type
        let current_state = downcast_internal_state_mut::<usize>(internal_state);

        if *current_state > 0 {
            if on_value > 0 {
                // No change
            } else {
                let off_value = self.off_parameter.get_value(network, state)?;

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
