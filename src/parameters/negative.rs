use crate::metric::Metric;
use crate::parameters::{Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::ParameterState;
use crate::timestep::Timestep;
use crate::{NetworkState, PywrError};

pub struct NegativeParameter {
    meta: ParameterMeta,
    metric: Metric,
}

impl NegativeParameter {
    pub fn new(name: &str, metric: Metric) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
        }
    }
}

impl Parameter for NegativeParameter {
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
        Ok(-x)
    }
}
