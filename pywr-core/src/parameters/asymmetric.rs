use crate::metric::MetricU64;
use crate::network::Network;
use crate::parameters::errors::{ParameterCalculationError, ParameterSetupError};
use crate::parameters::{
    GeneralParameter, Parameter, ParameterMeta, ParameterName, ParameterState, downcast_internal_state_mut,
};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;

pub struct AsymmetricSwitchIndexParameter {
    meta: ParameterMeta,
    on_parameter: MetricU64,
    off_parameter: MetricU64,
}

impl AsymmetricSwitchIndexParameter {
    pub fn new(name: ParameterName, on_parameter: MetricU64, off_parameter: MetricU64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            on_parameter,
            off_parameter,
        }
    }
}

impl Parameter for AsymmetricSwitchIndexParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, ParameterSetupError> {
        Ok(Some(Box::new(0_u64)))
    }
}

impl GeneralParameter<u64> for AsymmetricSwitchIndexParameter {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, ParameterCalculationError> {
        let on_value = self.on_parameter.get_value(network, state)?;

        // Downcast the internal state to the correct type
        let current_state = downcast_internal_state_mut::<u64>(internal_state);

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

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}
