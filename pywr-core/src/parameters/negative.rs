use crate::metric::MetricF64;
use crate::network::Network;
use crate::parameters::errors::ParameterCalculationError;
use crate::parameters::{GeneralParameter, Parameter, ParameterMeta, ParameterName, ParameterState};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;

pub struct NegativeParameter {
    meta: ParameterMeta,
    metric: MetricF64,
}

impl NegativeParameter {
    pub fn new(name: ParameterName, metric: MetricF64) -> Self {
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
}

impl GeneralParameter<f64> for NegativeParameter {
    fn before(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<f64>, ParameterCalculationError> {
        // Current value
        let x = self.metric.get_value(model, state)?;
        Ok(Some(-x))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}
