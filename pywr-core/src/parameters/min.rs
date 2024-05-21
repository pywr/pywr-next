use crate::metric::MetricF64;
use crate::network::Network;
use crate::parameters::{GeneralParameter, Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{ParameterState, State};
use crate::timestep::Timestep;
use crate::PywrError;

pub struct MinParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    threshold: f64,
}

impl MinParameter {
    pub fn new(name: &str, metric: MetricF64, threshold: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            threshold,
        }
    }
}

impl Parameter for MinParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}
impl GeneralParameter<f64> for MinParameter {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, PywrError> {
        let x = self.metric.get_value(model, state)?;
        Ok(x.min(self.threshold))
    }
}
