use crate::network::Network;
use crate::parameters::{downcast_internal_state, Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;

pub struct InterModelTransfer {
    meta: ParameterMeta,
}

impl InterModelTransfer {
    pub fn new(name: &str) -> Self {
        Self {
            meta: ParameterMeta::new(name),
        }
    }
}

impl Parameter for InterModelTransfer {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn setup(
        &self,
        timesteps: &[Timestep],
        scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn Any + Send>>, PywrError> {
        // Internally we store the value received from the other model
        let value: Option<f64> = None;
        Ok(Some(Box::new(value)))
    }

    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<f64, PywrError> {
        // Downcast the internal state to the correct type
        let value = downcast_internal_state::<Option<f64>>(internal_state);
        value.ok_or(PywrError::InterNetworkParameterStateNotInitialised)
    }
}
