use crate::metric::MetricF64;
use crate::network::Network;
use crate::parameters::{
    downcast_internal_state_mut, GeneralParameter, Parameter, ParameterMeta, ParameterName, ParameterState,
};
use crate::predicate::Predicate;
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;

pub struct ThresholdParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    threshold: MetricF64,
    predicate: Predicate,
    ratchet: bool,
}

impl ThresholdParameter {
    pub fn new(
        name: ParameterName,
        metric: MetricF64,
        threshold: MetricF64,
        predicate: Predicate,
        ratchet: bool,
    ) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            threshold,
            predicate,
            ratchet,
        }
    }
}

impl Parameter for ThresholdParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, PywrError> {
        // Internal state is just a boolean indicating if the threshold was triggered previously.
        // Initially this is false.
        Ok(Some(Box::new(false)))
    }
}

impl GeneralParameter<u64> for ThresholdParameter {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, PywrError> {
        // Downcast the internal state to the correct type
        let previously_activated = downcast_internal_state_mut::<bool>(internal_state);

        // Return early if ratchet has been hit
        if self.ratchet & *previously_activated {
            return Ok(1);
        }

        let threshold = self.threshold.get_value(model, state)?;
        let value = self.metric.get_value(model, state)?;

        let active = self.predicate.apply(value, threshold);

        if active {
            // Update the internal state to remember we've been triggered!
            *previously_activated = true;
            Ok(1)
        } else {
            Ok(0)
        }
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}
