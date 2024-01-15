use crate::network::Network;
use crate::parameters::{Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{ParameterState, State};
use crate::timestep::Timestep;
use crate::PywrError;
use ndarray::{Array1, Array2, Axis};
use std::any::Any;

pub struct Array1Parameter {
    meta: ParameterMeta,
    array: Array1<f64>,
    timestep_offset: Option<i32>,
}

impl Array1Parameter {
    pub fn new(name: &str, array: Array1<f64>, timestep_offset: Option<i32>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            array,
            timestep_offset,
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
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, PywrError> {
        let idx = match self.timestep_offset {
            None => timestep.index,
            Some(offset) => (timestep.index as i32 + offset).max(0).min(self.array.len() as i32 - 1) as usize,
        };
        // This panics if out-of-bounds
        let value = self.array[[idx]];
        Ok(value)
    }
}

pub struct Array2Parameter {
    meta: ParameterMeta,
    array: Array2<f64>,
    scenario_group_index: usize,
    timestep_offset: Option<i32>,
}

impl Array2Parameter {
    pub fn new(name: &str, array: Array2<f64>, scenario_group_index: usize, timestep_offset: Option<i32>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            array,
            scenario_group_index,
            timestep_offset,
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
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, PywrError> {
        // This panics if out-of-bounds
        let t_idx = match self.timestep_offset {
            None => timestep.index,
            Some(offset) => (timestep.index as i32 + offset)
                .max(0)
                .min(self.array.len_of(Axis(0)) as i32 - 1) as usize,
        };
        let s_idx = scenario_index.indices[self.scenario_group_index];

        Ok(self.array[[t_idx, s_idx]])
    }
}
