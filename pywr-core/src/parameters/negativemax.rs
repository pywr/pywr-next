use crate::metric::Metric;
use crate::model::Model;
use crate::parameters::{Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use std::any::Any;

use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;

pub struct NegativeMaxParameter {
    meta: ParameterMeta,
    metric: Metric,
    threshold: f64,
}

impl NegativeMaxParameter {
    pub fn new(name: &str, metric: Metric, threshold: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            threshold,
        }
    }
}

impl Parameter for NegativeMaxParameter {
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
        let x = -self.metric.get_value(model, state)?;
        Ok(x.max(self.threshold))
    }
}
