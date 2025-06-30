use crate::network::Network;
use crate::parameters::errors::ParameterCalculationError;
use crate::parameters::{GeneralParameter, Parameter, ParameterMeta, ParameterName, ParameterState};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;

pub struct VectorParameter {
    meta: ParameterMeta,
    values: Vec<f64>,
}

impl VectorParameter {
    pub fn new(name: ParameterName, values: Vec<f64>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            values,
        }
    }
}

impl Parameter for VectorParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter<f64> for VectorParameter {
    fn compute(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _model: &Network,
        _state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, ParameterCalculationError> {
        match self.values.get(timestep.index) {
            Some(v) => Ok(*v),
            None => Err(ParameterCalculationError::OutOfBoundsError {
                index: timestep.index,
                length: self.values.len(),
                axis: 0,
            }),
        }
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}
