mod activation_function;
mod aggregated;
mod aggregated_index;
mod array;
mod asymmetric;
mod constant;
mod control_curves;
mod delay;
mod discount_factor;
mod division;
mod indexed_array;
mod interpolate;
mod interpolated;
mod max;
mod min;
mod negative;
mod offset;
mod polynomial;
mod profiles;
mod py;
mod rhai;
mod threshold;
mod vector;

use std::any::Any;
// Re-imports
pub use self::rhai::RhaiParameter;
use super::PywrError;
use crate::network::Network;
use crate::scenario::ScenarioIndex;
use crate::state::{MultiValue, ParameterState, State};
use crate::timestep::Timestep;
pub use activation_function::ActivationFunction;
pub use aggregated::{AggFunc, AggregatedParameter};
pub use aggregated_index::{AggIndexFunc, AggregatedIndexParameter};
pub use array::{Array1Parameter, Array2Parameter};
pub use asymmetric::AsymmetricSwitchIndexParameter;
pub use constant::ConstantParameter;
pub use control_curves::{
    ApportionParameter, ControlCurveIndexParameter, ControlCurveInterpolatedParameter, ControlCurveParameter,
    PiecewiseInterpolatedParameter, VolumeBetweenControlCurvesParameter,
};
pub use delay::DelayParameter;
pub use discount_factor::DiscountFactorParameter;
pub use division::DivisionParameter;
pub use indexed_array::IndexedArrayParameter;
pub use interpolate::{interpolate, linear_interpolation, InterpolationError};
pub use interpolated::InterpolatedParameter;
pub use max::MaxParameter;
pub use min::MinParameter;
pub use negative::NegativeParameter;
pub use offset::OffsetParameter;
pub use polynomial::Polynomial1DParameter;
pub use profiles::{
    DailyProfileParameter, MonthlyInterpDay, MonthlyProfileParameter, RadialBasisFunction, RbfProfileParameter,
    RbfProfileVariableConfig, UniformDrawdownProfileParameter,
};
pub use py::PyParameter;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
pub use threshold::{Predicate, ThresholdParameter};
pub use vector::VectorParameter;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct ParameterIndex(usize);

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct IndexParameterIndex(usize);

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MultiValueParameterIndex(usize);

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

impl MultiValueParameterIndex {
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

impl Deref for MultiValueParameterIndex {
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

impl Display for MultiValueParameterIndex {
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

/// Helper function to downcast to internal parameter state and print a helpful panic
/// message if this fails.
pub fn downcast_internal_state_mut<T: 'static>(internal_state: &mut Option<Box<dyn ParameterState>>) -> &mut T {
    // Downcast the internal state to the correct type
    match internal_state {
        Some(internal) => match internal.as_mut().as_any_mut().downcast_mut::<T>() {
            Some(pa) => pa,
            None => panic!("Internal state did not downcast to the correct type! :("),
        },
        None => panic!("No internal state defined when one was expected! :("),
    }
}

/// Helper function to downcast to internal parameter state and print a helpful panic
/// message if this fails.
pub fn downcast_internal_state_ref<T: 'static>(internal_state: &Option<Box<dyn ParameterState>>) -> &T {
    // Downcast the internal state to the correct type
    match internal_state {
        Some(internal) => match internal.as_ref().as_any().downcast_ref::<T>() {
            Some(pa) => pa,
            None => panic!("Internal state did not downcast to the correct type! :("),
        },
        None => panic!("No internal state defined when one was expected! :("),
    }
}

// TODO It might be possible to make these three traits into a single generic trait
pub trait Parameter: Send + Sync {
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn meta(&self) -> &ParameterMeta;
    fn name(&self) -> &str {
        self.meta().name.as_str()
    }

    fn setup(
        &self,
        #[allow(unused_variables)] timesteps: &[Timestep],
        #[allow(unused_variables)] scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, PywrError> {
        Ok(None)
    }

    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, PywrError>;

    fn after(
        &self,
        #[allow(unused_variables)] timestep: &Timestep,
        #[allow(unused_variables)] scenario_index: &ScenarioIndex,
        #[allow(unused_variables)] model: &Network,
        #[allow(unused_variables)] state: &State,
        #[allow(unused_variables)] internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), PywrError> {
        Ok(())
    }

    /// Return the parameter as a [`VariableParameter<f64>'] if it supports being a variable.
    fn as_f64_variable(&self) -> Option<&dyn VariableParameter<f64>> {
        None
    }

    /// Return the parameter as a [`VariableParameter<f64>'] if it supports being a variable.
    fn as_f64_variable_mut(&mut self) -> Option<&mut dyn VariableParameter<f64>> {
        None
    }

    /// Can this parameter be a variable
    fn can_be_f64_variable(&self) -> bool {
        self.as_f64_variable().is_some()
    }

    /// Is this parameter an active variable
    fn is_f64_variable_active(&self) -> bool {
        match self.as_f64_variable() {
            Some(var) => var.is_active(),
            None => false,
        }
    }

    /// Return the parameter as a [`VariableParameter<u32>'] if it supports being a variable.
    fn as_u32_variable(&self) -> Option<&dyn VariableParameter<u32>> {
        None
    }

    /// Return the parameter as a [`VariableParameter<u32>'] if it supports being a variable.
    fn as_u32_variable_mut(&mut self) -> Option<&mut dyn VariableParameter<u32>> {
        None
    }

    /// Can this parameter be a variable
    fn can_be_u32_variable(&self) -> bool {
        self.as_u32_variable().is_some()
    }

    /// Is this parameter an active variable
    fn is_u32_variable_active(&self) -> bool {
        match self.as_u32_variable() {
            Some(var) => var.is_active(),
            None => false,
        }
    }
}

pub trait IndexParameter: Send + Sync {
    fn meta(&self) -> &ParameterMeta;
    fn name(&self) -> &str {
        self.meta().name.as_str()
    }

    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, PywrError> {
        Ok(None)
    }

    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<usize, PywrError>;

    fn after(
        &self,
        #[allow(unused_variables)] timestep: &Timestep,
        #[allow(unused_variables)] scenario_index: &ScenarioIndex,
        #[allow(unused_variables)] model: &Network,
        #[allow(unused_variables)] state: &State,
        #[allow(unused_variables)] internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), PywrError> {
        Ok(())
    }
}

pub trait MultiValueParameter: Send + Sync {
    fn meta(&self) -> &ParameterMeta;
    fn name(&self) -> &str {
        self.meta().name.as_str()
    }
    fn setup(
        &self,
        #[allow(unused_variables)] timesteps: &[Timestep],
        #[allow(unused_variables)] scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, PywrError> {
        Ok(None)
    }

    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<MultiValue, PywrError>;

    fn after(
        &self,
        #[allow(unused_variables)] timestep: &Timestep,
        #[allow(unused_variables)] scenario_index: &ScenarioIndex,
        #[allow(unused_variables)] model: &Network,
        #[allow(unused_variables)] state: &State,
        #[allow(unused_variables)] internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), PywrError> {
        Ok(())
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
    Multi(MultiValueParameterIndex),
}

/// A parameter that can be optimised.
///
/// This trait is used to allow parameter's internal values to be accessed and altered by
/// external algorithms. It is primarily designed to be used by the optimisation algorithms
/// such as multi-objective evolutionary algorithms. The trait is generic to the type of
/// the variable values being optimised but these will typically by `f64` and `u32`.
pub trait VariableParameter<T> {
    fn meta(&self) -> &ParameterMeta;
    fn name(&self) -> &str {
        self.meta().name.as_str()
    }

    /// Is this variable activated (i.e. should be used in optimisation)
    fn is_active(&self) -> bool;
    /// Return the number of variables required
    fn size(&self) -> usize;
    /// Apply new variable values to the parameter's state
    fn set_variables(
        &self,
        values: &[T],
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), PywrError>;
    /// Get the current variable values
    fn get_variables(&self, internal_state: &Option<Box<dyn ParameterState>>) -> Option<Vec<T>>;
    /// Get variable lower bounds
    fn get_lower_bounds(&self) -> Result<Vec<T>, PywrError>;
    /// Get variable upper bounds
    fn get_upper_bounds(&self) -> Result<Vec<T>, PywrError>;
}

#[cfg(test)]
mod tests {

    use crate::timestep::Timestepper;
    use time::macros::date;

    // TODO tests need re-enabling
    #[allow(dead_code)]
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
