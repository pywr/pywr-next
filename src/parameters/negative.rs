use crate::metric::Metric;
use crate::model::Model;
use crate::parameters::{ParameterMeta, _Parameter};
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

impl _Parameter for NegativeParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &mut self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Model,
        state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<f64, PywrError> {
        // Current value
        let x = self.metric.get_value(model, state, parameter_state)?;
        Ok(-x)
    }
}
