use super::{Parameter, ParameterName};
use crate::metric::MetricF64;
use crate::network::Network;
use crate::parameters::errors::ParameterCalculationError;
use crate::parameters::{GeneralParameter, ParameterMeta, ParameterState};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;

pub struct DivisionParameter {
    meta: ParameterMeta,
    numerator: MetricF64,
    denominator: MetricF64,
}

impl DivisionParameter {
    pub fn new(name: ParameterName, numerator: MetricF64, denominator: MetricF64) -> Self {
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
    ) -> Result<f64, ParameterCalculationError> {
        let denominator = self.denominator.get_value(model, state)?;

        if denominator == 0.0 {
            return Err(ParameterCalculationError::DivisionByZeroError);
        }

        let numerator = self.numerator.get_value(model, state)?;
        Ok(numerator / denominator)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}
