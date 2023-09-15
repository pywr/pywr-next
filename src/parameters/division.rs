use super::PywrError;
use crate::metric::Metric;
use crate::model::Model;
use crate::parameters::{Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use std::any::Any;
use crate::PywrError::InvalidMetricValue;


pub struct DivisionParameter {
    meta: ParameterMeta,
    numerator: Metric,
    denominator: Metric,
}

impl DivisionParameter {
    pub fn new(name: &str, numerator: Metric, denominator: Metric) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            numerator,
            denominator,
        }
    }
}

impl Parameter for DivisionParameter {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Model,
        state: &State,
        _internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<f64, PywrError> {
        // TODO handle scenarios
        let denominator = self.denominator.get_value(model, state)?;

        if denominator == 0.0 {
            return Err(InvalidMetricValue(format!("Division by zero creates a NaN in {}.", self.name())));
        }

        let numerator = self.numerator.get_value(model, state)?;
        Ok(numerator / denominator)
    }
}
