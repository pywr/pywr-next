use super::{Parameter, PywrError};
use crate::metric::MetricF64;
use crate::network::Network;
use crate::parameters::{GeneralParameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{ParameterState, State};
use crate::timestep::Timestep;
use crate::PywrError::InvalidMetricValue;

pub struct DivisionParameter {
    meta: ParameterMeta,
    numerator: MetricF64,
    denominator: MetricF64,
}

impl DivisionParameter {
    pub fn new(name: &str, numerator: MetricF64, denominator: MetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            numerator,
            denominator,
        }
    }
}
impl Parameter for DivisionParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}
impl GeneralParameter<f64> for DivisionParameter {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, PywrError> {
        // TODO handle scenarios
        let denominator = self.denominator.get_value(model, state)?;

        if denominator == 0.0 {
            return Err(InvalidMetricValue(format!(
                "Division by zero creates a NaN in {}.",
                self.name()
            )));
        }

        let numerator = self.numerator.get_value(model, state)?;
        Ok(numerator / denominator)
    }
}
