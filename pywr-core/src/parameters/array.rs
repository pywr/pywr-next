use crate::network::Network;
use crate::parameters::{Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use ndarray::{Array1, Array2};
use std::any::Any;

pub struct Array1Parameter {
    meta: ParameterMeta,
    array: Array1<f64>,
}

impl Array1Parameter {
    pub fn new(name: &str, array: Array1<f64>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            array,
        }
    }
}

impl Parameter for Array1Parameter {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _model: &Network,
        _state: &State,
        _internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<f64, PywrError> {
        // This panics if out-of-bounds
        let value = self.array[[timestep.index]];
        Ok(value)
    }
}

pub struct Array2Parameter {
    meta: ParameterMeta,
    array: Array2<f64>,
    scenario_group_index: usize,
}

impl Array2Parameter {
    pub fn new(name: &str, array: Array2<f64>, scenario_group_index: usize) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            array,
            scenario_group_index,
        }
    }
}

impl Parameter for Array2Parameter {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        _model: &Network,
        _state: &State,
        _internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<f64, PywrError> {
        // This panics if out-of-bounds
        let idx = scenario_index.indices[self.scenario_group_index];

        Ok(self.array[[timestep.index, idx]])
    }
}
