mod aggregated;
mod aggregated_index;
mod array;
mod asymmetric;
mod control_curves;
mod indexed_array;
mod max;
mod negative;
mod polynomial;
mod profiles;
pub mod py;
pub mod simple_wasm;
mod threshold;

use std::any::Any;
// Re-imports
use super::{NetworkState, PywrError};
use crate::scenario::ScenarioIndex;
pub use aggregated::{AggFunc, AggregatedParameter};
pub use aggregated_index::{AggIndexFunc, AggregatedIndexParameter};
pub use asymmetric::AsymmetricSwitchIndexParameter;
pub use control_curves::{
    ControlCurveIndexParameter, ControlCurveParameter, InterpolatedParameter, PiecewiseInterpolatedParameter,
};
pub use indexed_array::IndexedArrayParameter;
pub use max::MaxParameter;
pub use negative::NegativeParameter;
pub use polynomial::Polynomial1DParameter;
pub use profiles::{DailyProfileParameter, MonthlyProfileParameter, UniformDrawdownProfileParameter};
pub use threshold::{Predicate, ThresholdParameter};

use crate::model::Model;
use crate::state::State;
use crate::timestep::Timestep;
use ndarray::{Array1, Array2};
use std::fmt;
use std::fmt::{Display, Formatter};
use std::ops::Deref;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct ParameterIndex(usize);

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct IndexParameterIndex(usize);

impl ParameterIndex {
    pub fn new(idx: usize) -> Self {
        Self(idx)
    }
}

impl IndexParameterIndex {
    pub fn new(idx: usize) -> Self {
        Self(idx)
    }
}

impl Deref for ParameterIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for IndexParameterIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for ParameterIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Display for IndexParameterIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

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

pub trait Parameter: Send + Sync {
    fn meta(&self) -> &ParameterMeta;
    fn name(&self) -> &str {
        self.meta().name.as_str()
    }
    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn Any + Send>>, PywrError> {
        Ok(None)
    }
    fn before(&self) {}
    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Model,
        state: &State,
        internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<f64, PywrError>;
}

// pub trait MultiStateParameter {
//     fn before(&self) {}
//     fn compute(
//         &mut self,
//         timestep: &Timestep,
//         scenario_index: &ScenarioIndex,
//         network_state: &NetworkState,
//         parameter_state: &ParameterState,
//     ) -> Result<(), PywrError>;
//
//     fn get_value(&mut self, key: &str) -> Result<f64, PywrError>;
// }

pub trait IndexParameter: Send + Sync {
    fn meta(&self) -> &ParameterMeta;
    fn name(&self) -> &str {
        self.meta().name.as_str()
    }
    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn Any + Send>>, PywrError> {
        Ok(None)
    }
    fn before(&self) {}
    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Model,
        state: &State,
        internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<usize, PywrError>;
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum FloatValue {
    Constant(f64),
    Dynamic(ParameterIndex),
}

impl FloatValue {
    pub fn get_value(&self, state: &State) -> Result<f64, PywrError> {
        match self {
            Self::Constant(v) => Ok(*v),
            Self::Dynamic(p) => state.get_parameter_value(*p),
        }
    }
}

#[derive(Copy, Clone)]
pub enum IndexValue {
    Constant(usize),
    Dynamic(IndexParameterIndex),
}

impl IndexValue {
    pub fn get_index(&self, state: &State) -> Result<usize, PywrError> {
        match self {
            Self::Constant(v) => Ok(*v),
            Self::Dynamic(p) => state.get_parameter_index(*p),
        }
    }
}

pub enum ParameterType {
    Parameter(ParameterIndex),
    Index(IndexParameterIndex),
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

impl Parameter for ConstantParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _model: &Model,
        _state: &State,
        _internal_state: &mut Option<Box<dyn Any + Send>>,
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

impl Parameter for VectorParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _model: &Model,
        _state: &State,
        _internal_state: &mut Option<Box<dyn Any + Send>>,
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

impl Parameter for Array1Parameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _model: &Model,
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
}

impl Array2Parameter {
    pub fn new(name: &str, array: Array2<f64>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            array,
        }
    }
}

impl Parameter for Array2Parameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        _model: &Model,
        _state: &State,
        _internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<f64, PywrError> {
        // This panics if out-of-bounds
        Ok(self.array[[timestep.index, scenario_index.index]])
    }
}

#[cfg(test)]
mod tests {

    use crate::timestep::Timestepper;
    use time::macros::date;

    fn default_timestepper() -> Timestepper {
        Timestepper::new(date!(2020 - 01 - 01), date!(2020 - 01 - 15), 1)
    }

    // #[test]
    // /// Test `ConstantParameter` returns the correct value.
    // fn test_constant_parameter() {
    //     let mut param = ConstantParameter::new("my-parameter", PI);
    //     let timestepper = test_timestepper();
    //     let si = ScenarioIndex {
    //         index: 0,
    //         indices: vec![0],
    //     };
    //
    //     for ts in timestepper.timesteps().iter() {
    //         let ns = NetworkState::new();
    //         let ps = ParameterState::new();
    //         assert_almost_eq!(param.compute(ts, &si, &ns, &ps).unwrap(), PI);
    //     }
    // }

    // #[test]
    // /// Test `Array2Parameter` returns the correct value.
    // fn test_array2_parameter() {
    //     let data = Array::range(0.0, 366.0, 1.0);
    //     let data = data.insert_axis(Axis(1));
    //     let mut param = Array2Parameter::new("my-array-parameter", data);
    //     let timestepper = test_timestepper();
    //     let si = ScenarioIndex {
    //         index: 0,
    //         indices: vec![0],
    //     };
    //
    //     for ts in timestepper.timesteps().iter() {
    //         let ns = NetworkState::new();
    //         let ps = ParameterState::new();
    //         assert_almost_eq!(param.compute(ts, &si, &ns, &ps).unwrap(), ts.index as f64);
    //     }
    // }

    // #[test]
    // #[should_panic] // TODO this is not great; but a problem with using ndarray slicing.
    // /// Test `Array2Parameter` returns the correct value.
    // fn test_array2_parameter_not_enough_data() {
    //     let data = Array::range(0.0, 100.0, 1.0);
    //     let data = data.insert_axis(Axis(1));
    //     let mut param = Array2Parameter::new("my-array-parameter", data);
    //     let timestepper = test_timestepper();
    //     let si = ScenarioIndex {
    //         index: 0,
    //         indices: vec![0],
    //     };
    //
    //     for ts in timestepper.timesteps().iter() {
    //         let ns = NetworkState::new();
    //         let ps = ParameterState::new();
    //         let value = param.compute(ts, &si, &ns, &ps);
    //     }
    // }

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
