mod activation_function;
mod aggregated;
mod aggregated_index;
mod array;
mod asymmetric;
mod constant;
mod constant_scenario;
mod control_curves;
mod delay;
mod difference;
mod discount_factor;
mod division;
mod errors;
mod hydropower;
mod indexed_array;
mod interpolate;
mod interpolated;
mod max;
mod min;
mod multi_threshold;
mod muskingum;
mod negative;
mod negativemax;
mod negativemin;
mod offset;
mod polynomial;
mod profiles;
#[cfg(feature = "pyo3")]
mod py;
mod rolling;
mod threshold;
mod vector;

use std::any::Any;
// Re-imports
use crate::network::Network;
use crate::scenario::ScenarioIndex;
use crate::state::{ConstParameterValues, MultiValue, SetStateError, SimpleParameterValues, State};
use crate::timestep::Timestep;
pub use activation_function::ActivationFunction;
pub use aggregated::AggregatedParameter;
pub use aggregated_index::AggregatedIndexParameter;
pub use array::{Array1Parameter, Array2Parameter};
pub use asymmetric::AsymmetricSwitchIndexParameter;
pub use constant::ConstantParameter;
pub use constant_scenario::ConstantScenarioParameter;
pub use control_curves::{
    ApportionParameter, ControlCurveIndexParameter, ControlCurveInterpolatedParameter, ControlCurveParameter,
    PiecewiseInterpolatedParameter, VolumeBetweenControlCurvesParameter,
};
pub use delay::DelayParameter;
pub use difference::DifferenceParameter;
pub use discount_factor::DiscountFactorParameter;
pub use division::DivisionParameter;
use errors::{ConstCalculationError, SimpleCalculationError};
pub use errors::{ParameterCalculationError, ParameterSetupError};
pub use hydropower::{HydropowerTargetData, HydropowerTargetParameter};
pub use indexed_array::IndexedArrayParameter;
pub use interpolate::{InterpolationError, interpolate, linear_interpolation};
pub use interpolated::InterpolatedParameter;
pub use max::MaxParameter;
pub use min::MinParameter;
pub use multi_threshold::MultiThresholdParameter;
pub use muskingum::{MuskingumInitialCondition, MuskingumParameter};
pub use negative::NegativeParameter;
pub use negativemax::NegativeMaxParameter;
pub use negativemin::NegativeMinParameter;
pub use offset::OffsetParameter;
pub use polynomial::Polynomial1DParameter;
pub use profiles::{
    DailyProfileParameter, DiurnalProfileParameter, MonthlyInterpDay, MonthlyProfileParameter, RadialBasisFunction,
    RbfProfileParameter, RbfProfileVariableConfig, UniformDrawdownProfileParameter, WeeklyInterpDay,
    WeeklyProfileError, WeeklyProfileParameter, WeeklyProfileValues,
};
#[cfg(feature = "pyo3")]
pub use py::{ParameterInfo, PyClassParameter, PyFuncParameter};
pub use rolling::RollingParameter;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::Deref;
use thiserror::Error;
pub use threshold::{Predicate, ThresholdParameter};
pub use vector::VectorParameter;

/// Simple parameter index.
///
/// This is a wrapper around usize that is used to index parameters in the state. It is
/// generic over the type of the value that the parameter returns.
#[derive(Debug)]
pub struct ConstParameterIndex<T> {
    idx: usize,
    phantom: PhantomData<T>,
}

// These implementations are required because the derive macro does not work well with PhantomData.
// See issue: https://github.com/rust-lang/rust/issues/26925
impl<T> Clone for ConstParameterIndex<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for ConstParameterIndex<T> {}
impl<T> PartialEq<Self> for ConstParameterIndex<T> {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
    }
}

impl<T> Eq for ConstParameterIndex<T> {}

impl<T> ConstParameterIndex<T> {
    pub fn new(idx: usize) -> Self {
        Self {
            idx,
            phantom: PhantomData,
        }
    }
}

impl<T> Deref for ConstParameterIndex<T> {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.idx
    }
}

impl<T> Display for ConstParameterIndex<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.idx)
    }
}

/// Simple parameter index.
///
/// This is a wrapper around usize that is used to index parameters in the state. It is
/// generic over the type of the value that the parameter returns.
#[derive(Debug)]
pub struct SimpleParameterIndex<T> {
    idx: usize,
    phantom: PhantomData<T>,
}

// These implementations are required because the derive macro does not work well with PhantomData.
// See issue: https://github.com/rust-lang/rust/issues/26925
impl<T> Clone for SimpleParameterIndex<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for SimpleParameterIndex<T> {}
impl<T> PartialEq<Self> for SimpleParameterIndex<T> {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
    }
}

impl<T> Eq for SimpleParameterIndex<T> {}

impl<T> SimpleParameterIndex<T> {
    pub fn new(idx: usize) -> Self {
        Self {
            idx,
            phantom: PhantomData,
        }
    }
}

impl<T> Deref for SimpleParameterIndex<T> {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.idx
    }
}

impl<T> Display for SimpleParameterIndex<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.idx)
    }
}

/// Generic parameter index.
///
/// This is a wrapper around usize that is used to index parameters in the state. It is
/// generic over the type of the value that the parameter returns.
#[derive(Debug)]
pub struct GeneralParameterIndex<T> {
    idx: usize,
    phantom: PhantomData<T>,
}

// These implementations are required because the derive macro does not work well with PhantomData.
// See issue: https://github.com/rust-lang/rust/issues/26925
impl<T> Clone for GeneralParameterIndex<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for GeneralParameterIndex<T> {}

impl<T> PartialEq<Self> for GeneralParameterIndex<T> {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
    }
}

impl<T> Eq for GeneralParameterIndex<T> {}

impl<T> GeneralParameterIndex<T> {
    pub fn new(idx: usize) -> Self {
        Self {
            idx,
            phantom: PhantomData,
        }
    }
}

impl<T> Deref for GeneralParameterIndex<T> {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.idx
    }
}

impl<T> Display for GeneralParameterIndex<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.idx)
    }
}

impl<T> Hash for GeneralParameterIndex<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.idx.hash(state);
    }
}

#[derive(Debug, Copy, Clone)]
pub enum ParameterIndex<T> {
    Const(ConstParameterIndex<T>),
    Simple(SimpleParameterIndex<T>),
    General(GeneralParameterIndex<T>),
}

impl<T> PartialEq<Self> for ParameterIndex<T> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Const(idx1), Self::Const(idx2)) => idx1 == idx2,
            (Self::Simple(idx1), Self::Simple(idx2)) => idx1 == idx2,
            (Self::General(idx1), Self::General(idx2)) => idx1 == idx2,
            _ => false,
        }
    }
}

impl<T> Eq for ParameterIndex<T> {}

impl<T> Display for ParameterIndex<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Const(idx) => write!(f, "{idx}",),
            Self::Simple(idx) => write!(f, "{idx}",),
            Self::General(idx) => write!(f, "{idx}",),
        }
    }
}
impl<T> From<GeneralParameterIndex<T>> for ParameterIndex<T> {
    fn from(idx: GeneralParameterIndex<T>) -> Self {
        Self::General(idx)
    }
}

impl<T> From<SimpleParameterIndex<T>> for ParameterIndex<T> {
    fn from(idx: SimpleParameterIndex<T>) -> Self {
        Self::Simple(idx)
    }
}

impl<T> From<ConstParameterIndex<T>> for ParameterIndex<T> {
    fn from(idx: ConstParameterIndex<T>) -> Self {
        Self::Const(idx)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterName {
    name: String,
    // Optional sub-name for parameters that are part of multi-parameter groups
    sub_name: Option<String>,
    // Optional parent name for parameters that are added by a node
    parent: Option<String>,
}

impl ParameterName {
    pub fn new(name: &str, parent: Option<&str>) -> Self {
        Self {
            name: name.to_string(),
            sub_name: None,
            parent: parent.map(|p| p.to_string()),
        }
    }

    pub fn new_with_subname(name: &str, sub_name: Option<&str>, parent: Option<&str>) -> Self {
        Self {
            name: name.to_string(),
            sub_name: sub_name.map(|s| s.to_string()),
            parent: parent.map(|p| p.to_string()),
        }
    }

    /// Get the parameter name.
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// Get the parameter optional parent's name.
    pub fn parent(&self) -> Option<String> {
        self.parent.clone()
    }
}

impl Display for ParameterName {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self.parent {
            Some(parent) => write!(f, "{}.{}", parent, self.name),
            None => write!(f, "{}", self.name),
        }
    }
}

impl From<&str> for ParameterName {
    fn from(name: &str) -> Self {
        Self {
            name: name.to_string(),
            sub_name: None,
            parent: None,
        }
    }
}

/// Meta data common to all parameters.
#[derive(Debug, Clone)]
pub struct ParameterMeta {
    name: ParameterName,
}

impl ParameterMeta {
    pub fn new(name: ParameterName) -> Self {
        Self { name }
    }
}

pub trait ParameterState: Any + Send {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T> ParameterState for T
where
    T: Any + Send,
{
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

struct ParameterStatesByType {
    f64: Vec<Option<Box<dyn ParameterState>>>,
    u64: Vec<Option<Box<dyn ParameterState>>>,
    multi: Vec<Option<Box<dyn ParameterState>>>,
}

pub struct ParameterStates {
    constant: ParameterStatesByType,
    simple: ParameterStatesByType,
    general: ParameterStatesByType,
}

impl ParameterStates {
    /// Create new default states for the desired number of parameters.
    pub fn from_collection(
        collection: &ParameterCollection,
        timesteps: &[Timestep],
        scenario_index: &ScenarioIndex,
    ) -> Result<Self, ParameterCollectionSetupError> {
        let constant = collection.const_initial_states(timesteps, scenario_index)?;
        let simple = collection.simple_initial_states(timesteps, scenario_index)?;
        let general = collection.general_initial_states(timesteps, scenario_index)?;

        Ok(Self {
            constant,
            simple,
            general,
        })
    }

    pub fn get_f64_state(&self, index: ParameterIndex<f64>) -> Option<&Option<Box<dyn ParameterState>>> {
        match index {
            ParameterIndex::Const(idx) => self.constant.f64.get(*idx.deref()),
            ParameterIndex::Simple(idx) => self.simple.f64.get(*idx.deref()),
            ParameterIndex::General(idx) => self.general.f64.get(*idx.deref()),
        }
    }
    pub fn get_general_f64_state(&self, index: GeneralParameterIndex<f64>) -> Option<&Option<Box<dyn ParameterState>>> {
        self.general.f64.get(*index.deref())
    }

    pub fn get_simple_f64_state(&self, index: SimpleParameterIndex<f64>) -> Option<&Option<Box<dyn ParameterState>>> {
        self.simple.f64.get(*index.deref())
    }

    pub fn get_const_f64_state(&self, index: SimpleParameterIndex<f64>) -> Option<&Option<Box<dyn ParameterState>>> {
        self.constant.f64.get(*index.deref())
    }

    pub fn get_mut_f64_state(&mut self, index: ParameterIndex<f64>) -> Option<&mut Option<Box<dyn ParameterState>>> {
        match index {
            ParameterIndex::Const(idx) => self.constant.f64.get_mut(*idx.deref()),
            ParameterIndex::Simple(idx) => self.simple.f64.get_mut(*idx.deref()),
            ParameterIndex::General(idx) => self.general.f64.get_mut(*idx.deref()),
        }
    }

    pub fn get_general_mut_f64_state(
        &mut self,
        index: GeneralParameterIndex<f64>,
    ) -> Option<&mut Option<Box<dyn ParameterState>>> {
        self.general.f64.get_mut(*index.deref())
    }
    pub fn get_simple_mut_f64_state(
        &mut self,
        index: SimpleParameterIndex<f64>,
    ) -> Option<&mut Option<Box<dyn ParameterState>>> {
        self.simple.f64.get_mut(*index.deref())
    }
    pub fn get_const_mut_f64_state(
        &mut self,
        index: ConstParameterIndex<f64>,
    ) -> Option<&mut Option<Box<dyn ParameterState>>> {
        self.constant.f64.get_mut(*index.deref())
    }
    pub fn get_general_mut_u64_state(
        &mut self,
        index: GeneralParameterIndex<u64>,
    ) -> Option<&mut Option<Box<dyn ParameterState>>> {
        self.general.u64.get_mut(*index.deref())
    }

    pub fn get_simple_mut_u64_state(
        &mut self,
        index: SimpleParameterIndex<u64>,
    ) -> Option<&mut Option<Box<dyn ParameterState>>> {
        self.simple.u64.get_mut(*index.deref())
    }
    pub fn get_const_mut_u64_state(
        &mut self,
        index: ConstParameterIndex<u64>,
    ) -> Option<&mut Option<Box<dyn ParameterState>>> {
        self.constant.u64.get_mut(*index.deref())
    }

    pub fn get_general_mut_multi_state(
        &mut self,
        index: GeneralParameterIndex<MultiValue>,
    ) -> Option<&mut Option<Box<dyn ParameterState>>> {
        self.general.multi.get_mut(*index.deref())
    }

    pub fn get_simple_mut_multi_state(
        &mut self,
        index: SimpleParameterIndex<MultiValue>,
    ) -> Option<&mut Option<Box<dyn ParameterState>>> {
        self.simple.multi.get_mut(*index.deref())
    }

    pub fn get_const_mut_multi_state(
        &mut self,
        index: ConstParameterIndex<MultiValue>,
    ) -> Option<&mut Option<Box<dyn ParameterState>>> {
        self.constant.multi.get_mut(*index.deref())
    }
}

/// Helper function to downcast to internal parameter state and print a helpful panic
/// message if this fails.
fn downcast_internal_state_mut<T: 'static>(internal_state: &mut Option<Box<dyn ParameterState>>) -> &mut T {
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
fn downcast_internal_state_ref<T: 'static>(internal_state: &Option<Box<dyn ParameterState>>) -> &T {
    // Downcast the internal state to the correct type
    match internal_state {
        Some(internal) => match internal.as_ref().as_any().downcast_ref::<T>() {
            Some(pa) => pa,
            None => panic!("Internal state did not downcast to the correct type! :("),
        },
        None => panic!("No internal state defined when one was expected! :("),
    }
}

pub trait VariableConfig: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T> VariableConfig for T
where
    T: Any + Send + Sync,
{
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Helper function to downcast to variable config and print a helpful panic message if this fails.
pub fn downcast_variable_config_ref<T: 'static>(variable_config: &dyn VariableConfig) -> &T {
    // Downcast the internal state to the correct type
    match variable_config.as_any().downcast_ref::<T>() {
        Some(pa) => pa,
        None => panic!("Variable config did not downcast to the correct type! :("),
    }
}

/// A trait that defines a component that produces a value each time-step.
///
/// The trait is generic over the type of the value produced.
pub trait Parameter: Send + Sync {
    fn meta(&self) -> &ParameterMeta;
    fn name(&self) -> &ParameterName {
        &self.meta().name
    }

    fn setup(
        &self,
        #[allow(unused_variables)] timesteps: &[Timestep],
        #[allow(unused_variables)] scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, ParameterSetupError> {
        Ok(None)
    }

    /// Return the parameter as a [`VariableParameter<f64>`] if it supports being a variable.
    fn as_f64_variable(&self) -> Option<&dyn VariableParameter<f64>> {
        None
    }

    /// Return the parameter as a [`VariableParameter<f64>`] if it supports being a variable.
    fn as_f64_variable_mut(&mut self) -> Option<&mut dyn VariableParameter<f64>> {
        None
    }

    /// Can this parameter be a variable
    fn can_be_f64_variable(&self) -> bool {
        self.as_f64_variable().is_some()
    }

    /// Return the parameter as a [`VariableParameter<u32>`] if it supports being a variable.
    fn as_u32_variable(&self) -> Option<&dyn VariableParameter<u32>> {
        None
    }

    /// Return the parameter as a [`VariableParameter<u32>`] if it supports being a variable.
    fn as_u32_variable_mut(&mut self) -> Option<&mut dyn VariableParameter<u32>> {
        None
    }

    /// Can this parameter be a variable
    fn can_be_u32_variable(&self) -> bool {
        self.as_u32_variable().is_some()
    }
}

/// A trait that defines a component that produces a value each time-step.
///
/// The trait is generic over the type of the value produced.
pub trait GeneralParameter<T>: Parameter {
    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<T, ParameterCalculationError>;

    fn after(
        &self,
        #[allow(unused_variables)] timestep: &Timestep,
        #[allow(unused_variables)] scenario_index: &ScenarioIndex,
        #[allow(unused_variables)] model: &Network,
        #[allow(unused_variables)] state: &State,
        #[allow(unused_variables)] internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), ParameterCalculationError> {
        Ok(())
    }

    fn try_into_simple(&self) -> Option<Box<dyn SimpleParameter<T>>> {
        None
    }

    fn as_parameter(&self) -> &dyn Parameter;
}

/// A trait that defines a component that produces a value each time-step.
///
/// The trait is generic over the type of the value produced.
pub trait SimpleParameter<T>: Parameter {
    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        values: &SimpleParameterValues,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<T, SimpleCalculationError>;

    fn after(
        &self,
        #[allow(unused_variables)] timestep: &Timestep,
        #[allow(unused_variables)] scenario_index: &ScenarioIndex,
        #[allow(unused_variables)] values: &SimpleParameterValues,
        #[allow(unused_variables)] internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), SimpleCalculationError> {
        Ok(())
    }

    fn as_parameter(&self) -> &dyn Parameter;

    fn try_into_const(&self) -> Option<Box<dyn ConstParameter<T>>> {
        None
    }
}

/// A trait that defines a component that produces a value each time-step.
///
/// The trait is generic over the type of the value produced.
pub trait ConstParameter<T>: Parameter {
    fn compute(
        &self,
        scenario_index: &ScenarioIndex,
        values: &ConstParameterValues,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<T, ConstCalculationError>;

    fn as_parameter(&self) -> &dyn Parameter;
}

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
pub enum GeneralParameterType {
    Parameter(GeneralParameterIndex<f64>),
    Index(GeneralParameterIndex<u64>),
    Multi(GeneralParameterIndex<MultiValue>),
}

impl From<GeneralParameterIndex<f64>> for GeneralParameterType {
    fn from(idx: GeneralParameterIndex<f64>) -> Self {
        Self::Parameter(idx)
    }
}

impl From<GeneralParameterIndex<u64>> for GeneralParameterType {
    fn from(idx: GeneralParameterIndex<u64>) -> Self {
        Self::Index(idx)
    }
}

impl From<GeneralParameterIndex<MultiValue>> for GeneralParameterType {
    fn from(idx: GeneralParameterIndex<MultiValue>) -> Self {
        Self::Multi(idx)
    }
}

pub enum SimpleParameterType {
    Parameter(SimpleParameterIndex<f64>),
    Index(SimpleParameterIndex<u64>),
    Multi(SimpleParameterIndex<MultiValue>),
}

impl From<SimpleParameterIndex<f64>> for SimpleParameterType {
    fn from(idx: SimpleParameterIndex<f64>) -> Self {
        Self::Parameter(idx)
    }
}

impl From<SimpleParameterIndex<u64>> for SimpleParameterType {
    fn from(idx: SimpleParameterIndex<u64>) -> Self {
        Self::Index(idx)
    }
}

impl From<SimpleParameterIndex<MultiValue>> for SimpleParameterType {
    fn from(idx: SimpleParameterIndex<MultiValue>) -> Self {
        Self::Multi(idx)
    }
}

pub enum ConstParameterType {
    Parameter(ConstParameterIndex<f64>),
    Index(ConstParameterIndex<u64>),
    Multi(ConstParameterIndex<MultiValue>),
}

impl From<ConstParameterIndex<f64>> for ConstParameterType {
    fn from(idx: ConstParameterIndex<f64>) -> Self {
        Self::Parameter(idx)
    }
}

impl From<ConstParameterIndex<u64>> for ConstParameterType {
    fn from(idx: ConstParameterIndex<u64>) -> Self {
        Self::Index(idx)
    }
}

impl From<ConstParameterIndex<MultiValue>> for ConstParameterType {
    fn from(idx: ConstParameterIndex<MultiValue>) -> Self {
        Self::Multi(idx)
    }
}

pub enum ParameterType {
    Parameter(ParameterIndex<f64>),
    Index(ParameterIndex<u64>),
    Multi(ParameterIndex<MultiValue>),
}

impl From<ParameterIndex<f64>> for ParameterType {
    fn from(idx: ParameterIndex<f64>) -> Self {
        Self::Parameter(idx)
    }
}

impl From<ParameterIndex<u64>> for ParameterType {
    fn from(idx: ParameterIndex<u64>) -> Self {
        Self::Index(idx)
    }
}

impl From<ParameterIndex<MultiValue>> for ParameterType {
    fn from(idx: ParameterIndex<MultiValue>) -> Self {
        Self::Multi(idx)
    }
}

/// Error types for the trait [`VariableParameter`].
#[derive(Error, Debug)]
pub enum VariableParameterError {
    #[error("Incorrect number of values provided for parameter. Expected {expected}, received {received}")]
    IncorrectNumberOfValues { expected: usize, received: usize },
}

/// A parameter that can be optimised.
///
/// This trait is used to allow parameter's internal values to be accessed and altered by
/// external algorithms. It is primarily designed to be used by the optimisation algorithms
/// such as multi-objective evolutionary algorithms. The trait is generic to the type of
/// the variable values being optimised but these will typically by `f64` and `u32`.
pub trait VariableParameter<T> {
    fn meta(&self) -> &ParameterMeta;
    fn name(&self) -> &ParameterName {
        &self.meta().name
    }

    /// Return the number of variables required
    fn size(&self, variable_config: &dyn VariableConfig) -> usize;
    /// Apply new variable values to the parameter's state
    fn set_variables(
        &self,
        values: &[T],
        variable_config: &dyn VariableConfig,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), VariableParameterError>;
    /// Get the current variable values
    fn get_variables(&self, internal_state: &Option<Box<dyn ParameterState>>) -> Option<Vec<T>>;
    /// Get variable lower bounds
    fn get_lower_bounds(&self, variable_config: &dyn VariableConfig) -> Option<Vec<T>>;
    /// Get variable upper bounds
    fn get_upper_bounds(&self, variable_config: &dyn VariableConfig) -> Option<Vec<T>>;
}

#[derive(Debug, Clone, Copy)]
pub struct ParameterCollectionSize {
    pub const_f64: usize,
    pub const_usize: usize,
    pub const_multi: usize,
    pub simple_f64: usize,
    pub simple_usize: usize,
    pub simple_multi: usize,
    pub general_f64: usize,
    pub general_usize: usize,
    pub general_multi: usize,
}

/// Error types for the parameter collection.
///
/// These errors will typically occur when creating the collection. See also
/// [`ParameterCollectionSetupError`] and [`ParameterCollectionConstCalculationError`].
#[derive(Error, Debug)]
pub enum ParameterCollectionError {
    #[error("Parameter name `{0}` already exists")]
    NameAlreadyExists(String),
}

/// Error in a parameter during setup.
#[derive(Error, Debug)]
#[error("Error setting up parameter '{name}': {source}")]
pub struct ParameterCollectionSetupError {
    name: Box<ParameterName>,
    #[source]
    source: Box<ParameterSetupError>,
}

/// Error in a constant parameter during calculation.
#[derive(Error, Debug)]
pub enum ParameterCollectionConstCalculationError {
    #[error("Constant parameter F64 index '{0}' not found in collection")]
    F64IndexNotFound(ConstParameterIndex<f64>),
    #[error("Constant parameter U64 index '{0}' not found in collection")]
    U64IndexNotFound(ConstParameterIndex<u64>),
    #[error("Constant parameter Multi index '{0}' not found in collection")]
    MultiIndexNotFound(ConstParameterIndex<MultiValue>),
    #[error("Error calculating constant parameter '{name}': {source}")]
    CalculationError {
        name: ParameterName,
        #[source]
        source: ConstCalculationError,
    },
    #[error("Error setting state for constant F64 parameter '{name}': {source}")]
    F64SetStateError {
        name: ParameterName,
        #[source]
        source: SetStateError<ConstParameterIndex<f64>>,
    },
    #[error("Error setting state for constant U64 parameter '{name}': {source}")]
    U64SetStateError {
        name: ParameterName,
        #[source]
        source: SetStateError<ConstParameterIndex<u64>>,
    },
    #[error("Error setting state for constant Multi parameter '{name}': {source}")]
    MultiSetStateError {
        name: ParameterName,
        #[source]
        source: SetStateError<ConstParameterIndex<MultiValue>>,
    },
}

#[derive(Error, Debug)]
#[error("Error calculating simple parameter '{name}': {source}")]
pub enum ParameterCollectionSimpleCalculationError {
    #[error("Simple parameter F64 index '{0}' not found in collection")]
    F64IndexNotFound(SimpleParameterIndex<f64>),
    #[error("Simple parameter U64 index '{0}' not found in collection")]
    U64IndexNotFound(SimpleParameterIndex<u64>),
    #[error("Simple parameter Multi index '{0}' not found in collection")]
    MultiIndexNotFound(SimpleParameterIndex<MultiValue>),
    #[error("Error calculating simple parameter '{name}': {source}")]
    CalculationError {
        name: ParameterName,
        #[source]
        source: SimpleCalculationError,
    },
    #[error("Error setting state for simple F64 parameter '{name}': {source}")]
    F64SetStateError {
        name: ParameterName,
        #[source]
        source: SetStateError<SimpleParameterIndex<f64>>,
    },
    #[error("Error setting state for simple U64 parameter '{name}': {source}")]
    U64SetStateError {
        name: ParameterName,
        #[source]
        source: SetStateError<SimpleParameterIndex<u64>>,
    },
    #[error("Error setting state for simple Multi parameter '{name}': {source}")]
    MultiSetStateError {
        name: ParameterName,
        #[source]
        source: SetStateError<SimpleParameterIndex<MultiValue>>,
    },
}

/// A collection of parameters that return different types.
#[derive(Default)]
pub struct ParameterCollection {
    constant_f64: Vec<Box<dyn ConstParameter<f64>>>,
    constant_u64: Vec<Box<dyn ConstParameter<u64>>>,
    constant_multi: Vec<Box<dyn ConstParameter<MultiValue>>>,
    constant_resolve_order: Vec<ConstParameterType>,

    simple_f64: Vec<Box<dyn SimpleParameter<f64>>>,
    simple_u64: Vec<Box<dyn SimpleParameter<u64>>>,
    simple_multi: Vec<Box<dyn SimpleParameter<MultiValue>>>,
    simple_resolve_order: Vec<SimpleParameterType>,

    // There is no resolve order for general parameters as they are resolved at a model
    // level with other component types (e.g. nodes).
    general_f64: Vec<Box<dyn GeneralParameter<f64>>>,
    general_u64: Vec<Box<dyn GeneralParameter<u64>>>,
    general_multi: Vec<Box<dyn GeneralParameter<MultiValue>>>,
}

impl ParameterCollection {
    pub fn size(&self) -> ParameterCollectionSize {
        ParameterCollectionSize {
            const_f64: self.constant_f64.len(),
            const_usize: self.constant_u64.len(),
            const_multi: self.constant_multi.len(),
            simple_f64: self.simple_f64.len(),
            simple_usize: self.simple_u64.len(),
            simple_multi: self.simple_multi.len(),
            general_f64: self.general_f64.len(),
            general_usize: self.general_u64.len(),
            general_multi: self.general_multi.len(),
        }
    }
    fn general_initial_states(
        &self,
        timesteps: &[Timestep],
        scenario_index: &ScenarioIndex,
    ) -> Result<ParameterStatesByType, ParameterCollectionSetupError> {
        // Get the initial internal state
        let f64_states = self
            .general_f64
            .iter()
            .map(|p| {
                p.setup(timesteps, scenario_index)
                    .map_err(|source| ParameterCollectionSetupError {
                        name: Box::new(p.name().clone()),
                        source: Box::new(source),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let usize_states = self
            .general_u64
            .iter()
            .map(|p| {
                p.setup(timesteps, scenario_index)
                    .map_err(|source| ParameterCollectionSetupError {
                        name: Box::new(p.name().clone()),
                        source: Box::new(source),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let multi_states = self
            .general_multi
            .iter()
            .map(|p| {
                p.setup(timesteps, scenario_index)
                    .map_err(|source| ParameterCollectionSetupError {
                        name: Box::new(p.name().clone()),
                        source: Box::new(source),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ParameterStatesByType {
            f64: f64_states,
            u64: usize_states,
            multi: multi_states,
        })
    }

    fn simple_initial_states(
        &self,
        timesteps: &[Timestep],
        scenario_index: &ScenarioIndex,
    ) -> Result<ParameterStatesByType, ParameterCollectionSetupError> {
        // Get the initial internal state
        let f64_states = self
            .simple_f64
            .iter()
            .map(|p| {
                p.setup(timesteps, scenario_index)
                    .map_err(|source| ParameterCollectionSetupError {
                        name: Box::new(p.name().clone()),
                        source: Box::new(source),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let usize_states = self
            .simple_u64
            .iter()
            .map(|p| {
                p.setup(timesteps, scenario_index)
                    .map_err(|source| ParameterCollectionSetupError {
                        name: Box::new(p.name().clone()),
                        source: Box::new(source),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let multi_states = self
            .simple_multi
            .iter()
            .map(|p| {
                p.setup(timesteps, scenario_index)
                    .map_err(|source| ParameterCollectionSetupError {
                        name: Box::new(p.name().clone()),
                        source: Box::new(source),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ParameterStatesByType {
            f64: f64_states,
            u64: usize_states,
            multi: multi_states,
        })
    }

    fn const_initial_states(
        &self,
        timesteps: &[Timestep],
        scenario_index: &ScenarioIndex,
    ) -> Result<ParameterStatesByType, ParameterCollectionSetupError> {
        // Get the initial internal state
        let f64_states = self
            .constant_f64
            .iter()
            .map(|p| {
                p.setup(timesteps, scenario_index)
                    .map_err(|source| ParameterCollectionSetupError {
                        name: Box::new(p.name().clone()),
                        source: Box::new(source),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let usize_states = self
            .constant_u64
            .iter()
            .map(|p| {
                p.setup(timesteps, scenario_index)
                    .map_err(|source| ParameterCollectionSetupError {
                        name: Box::new(p.name().clone()),
                        source: Box::new(source),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let multi_states = self
            .constant_multi
            .iter()
            .map(|p| {
                p.setup(timesteps, scenario_index)
                    .map_err(|source| ParameterCollectionSetupError {
                        name: Box::new(p.name().clone()),
                        source: Box::new(source),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ParameterStatesByType {
            f64: f64_states,
            u64: usize_states,
            multi: multi_states,
        })
    }

    /// Does a parameter with the given name exist in the collection.
    pub fn has_name(&self, name: &ParameterName) -> bool {
        self.get_f64_index_by_name(name).is_some()
            || self.get_u64_index_by_name(name).is_some()
            || self.get_multi_index_by_name(name).is_some()
    }

    /// Add a [`GeneralParameter<f64>`] parameter to the collection.
    ///
    /// This function will add attempt to simplify the parameter and add it to the simple or
    /// constant parameter list. If the parameter cannot be simplified it will be added to the
    /// general parameter list.
    pub fn add_general_f64(
        &mut self,
        parameter: Box<dyn GeneralParameter<f64>>,
    ) -> Result<ParameterIndex<f64>, ParameterCollectionError> {
        if self.has_name(parameter.name()) {
            return Err(ParameterCollectionError::NameAlreadyExists(
                parameter.meta().name.to_string(),
            ));
        }

        match parameter.try_into_simple() {
            Some(simple) => self.add_simple_f64(simple),
            None => {
                let index = GeneralParameterIndex::new(self.general_f64.len());
                self.general_f64.push(parameter);
                Ok(index.into())
            }
        }
    }

    pub fn add_simple_f64(
        &mut self,
        parameter: Box<dyn SimpleParameter<f64>>,
    ) -> Result<ParameterIndex<f64>, ParameterCollectionError> {
        if self.has_name(parameter.name()) {
            return Err(ParameterCollectionError::NameAlreadyExists(
                parameter.meta().name.to_string(),
            ));
        }

        match parameter.try_into_const() {
            Some(constant) => self.add_const_f64(constant),
            None => {
                let index = SimpleParameterIndex::new(self.simple_f64.len());

                self.simple_f64.push(parameter);
                self.simple_resolve_order.push(SimpleParameterType::Parameter(index));

                Ok(index.into())
            }
        }
    }

    pub fn add_const_f64(
        &mut self,
        parameter: Box<dyn ConstParameter<f64>>,
    ) -> Result<ParameterIndex<f64>, ParameterCollectionError> {
        if self.has_name(parameter.name()) {
            return Err(ParameterCollectionError::NameAlreadyExists(
                parameter.meta().name.to_string(),
            ));
        }

        let index = ConstParameterIndex::new(self.constant_f64.len());

        self.constant_f64.push(parameter);
        self.constant_resolve_order.push(ConstParameterType::Parameter(index));

        Ok(index.into())
    }

    pub fn get_f64(&self, index: ParameterIndex<f64>) -> Option<&dyn Parameter> {
        match index {
            ParameterIndex::Const(idx) => self.constant_f64.get(*idx.deref()).map(|p| p.as_parameter()),
            ParameterIndex::Simple(idx) => self.simple_f64.get(*idx.deref()).map(|p| p.as_parameter()),
            ParameterIndex::General(idx) => self.general_f64.get(*idx.deref()).map(|p| p.as_parameter()),
        }
    }

    pub fn get_general_f64(&self, index: GeneralParameterIndex<f64>) -> Option<&dyn GeneralParameter<f64>> {
        self.general_f64.get(*index.deref()).map(|p| p.as_ref())
    }

    pub fn get_f64_by_name(&self, name: &ParameterName) -> Option<&dyn Parameter> {
        self.general_f64
            .iter()
            .find(|p| p.name() == name)
            .map(|p| p.as_parameter())
    }

    pub fn get_f64_index_by_name(&self, name: &ParameterName) -> Option<ParameterIndex<f64>> {
        if let Some(idx) = self
            .general_f64
            .iter()
            .position(|p| p.name() == name)
            .map(GeneralParameterIndex::new)
        {
            Some(idx.into())
        } else if let Some(idx) = self
            .simple_f64
            .iter()
            .position(|p| p.name() == name)
            .map(SimpleParameterIndex::new)
        {
            Some(idx.into())
        } else {
            self.constant_f64
                .iter()
                .position(|p| p.name() == name)
                .map(ConstParameterIndex::new)
                .map(|idx| idx.into())
        }
    }

    pub fn add_general_u64(
        &mut self,
        parameter: Box<dyn GeneralParameter<u64>>,
    ) -> Result<ParameterIndex<u64>, ParameterCollectionError> {
        if self.has_name(parameter.name()) {
            return Err(ParameterCollectionError::NameAlreadyExists(
                parameter.meta().name.to_string(),
            ));
        }

        match parameter.try_into_simple() {
            Some(simple) => self.add_simple_u64(simple),
            None => {
                let index = GeneralParameterIndex::new(self.general_u64.len());
                self.general_u64.push(parameter);
                Ok(index.into())
            }
        }
    }

    pub fn add_simple_u64(
        &mut self,
        parameter: Box<dyn SimpleParameter<u64>>,
    ) -> Result<ParameterIndex<u64>, ParameterCollectionError> {
        if self.has_name(parameter.name()) {
            return Err(ParameterCollectionError::NameAlreadyExists(
                parameter.meta().name.to_string(),
            ));
        }

        match parameter.try_into_const() {
            Some(constant) => self.add_const_u64(constant),
            None => {
                let index = SimpleParameterIndex::new(self.simple_u64.len());

                self.simple_u64.push(parameter);
                self.simple_resolve_order.push(SimpleParameterType::Index(index));

                Ok(index.into())
            }
        }
    }

    pub fn add_const_u64(
        &mut self,
        parameter: Box<dyn ConstParameter<u64>>,
    ) -> Result<ParameterIndex<u64>, ParameterCollectionError> {
        if self.has_name(parameter.name()) {
            return Err(ParameterCollectionError::NameAlreadyExists(
                parameter.meta().name.to_string(),
            ));
        }

        let index = ConstParameterIndex::new(self.constant_u64.len());

        self.constant_u64.push(parameter);
        self.constant_resolve_order.push(ConstParameterType::Index(index));

        Ok(index.into())
    }

    pub fn get_u64(&self, index: ParameterIndex<u64>) -> Option<&dyn Parameter> {
        match index {
            ParameterIndex::Const(idx) => self.constant_u64.get(*idx.deref()).map(|p| p.as_parameter()),
            ParameterIndex::Simple(idx) => self.simple_u64.get(*idx.deref()).map(|p| p.as_parameter()),
            ParameterIndex::General(idx) => self.general_u64.get(*idx.deref()).map(|p| p.as_parameter()),
        }
    }

    pub fn get_general_u64(&self, index: GeneralParameterIndex<u64>) -> Option<&dyn GeneralParameter<u64>> {
        self.general_u64.get(*index.deref()).map(|p| p.as_ref())
    }

    pub fn get_u64_by_name(&self, name: &ParameterName) -> Option<&dyn Parameter> {
        self.general_u64
            .iter()
            .find(|p| p.name() == name)
            .map(|p| p.as_parameter())
    }

    pub fn get_u64_index_by_name(&self, name: &ParameterName) -> Option<ParameterIndex<u64>> {
        if let Some(idx) = self
            .general_u64
            .iter()
            .position(|p| p.name() == name)
            .map(GeneralParameterIndex::new)
        {
            Some(idx.into())
        } else if let Some(idx) = self
            .simple_u64
            .iter()
            .position(|p| p.name() == name)
            .map(SimpleParameterIndex::new)
        {
            Some(idx.into())
        } else {
            self.constant_u64
                .iter()
                .position(|p| p.name() == name)
                .map(ConstParameterIndex::new)
                .map(|idx| idx.into())
        }
    }

    pub fn add_general_multi(
        &mut self,
        parameter: Box<dyn GeneralParameter<MultiValue>>,
    ) -> Result<ParameterIndex<MultiValue>, ParameterCollectionError> {
        if self.has_name(parameter.name()) {
            return Err(ParameterCollectionError::NameAlreadyExists(
                parameter.meta().name.to_string(),
            ));
        }

        match parameter.try_into_simple() {
            Some(simple) => self.add_simple_multi(simple).map(|idx| idx.into()),
            None => {
                let index = GeneralParameterIndex::new(self.general_multi.len());
                self.general_multi.push(parameter);
                Ok(index.into())
            }
        }
    }

    pub fn add_simple_multi(
        &mut self,
        parameter: Box<dyn SimpleParameter<MultiValue>>,
    ) -> Result<SimpleParameterIndex<MultiValue>, ParameterCollectionError> {
        if self.has_name(parameter.name()) {
            return Err(ParameterCollectionError::NameAlreadyExists(
                parameter.meta().name.to_string(),
            ));
        }

        let index = SimpleParameterIndex::new(self.simple_multi.len());

        self.simple_multi.push(parameter);
        self.simple_resolve_order.push(SimpleParameterType::Multi(index));

        Ok(index)
    }

    pub fn add_const_multi(
        &mut self,
        parameter: Box<dyn ConstParameter<MultiValue>>,
    ) -> Result<ConstParameterIndex<MultiValue>, ParameterCollectionError> {
        if self.has_name(parameter.name()) {
            return Err(ParameterCollectionError::NameAlreadyExists(
                parameter.meta().name.to_string(),
            ));
        }

        let index = ConstParameterIndex::new(self.constant_multi.len());

        self.constant_multi.push(parameter);
        self.constant_resolve_order.push(ConstParameterType::Multi(index));

        Ok(index)
    }
    pub fn get_multi(&self, index: &ParameterIndex<MultiValue>) -> Option<&dyn Parameter> {
        match index {
            ParameterIndex::Const(idx) => self.constant_multi.get(*idx.deref()).map(|p| p.as_parameter()),
            ParameterIndex::Simple(idx) => self.simple_multi.get(*idx.deref()).map(|p| p.as_parameter()),
            ParameterIndex::General(idx) => self.general_multi.get(*idx.deref()).map(|p| p.as_parameter()),
        }
    }

    pub fn get_general_multi(
        &self,
        index: &GeneralParameterIndex<MultiValue>,
    ) -> Option<&dyn GeneralParameter<MultiValue>> {
        self.general_multi.get(*index.deref()).map(|p| p.as_ref())
    }

    pub fn get_multi_by_name(&self, name: &ParameterName) -> Option<&dyn Parameter> {
        self.general_multi
            .iter()
            .find(|p| p.name() == name)
            .map(|p| p.as_parameter())
    }

    pub fn get_multi_index_by_name(&self, name: &ParameterName) -> Option<ParameterIndex<MultiValue>> {
        if let Some(idx) = self
            .general_multi
            .iter()
            .position(|p| p.name() == name)
            .map(GeneralParameterIndex::new)
        {
            Some(idx.into())
        } else if let Some(idx) = self
            .simple_multi
            .iter()
            .position(|p| p.name() == name)
            .map(SimpleParameterIndex::new)
        {
            Some(idx.into())
        } else {
            self.constant_multi
                .iter()
                .position(|p| p.name() == name)
                .map(ConstParameterIndex::new)
                .map(|idx| idx.into())
        }
    }

    pub fn compute_simple(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        state: &mut State,
        internal_states: &mut ParameterStates,
    ) -> Result<(), ParameterCollectionSimpleCalculationError> {
        for p in &self.simple_resolve_order {
            match p {
                SimpleParameterType::Parameter(idx) => {
                    // Find the parameter itself
                    let p = self
                        .simple_f64
                        .get(*idx.deref())
                        .ok_or(ParameterCollectionSimpleCalculationError::F64IndexNotFound(*idx))?;
                    // .. and its internal state
                    let internal_state = internal_states
                        .get_simple_mut_f64_state(*idx)
                        .ok_or(ParameterCollectionSimpleCalculationError::F64IndexNotFound(*idx))?;

                    let value = p
                        .compute(
                            timestep,
                            scenario_index,
                            &state.get_simple_parameter_values(),
                            internal_state,
                        )
                        .map_err(|source| ParameterCollectionSimpleCalculationError::CalculationError {
                            name: p.name().clone(),
                            source,
                        })?;

                    state.set_simple_parameter_value(*idx, value).map_err(|source| {
                        ParameterCollectionSimpleCalculationError::F64SetStateError {
                            name: p.name().clone(),
                            source,
                        }
                    })?;
                }
                SimpleParameterType::Index(idx) => {
                    // Find the parameter itself
                    let p = self
                        .simple_u64
                        .get(*idx.deref())
                        .ok_or(ParameterCollectionSimpleCalculationError::U64IndexNotFound(*idx))?;
                    // ... and its internal state
                    let internal_state = internal_states
                        .get_simple_mut_u64_state(*idx)
                        .ok_or(ParameterCollectionSimpleCalculationError::U64IndexNotFound(*idx))?;

                    let value = p
                        .compute(
                            timestep,
                            scenario_index,
                            &state.get_simple_parameter_values(),
                            internal_state,
                        )
                        .map_err(|source| ParameterCollectionSimpleCalculationError::CalculationError {
                            name: p.name().clone(),
                            source,
                        })?;

                    state.set_simple_parameter_index(*idx, value).map_err(|source| {
                        ParameterCollectionSimpleCalculationError::U64SetStateError {
                            name: p.name().clone(),
                            source,
                        }
                    })?;
                }
                SimpleParameterType::Multi(idx) => {
                    // Find the parameter itself
                    let p = self
                        .simple_multi
                        .get(*idx.deref())
                        .ok_or(ParameterCollectionSimpleCalculationError::MultiIndexNotFound(*idx))?;
                    // ... and its internal state
                    let internal_state = internal_states
                        .get_simple_mut_multi_state(*idx)
                        .ok_or(ParameterCollectionSimpleCalculationError::MultiIndexNotFound(*idx))?;

                    let value = p
                        .compute(
                            timestep,
                            scenario_index,
                            &state.get_simple_parameter_values(),
                            internal_state,
                        )
                        .map_err(|source| ParameterCollectionSimpleCalculationError::CalculationError {
                            name: p.name().clone(),
                            source,
                        })?;

                    state.set_simple_multi_parameter_value(*idx, value).map_err(|source| {
                        ParameterCollectionSimpleCalculationError::MultiSetStateError {
                            name: p.name().clone(),
                            source,
                        }
                    })?;
                }
            }
        }

        Ok(())
    }

    /// Perform the after step for simple parameters.
    pub fn after_simple(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        state: &mut State,
        internal_states: &mut ParameterStates,
    ) -> Result<(), ParameterCollectionSimpleCalculationError> {
        for p in &self.simple_resolve_order {
            match p {
                SimpleParameterType::Parameter(idx) => {
                    // Find the parameter itself
                    let p = self
                        .simple_f64
                        .get(*idx.deref())
                        .ok_or(ParameterCollectionSimpleCalculationError::F64IndexNotFound(*idx))?;
                    // .. and its internal state
                    let internal_state = internal_states
                        .get_simple_mut_f64_state(*idx)
                        .ok_or(ParameterCollectionSimpleCalculationError::F64IndexNotFound(*idx))?;

                    p.after(
                        timestep,
                        scenario_index,
                        &state.get_simple_parameter_values(),
                        internal_state,
                    )
                    .map_err(|source| {
                        ParameterCollectionSimpleCalculationError::CalculationError {
                            name: p.name().clone(),
                            source,
                        }
                    })?;
                }
                SimpleParameterType::Index(idx) => {
                    // Find the parameter itself
                    let p = self
                        .simple_u64
                        .get(*idx.deref())
                        .ok_or(ParameterCollectionSimpleCalculationError::U64IndexNotFound(*idx))?;
                    // .. and its internal state
                    let internal_state = internal_states
                        .get_simple_mut_u64_state(*idx)
                        .ok_or(ParameterCollectionSimpleCalculationError::U64IndexNotFound(*idx))?;

                    p.after(
                        timestep,
                        scenario_index,
                        &state.get_simple_parameter_values(),
                        internal_state,
                    )
                    .map_err(|source| {
                        ParameterCollectionSimpleCalculationError::CalculationError {
                            name: p.name().clone(),
                            source,
                        }
                    })?;
                }
                SimpleParameterType::Multi(idx) => {
                    // Find the parameter itself
                    let p = self
                        .simple_multi
                        .get(*idx.deref())
                        .ok_or(ParameterCollectionSimpleCalculationError::MultiIndexNotFound(*idx))?;
                    // .. and its internal state
                    let internal_state = internal_states
                        .get_simple_mut_multi_state(*idx)
                        .ok_or(ParameterCollectionSimpleCalculationError::MultiIndexNotFound(*idx))?;

                    p.compute(
                        timestep,
                        scenario_index,
                        &state.get_simple_parameter_values(),
                        internal_state,
                    )
                    .map_err(|source| {
                        ParameterCollectionSimpleCalculationError::CalculationError {
                            name: p.name().clone(),
                            source,
                        }
                    })?;
                }
            }
        }

        Ok(())
    }

    /// Compute the constant parameters.
    pub fn compute_const(
        &self,
        scenario_index: &ScenarioIndex,
        state: &mut State,
        internal_states: &mut ParameterStates,
    ) -> Result<(), ParameterCollectionConstCalculationError> {
        for p in &self.constant_resolve_order {
            match p {
                ConstParameterType::Parameter(idx) => {
                    // Find the parameter itself
                    let p = self
                        .constant_f64
                        .get(*idx.deref())
                        .ok_or(ParameterCollectionConstCalculationError::F64IndexNotFound(*idx))?;
                    // ... and its internal state
                    let internal_state = internal_states
                        .get_const_mut_f64_state(*idx)
                        .ok_or(ParameterCollectionConstCalculationError::F64IndexNotFound(*idx))?;

                    let value = p
                        .compute(scenario_index, &state.get_const_parameter_values(), internal_state)
                        .map_err(|source| ParameterCollectionConstCalculationError::CalculationError {
                            name: p.name().clone(),
                            source,
                        })?;

                    state.set_const_parameter_value(*idx, value).map_err(|source| {
                        ParameterCollectionConstCalculationError::F64SetStateError {
                            name: p.name().clone(),
                            source,
                        }
                    })?;
                }
                ConstParameterType::Index(idx) => {
                    // Find the parameter itself
                    let p = self
                        .constant_u64
                        .get(*idx.deref())
                        .ok_or(ParameterCollectionConstCalculationError::U64IndexNotFound(*idx))?;
                    // ... and its internal state
                    let internal_state = internal_states
                        .get_const_mut_u64_state(*idx)
                        .ok_or(ParameterCollectionConstCalculationError::U64IndexNotFound(*idx))?;

                    let value = p
                        .compute(scenario_index, &state.get_const_parameter_values(), internal_state)
                        .map_err(|source| ParameterCollectionConstCalculationError::CalculationError {
                            name: p.name().clone(),
                            source,
                        })?;
                    state.set_const_parameter_index(*idx, value).map_err(|source| {
                        ParameterCollectionConstCalculationError::U64SetStateError {
                            name: p.name().clone(),
                            source,
                        }
                    })?;
                }
                ConstParameterType::Multi(idx) => {
                    // Find the parameter itself
                    let p = self
                        .constant_multi
                        .get(*idx.deref())
                        .ok_or(ParameterCollectionConstCalculationError::MultiIndexNotFound(*idx))?;
                    // ... and its internal state
                    let internal_state = internal_states
                        .get_const_mut_multi_state(*idx)
                        .ok_or(ParameterCollectionConstCalculationError::MultiIndexNotFound(*idx))?;

                    let value = p
                        .compute(scenario_index, &state.get_const_parameter_values(), internal_state)
                        .map_err(|source| ParameterCollectionConstCalculationError::CalculationError {
                            name: p.name().clone(),
                            source,
                        })?;
                    state.set_const_multi_parameter_value(*idx, value).map_err(|source| {
                        ParameterCollectionConstCalculationError::MultiSetStateError {
                            name: p.name().clone(),
                            source,
                        }
                    })?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ConstParameter, GeneralParameter, Parameter, ParameterCalculationError, ParameterCollection, ParameterMeta,
        ParameterState, SimpleParameter,
    };
    use crate::parameters::errors::{ConstCalculationError, SimpleCalculationError};
    use crate::scenario::ScenarioIndex;
    use crate::state::{ConstParameterValues, MultiValue};

    /// Parameter for testing purposes
    struct TestParameter {
        meta: ParameterMeta,
    }

    impl Default for TestParameter {
        fn default() -> Self {
            Self {
                meta: ParameterMeta::new("test-parameter".into()),
            }
        }
    }
    impl Parameter for TestParameter {
        fn meta(&self) -> &ParameterMeta {
            &self.meta
        }
    }

    impl<T> ConstParameter<T> for TestParameter
    where
        T: From<u8>,
    {
        fn compute(
            &self,
            _scenario_index: &ScenarioIndex,
            _values: &ConstParameterValues,
            _internal_state: &mut Option<Box<dyn ParameterState>>,
        ) -> Result<T, ConstCalculationError> {
            Ok(T::from(1))
        }

        fn as_parameter(&self) -> &dyn Parameter {
            self
        }
    }

    impl ConstParameter<MultiValue> for TestParameter {
        fn compute(
            &self,
            _scenario_index: &ScenarioIndex,
            _values: &ConstParameterValues,
            _internal_state: &mut Option<Box<dyn ParameterState>>,
        ) -> Result<MultiValue, ConstCalculationError> {
            Ok(MultiValue::default())
        }

        fn as_parameter(&self) -> &dyn Parameter {
            self
        }
    }
    impl<T> SimpleParameter<T> for TestParameter
    where
        T: From<u8>,
    {
        fn compute(
            &self,
            _timestep: &crate::timestep::Timestep,
            _scenario_index: &ScenarioIndex,
            _values: &crate::state::SimpleParameterValues,
            _internal_state: &mut Option<Box<dyn ParameterState>>,
        ) -> Result<T, SimpleCalculationError> {
            Ok(T::from(1))
        }

        fn as_parameter(&self) -> &dyn Parameter {
            self
        }
    }

    impl SimpleParameter<MultiValue> for TestParameter {
        fn compute(
            &self,
            _timestep: &crate::timestep::Timestep,
            _scenario_index: &ScenarioIndex,
            _values: &crate::state::SimpleParameterValues,
            _internal_state: &mut Option<Box<dyn ParameterState>>,
        ) -> Result<MultiValue, SimpleCalculationError> {
            Ok(MultiValue::default())
        }

        fn as_parameter(&self) -> &dyn Parameter {
            self
        }
    }
    impl<T> GeneralParameter<T> for TestParameter
    where
        T: From<u8>,
    {
        fn compute(
            &self,
            _timestep: &crate::timestep::Timestep,
            _scenario_index: &ScenarioIndex,
            _model: &crate::network::Network,
            _state: &crate::state::State,
            _internal_state: &mut Option<Box<dyn ParameterState>>,
        ) -> Result<T, ParameterCalculationError> {
            Ok(T::from(1))
        }

        fn as_parameter(&self) -> &dyn Parameter {
            self
        }
    }

    impl GeneralParameter<MultiValue> for TestParameter {
        fn compute(
            &self,
            _timestep: &crate::timestep::Timestep,
            _scenario_index: &ScenarioIndex,
            _model: &crate::network::Network,
            _state: &crate::state::State,
            _internal_state: &mut Option<Box<dyn ParameterState>>,
        ) -> Result<MultiValue, ParameterCalculationError> {
            Ok(MultiValue::default())
        }

        fn as_parameter(&self) -> &dyn Parameter {
            self
        }
    }

    /// Test naming constraints on parameter collection.
    #[test]
    fn test_parameter_collection_name_constraints() {
        let mut collection = ParameterCollection::default();

        let ret = collection.add_const_f64(Box::new(TestParameter::default()));
        assert!(ret.is_ok());

        assert!(collection.has_name(&"test-parameter".into()));

        // Try to add a parameter with the same name
        let ret = collection.add_const_f64(Box::new(TestParameter::default()));
        assert!(ret.is_err());

        let ret = collection.add_simple_f64(Box::new(TestParameter::default()));
        assert!(ret.is_err());

        let ret = collection.add_general_f64(Box::new(TestParameter::default()));
        assert!(ret.is_err());

        let ret = collection.add_const_u64(Box::new(TestParameter::default()));
        assert!(ret.is_err());

        let ret = collection.add_simple_u64(Box::new(TestParameter::default()));
        assert!(ret.is_err());

        let ret = collection.add_general_u64(Box::new(TestParameter::default()));
        assert!(ret.is_err());

        let ret = collection.add_const_multi(Box::new(TestParameter::default()));
        assert!(ret.is_err());

        let ret = collection.add_simple_multi(Box::new(TestParameter::default()));
        assert!(ret.is_err());

        let ret = collection.add_general_multi(Box::new(TestParameter::default()));
        assert!(ret.is_err());
    }
}
