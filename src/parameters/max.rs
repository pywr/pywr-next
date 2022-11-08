use crate::metric::Metric;
use crate::parameters::{Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::ParameterState;
use crate::timestep::Timestep;
use crate::{NetworkState, PywrError};

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
        &mut self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<f64, PywrError> {
        // Current value
        let x = self.metric.get_value(state, parameter_state)?;
        Ok(x.max(self.threshold))
    }
}
