use crate::metric::Metric;
use crate::parameters::{Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use std::any::Any;

use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;

pub struct MaxParameter {
    meta: ParameterMeta,
    metric: Metric,
    threshold: f64,
}

impl MaxParameter {
    pub fn new(name: &str, metric: Metric, threshold: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            threshold,
        }
    }
}

impl Parameter for MaxParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        state: &State,
        _internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<f64, PywrError> {
        // Current value
        let x = self.metric.get_value(state)?;
        Ok(x.max(self.threshold))
    }
}
