use crate::metric::MetricF64;
use crate::network::Network;
use crate::parameters::{
    downcast_internal_state_mut, GeneralParameter, Parameter, ParameterMeta, ParameterName, ParameterState,
};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use std::str::FromStr;

pub enum Predicate {
    LessThan,
    GreaterThan,
    EqualTo,
    LessThanOrEqualTo,
    GreaterThanOrEqualTo,
}

impl FromStr for Predicate {
    type Err = PywrError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "<" => Ok(Self::LessThan),
            ">" => Ok(Self::GreaterThan),
            "=" => Ok(Self::EqualTo),
            "<=" => Ok(Self::LessThanOrEqualTo),
            ">=" => Ok(Self::GreaterThanOrEqualTo),
            _ => Err(PywrError::InvalidAggregationFunction(name.to_string())),
        }
    }
}

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

impl GeneralParameter<usize> for ThresholdParameter {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<usize, PywrError> {
        // Downcast the internal state to the correct type
        let previously_activated = downcast_internal_state_mut::<bool>(internal_state);

        // Return early if ratchet has been hit
        if self.ratchet & *previously_activated {
            return Ok(1);
        }

        let threshold = self.threshold.get_value(model, state)?;
        let value = self.metric.get_value(model, state)?;

        let active = match self.predicate {
            Predicate::LessThan => value < threshold,
            Predicate::GreaterThan => value > threshold,
            Predicate::EqualTo => (value - threshold).abs() < 1E-6, // TODO make this a global constant
            Predicate::LessThanOrEqualTo => value <= threshold,
            Predicate::GreaterThanOrEqualTo => value >= threshold,
        };

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
