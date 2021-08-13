use crate::metric::Metric;
use crate::model::Model;
use crate::parameters::{InternalParameterState, Parameter, ParameterMeta, _IndexParameter};
use crate::scenario::ScenarioIndex;
use crate::state::{NetworkState, ParameterState};
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
    metric: Metric,
    threshold: Parameter,
    predicate: Predicate,
    ratchet: bool,
    previously_activated: InternalParameterState<bool>,
}

impl ThresholdParameter {
    pub fn new(name: &str, metric: Metric, threshold: Parameter, predicate: Predicate, ratchet: bool) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            threshold,
            predicate,
            ratchet,
            previously_activated: InternalParameterState::new(),
        }
    }
}

impl _IndexParameter for ThresholdParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn setup(
        &mut self,
        _model: &Model,
        _timesteps: &Vec<Timestep>,
        scenario_indices: &Vec<ScenarioIndex>,
    ) -> Result<(), PywrError> {
        self.previously_activated.setup(scenario_indices.len(), false);
        Ok(())
    }

    fn compute(
        &mut self,
        _timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Model,
        network_state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<usize, PywrError> {
        // Return early if ratchet has been hit
        if self.ratchet & self.previously_activated.get(scenario_index.index) {
            return Ok(1);
        }

        let threshold = parameter_state.get_value(self.threshold.index())?;
        let value = self.metric.get_value(model, network_state, parameter_state)?;

        let active = match self.predicate {
            Predicate::LessThan => value < threshold,
            Predicate::GreaterThan => value > threshold,
            Predicate::EqualTo => (value - threshold).abs() < 1E-6, // TODO make this a global constant
            Predicate::LessThanOrEqualTo => value <= threshold,
            Predicate::GreaterThanOrEqualTo => value >= threshold,
        };

        match active {
            true => {
                self.previously_activated.set(scenario_index.index, true);
                Ok(1)
            }
            false => Ok(0),
        }
    }
}
