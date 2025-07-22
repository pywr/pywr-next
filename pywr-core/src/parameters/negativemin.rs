use crate::metric::MetricF64;
use crate::network::Network;
use crate::parameters::errors::ParameterCalculationError;
use crate::parameters::{GeneralParameter, Parameter, ParameterMeta, ParameterName, ParameterState};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;

pub struct NegativeMinParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    threshold: f64,
}

impl NegativeMinParameter {
    pub fn new(name: ParameterName, metric: MetricF64, threshold: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            threshold,
        }
    }
}

impl Parameter for NegativeMinParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}
impl GeneralParameter<f64> for NegativeMinParameter {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, ParameterCalculationError> {
        let x = -self.metric.get_value(network, state)?;
        Ok(x.min(self.threshold))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}
