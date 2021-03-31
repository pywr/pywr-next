pub mod py;

use super::{NetworkState, PywrError};
use crate::scenario::ScenarioIndex;
use crate::timestep::Timestep;
use ndarray::{Array1, Array2};
use std::cell::RefCell;
use std::fmt;
use std::ops::Deref;
use std::rc::Rc;

pub type ParameterIndex = usize;
pub type ParameterRef = Rc<RefCell<Box<dyn _Parameter>>>;

/// Meta data common to all parameters.
#[derive(Debug)]
pub struct ParameterMeta {
    pub name: String,
    pub comment: String,
}

impl ParameterMeta {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            comment: "".to_string(),
        }
    }
}

pub trait _Parameter {
    fn meta(&self) -> &ParameterMeta;
    fn before(&self) {}
    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network_state: &NetworkState,
        parameter_state: &[f64],
    ) -> Result<f64, PywrError>;
}

#[derive(Clone)]
pub struct Parameter(ParameterRef, ParameterIndex);

impl PartialEq for Parameter {
    fn eq(&self, other: &Parameter) -> bool {
        // TODO which
        self.1 == other.1
    }
}

impl fmt::Debug for Parameter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Parameter").field(&self.name()).field(&self.1).finish()
    }
}

impl Parameter {
    pub fn new(parameter: Box<dyn _Parameter>, index: ParameterIndex) -> Self {
        Self(Rc::new(RefCell::new(parameter)), index)
    }

    pub fn index(&self) -> ParameterIndex {
        self.1
    }

    pub fn name(&self) -> String {
        self.0.borrow().deref().meta().name.to_string()
    }

    pub fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network_state: &NetworkState,
        parameter_state: &[f64],
    ) -> Result<f64, PywrError> {
        self.0
            .borrow()
            .deref()
            .compute(timestep, scenario_index, network_state, parameter_state)
    }
}

pub struct ConstantParameter {
    meta: ParameterMeta,
    value: f64,
}

impl ConstantParameter {
    pub fn new(name: &str, value: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            value,
        }
    }
}

impl _Parameter for ConstantParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _state: &NetworkState,
        _parameter_state: &[f64],
    ) -> Result<f64, PywrError> {
        Ok(self.value)
    }
}

pub struct VectorParameter {
    meta: ParameterMeta,
    values: Vec<f64>,
}

impl VectorParameter {
    pub fn new(name: &str, values: Vec<f64>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            values,
        }
    }
}

impl _Parameter for VectorParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _state: &NetworkState,
        _parameter_state: &[f64],
    ) -> Result<f64, PywrError> {
        match self.values.get(timestep.index) {
            Some(v) => Ok(*v),
            None => Err(PywrError::TimestepIndexOutOfRange),
        }
    }
}

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

impl _Parameter for Array1Parameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _state: &NetworkState,
        _parameter_state: &[f64],
    ) -> Result<f64, PywrError> {
        // This panics if out-of-bounds
        let value = self.array[[timestep.index]];
        Ok(value)
    }
}

pub struct Array2Parameter {
    meta: ParameterMeta,
    array: Array2<f64>,
}

impl Array2Parameter {
    pub fn new(name: &str, array: Array2<f64>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            array,
        }
    }
}

impl _Parameter for Array2Parameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _state: &NetworkState,
        _parameter_state: &[f64],
    ) -> Result<f64, PywrError> {
        // This panics if out-of-bounds
        // TODO scenarios!
        Ok(self.array[[timestep.index, 0]])
    }
}

pub enum AggFunc {
    Sum,
    Product,
    Mean,
    Min,
    Max,
}

pub struct AggregatedParameter {
    meta: ParameterMeta,
    parameters: Vec<Parameter>,
    agg_func: AggFunc,
}

impl AggregatedParameter {
    pub fn new(name: &str, parameters: Vec<Parameter>, agg_func: AggFunc) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            parameters,
            agg_func,
        }
    }
}

impl _Parameter for AggregatedParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _state: &NetworkState,
        parameter_state: &[f64],
    ) -> Result<f64, PywrError> {
        // TODO scenarios!

        let value: f64 = match self.agg_func {
            AggFunc::Sum => {
                let mut total = 0.0_f64;
                for p in &self.parameters {
                    total += match parameter_state.get(p.index()) {
                        Some(v) => v,
                        None => return Err(PywrError::ParameterIndexNotFound),
                    };
                }
                total
            }
            AggFunc::Mean => {
                let mut total = 0.0_f64;
                for p in &self.parameters {
                    total += match parameter_state.get(p.index()) {
                        Some(v) => v,
                        None => return Err(PywrError::ParameterIndexNotFound),
                    };
                }
                total / self.parameters.len() as f64
            }
            AggFunc::Max => {
                let mut total = f64::MIN;
                for p in &self.parameters {
                    total = total.max(match parameter_state.get(p.index()) {
                        Some(v) => *v,
                        None => return Err(PywrError::ParameterIndexNotFound),
                    });
                }
                total
            }
            AggFunc::Min => {
                let mut total = f64::MAX;
                for p in &self.parameters {
                    total = total.min(match parameter_state.get(p.index()) {
                        Some(v) => *v,
                        None => return Err(PywrError::ParameterIndexNotFound),
                    });
                }
                total
            }
            AggFunc::Product => {
                let mut total = 1.0_f64;
                for p in &self.parameters {
                    total *= match parameter_state.get(p.index()) {
                        Some(v) => *v,
                        None => return Err(PywrError::ParameterIndexNotFound),
                    };
                }
                total
            }
        };

        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_almost_eq;
    use crate::state::ParameterState;
    use crate::timestep::Timestepper;
    use ndarray::prelude::*;
    use std::f64::consts::PI;

    fn test_timestepper() -> Timestepper {
        Timestepper::new("2020-01-01", "2020-12-31", "%Y-%m-%d", 1).unwrap()
    }

    #[test]
    /// Test `ConstantParameter` returns the correct value.
    fn test_constant_parameter() {
        let param = ConstantParameter::new("my-parameter", PI);
        let timestepper = test_timestepper();
        let si = ScenarioIndex {
            index: 0,
            indices: vec![0],
        };

        for ts in timestepper.timesteps().iter() {
            let ns = NetworkState::new();
            let ps = ParameterState::new();
            assert_almost_eq!(param.compute(ts, &si, &ns, &ps).unwrap(), PI);
        }
    }

    #[test]
    /// Test `Array2Parameter` returns the correct value.
    fn test_array2_parameter() {
        let data = Array::range(0.0, 366.0, 1.0);
        let data = data.insert_axis(Axis(1));
        let param = Array2Parameter::new("my-array-parameter", data);
        let timestepper = test_timestepper();
        let si = ScenarioIndex {
            index: 0,
            indices: vec![0],
        };

        for ts in timestepper.timesteps().iter() {
            let ns = NetworkState::new();
            let ps = ParameterState::new();
            assert_almost_eq!(param.compute(ts, &si, &ns, &ps).unwrap(), ts.index as f64);
        }
    }

    #[test]
    #[should_panic] // TODO this is not great; but a problem with using ndarray slicing.
    /// Test `Array2Parameter` returns the correct value.
    fn test_array2_parameter_not_enough_data() {
        let data = Array::range(0.0, 100.0, 1.0);
        let data = data.insert_axis(Axis(1));
        let param = Array2Parameter::new("my-array-parameter", data);
        let timestepper = test_timestepper();
        let si = ScenarioIndex {
            index: 0,
            indices: vec![0],
        };

        for ts in timestepper.timesteps().iter() {
            let ns = NetworkState::new();
            let ps = ParameterState::new();
            let value = param.compute(ts, &si, &ns, &ps);
        }
    }

    // #[test]
    // fn test_aggregated_parameter_sum() {
    //     let mut parameter_state = ParameterState::new();
    //     // Parameter's 0 and 1 have values of 10.0 and 2.0 respectively
    //     parameter_state.push(10.0);
    //     parameter_state.push(2.0);
    //     test_aggregated_parameter(vec![0, 1], &parameter_state, AggFunc::Sum, 12.0);
    // }
    //
    // #[test]
    // fn test_aggregated_parameter_mean() {
    //     let mut parameter_state = ParameterState::new();
    //     // Parameter's 0 and 1 have values of 10.0 and 2.0 respectively
    //     parameter_state.push(10.0);
    //     parameter_state.push(2.0);
    //     test_aggregated_parameter(vec![0, 1], &parameter_state, AggFunc::Mean, 6.0);
    // }
    //
    // #[test]
    // fn test_aggregated_parameter_max() {
    //     let mut parameter_state = ParameterState::new();
    //     // Parameter's 0 and 1 have values of 10.0 and 2.0 respectively
    //     parameter_state.push(10.0);
    //     parameter_state.push(2.0);
    //     test_aggregated_parameter(vec![0, 1], &parameter_state, AggFunc::Max, 10.0);
    // }
    //
    // #[test]
    // fn test_aggregated_parameter_min() {
    //     let mut parameter_state = ParameterState::new();
    //     // Parameter's 0 and 1 have values of 10.0 and 2.0 respectively
    //     parameter_state.push(10.0);
    //     parameter_state.push(2.0);
    //     test_aggregated_parameter(vec![0, 1], &parameter_state, AggFunc::Min, 2.0);
    // }
    //
    // #[test]
    // fn test_aggregated_parameter_product() {
    //     let mut parameter_state = ParameterState::new();
    //     // Parameter's 0 and 1 have values of 10.0 and 2.0 respectively
    //     parameter_state.push(10.0);
    //     parameter_state.push(2.0);
    //     test_aggregated_parameter(vec![0, 1], &parameter_state, AggFunc::Product, 20.0);
    // }
    //
    // /// Test `AggregatedParameter` returns the correct value.
    // fn test_aggregated_parameter(
    //     parameter_indices: Vec<ParameterIndex>,
    //     parameter_state: &ParameterState,
    //     agg_func: AggFunc,
    //     expected: f64,
    // ) {
    //     let param = AggregatedParameter::new("my-aggregation", parameters, agg_func);
    //     let timestepper = test_timestepper();
    //     let si = ScenarioIndex {
    //         index: 0,
    //         indices: vec![0],
    //     };
    //
    //     for ts in timestepper.timesteps().iter() {
    //         let ns = NetworkState::new();
    //         assert_almost_eq!(param.compute(ts, &si, &ns, &parameter_state).unwrap(), expected);
    //     }
    // }
}
