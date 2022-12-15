use crate::metric::Metric;
use crate::parameters::{IndexParameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;
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
    metric: Metric,
    threshold: Metric,
    predicate: Predicate,
    ratchet: bool,
}

impl ThresholdParameter {
    pub fn new(name: &str, metric: Metric, threshold: Metric, predicate: Predicate, ratchet: bool) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            threshold,
            predicate,
            ratchet,
        }
    }
}

impl IndexParameter for ThresholdParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn Any>>, PywrError> {
        // Internal state is just a boolean indicating if the threshold was triggered previously.
        // Initially this is false.
        Ok(Some(Box::new(false)))
    }

    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        state: &State,
        internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<usize, PywrError> {
        // Downcast the internal state to the correct type
        let previously_activated = match internal_state {
            Some(internal) => match internal.downcast_mut::<bool>() {
                Some(pa) => pa,
                None => panic!("Internal state did not downcast to the correct type! :("),
            },
            None => panic!("No internal state defined when one was expected! :("),
        };

        // Return early if ratchet has been hit
        if self.ratchet & *previously_activated {
            return Ok(1);
        }

        let threshold = self.threshold.get_value(state)?;
        let value = self.metric.get_value(state)?;

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
}
