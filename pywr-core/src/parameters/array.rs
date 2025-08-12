use crate::parameters::errors::SimpleCalculationError;
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
    ) -> Result<f64, SimpleCalculationError> {
        let idx = self.timestep_index(timestep);

        let value = self
            .array
            .get(idx)
            .ok_or_else(|| SimpleCalculationError::OutOfBoundsError {
                index: idx,
                length: self.array.len(),
                axis: 0,
            })?;
        Ok(*value)
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
    ) -> Result<u64, SimpleCalculationError> {
        let idx = self.timestep_index(timestep);
        let value = self
            .array
            .get(idx)
            .ok_or_else(|| SimpleCalculationError::OutOfBoundsError {
                index: idx,
                length: self.array.len(),
                axis: 0,
            })?;
        Ok(*value)
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
    ) -> Result<f64, SimpleCalculationError> {
        let t_idx = self.timestep_index(timestep);
        let s_idx = scenario_index.simulation_index_for_group(self.scenario_group_index);

        let value = self.array.get([t_idx, s_idx]).ok_or_else(|| {
            let shape = self.array.shape();
            if t_idx >= shape[0] {
                SimpleCalculationError::OutOfBoundsError {
                    index: t_idx,
                    length: shape[0],
                    axis: 0,
                }
            } else if s_idx >= shape[1] {
                SimpleCalculationError::OutOfBoundsError {
                    index: s_idx,
                    length: shape[1],
                    axis: 1,
                }
            } else {
                unreachable!(
                    "Invalid indices for array: t_idx = {}, s_idx = {}, shape = {:?}",
                    t_idx, s_idx, shape
                )
            }
        })?;
        Ok(*value)
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
    ) -> Result<u64, SimpleCalculationError> {
        let t_idx = self.timestep_index(timestep);
        let s_idx = scenario_index.simulation_index_for_group(self.scenario_group_index);

        let value = self.array.get([t_idx, s_idx]).ok_or_else(|| {
            let shape = self.array.shape();
            if t_idx >= shape[0] {
                SimpleCalculationError::OutOfBoundsError {
                    index: t_idx,
                    length: shape[0],
                    axis: 0,
                }
            } else if s_idx >= shape[1] {
                SimpleCalculationError::OutOfBoundsError {
                    index: s_idx,
                    length: shape[1],
                    axis: 1,
                }
            } else {
                unreachable!(
                    "Invalid indices for array: t_idx = {}, s_idx = {}, shape = {:?}",
                    t_idx, s_idx, shape
                )
            }
        })?;
        Ok(*value)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::StateBuilder;
    use crate::test_utils::default_domain;
    use float_cmp::assert_approx_eq;
    use ndarray::Array;

    #[test]
    fn test_array1_parameter() {
        let domain = default_domain();

        let data = Array::range(0.0, 366.0, 1.0);
        let p = Array1Parameter::new("my-array-parameter".into(), data, None);

        let spv = StateBuilder::new(Vec::new(), 0).build();

        let mut state = p
            .setup(domain.time().timesteps(), domain.scenarios().indices().first().unwrap())
            .unwrap();

        for ts in domain.time().timesteps().iter() {
            for si in domain.scenarios().indices().iter() {
                assert_approx_eq!(
                    f64,
                    p.compute(ts, si, &spv.get_simple_parameter_values(), &mut state)
                        .unwrap(),
                    ts.index as f64
                );
            }
        }
    }

    #[test]
    /// Test `Array2Parameter` returns the correct value.
    fn test_array2_parameter() {
        let domain = default_domain();

        let data = Array::range(0.0, 366.0, 1.0);
        let data = data.insert_axis(Axis(1));
        let p = Array2Parameter::new("my-array-parameter".into(), data, 0, None);

        let spv = StateBuilder::new(Vec::new(), 0).build();

        let mut state = p
            .setup(domain.time().timesteps(), domain.scenarios().indices().first().unwrap())
            .unwrap();

        for ts in domain.time().timesteps().iter() {
            for si in domain.scenarios().indices().iter() {
                assert_approx_eq!(
                    f64,
                    p.compute(ts, si, &spv.get_simple_parameter_values(), &mut state)
                        .unwrap(),
                    ts.index as f64
                );
            }
        }
    }

    #[test]
    /// Test `Array2Parameter` returns the correct value.
    fn test_array2_parameter_not_enough_data() {
        let domain = default_domain();

        let data = Array::range(0.0, 5.0, 1.0);
        let data = data.insert_axis(Axis(1));
        let p = Array2Parameter::new("my-array-parameter".into(), data, 0, None);

        let spv = StateBuilder::new(Vec::new(), 0).build();

        let mut state = p
            .setup(domain.time().timesteps(), domain.scenarios().indices().first().unwrap())
            .unwrap();

        for ts in domain.time().timesteps().iter() {
            for si in domain.scenarios().indices().iter() {
                if ts.index >= 5 {
                    // If the time-step index is out of bounds, we should return an error
                    assert!(
                        p.compute(ts, si, &spv.get_simple_parameter_values(), &mut state)
                            .is_err()
                    );
                } else {
                    // Otherwise, we should return the value
                    assert_approx_eq!(
                        f64,
                        p.compute(ts, si, &spv.get_simple_parameter_values(), &mut state)
                            .unwrap(),
                        ts.index as f64
                    );
                }
            }
        }
    }
}
