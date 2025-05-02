use crate::PywrError;
use crate::parameters::{Parameter, ParameterMeta, ParameterName, ParameterState, SimpleParameter};
use crate::scenario::ScenarioIndex;
use crate::state::SimpleParameterValues;
use crate::timestep::{Timestep, TimestepIndex};
use ndarray::{Array1, Array2, Axis};

pub struct Array1Parameter<T> {
    meta: ParameterMeta,
    array: Array1<T>,
    timestep_offset: Option<i32>,
}

impl<T> Array1Parameter<T> {
    pub fn new(name: ParameterName, array: Array1<T>, timestep_offset: Option<i32>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            array,
            timestep_offset,
        }
    }

    /// Compute the time-step index to use accounting for any defined offset.
    ///
    /// The offset is applied to the time-step index and then clamped to the bounds of the array.
    /// This ensures that the time-step index is always within the bounds of the array.
    fn timestep_index(&self, timestep: &Timestep) -> TimestepIndex {
        match self.timestep_offset {
            None => timestep.index,
            Some(offset) => (timestep.index as i32 + offset)
                .max(0)
                .min(self.array.len_of(Axis(0)) as i32 - 1) as usize,
        }
    }
}
impl<T> Parameter for Array1Parameter<T>
where
    T: Send + Sync + Clone,
{
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}
impl SimpleParameter<f64> for Array1Parameter<f64> {
    fn compute(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _values: &SimpleParameterValues,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, PywrError> {
        let idx = self.timestep_index(timestep);
        // This panics if out-of-bounds
        let value = self.array[[idx]];
        Ok(value)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl SimpleParameter<u64> for Array1Parameter<u64> {
    fn compute(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _values: &SimpleParameterValues,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, PywrError> {
        let idx = self.timestep_index(timestep);
        // This panics if out-of-bounds
        let value = self.array[[idx]];
        Ok(value)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

pub struct Array2Parameter<T> {
    meta: ParameterMeta,
    array: Array2<T>,
    scenario_group_index: usize,
    timestep_offset: Option<i32>,
}

impl<T> Array2Parameter<T> {
    pub fn new(
        name: ParameterName,
        array: Array2<T>,
        scenario_group_index: usize,
        timestep_offset: Option<i32>,
    ) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            array,
            scenario_group_index,
            timestep_offset,
        }
    }

    /// Compute the time-step index to use accounting for any defined offset.
    ///
    /// The offset is applied to the time-step index and then clamped to the bounds of the array.
    /// This ensures that the time-step index is always within the bounds of the array.
    fn timestep_index(&self, timestep: &Timestep) -> TimestepIndex {
        match self.timestep_offset {
            None => timestep.index,
            Some(offset) => (timestep.index as i32 + offset)
                .max(0)
                .min(self.array.len_of(Axis(0)) as i32 - 1) as usize,
        }
    }
}

impl<T> Parameter for Array2Parameter<T>
where
    T: Send + Sync + Clone,
{
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl SimpleParameter<f64> for Array2Parameter<f64> {
    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        _values: &SimpleParameterValues,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, PywrError> {
        // This panics if out-of-bounds
        let t_idx = self.timestep_index(timestep);
        let s_idx = scenario_index.simulation_index_for_group(self.scenario_group_index);

        Ok(self.array[[t_idx, s_idx]])
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl SimpleParameter<u64> for Array2Parameter<u64> {
    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        _values: &SimpleParameterValues,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, PywrError> {
        // This panics if out-of-bounds
        let t_idx = self.timestep_index(timestep);
        let s_idx = scenario_index.simulation_index_for_group(self.scenario_group_index);

        Ok(self.array[[t_idx, s_idx]])
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}
