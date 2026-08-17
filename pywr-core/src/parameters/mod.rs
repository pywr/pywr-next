mod activation_function;
mod aggregated;
mod aggregated_index;
mod array;
mod asymmetric;
mod constant;
mod constant_scenario;
mod control_curves;
mod deficit;
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
#[cfg(test)]
pub(crate) mod test_utils;
mod threshold;
mod vector;

use std::any::Any;
use std::collections::HashSet;
// Re-imports
use crate::metric::{MetricF64Error, MetricF64ResolutionError, MetricU64ResolutionError};
use crate::network::{Network, ResolutionMaps};
use crate::scenario::{ScenarioGroupNotFound, ScenarioIndex};
use crate::state::{ConstParameterValues, MultiValue, SetStateError, SimpleParameterValues, State};
use crate::timestep::Timestep;
pub use activation_function::ActivationFunction;
pub use aggregated::{AggregatedParameter, AggregatedParameterBuilder};
pub use aggregated_index::{AggregatedIndexParameter, AggregatedIndexParameterBuilder};
pub use array::{Array1Parameter, Array1ParameterBuilder, Array2Parameter, Array2ParameterBuilder};
pub use asymmetric::{AsymmetricSwitchIndexParameter, AsymmetricSwitchIndexParameterBuilder};
pub use constant::{ConstantParameter, ConstantParameterBuilder};
pub use constant_scenario::{ConstantScenarioParameter, ConstantScenarioParameterBuilder};
pub use control_curves::{
    ApportionParameter, ApportionParameterBuilder, ControlCurveIndexParameter, ControlCurveIndexParameterBuilder,
    ControlCurveInterpolatedParameter, ControlCurveInterpolatedParameterBuilder, ControlCurveParameter,
    ControlCurveParameterBuilder, PiecewiseInterpolatedParameter, PiecewiseInterpolatedParameterBuilder,
    VolumeBetweenControlCurvesParameter, VolumeBetweenControlCurvesParameterBuilder,
};
pub use deficit::{DeficitParameter, DeficitParameterBuilder};
pub use delay::{DelayParameter, DelayParameterBuilder};
pub use difference::{DifferenceParameter, DifferenceParameterBuilder};
pub use discount_factor::{DiscountFactorParameter, DiscountFactorParameterBuilder};
pub use division::{DivisionParameter, DivisionParameterBuilder};
use errors::{ConstCalculationError, SimpleCalculationError};
pub use errors::{GeneralCalculationError, ParameterSetupError};
pub use hydropower::{HydropowerTargetData, HydropowerTargetParameter, HydropowerTargetParameterBuilder};
pub use indexed_array::{IndexedArrayParameter, IndexedArrayParameterBuilder};
pub use interpolate::{InterpolationError, interpolate, linear_interpolation};
pub use interpolated::{InterpolatedParameter, InterpolatedParameterBuilder};
pub use max::{MaxParameter, MaxParameterBuilder};
pub use min::{MinParameter, MinParameterBuilder};
pub use multi_threshold::{MultiThresholdParameter, MultiThresholdParameterBuilder};
pub use muskingum::{MuskingumInitialCondition, MuskingumParameter, MuskingumParameterBuilder};
use ndarray::ShapeError;
pub use negative::{NegativeParameter, NegativeParameterBuilder};
pub use negativemax::{NegativeMaxParameter, NegativeMaxParameterBuilder};
pub use negativemin::{NegativeMinParameter, NegativeMinParameterBuilder};
pub use offset::{OffsetParameter, OffsetParameterBuilder};
pub use polynomial::{Polynomial1DParameter, Polynomial1DParameterBuilder};
pub use profiles::{
    DailyProfileParameter, DailyProfileParameterBuilder, DiurnalProfileParameter, DiurnalProfileParameterBuilder,
    MonthlyInterpDay, MonthlyProfileParameter, MonthlyProfileParameterBuilder, RadialBasisFunction,
    RbfProfileParameter, RbfProfileParameterBuilder, RbfProfileVariableConfig, UniformDrawdownProfileParameter,
    UniformDrawdownProfileParameterBuilder, WeeklyInterpDay, WeeklyProfileError, WeeklyProfileParameter,
    WeeklyProfileParameterBuilder, WeeklyProfileValues,
};
#[cfg(feature = "pyo3")]
pub use py::{ParameterInfo, PyClassParameter, PyClassParameterBuilder, PyFuncParameter, PyFuncParameterBuilder};
pub use rolling::{RollingParameter, RollingParameterBuilder};
use std::fmt;
use std::fmt::{Debug, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use thiserror::Error;
pub use threshold::{Predicate, ThresholdParameter, ThresholdParameterBuilder};
pub use vector::{VectorParameter, VectorParameterBuilder};

/// Constant parameter index.
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
    fn new(idx: usize) -> Self {
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
    fn new(idx: usize) -> Self {
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
/// This is a wrapper around usize that is used to index parameters in the collection. It is
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
    fn new(idx: usize) -> Self {
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

/// An index for a general parameter's before value in the state.
#[derive(Debug)]
pub struct GeneralBeforeValueIndex<T> {
    idx: usize,
    phantom: PhantomData<T>,
}

impl<T> GeneralBeforeValueIndex<T> {
    fn new(idx: usize) -> Self {
        Self {
            idx,
            phantom: PhantomData,
        }
    }
}

// These implementations are required because the derive macro does not work well with PhantomData.
// See issue: https://github.com/rust-lang/rust/issues/26925
impl<T> Clone for GeneralBeforeValueIndex<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for GeneralBeforeValueIndex<T> {}

impl<T> Deref for GeneralBeforeValueIndex<T> {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.idx
    }
}

impl<T> Display for GeneralBeforeValueIndex<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.idx)
    }
}

impl<T> PartialEq<Self> for GeneralBeforeValueIndex<T> {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
    }
}

impl<T> Eq for GeneralBeforeValueIndex<T> {}

/// An index for a general parameter's after value in the state.
#[derive(Debug)]
pub struct GeneralAfterValueIndex<T> {
    idx: usize,
    phantom: PhantomData<T>,
}
impl<T> GeneralAfterValueIndex<T> {
    fn new(idx: usize) -> Self {
        Self {
            idx,
            phantom: PhantomData,
        }
    }
}

// These implementations are required because the derive macro does not work well with PhantomData.
// See issue: https://github.com/rust-lang/rust/issues/26925
impl<T> Clone for GeneralAfterValueIndex<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for GeneralAfterValueIndex<T> {}

impl<T> Deref for GeneralAfterValueIndex<T> {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.idx
    }
}

impl<T> Display for GeneralAfterValueIndex<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.idx)
    }
}

impl<T> PartialEq<Self> for GeneralAfterValueIndex<T> {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
    }
}

impl<T> Eq for GeneralAfterValueIndex<T> {}
#[derive(Debug, PartialEq, Eq)]
pub struct GeneralParameterRegistration<T> {
    pub parameter: GeneralParameterIndex<T>,
    pub before: Option<GeneralBeforeValueIndex<T>>,
    pub after: Option<GeneralAfterValueIndex<T>>,
}

// These implementations are required because the derive macro does not work well with PhantomData.
// See issue: https://github.com/rust-lang/rust/issues/26925
impl<T> Clone for GeneralParameterRegistration<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for GeneralParameterRegistration<T> {}

#[derive(Debug, Copy, Clone)]
pub enum ParameterIndex<T> {
    Const(ConstParameterIndex<T>),
    Simple(SimpleParameterIndex<T>),
    General(GeneralParameterRegistration<T>),
}

impl<T> PartialEq<Self> for ParameterIndex<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Const(idx1), Self::Const(idx2)) => idx1 == idx2,
            (Self::Simple(idx1), Self::Simple(idx2)) => idx1 == idx2,
            (Self::General(idx1), Self::General(idx2)) => idx1 == idx2,
            _ => false,
        }
    }
}

impl<T> Eq for ParameterIndex<T> where T: Eq {}

impl<T> Display for ParameterIndex<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Const(idx) => write!(f, "{idx}",),
            Self::Simple(idx) => write!(f, "{idx}",),
            Self::General(idx) => write!(f, "{}", idx.parameter),
        }
    }
}
impl<T> From<GeneralParameterRegistration<T>> for ParameterIndex<T> {
    fn from(idx: GeneralParameterRegistration<T>) -> Self {
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

/// Specifies whether to use the 'before' or 'after' parameter values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParameterReturnValue {
    Before,
    After,
    AfterOrElseInitial,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, PartialEq)]
pub struct ParameterMeta {
    pub name: ParameterName,
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
            ParameterIndex::General(idx) => self.general.f64.get(*idx.parameter),
        }
    }
    pub fn get_general_f64_state(&self, index: GeneralParameterIndex<f64>) -> Option<&Option<Box<dyn ParameterState>>> {
        self.general.f64.get(*index.deref())
    }

    pub fn get_simple_f64_state(&self, index: SimpleParameterIndex<f64>) -> Option<&Option<Box<dyn ParameterState>>> {
        self.simple.f64.get(*index.deref())
    }

    pub fn get_const_f64_state(&self, index: ConstParameterIndex<f64>) -> Option<&Option<Box<dyn ParameterState>>> {
        self.constant.f64.get(*index.deref())
    }

    pub fn get_mut_f64_state(&mut self, index: ParameterIndex<f64>) -> Option<&mut Option<Box<dyn ParameterState>>> {
        match index {
            ParameterIndex::Const(idx) => self.constant.f64.get_mut(*idx.deref()),
            ParameterIndex::Simple(idx) => self.simple.f64.get_mut(*idx.deref()),
            ParameterIndex::General(idx) => self.general.f64.get_mut(*idx.parameter),
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
pub trait Parameter: Send + Sync + Debug {
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

/// A context struct that is passed to the `before` and `after` methods of a [`GeneralParameter`].
#[derive(Clone, Copy)]
pub struct GeneralParameterContext<'a> {
    pub timestep: &'a Timestep,
    pub scenario_index: &'a ScenarioIndex,
    pub network: &'a Network,
    pub state: &'a State,
}

/// A trait that defines a component that may produce a value each time-step, and may have an
/// internal state that is updated each time-step.
///
/// See [`GeneralBeforeParameter`] and [`GeneralAfterParameter`] for more specific traits that define
/// the behaviour of parameters that produce values before or after the network is updated.
pub trait GeneralParameter: Parameter {
    fn as_parameter(&self) -> &dyn Parameter;
}

/// A trait that defines a component that produces a value before the network is updated each time-step.
pub trait GeneralBeforeParameter<T>: GeneralParameter {
    fn before(
        &self,
        context: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<T, GeneralCalculationError>;
}

/// A trait that defines a component that produces a value after the network is updated each time-step.
pub trait GeneralAfterParameter<T>: GeneralParameter {
    fn after(
        &self,
        context: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<T, GeneralCalculationError>;
}

/// A trait that defines a component that performs an action after the network is updated each
/// time-step, but does not produce a value.
pub trait GeneralAfterParameterHook<T>: GeneralParameter {
    fn after(
        &self,
        context: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), GeneralCalculationError>;
}

#[derive(Debug, Clone)]
enum GeneralAfterOperation<T> {
    Value(Arc<dyn GeneralAfterParameter<T>>),
    Hook(Arc<dyn GeneralAfterParameterHook<T>>),
}

#[derive(Debug)]
pub struct GeneralParameterEntry<T> {
    parameter: Arc<dyn GeneralParameter>,
    before: Option<Arc<dyn GeneralBeforeParameter<T>>>,
    after: Option<GeneralAfterOperation<T>>,
}

impl<T: 'static> GeneralParameterEntry<T> {
    pub fn before<P>(parameter: P) -> Self
    where
        P: GeneralBeforeParameter<T> + 'static,
    {
        let parameter = Arc::new(parameter);
        Self {
            parameter: parameter.clone(),
            before: Some(parameter),
            after: None,
        }
    }

    pub fn after<P>(parameter: P) -> Self
    where
        P: GeneralAfterParameter<T> + 'static,
    {
        let parameter = Arc::new(parameter);
        Self {
            parameter: parameter.clone(),
            before: None,
            after: Some(GeneralAfterOperation::Value(parameter)),
        }
    }

    pub fn both<P>(parameter: P) -> Self
    where
        P: GeneralBeforeParameter<T> + GeneralAfterParameter<T> + 'static,
    {
        let parameter = Arc::new(parameter);
        Self {
            parameter: parameter.clone(),
            before: Some(parameter.clone()),
            after: Some(GeneralAfterOperation::Value(parameter)),
        }
    }

    pub fn before_with_after_hook<P>(parameter: P) -> Self
    where
        P: GeneralBeforeParameter<T> + GeneralAfterParameterHook<T> + 'static,
    {
        let parameter = Arc::new(parameter);
        Self {
            parameter: parameter.clone(),
            before: Some(parameter.clone()),
            after: Some(GeneralAfterOperation::Hook(parameter)),
        }
    }

    fn setup(
        &self,
        timesteps: &[Timestep],
        scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, ParameterSetupError> {
        self.parameter.setup(timesteps, scenario_index)
    }

    pub fn name(&self) -> &ParameterName {
        self.parameter.name()
    }

    fn as_parameter(&self) -> &dyn Parameter {
        self.parameter.as_parameter()
    }
}

#[derive(Debug, Error)]
pub enum ParameterBuildError {
    #[error("Scenario group not found: {}", .0.name)]
    ScenarioGroupNotFound(#[from] ScenarioGroupNotFound),
    #[error(
        "Number of values ({values}) does not match the size ({scenarios}) of the specified scenario group '{group}'."
    )]
    ScenarioValuesLengthMismatch {
        values: usize,
        scenarios: usize,
        group: String,
    },
    #[error("Error subsetting array with dimensions {array_shape:?} with subset {subset:?}: {source}")]
    ArraySubSetError {
        array_shape: Vec<usize>,
        subset: Vec<usize>,
        #[source]
        source: ShapeError,
    },
    #[error("Could not resolve f64 metric for `{attr}` attribute: {source}")]
    ResolveMetricF64Error {
        attr: String,
        #[source]
        source: MetricF64ResolutionError,
    },
    #[error("Could not resolve u64 metric for `{attr}` attribute: {source}")]
    ResolveMetricU64Error {
        attr: String,
        #[source]
        source: MetricU64ResolutionError,
    },
    #[error("Could not simplify f64 metric for `{attr}`: {source}")]
    CouldNotSimplifyMetricF64 {
        attr: String,
        #[source]
        source: MetricF64Error,
    },
    #[error("Could not compute day of the year; invalid date: day: {day}, month: {day}")]
    InvalidDayOfYear { day: u32, month: u32 },
    #[error("Parameter is configured without a valid calculation phase: {detail}")]
    NoCalculationPhase { detail: String },
    #[error("Metric for `{attr}` attribute is unused. {message}")]
    UnusedMetric { attr: String, message: String },
}

pub enum BuiltParameter<T> {
    General(GeneralParameterEntry<T>),
    Simple(Box<dyn SimpleParameter<T>>),
    Const(Box<dyn ConstParameter<T>>),
}

pub enum MaybeBuiltParameter<T> {
    Built(BuiltParameter<T>),
    Retry {
        builder: Box<dyn ParameterBuilder<T>>,
        parameter_not_found: ParameterName,
    },
}

impl<T> From<BuiltParameter<T>> for MaybeBuiltParameter<T> {
    fn from(built: BuiltParameter<T>) -> Self {
        Self::Built(built)
    }
}

pub trait ParameterBuilder<T>: Debug {
    /// The name of the parameter
    fn name(&self) -> &ParameterName;
    /// Construct a parameter from the builder.
    ///
    /// If the construction requires a parameter that is not yet available. This method
    /// should return the builder via one of the parameter not found variants of
    /// [`ParameterBuildError`] error. This will allow the parameter collection builder to retry
    /// the build.
    fn build(self: Box<Self>, resolution_maps: &ResolutionMaps) -> Result<MaybeBuiltParameter<T>, ParameterBuildError>;
}

/// Resolve a single `UnresolvedMetricF64` into a `MetricF64` inside a
/// `ParameterBuilder::build` implementation.
///
/// On `ParameterNotFound`, the macro early-returns `Ok(MaybeBuiltParameter::Retry($self))`
/// so the builder can be retried after more parameters are added. Any other
/// `MetricF64ResolutionError` is wrapped in `ParameterBuildError::ResolveMetricF64Error`
/// (tagged with `$attr`) and early-returned.
///
/// # Example
/// ```ignore
/// let metric = resolve_metric_f64!(self, self.metric, resolution_maps, phase, "metric");
/// ```
#[macro_export]
macro_rules! resolve_metric_f64 {
    ($self:ident, $unresolved:expr, $maps:expr, $phase:expr, $attr:expr $(,)?) => {
        match $unresolved.resolve($maps, $phase) {
            Ok(m) => m,
            Err(err) => {
                return if let $crate::metric::MetricF64ResolutionError::ParameterNotFound { parameter } = err {
                    Ok($crate::parameters::MaybeBuiltParameter::Retry {
                        builder: $self,
                        parameter_not_found: parameter,
                    })
                } else {
                    Err($crate::parameters::ParameterBuildError::ResolveMetricF64Error {
                        attr: ($attr).to_string(),
                        source: err,
                    })
                };
            }
        }
    };
}

/// Resolve a single `UnresolvedMetricU64` into a `MetricU64` inside a
/// `ParameterBuilder::build` implementation.
///
/// On `ParameterNotFound`, the macro early-returns `Ok(MaybeBuiltParameter::Retry($self))`
/// so the builder can be retried after more parameters are added. Any other
/// `MetricU64ResolutionError` is wrapped in `ParameterBuildError::ResolveMetricU64Error`
/// (tagged with `$attr`) and early-returned.
///
/// # Example
/// ```ignore
/// let metric = resolve_metric_u64!(self, self.metric, resolution_maps, phase, "metric");
/// ```
#[macro_export]
macro_rules! resolve_metric_u64 {
    ($self:ident, $unresolved:expr, $maps:expr, $phase:expr, $attr:expr $(,)?) => {
        match $unresolved.resolve($maps, $phase) {
            Ok(m) => m,
            Err(err) => {
                return if let $crate::metric::MetricU64ResolutionError::ParameterNotFound { parameter } = err {
                    Ok($crate::parameters::MaybeBuiltParameter::Retry {
                        builder: $self,
                        parameter_not_found: parameter,
                    })
                } else {
                    Err($crate::parameters::ParameterBuildError::ResolveMetricU64Error {
                        attr: ($attr).to_string(),
                        source: err,
                    })
                };
            }
        }
    };
}

/// Resolve a single `Option<UnresolvedMetricF64>` into a `Option<MetricF64>` inside a
/// `ParameterBuilder::build` implementation.
///
/// On `ParameterNotFound`, the macro early-returns `Ok(MaybeBuiltParameter::Retry($self))`
/// so the builder can be retried after more parameters are added. Any other
/// `MetricF64ResolutionError` is wrapped in `ParameterBuildError::ResolveMetricF64Error`
/// (tagged with `$attr`) and early-returned.
///
#[macro_export]
macro_rules! resolve_optional_metric_f64 {
    ($self:ident, $unresolved:expr, $maps:expr, $phase:expr, $attr:expr $(,)?) => {
        match $unresolved {
            Some(u) => match u.resolve($maps, $phase) {
                Ok(m) => Some(m),
                Err(err) => {
                    return if let $crate::metric::MetricF64ResolutionError::ParameterNotFound { parameter } = err {
                        Ok($crate::parameters::MaybeBuiltParameter::Retry {
                            builder: $self,
                            parameter_not_found: parameter,
                        })
                    } else {
                        Err($crate::parameters::ParameterBuildError::ResolveMetricF64Error {
                            attr: ($attr).to_string(),
                            source: err,
                        })
                    };
                }
            },
            None => None,
        }
    };
}

/// Resolve a slice/`Vec` of `UnresolvedMetricF64` into a `Vec<MetricF64>` inside a
/// `ParameterBuilder::build` implementation. Same retry / error semantics as
/// [`resolve_metric_f64!`].
///
/// `$unresolved` must be something that can be iterated as `&UnresolvedMetricF64`
/// and on which `.len()` is callable (e.g. `&self.values`, `self.control_curves.as_slice()`).
///
/// # Example
/// ```ignore
/// let control_curves =
///     resolve_metric_f64_vec!(self, &self.control_curves, resolution_maps, phase, "control_curves");
/// ```
#[macro_export]
macro_rules! resolve_metric_f64_vec {
    ($self:ident, $unresolved:expr, $maps:expr, $phase:expr, $attr:expr $(,)?) => {{
        let unresolved = $unresolved;
        let mut resolved = Vec::with_capacity(unresolved.len());
        for m in unresolved.iter() {
            match m.resolve($maps, $phase) {
                Ok(m) => resolved.push(m),
                Err(err) => {
                    return if let $crate::metric::MetricF64ResolutionError::ParameterNotFound { parameter } = err {
                        Ok($crate::parameters::MaybeBuiltParameter::Retry {
                            builder: $self,
                            parameter_not_found: parameter,
                        })
                    } else {
                        Err($crate::parameters::ParameterBuildError::ResolveMetricF64Error {
                            attr: ($attr).to_string(),
                            source: err,
                        })
                    };
                }
            }
        }
        resolved
    }};
}

/// Resolve a slice/`Vec` of `UnresolvedMetricU64` into a `Vec<MetricU64>` inside a
/// `ParameterBuilder::build` implementation. Same retry / error semantics as
/// [`resolve_metric_u64!`].
///
/// `$unresolved` must be something that can be iterated as `&UnresolvedMetricU64`
/// and on which `.len()` is callable (e.g. `&self.values`, `self.indices.as_slice()`).
///
#[macro_export]
macro_rules! resolve_metric_u64_vec {
    ($self:ident, $unresolved:expr, $maps:expr, $phase:expr, $attr:expr $(,)?) => {{
        let unresolved = $unresolved;
        let mut resolved = Vec::with_capacity(unresolved.len());
        for m in unresolved.iter() {
            match m.resolve($maps, $phase) {
                Ok(m) => resolved.push(m),
                Err(err) => {
                    return if let $crate::metric::MetricU64ResolutionError::ParameterNotFound { parameter } = err {
                        Ok($crate::parameters::MaybeBuiltParameter::Retry {
                            builder: $self,
                            parameter_not_found: parameter,
                        })
                    } else {
                        Err($crate::parameters::ParameterBuildError::ResolveMetricU64Error {
                            attr: ($attr).to_string(),
                            source: err,
                        })
                    };
                }
            }
        }
        resolved
    }};
}

/// Resolve a `HashMap<String, UnresolvedMetricF64`> into a `HashMap<String, MetricF64>` inside a
/// `ParameterBuilder::build` implementation. Same retry / error semantics as
/// [`resolve_metric_f64!`].
///
/// `$unresolved` must be something that can be iterated as `(&String, &UnresolvedMetricF64)`
/// and on which `.len()` is callable.
///
#[macro_export]
macro_rules! resolve_metric_f64_hashmap {
    ($self:ident, $unresolved:expr, $maps:expr, $phase:expr, $attr:expr $(,)?) => {{
        let unresolved = $unresolved;
        let mut resolved = HashMap::with_capacity(unresolved.len());
        for (k, m) in unresolved.iter() {
            match m.resolve($maps, $phase) {
                Ok(m) => {
                    resolved.insert(k.clone(), m);
                }
                Err(err) => {
                    return if let $crate::metric::MetricF64ResolutionError::ParameterNotFound { parameter } = err {
                        Ok($crate::parameters::MaybeBuiltParameter::Retry {
                            builder: $self,
                            parameter_not_found: parameter,
                        })
                    } else {
                        Err($crate::parameters::ParameterBuildError::ResolveMetricF64Error {
                            attr: ($attr).to_string(),
                            source: err,
                        })
                    };
                }
            }
        }
        resolved
    }};
}

/// Resolve a `HashMap<String, UnresolvedMetricU64`> into a `HashMap<String, MetricU64>` inside a
/// `ParameterBuilder::build` implementation. Same retry / error semantics as
/// [`resolve_metric_u64!`].
///
/// `$unresolved` must be something that can be iterated as `(&String, &UnresolvedMetricU64)`
/// and on which `.len()` is callable.
///
#[macro_export]
macro_rules! resolve_metric_u64_hashmap {
    ($self:ident, $unresolved:expr, $maps:expr, $phase:expr, $attr:expr $(,)?) => {{
        let unresolved = $unresolved;
        let mut resolved = HashMap::with_capacity(unresolved.len());
        for (k, m) in unresolved.iter() {
            match m.resolve($maps, $phase) {
                Ok(m) => {
                    resolved.insert(k.clone(), m);
                }
                Err(err) => {
                    return if let $crate::metric::MetricU64ResolutionError::ParameterNotFound { parameter } = err {
                        Ok($crate::parameters::MaybeBuiltParameter::Retry {
                            builder: $self,
                            parameter_not_found: parameter,
                        })
                    } else {
                        Err($crate::parameters::ParameterBuildError::ResolveMetricU64Error {
                            attr: ($attr).to_string(),
                            source: err,
                        })
                    };
                }
            }
        }
        resolved
    }};
}

/// A context struct that is passed to the `before` and `after` methods of a [`SimpleParameter`].
#[derive(Clone, Copy)]
pub struct SimpleParameterContext<'a> {
    pub timestep: &'a Timestep,
    pub scenario_index: &'a ScenarioIndex,
    pub values: &'a SimpleParameterValues<'a>,
}

/// A trait that defines a component that may produce a value each time-step, and may have an
/// internal state that is updated each time-step.
///
pub trait SimpleParameter<T>: Parameter {
    fn compute(
        &self,
        context: SimpleParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<T, SimpleCalculationError>;
    fn as_parameter(&self) -> &dyn Parameter;
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

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

/// A struct that holds the required state sizes for a parameter collection.
#[derive(Debug, Clone, Copy)]
pub struct ParameterCollectionStateSize {
    pub const_f64: usize,
    pub const_u64: usize,
    pub const_multi: usize,

    pub simple_f64: usize,
    pub simple_u64: usize,
    pub simple_multi: usize,

    pub general_before_f64: usize,
    pub general_before_u64: usize,
    pub general_before_multi: usize,

    pub general_after_f64: usize,
    pub general_after_u64: usize,
    pub general_after_multi: usize,
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
    #[error("Before is not implemented for simple parameter '{name}'.")]
    BeforeNotImplemented { name: ParameterName },
    #[error("After is not implemented for simple parameter '{name}'.")]
    AfterNotImplemented { name: ParameterName },
}

// Unique ID for each parameter collection.
static PARAMETER_COLLECTION_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Error)]
#[error("Parameter collection ID mismatch: expected {expected}, actual {actual}: {context}")]
pub struct ParameterCollectionIdMismatchError {
    expected: u64,
    actual: u64,
    context: String,
}

#[derive(Default, Copy, Clone)]
pub struct ParameterTiming {
    before: Duration,
    after: Duration,
}

impl ParameterTiming {
    /// Time spent in the before method of the component.
    pub fn before(&self) -> Duration {
        self.before
    }

    /// Time spent in the "after" method of the component.
    pub fn after(&self) -> Duration {
        self.after
    }

    /// Total time spent in calculation and after methods.
    pub fn total(&self) -> Duration {
        self.before + self.after
    }
}

/// Timing accumulator for parameters
#[derive(Clone)]
pub struct ParameterTimings {
    general_f64: Vec<ParameterTiming>,
    general_u64: Vec<ParameterTiming>,
    general_multi: Vec<ParameterTiming>,
    id: u64,
}

impl ParameterTimings {
    pub fn from_collection(collection: &ParameterCollection) -> Self {
        Self {
            general_f64: vec![ParameterTiming::default(); collection.general_f64.len()],
            general_u64: vec![ParameterTiming::default(); collection.general_u64.len()],
            general_multi: vec![ParameterTiming::default(); collection.general_multi.len()],
            id: collection.id,
        }
    }

    /// Return the `n` slowest f64 parameter indices and their timings.
    fn slowest_f64(&self, n: usize) -> Vec<(GeneralParameterIndex<f64>, ParameterTiming)> {
        let mut timings = self
            .general_f64
            .iter()
            .enumerate()
            .map(|(idx, timing)| (GeneralParameterIndex::new(idx), *timing))
            .collect::<Vec<_>>();

        timings.sort_by_key(|(_, timing)| timing.total());

        timings.into_iter().rev().take(n).collect()
    }

    /// Return the `n` slowest u64 parameter indices and their timings.
    fn slowest_u64(&self, n: usize) -> Vec<(GeneralParameterIndex<u64>, ParameterTiming)> {
        let mut timings = self
            .general_u64
            .iter()
            .enumerate()
            .map(|(idx, timing)| (GeneralParameterIndex::new(idx), *timing))
            .collect::<Vec<_>>();

        timings.sort_by_key(|(_, timing)| timing.total());

        timings.into_iter().rev().take(n).collect()
    }

    /// Return the `n` slowest multi parameter indices and their timings.
    fn slowest_multi(&self, n: usize) -> Vec<(GeneralParameterIndex<MultiValue>, ParameterTiming)> {
        let mut timings = self
            .general_multi
            .iter()
            .enumerate()
            .map(|(idx, timing)| (GeneralParameterIndex::new(idx), *timing))
            .collect::<Vec<_>>();

        timings.sort_by_key(|(_, timing)| timing.total());

        timings.into_iter().rev().take(n).collect()
    }
    pub fn slowest_parameters(&self, n: usize) -> Vec<(GeneralParameterType, ParameterTiming)> {
        let f64 = self
            .slowest_f64(n)
            .into_iter()
            .map(|(idx, timing)| (GeneralParameterType::from(idx), timing));

        let u64 = self
            .slowest_u64(n)
            .into_iter()
            .map(|(idx, timing)| (GeneralParameterType::from(idx), timing));

        let multi = self
            .slowest_multi(n)
            .into_iter()
            .map(|(idx, timing)| (GeneralParameterType::from(idx), timing));

        let mut all = f64
            .chain(u64)
            .chain(multi)
            .collect::<Vec<(GeneralParameterType, ParameterTiming)>>();
        all.sort_by_key(|(_, timing)| timing.total());
        all.into_iter().rev().take(n).collect()
    }

    pub fn slowest_parameters_named(
        &self,
        n: usize,
        collection: &ParameterCollection,
    ) -> Result<Vec<(ParameterName, ParameterTiming)>, ParameterCollectionIdMismatchError> {
        if self.id != collection.id {
            return Err(ParameterCollectionIdMismatchError{
                expected: collection.id,
                actual: self.id,
                context: "ParameterTimings and ParameterCollection must have the same ID to ensure that the indices are correct.".to_string(),
            });
        }

        // SAFETY: The id of the timings must match the id of the collection to ensure that the
        // indices are correct. This is checked above and should be guaranteed by construction.
        let slowest = unsafe {
            self.slowest_parameters(n)
                .into_iter()
                .map(|(idx, timing)| (collection.get_general_unchecked(idx).name().clone(), timing))
                .collect()
        };

        Ok(slowest)
    }
}

#[derive(Debug)]
enum GeneralBeforeScheduleEntry {
    F64 {
        index: GeneralParameterIndex<f64>,
        parameter: Arc<dyn GeneralBeforeParameter<f64>>,
        output: GeneralBeforeValueIndex<f64>,
    },
    U64 {
        index: GeneralParameterIndex<u64>,
        parameter: Arc<dyn GeneralBeforeParameter<u64>>,
        output: GeneralBeforeValueIndex<u64>,
    },
    Multi {
        index: GeneralParameterIndex<MultiValue>,
        parameter: Arc<dyn GeneralBeforeParameter<MultiValue>>,
        output: GeneralBeforeValueIndex<MultiValue>,
    },
}

#[derive(Debug)]
enum GeneralAfterScheduleOperation<T> {
    Value {
        parameter: Arc<dyn GeneralAfterParameter<T>>,
        output: GeneralAfterValueIndex<T>,
    },
    Hook {
        parameter: Arc<dyn GeneralAfterParameterHook<T>>,
    },
}

#[derive(Debug)]
enum GeneralAfterScheduleEntry {
    F64 {
        index: GeneralParameterIndex<f64>,
        op: GeneralAfterScheduleOperation<f64>,
    },
    U64 {
        index: GeneralParameterIndex<u64>,
        op: GeneralAfterScheduleOperation<u64>,
    },
    Multi {
        index: GeneralParameterIndex<MultiValue>,
        op: GeneralAfterScheduleOperation<MultiValue>,
    },
}

#[derive(Error, Debug)]
#[error("Error calculating general parameter '{name}': {source}")]
pub enum ParameterCollectionGeneralCalculationError {
    #[error("General parameter F64 index '{0}' not found in collection")]
    F64IndexNotFound(GeneralParameterIndex<f64>),
    #[error("General parameter U64 index '{0}' not found in collection")]
    U64IndexNotFound(GeneralParameterIndex<u64>),
    #[error("General parameter Multi index '{0}' not found in collection")]
    MultiIndexNotFound(GeneralParameterIndex<MultiValue>),
    #[error("Error calculating general parameter '{name}': {source}")]
    CalculationError {
        name: ParameterName,
        #[source]
        source: Box<GeneralCalculationError>,
    },
    #[error("Error setting before state for general F64 parameter '{name}': {source}")]
    F64SetBeforeStateError {
        name: ParameterName,
        #[source]
        source: SetStateError<GeneralBeforeValueIndex<f64>>,
    },
    #[error("Error setting after state for general F64 parameter '{name}': {source}")]
    F64SetAfterStateError {
        name: ParameterName,
        #[source]
        source: SetStateError<GeneralAfterValueIndex<f64>>,
    },
    #[error("Error setting before state for general U64 parameter '{name}': {source}")]
    U64SetBeforeStateError {
        name: ParameterName,
        #[source]
        source: SetStateError<GeneralBeforeValueIndex<u64>>,
    },
    #[error("Error setting after state for general U64 parameter '{name}': {source}")]
    U64SetAfterStateError {
        name: ParameterName,
        #[source]
        source: SetStateError<GeneralAfterValueIndex<u64>>,
    },
    #[error("Error setting before state for general Multi parameter '{name}': {source}")]
    MultiSetBeforeStateError {
        name: ParameterName,
        #[source]
        source: SetStateError<GeneralBeforeValueIndex<MultiValue>>,
    },
    #[error("Error setting after state for general Multi parameter '{name}': {source}")]
    MultiSetAfterStateError {
        name: ParameterName,
        #[source]
        source: SetStateError<GeneralAfterValueIndex<MultiValue>>,
    },
    #[error("The timing data was created with from a different parameter collection. ")]
    TimingsFromAnotherCollection,
    #[error("Before is not implemented for general parameter '{name}'.")]
    BeforeNotImplemented { name: ParameterName },
    #[error("After is not implemented for general parameter '{name}'.")]
    AfterNotImplemented { name: ParameterName },
}

/// A collection of parameters that return different types.
#[derive(Debug)]
pub struct ParameterCollection {
    constant_f64: Vec<Box<dyn ConstParameter<f64>>>,
    constant_u64: Vec<Box<dyn ConstParameter<u64>>>,
    constant_multi: Vec<Box<dyn ConstParameter<MultiValue>>>,
    constant_resolve_order: Vec<ConstParameterType>,

    simple_f64: Vec<Box<dyn SimpleParameter<f64>>>,
    simple_u64: Vec<Box<dyn SimpleParameter<u64>>>,
    simple_multi: Vec<Box<dyn SimpleParameter<MultiValue>>>,
    simple_resolve_order: Vec<SimpleParameterType>,

    general_f64: Vec<GeneralParameterEntry<f64>>,
    general_u64: Vec<GeneralParameterEntry<u64>>,
    general_multi: Vec<GeneralParameterEntry<MultiValue>>,
    num_general_before_f64: usize,
    num_general_before_u64: usize,
    num_general_before_multi: usize,
    num_general_after_f64: usize,
    num_general_after_u64: usize,
    num_general_after_multi: usize,
    general_before_order: Vec<GeneralBeforeScheduleEntry>,
    general_after_order: Vec<GeneralAfterScheduleEntry>,
    id: u64,
}

impl Default for ParameterCollection {
    fn default() -> Self {
        Self {
            constant_f64: Vec::new(),
            constant_u64: Vec::new(),
            constant_multi: Vec::new(),
            constant_resolve_order: Vec::new(),
            simple_f64: Vec::new(),
            simple_u64: Vec::new(),
            simple_multi: Vec::new(),
            simple_resolve_order: Vec::new(),
            general_f64: Vec::new(),
            general_u64: Vec::new(),
            general_multi: Vec::new(),
            num_general_before_f64: 0,
            num_general_before_u64: 0,
            num_general_before_multi: 0,
            num_general_after_f64: 0,
            num_general_after_u64: 0,
            num_general_after_multi: 0,
            general_before_order: Vec::new(),
            general_after_order: Vec::new(),
            id: PARAMETER_COLLECTION_ID.fetch_add(1, Ordering::Relaxed),
        }
    }
}

impl ParameterCollection {
    pub fn size(&self) -> ParameterCollectionStateSize {
        ParameterCollectionStateSize {
            const_f64: self.constant_f64.len(),
            const_u64: self.constant_u64.len(),
            const_multi: self.constant_multi.len(),
            simple_f64: self.simple_f64.len(),
            simple_u64: self.simple_u64.len(),
            simple_multi: self.simple_multi.len(),
            general_before_f64: self.num_general_before_f64,
            general_before_u64: self.num_general_before_u64,
            general_before_multi: self.num_general_before_multi,
            general_after_f64: self.num_general_after_f64,
            general_after_u64: self.num_general_after_u64,
            general_after_multi: self.num_general_after_multi,
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
            .map(|entry| {
                entry
                    .parameter
                    .setup(timesteps, scenario_index)
                    .map_err(|source| ParameterCollectionSetupError {
                        name: Box::new(entry.name().clone()),
                        source: Box::new(source),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let usize_states = self
            .general_u64
            .iter()
            .map(|entry| {
                entry
                    .setup(timesteps, scenario_index)
                    .map_err(|source| ParameterCollectionSetupError {
                        name: Box::new(entry.name().clone()),
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

    unsafe fn get_general_unchecked(&self, index: GeneralParameterType) -> &dyn Parameter {
        unsafe {
            match index {
                GeneralParameterType::Parameter(idx) => self.general_f64.get_unchecked(idx.idx).as_parameter(),
                GeneralParameterType::Index(idx) => self.general_u64.get_unchecked(idx.idx).as_parameter(),
                GeneralParameterType::Multi(idx) => self.general_multi.get_unchecked(idx.idx).as_parameter(),
            }
        }
    }

    /// Push a new general parameter to the collection.
    ///
    /// The new parameter will be simplified as much as possible.
    ///
    /// SAFETY: This must remain a private function to maintain the indexing guarantees.
    fn push_general_f64(&mut self, entry: GeneralParameterEntry<f64>) -> ParameterIndex<f64> {
        let index = GeneralParameterIndex::new(self.general_f64.len());

        let before = entry.before.clone().map(|parameter| {
            let output = GeneralBeforeValueIndex::new(self.num_general_before_f64);
            self.num_general_before_f64 += 1;

            self.general_before_order.push(GeneralBeforeScheduleEntry::F64 {
                index,
                parameter,
                output,
            });

            output
        });

        let after = entry.after.clone().and_then(|op| match op {
            GeneralAfterOperation::Value(parameter) => {
                let output = GeneralAfterValueIndex::new(self.num_general_after_f64);
                self.num_general_after_f64 += 1;

                let op = GeneralAfterScheduleOperation::Value { parameter, output };

                self.general_after_order
                    .push(GeneralAfterScheduleEntry::F64 { index, op });

                Some(output)
            }
            GeneralAfterOperation::Hook(parameter) => {
                let op = GeneralAfterScheduleOperation::Hook { parameter };
                self.general_after_order
                    .push(GeneralAfterScheduleEntry::F64 { index, op });

                None
            }
        });

        self.general_f64.push(entry);

        ParameterIndex::General(GeneralParameterRegistration {
            parameter: index,
            before,
            after,
        })
    }

    /// Push a new simple parameter to the collection.
    ///
    /// SAFETY: This must remain a private function to maintain the indexing guarantees.
    fn push_simple_f64(&mut self, parameter: Box<dyn SimpleParameter<f64>>) -> ParameterIndex<f64> {
        let index = SimpleParameterIndex::new(self.simple_f64.len());

        self.simple_f64.push(parameter);
        self.simple_resolve_order.push(index.into());

        ParameterIndex::Simple(index)
    }

    /// Push a new const parameter to the collection.
    ///
    /// SAFETY: This must remain a private function to maintain the indexing guarantees.
    fn push_const_f64(&mut self, p: Box<dyn ConstParameter<f64>>) -> ParameterIndex<f64> {
        let index = ConstParameterIndex::new(self.constant_f64.len());

        self.constant_resolve_order.push(ConstParameterType::from(index));
        self.constant_f64.push(p);

        ParameterIndex::Const(index)
    }

    pub fn get_f64(&self, index: ParameterIndex<f64>) -> Option<&dyn Parameter> {
        match index {
            ParameterIndex::Const(idx) => self.constant_f64.get(*idx.deref()).map(|p| p.as_parameter()),
            ParameterIndex::Simple(idx) => self.simple_f64.get(*idx.deref()).map(|p| p.as_parameter()),
            ParameterIndex::General(idx) => self.general_f64.get(*idx.parameter.deref()).map(|p| p.as_parameter()),
        }
    }

    pub fn get_general_f64(&self, index: GeneralParameterIndex<f64>) -> Option<&GeneralParameterEntry<f64>> {
        self.general_f64.get(*index.deref())
    }

    pub fn get_f64_by_name(&self, name: &ParameterName) -> Option<&dyn Parameter> {
        if let Some(p) = self
            .general_f64
            .iter()
            .find(|p| p.name() == name)
            .map(|p| p.as_parameter())
        {
            Some(p)
        } else if let Some(p) = self
            .simple_f64
            .iter()
            .find(|p| p.name() == name)
            .map(|p| p.as_parameter())
        {
            Some(p)
        } else {
            self.constant_f64
                .iter()
                .find(|p| p.name() == name)
                .map(|p| p.as_parameter())
        }
    }

    pub fn get_f64_index_by_name(&self, name: &ParameterName) -> Option<ParameterIndex<f64>> {
        if let Some(parameter_idx) = self
            .general_f64
            .iter()
            .position(|p| p.name() == name)
            .map(GeneralParameterIndex::new)
        {
            // Find if this index is used in the before or after schedule and return the appropriate index type.
            let before = self.general_before_order.iter().find_map(|entry| {
                if let GeneralBeforeScheduleEntry::F64 { index, output, .. } = entry {
                    if *index == parameter_idx { Some(*output) } else { None }
                } else {
                    None
                }
            });

            let after = self.general_after_order.iter().find_map(|entry| {
                if let GeneralAfterScheduleEntry::F64 { index, op } = entry {
                    if *index == parameter_idx {
                        match op {
                            GeneralAfterScheduleOperation::Value { output, .. } => Some(*output),
                            GeneralAfterScheduleOperation::Hook { .. } => None,
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            });

            let reg = GeneralParameterRegistration {
                parameter: parameter_idx,
                before,
                after,
            };

            Some(reg.into())
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

    /// Push a new general parameter to the collection.
    ///
    /// SAFETY: This must remain a private function to maintain the indexing guarantees.
    fn push_general_u64(&mut self, entry: GeneralParameterEntry<u64>) -> ParameterIndex<u64> {
        let index = GeneralParameterIndex::new(self.general_u64.len());

        let before = entry.before.clone().map(|parameter| {
            let output = GeneralBeforeValueIndex::new(self.num_general_before_u64);
            self.num_general_before_u64 += 1;

            self.general_before_order.push(GeneralBeforeScheduleEntry::U64 {
                index,
                parameter,
                output,
            });

            output
        });

        let after = entry.after.clone().and_then(|op| match op {
            GeneralAfterOperation::Value(parameter) => {
                let output = GeneralAfterValueIndex::new(self.num_general_after_u64);
                self.num_general_after_u64 += 1;

                let op = GeneralAfterScheduleOperation::Value { parameter, output };

                self.general_after_order
                    .push(GeneralAfterScheduleEntry::U64 { index, op });

                Some(output)
            }
            GeneralAfterOperation::Hook(parameter) => {
                let op = GeneralAfterScheduleOperation::Hook { parameter };
                self.general_after_order
                    .push(GeneralAfterScheduleEntry::U64 { index, op });

                None
            }
        });

        self.general_u64.push(entry);

        ParameterIndex::General(GeneralParameterRegistration {
            parameter: index,
            before,
            after,
        })
    }

    /// Push a new simple parameter to the collection.
    ///
    /// The new parameter will be simplified as much as possible.
    ///
    /// SAFETY: This must remain a private function to maintain the indexing guarantees.
    fn push_simple_u64(&mut self, parameter: Box<dyn SimpleParameter<u64>>) -> ParameterIndex<u64> {
        let index = SimpleParameterIndex::new(self.simple_u64.len());

        self.simple_u64.push(parameter);
        self.simple_resolve_order.push(index.into());

        ParameterIndex::Simple(index)
    }

    /// Push a new const parameter to the collection.
    ///
    /// SAFETY: This must remain a private function to maintain the indexing guarantees.
    fn push_const_u64(&mut self, p: Box<dyn ConstParameter<u64>>) -> ParameterIndex<u64> {
        let index = ConstParameterIndex::new(self.constant_u64.len());

        self.constant_resolve_order.push(ConstParameterType::from(index));
        self.constant_u64.push(p);

        ParameterIndex::Const(index)
    }

    pub fn get_u64(&self, index: ParameterIndex<u64>) -> Option<&dyn Parameter> {
        match index {
            ParameterIndex::Const(idx) => self.constant_u64.get(*idx.deref()).map(|p| p.as_parameter()),
            ParameterIndex::Simple(idx) => self.simple_u64.get(*idx.deref()).map(|p| p.as_parameter()),
            ParameterIndex::General(idx) => self.general_u64.get(*idx.parameter.deref()).map(|p| p.as_parameter()),
        }
    }

    pub fn get_general_u64(&self, index: GeneralParameterIndex<u64>) -> Option<&GeneralParameterEntry<u64>> {
        self.general_u64.get(*index.deref())
    }

    pub fn get_u64_by_name(&self, name: &ParameterName) -> Option<&dyn Parameter> {
        if let Some(p) = self
            .general_u64
            .iter()
            .find(|p| p.name() == name)
            .map(|p| p.as_parameter())
        {
            Some(p)
        } else if let Some(p) = self
            .simple_u64
            .iter()
            .find(|p| p.name() == name)
            .map(|p| p.as_parameter())
        {
            Some(p)
        } else {
            self.constant_u64
                .iter()
                .find(|p| p.name() == name)
                .map(|p| p.as_parameter())
        }
    }

    pub fn get_u64_index_by_name(&self, name: &ParameterName) -> Option<ParameterIndex<u64>> {
        if let Some(parameter_idx) = self
            .general_u64
            .iter()
            .position(|p| p.name() == name)
            .map(GeneralParameterIndex::new)
        {
            // Find if this index is used in the before or after schedule and return the appropriate index type.
            let before = self.general_before_order.iter().find_map(|entry| {
                if let GeneralBeforeScheduleEntry::U64 { index, output, .. } = entry {
                    if *index == parameter_idx { Some(*output) } else { None }
                } else {
                    None
                }
            });

            let after = self.general_after_order.iter().find_map(|entry| {
                if let GeneralAfterScheduleEntry::U64 { index, op } = entry {
                    if *index == parameter_idx {
                        match op {
                            GeneralAfterScheduleOperation::Value { output, .. } => Some(*output),
                            GeneralAfterScheduleOperation::Hook { .. } => None,
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            });

            let reg = GeneralParameterRegistration {
                parameter: parameter_idx,
                before,
                after,
            };

            Some(reg.into())
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

    /// Push a new general parameter to the collection.
    ///
    /// The new parameter will be simplified as much as possible.
    ///
    /// SAFETY: This must remain a private function to maintain the indexing guarantees.
    fn push_general_multi(&mut self, entry: GeneralParameterEntry<MultiValue>) -> ParameterIndex<MultiValue> {
        let index = GeneralParameterIndex::new(self.general_multi.len());

        let before = entry.before.clone().map(|parameter| {
            let output = GeneralBeforeValueIndex::new(self.num_general_before_multi);
            self.num_general_before_multi += 1;

            self.general_before_order.push(GeneralBeforeScheduleEntry::Multi {
                index,
                parameter,
                output,
            });

            output
        });

        let after = entry.after.clone().and_then(|op| match op {
            GeneralAfterOperation::Value(parameter) => {
                let output = GeneralAfterValueIndex::new(self.num_general_after_multi);
                self.num_general_after_multi += 1;

                let op = GeneralAfterScheduleOperation::Value { parameter, output };

                self.general_after_order
                    .push(GeneralAfterScheduleEntry::Multi { index, op });

                Some(output)
            }
            GeneralAfterOperation::Hook(parameter) => {
                let op = GeneralAfterScheduleOperation::Hook { parameter };
                self.general_after_order
                    .push(GeneralAfterScheduleEntry::Multi { index, op });

                None
            }
        });

        self.general_multi.push(entry);

        ParameterIndex::General(GeneralParameterRegistration {
            parameter: index,
            before,
            after,
        })
    }

    /// Push a new simple parameter to the collection.
    ///
    /// The new parameter will be simplified as much as possible.
    ///
    /// SAFETY: This must remain a private function to maintain the indexing guarantees.
    fn push_simple_multi(&mut self, parameter: Box<dyn SimpleParameter<MultiValue>>) -> ParameterIndex<MultiValue> {
        let index = SimpleParameterIndex::new(self.simple_multi.len());

        self.simple_multi.push(parameter);
        self.simple_resolve_order.push(index.into());

        ParameterIndex::Simple(index)
    }

    /// Push a new const parameter to the collection.
    ///
    /// SAFETY: This must remain a private function to maintain the indexing guarantees.
    fn push_const_multi(&mut self, p: Box<dyn ConstParameter<MultiValue>>) -> ParameterIndex<MultiValue> {
        let index = ConstParameterIndex::new(self.constant_multi.len());

        self.constant_resolve_order.push(ConstParameterType::from(index));
        self.constant_multi.push(p);

        ParameterIndex::Const(index)
    }

    pub fn get_multi(&self, index: &ParameterIndex<MultiValue>) -> Option<&dyn Parameter> {
        match index {
            ParameterIndex::Const(idx) => self.constant_multi.get(*idx.deref()).map(|p| p.as_parameter()),
            ParameterIndex::Simple(idx) => self.simple_multi.get(*idx.deref()).map(|p| p.as_parameter()),
            ParameterIndex::General(idx) => self.general_multi.get(*idx.parameter.deref()).map(|p| p.as_parameter()),
        }
    }

    pub fn get_general_multi(
        &self,
        index: &GeneralParameterIndex<MultiValue>,
    ) -> Option<&GeneralParameterEntry<MultiValue>> {
        self.general_multi.get(*index.deref())
    }

    pub fn get_multi_by_name(&self, name: &ParameterName) -> Option<&dyn Parameter> {
        if let Some(p) = self
            .general_multi
            .iter()
            .find(|p| p.name() == name)
            .map(|p| p.as_parameter())
        {
            Some(p)
        } else if let Some(p) = self
            .simple_multi
            .iter()
            .find(|p| p.name() == name)
            .map(|p| p.as_parameter())
        {
            Some(p)
        } else {
            self.constant_multi
                .iter()
                .find(|p| p.name() == name)
                .map(|p| p.as_parameter())
        }
    }

    pub fn get_multi_index_by_name(&self, name: &ParameterName) -> Option<ParameterIndex<MultiValue>> {
        if let Some(parameter_idx) = self
            .general_multi
            .iter()
            .position(|p| p.name() == name)
            .map(GeneralParameterIndex::new)
        {
            // Find if this index is used in the before or after schedule and return the appropriate index type.
            let before = self.general_before_order.iter().find_map(|entry| {
                if let GeneralBeforeScheduleEntry::Multi { index, output, .. } = entry {
                    if *index == parameter_idx { Some(*output) } else { None }
                } else {
                    None
                }
            });

            let after = self.general_after_order.iter().find_map(|entry| {
                if let GeneralAfterScheduleEntry::Multi { index, op } = entry {
                    if *index == parameter_idx {
                        match op {
                            GeneralAfterScheduleOperation::Value { output, .. } => Some(*output),
                            GeneralAfterScheduleOperation::Hook { .. } => None,
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            });

            let reg = GeneralParameterRegistration {
                parameter: parameter_idx,
                before,
                after,
            };

            Some(reg.into())
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

    pub fn before_general(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network: &Network,
        state: &mut State,
        internal_states: &mut ParameterStates,
        mut timings: Option<&mut ParameterTimings>,
    ) -> Result<(), ParameterCollectionGeneralCalculationError> {
        if let Some(timings) = timings.as_deref() {
            if timings.id != self.id {
                return Err(ParameterCollectionGeneralCalculationError::TimingsFromAnotherCollection);
            }
        }

        for p in &self.general_before_order {
            let start = Instant::now();
            match p {
                GeneralBeforeScheduleEntry::F64 {
                    index,
                    parameter,
                    output,
                } => {
                    // Find any internal state
                    let internal_state = internal_states
                        .get_general_mut_f64_state(*index)
                        .ok_or(ParameterCollectionGeneralCalculationError::F64IndexNotFound(*index))?;

                    let ctx = GeneralParameterContext {
                        timestep,
                        scenario_index,
                        network,
                        state,
                    };

                    let value = parameter.before(ctx, internal_state).map_err(|source| {
                        ParameterCollectionGeneralCalculationError::CalculationError {
                            name: parameter.name().clone(),
                            source: Box::new(source),
                        }
                    })?;

                    state
                        .set_general_parameter_f64_before(*output, value)
                        .map_err(
                            |source| ParameterCollectionGeneralCalculationError::F64SetBeforeStateError {
                                name: parameter.name().clone(),
                                source,
                            },
                        )?;

                    if let Some(timings) = timings.as_deref_mut() {
                        unsafe {
                            timings.general_f64.get_unchecked_mut(*index.deref()).before += start.elapsed();
                        }
                    }
                }
                GeneralBeforeScheduleEntry::U64 {
                    index,
                    parameter,
                    output,
                } => {
                    // Find the internal state
                    let internal_state = internal_states
                        .get_general_mut_u64_state(*index)
                        .ok_or(ParameterCollectionGeneralCalculationError::U64IndexNotFound(*index))?;

                    let ctx = GeneralParameterContext {
                        timestep,
                        scenario_index,
                        network,
                        state,
                    };

                    let value = parameter.before(ctx, internal_state).map_err(|source| {
                        ParameterCollectionGeneralCalculationError::CalculationError {
                            name: parameter.name().clone(),
                            source: Box::new(source),
                        }
                    })?;

                    state
                        .set_general_parameter_u64_before(*output, value)
                        .map_err(
                            |source| ParameterCollectionGeneralCalculationError::U64SetBeforeStateError {
                                name: parameter.name().clone(),
                                source,
                            },
                        )?;

                    if let Some(timings) = timings.as_deref_mut() {
                        unsafe {
                            timings.general_u64.get_unchecked_mut(*index.deref()).before += start.elapsed();
                        }
                    }
                }
                GeneralBeforeScheduleEntry::Multi {
                    index,
                    parameter,
                    output,
                } => {
                    // Find the internal state
                    let internal_state = internal_states
                        .get_general_mut_multi_state(*index)
                        .ok_or(ParameterCollectionGeneralCalculationError::MultiIndexNotFound(*index))?;

                    let ctx = GeneralParameterContext {
                        timestep,
                        scenario_index,
                        network,
                        state,
                    };

                    let value = parameter.before(ctx, internal_state).map_err(|source| {
                        ParameterCollectionGeneralCalculationError::CalculationError {
                            name: parameter.name().clone(),
                            source: Box::new(source),
                        }
                    })?;

                    state
                        .set_general_parameter_multi_before(*output, value)
                        .map_err(
                            |source| ParameterCollectionGeneralCalculationError::MultiSetBeforeStateError {
                                name: parameter.name().clone(),
                                source,
                            },
                        )?;

                    if let Some(timings) = timings.as_deref_mut() {
                        unsafe {
                            timings.general_multi.get_unchecked_mut(*index.deref()).before += start.elapsed();
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Perform the after step for general parameters.
    pub fn after_general(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network: &Network,
        state: &mut State,
        internal_states: &mut ParameterStates,
        mut timings: Option<&mut ParameterTimings>,
    ) -> Result<(), ParameterCollectionGeneralCalculationError> {
        if let Some(timings) = timings.as_deref() {
            if timings.id != self.id {
                return Err(ParameterCollectionGeneralCalculationError::TimingsFromAnotherCollection);
            }
        }

        for p in &self.general_after_order {
            let start = Instant::now();
            match p {
                GeneralAfterScheduleEntry::F64 { index, op } => {
                    // Find the internal state
                    let internal_state = internal_states
                        .get_general_mut_f64_state(*index)
                        .ok_or(ParameterCollectionGeneralCalculationError::F64IndexNotFound(*index))?;

                    let ctx = GeneralParameterContext {
                        timestep,
                        scenario_index,
                        network,
                        state,
                    };

                    match op {
                        GeneralAfterScheduleOperation::Value { parameter, output } => {
                            let value = parameter.after(ctx, internal_state).map_err(|source| {
                                ParameterCollectionGeneralCalculationError::CalculationError {
                                    name: parameter.name().clone(),
                                    source: Box::new(source),
                                }
                            })?;

                            state
                                .set_general_parameter_f64_after(*output, value)
                                .map_err(|source| {
                                    ParameterCollectionGeneralCalculationError::F64SetAfterStateError {
                                        name: parameter.name().clone(),
                                        source,
                                    }
                                })?;
                        }
                        GeneralAfterScheduleOperation::Hook { parameter } => {
                            parameter.after(ctx, internal_state).map_err(|source| {
                                ParameterCollectionGeneralCalculationError::CalculationError {
                                    name: parameter.name().clone(),
                                    source: Box::new(source),
                                }
                            })?;
                        }
                    }

                    if let Some(timings) = timings.as_deref_mut() {
                        unsafe {
                            timings.general_f64.get_unchecked_mut(*index.deref()).after += start.elapsed();
                        }
                    }
                }
                GeneralAfterScheduleEntry::U64 { index, op } => {
                    // Find the internal state
                    let internal_state = internal_states
                        .get_general_mut_u64_state(*index)
                        .ok_or(ParameterCollectionGeneralCalculationError::U64IndexNotFound(*index))?;

                    let ctx = GeneralParameterContext {
                        timestep,
                        scenario_index,
                        network,
                        state,
                    };

                    match op {
                        GeneralAfterScheduleOperation::Value { parameter, output } => {
                            let value = parameter.after(ctx, internal_state).map_err(|source| {
                                ParameterCollectionGeneralCalculationError::CalculationError {
                                    name: parameter.name().clone(),
                                    source: Box::new(source),
                                }
                            })?;

                            state
                                .set_general_parameter_u64_after(*output, value)
                                .map_err(|source| {
                                    ParameterCollectionGeneralCalculationError::U64SetAfterStateError {
                                        name: parameter.name().clone(),
                                        source,
                                    }
                                })?;
                        }
                        GeneralAfterScheduleOperation::Hook { parameter } => {
                            parameter.after(ctx, internal_state).map_err(|source| {
                                ParameterCollectionGeneralCalculationError::CalculationError {
                                    name: parameter.name().clone(),
                                    source: Box::new(source),
                                }
                            })?;
                        }
                    }

                    if let Some(timings) = timings.as_deref_mut() {
                        unsafe {
                            timings.general_u64.get_unchecked_mut(*index.deref()).after += start.elapsed();
                        }
                    }
                }
                GeneralAfterScheduleEntry::Multi { index, op } => {
                    // Find the internal state
                    let internal_state = internal_states
                        .get_general_mut_multi_state(*index)
                        .ok_or(ParameterCollectionGeneralCalculationError::MultiIndexNotFound(*index))?;

                    let ctx = GeneralParameterContext {
                        timestep,
                        scenario_index,
                        network,
                        state,
                    };

                    match op {
                        GeneralAfterScheduleOperation::Value { parameter, output } => {
                            let value = parameter.after(ctx, internal_state).map_err(|source| {
                                ParameterCollectionGeneralCalculationError::CalculationError {
                                    name: parameter.name().clone(),
                                    source: Box::new(source),
                                }
                            })?;

                            state
                                .set_general_parameter_multi_after(*output, value)
                                .map_err(|source| {
                                    ParameterCollectionGeneralCalculationError::MultiSetAfterStateError {
                                        name: parameter.name().clone(),
                                        source,
                                    }
                                })?;
                        }
                        GeneralAfterScheduleOperation::Hook { parameter } => {
                            parameter.after(ctx, internal_state).map_err(|source| {
                                ParameterCollectionGeneralCalculationError::CalculationError {
                                    name: parameter.name().clone(),
                                    source: Box::new(source),
                                }
                            })?;
                        }
                    }

                    if let Some(timings) = timings.as_deref_mut() {
                        unsafe {
                            timings.general_multi.get_unchecked_mut(*index.deref()).after += start.elapsed();
                        }
                    }
                }
            }
        }

        Ok(())
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

                    let ctx = SimpleParameterContext {
                        timestep,
                        scenario_index,
                        values: &state.get_simple_parameter_values(),
                    };

                    let value = p.compute(ctx, internal_state).map_err(|source| {
                        ParameterCollectionSimpleCalculationError::CalculationError {
                            name: p.name().clone(),
                            source,
                        }
                    })?;

                    state.set_simple_parameter_f64(*idx, value).map_err(|source| {
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

                    let ctx = SimpleParameterContext {
                        timestep,
                        scenario_index,
                        values: &state.get_simple_parameter_values(),
                    };

                    let value = p.compute(ctx, internal_state).map_err(|source| {
                        ParameterCollectionSimpleCalculationError::CalculationError {
                            name: p.name().clone(),
                            source,
                        }
                    })?;

                    state.set_simple_parameter_u64(*idx, value).map_err(|source| {
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

                    let ctx = SimpleParameterContext {
                        timestep,
                        scenario_index,
                        values: &state.get_simple_parameter_values(),
                    };

                    let value = p.compute(ctx, internal_state).map_err(|source| {
                        ParameterCollectionSimpleCalculationError::CalculationError {
                            name: p.name().clone(),
                            source,
                        }
                    })?;

                    state.set_simple_parameter_multi(*idx, value).map_err(|source| {
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

                    state.set_const_parameter_f64(*idx, value).map_err(|source| {
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
                    state.set_const_parameter_u64(*idx, value).map_err(|source| {
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
                    state.set_const_parameter_multi(*idx, value).map_err(|source| {
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

#[derive(Debug, Error)]
pub enum ParameterCollectionBuilderError {
    #[error("Duplicate parameter `{name}` found.")]
    DuplicateParameterName { name: ParameterName },
    #[error("Error building parameter `{name}`: {source}")]
    ParameterBuildError {
        name: ParameterName,
        #[source]
        source: Box<ParameterBuildError>,
    },
    #[error("Circular (or self) parameter references found: {names:?}")]
    CircularParameterReference { names: Vec<ParameterName> },
    #[error("Parameter not found: {name}.")]
    ParameterNotFound { name: ParameterName },
}

/// A builder for [`ParameterCollection`] that allows adding parameters without worrying about the
/// internal structure of the collection.
#[derive(Default, Debug)]
pub struct ParameterCollectionBuilder {
    pub f64: Vec<Box<dyn ParameterBuilder<f64>>>,
    pub u64: Vec<Box<dyn ParameterBuilder<u64>>>,
    pub multi: Vec<Box<dyn ParameterBuilder<MultiValue>>>,
}

impl ParameterCollectionBuilder {
    pub fn f64(&mut self, value: Box<dyn ParameterBuilder<f64>>) -> &mut Self {
        self.f64.push(value);
        self
    }

    pub fn u64(&mut self, value: Box<dyn ParameterBuilder<u64>>) -> &mut Self {
        self.u64.push(value);
        self
    }

    pub fn multi(&mut self, value: Box<dyn ParameterBuilder<MultiValue>>) -> &mut Self {
        self.multi.push(value);
        self
    }

    /// Returns true if the builder is empty.
    fn is_empty(&self) -> bool {
        self.f64.is_empty() && self.u64.is_empty() && self.multi.is_empty()
    }

    /// Total number of parameter builders in the collection builder.
    fn len(&self) -> usize {
        self.f64.len() + self.u64.len() + self.multi.len()
    }

    /// Returns true if the builder contains a parameter with the `name`.
    pub fn contains_name(&self, name: &ParameterName) -> bool {
        self.f64.iter().any(|p| p.name() == name)
            || self.u64.iter().any(|p| p.name() == name)
            || self.multi.iter().any(|p| p.name() == name)
    }

    pub fn build(
        mut self,
        resolution_maps: &mut ResolutionMaps,
    ) -> Result<ParameterCollection, ParameterCollectionBuilderError> {
        // Validate names before attempting resolution so duplicate builders always produce the
        // same error, including when neither builder can yet be resolved.
        let mut names = HashSet::with_capacity(self.len());
        for name in self
            .f64
            .iter()
            .map(|p| p.name())
            .chain(self.u64.iter().map(|p| p.name()))
            .chain(self.multi.iter().map(|p| p.name()))
        {
            if !names.insert(name.clone()) {
                return Err(ParameterCollectionBuilderError::DuplicateParameterName { name: name.clone() });
            }
        }

        let mut collection = ParameterCollection::default();

        let mut num_unbuilt = self.len();

        while !self.is_empty() {
            let mut failed_f64 = Vec::new();
            let mut failed_u64 = Vec::new();
            let mut failed_multi = Vec::new();

            for p in self.f64.into_iter() {
                let name = p.name().clone();

                if collection.has_name(&name) {
                    return Err(ParameterCollectionBuilderError::DuplicateParameterName { name });
                }

                match p.build(resolution_maps) {
                    Ok(maybe) => {
                        match maybe {
                            MaybeBuiltParameter::Built(built) => {
                                // Parameter successfully built. Let's add it to the collection, resolve order and resolution map.
                                let idx = match built {
                                    BuiltParameter::General(p) => collection.push_general_f64(p),
                                    BuiltParameter::Simple(p) => collection.push_simple_f64(p),
                                    BuiltParameter::Const(p) => collection.push_const_f64(p),
                                };

                                resolution_maps.parameters_f64.insert(name, idx);
                            }
                            MaybeBuiltParameter::Retry {
                                builder,
                                parameter_not_found,
                            } => {
                                failed_f64.push((builder, parameter_not_found));
                            }
                        }
                    }
                    Err(source) => {
                        return Err(ParameterCollectionBuilderError::ParameterBuildError {
                            name,
                            source: Box::new(source),
                        });
                    }
                }
            }

            for p in self.u64.into_iter() {
                let name = p.name().clone();

                if collection.has_name(&name) {
                    return Err(ParameterCollectionBuilderError::DuplicateParameterName { name });
                }

                match p.build(resolution_maps) {
                    Ok(maybe) => {
                        match maybe {
                            MaybeBuiltParameter::Built(built) => {
                                // Parameter successfully built. Let's add it to the collection, resolve order and resolution map.
                                let idx = match built {
                                    BuiltParameter::General(p) => collection.push_general_u64(p),
                                    BuiltParameter::Simple(p) => collection.push_simple_u64(p),
                                    BuiltParameter::Const(p) => collection.push_const_u64(p),
                                };

                                resolution_maps.parameters_u64.insert(name, idx);
                            }
                            MaybeBuiltParameter::Retry {
                                builder,
                                parameter_not_found,
                            } => {
                                failed_u64.push((builder, parameter_not_found));
                            }
                        }
                    }
                    Err(source) => {
                        return Err(ParameterCollectionBuilderError::ParameterBuildError {
                            name,
                            source: Box::new(source),
                        });
                    }
                }
            }

            for p in self.multi.into_iter() {
                let name = p.name().clone();

                if collection.has_name(&name) {
                    return Err(ParameterCollectionBuilderError::DuplicateParameterName { name });
                }

                match p.build(resolution_maps) {
                    Ok(maybe) => {
                        match maybe {
                            MaybeBuiltParameter::Built(built) => {
                                // Parameter successfully built. Let's add it to the collection, resolve order and resolution map.
                                let idx = match built {
                                    BuiltParameter::General(p) => collection.push_general_multi(p),
                                    BuiltParameter::Simple(p) => collection.push_simple_multi(p),
                                    BuiltParameter::Const(p) => collection.push_const_multi(p),
                                };

                                resolution_maps.parameters_multi.insert(name, idx);
                            }
                            MaybeBuiltParameter::Retry {
                                builder,
                                parameter_not_found,
                            } => {
                                failed_multi.push((builder, parameter_not_found));
                            }
                        }
                    }
                    Err(source) => {
                        return Err(ParameterCollectionBuilderError::ParameterBuildError {
                            name,
                            source: Box::new(source),
                        });
                    }
                }
            }

            let new_total = failed_f64.len() + failed_u64.len() + failed_multi.len();

            if num_unbuilt == new_total {
                let (failed_names, missing_names): (Vec<_>, Vec<_>) = failed_f64
                    .into_iter()
                    .map(|(b, pn)| (b.name().clone(), pn))
                    .chain(
                        failed_u64
                            .into_iter()
                            .map(|(b, pn)| (b.name().clone(), pn))
                            .chain(failed_multi.into_iter().map(|(b, pn)| (b.name().clone(), pn))),
                    )
                    .unzip();

                // If any of the missing names are not in the failed names, then we have legitimate
                // missing parameter (or typo).
                for missing in missing_names {
                    if !failed_names.contains(&missing) {
                        return Err(ParameterCollectionBuilderError::ParameterNotFound { name: missing });
                    }
                }
                // Otherwise all the missing names are other failed parameters and this is a circular
                // or self reference.
                return Err(ParameterCollectionBuilderError::CircularParameterReference { names: failed_names });
            } else {
                // Keep the builders for the next iteration, but we no longer need the missing parameter names.
                self.f64 = failed_f64.into_iter().map(|(b, _)| b).collect();
                self.u64 = failed_u64.into_iter().map(|(b, _)| b).collect();
                self.multi = failed_multi.into_iter().map(|(b, _)| b).collect();

                num_unbuilt = new_total;
            }
        }

        Ok(collection)
    }
}

#[cfg(test)]
mod tests {
    use super::test_utils::{
        TestBuildKind, TestParameter, TestParameterBuilder, TestParameterFailure, TestValueType, test_parameter_state,
    };
    use super::{
        GeneralCalculationError, GeneralParameterEntry, ParameterBuildError, ParameterCollection,
        ParameterCollectionBuilder, ParameterCollectionBuilderError, ParameterCollectionConstCalculationError,
        ParameterCollectionGeneralCalculationError, ParameterCollectionSetupError,
        ParameterCollectionSimpleCalculationError, ParameterIndex, ParameterName, ParameterSetupError, ParameterState,
        ParameterStates, ParameterTimings,
    };
    use crate::network::{Network, ResolutionMaps};
    use crate::parameters::errors::{ConstCalculationError, SimpleCalculationError};
    use crate::scenario::ScenarioIndex;
    use crate::state::{MultiValue, SetStateError, StateBuilder};
    use crate::test_utils::default_domain;
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex};

    fn add_test_parameter_builder(
        collection: &mut ParameterCollectionBuilder,
        value_type: TestValueType,
        builder: TestParameterBuilder,
    ) {
        match value_type {
            TestValueType::F64 => {
                collection.f64(Box::new(builder));
            }
            TestValueType::U64 => {
                collection.u64(Box::new(builder));
            }
            TestValueType::Multi => {
                collection.multi(Box::new(builder));
            }
        }
    }

    fn assert_index_kind<T: std::fmt::Debug>(index: &ParameterIndex<T>, kind: TestBuildKind, expected_position: usize) {
        match (index, kind) {
            (ParameterIndex::Const(index), TestBuildKind::Const) => assert_eq!(**index, expected_position),
            (ParameterIndex::Simple(index), TestBuildKind::Simple) => assert_eq!(**index, expected_position),
            (ParameterIndex::General(registration), TestBuildKind::General) => {
                assert_eq!(*registration.parameter, expected_position);
                assert!(registration.before.is_some());
                assert!(registration.after.is_none());
            }
            (actual, expected) => panic!("expected {expected:?} index, got {actual:?}"),
        }
    }

    fn assert_general_registration<T: std::fmt::Debug>(index: &ParameterIndex<T>, has_before: bool, has_after: bool) {
        let ParameterIndex::General(registration) = index else {
            panic!("expected a general parameter index, got {index:?}");
        };
        assert_eq!(registration.before.is_some(), has_before);
        assert_eq!(registration.after.is_some(), has_after);
    }

    fn assert_f64_lookup(collection: &ParameterCollection, index: ParameterIndex<f64>, expected_name: &str) {
        let name: ParameterName = expected_name.into();
        assert_eq!(collection.get_f64(index).unwrap().name(), &name);
        assert_eq!(collection.get_f64_by_name(&name).unwrap().name(), &name);
        assert_eq!(collection.get_f64_index_by_name(&name).as_ref(), Some(&index));
        if let ParameterIndex::General(registration) = index {
            assert_eq!(
                collection.get_general_f64(registration.parameter).unwrap().name(),
                &name
            );
        }
    }

    fn assert_u64_lookup(collection: &ParameterCollection, index: ParameterIndex<u64>, expected_name: &str) {
        let name: ParameterName = expected_name.into();
        assert_eq!(collection.get_u64(index).unwrap().name(), &name);
        assert_eq!(collection.get_u64_by_name(&name).unwrap().name(), &name);
        assert_eq!(collection.get_u64_index_by_name(&name).as_ref(), Some(&index));
        if let ParameterIndex::General(registration) = index {
            assert_eq!(
                collection.get_general_u64(registration.parameter).unwrap().name(),
                &name
            );
        }
    }

    fn assert_multi_lookup(collection: &ParameterCollection, index: &ParameterIndex<MultiValue>, expected_name: &str) {
        let name: ParameterName = expected_name.into();
        assert_eq!(collection.get_multi(index).unwrap().name(), &name);
        assert_eq!(collection.get_multi_by_name(&name).unwrap().name(), &name);
        assert_eq!(collection.get_multi_index_by_name(&name).as_ref(), Some(index));
        if let ParameterIndex::General(registration) = index {
            assert_eq!(
                collection.get_general_multi(&registration.parameter).unwrap().name(),
                &name
            );
        }
    }

    fn expect_const_index<T>(index: ParameterIndex<T>) -> super::ConstParameterIndex<T> {
        match index {
            ParameterIndex::Const(index) => index,
            _ => panic!("expected a constant parameter index"),
        }
    }

    fn expect_simple_index<T>(index: ParameterIndex<T>) -> super::SimpleParameterIndex<T> {
        match index {
            ParameterIndex::Simple(index) => index,
            _ => panic!("expected a simple parameter index"),
        }
    }

    fn expect_general_registration<T>(index: ParameterIndex<T>) -> super::GeneralParameterRegistration<T> {
        match index {
            ParameterIndex::General(registration) => registration,
            _ => panic!("expected a general parameter index"),
        }
    }

    fn assert_test_parameter_state(
        state: &Option<Box<dyn ParameterState>>,
        owner: &str,
        timestep_count: usize,
        scenario_id: usize,
        calls: usize,
    ) {
        let state = test_parameter_state(state).expect("expected test parameter state");
        assert_eq!(state.owner(), owner);
        assert_eq!(state.timestep_count(), timestep_count);
        assert_eq!(state.scenario_id(), scenario_id);
        assert_eq!(state.calls(), calls);
    }

    fn assert_general_calculation_error(
        error: ParameterCollectionGeneralCalculationError,
        expected_name: &str,
        expected_message: &str,
    ) {
        match error {
            ParameterCollectionGeneralCalculationError::CalculationError { name, source } => {
                assert_eq!(name, ParameterName::from(expected_name));
                assert!(matches!(
                    source.as_ref(),
                    GeneralCalculationError::Internal { message } if message == expected_message
                ));
            }
            other => panic!("expected a general calculation error, got {other:?}"),
        }
    }

    /// Test naming constraints on parameter collection.
    #[test]
    fn test_parameter_collection_name_constraints() {
        let mut collection = ParameterCollectionBuilder::default();

        collection.f64(Box::new(TestParameterBuilder::default()));
        collection.f64(Box::new(TestParameterBuilder::default()));

        assert!(collection.build(&mut ResolutionMaps::new(default_domain())).is_err());

        let mut collection = ParameterCollectionBuilder::default();

        collection.u64(Box::new(TestParameterBuilder::default()));
        collection.u64(Box::new(TestParameterBuilder::default()));

        assert!(collection.build(&mut ResolutionMaps::new(default_domain())).is_err());

        let mut collection = ParameterCollectionBuilder::default();

        collection.f64(Box::new(TestParameterBuilder::default()));
        collection.u64(Box::new(TestParameterBuilder::default()));

        assert!(collection.build(&mut ResolutionMaps::new(default_domain())).is_err());
    }

    #[test]
    fn builder_registers_each_value_and_parameter_kind() {
        let mut builder = ParameterCollectionBuilder::default();
        for (value_type, prefix) in [
            (TestValueType::F64, "f64"),
            (TestValueType::U64, "u64"),
            (TestValueType::Multi, "multi"),
        ] {
            for (kind, suffix) in [
                (TestBuildKind::Const, "const"),
                (TestBuildKind::Simple, "simple"),
                (TestBuildKind::General, "general"),
            ] {
                add_test_parameter_builder(
                    &mut builder,
                    value_type,
                    TestParameterBuilder::new(&format!("{prefix}-{suffix}"), kind),
                );
            }
        }

        let mut maps = ResolutionMaps::new(default_domain());
        let collection = builder.build(&mut maps).unwrap();
        let size = collection.size();
        assert_eq!(size.const_f64, 1);
        assert_eq!(size.const_u64, 1);
        assert_eq!(size.const_multi, 1);
        assert_eq!(size.simple_f64, 1);
        assert_eq!(size.simple_u64, 1);
        assert_eq!(size.simple_multi, 1);
        assert_eq!(size.general_before_f64, 1);
        assert_eq!(size.general_before_u64, 1);
        assert_eq!(size.general_before_multi, 1);
        assert_eq!(size.general_after_f64, 0);
        assert_eq!(size.general_after_u64, 0);
        assert_eq!(size.general_after_multi, 0);

        for (name, kind) in [
            ("f64-const", TestBuildKind::Const),
            ("f64-simple", TestBuildKind::Simple),
            ("f64-general", TestBuildKind::General),
        ] {
            let name: ParameterName = name.into();
            let mapped = maps.parameters_f64.get(&name).unwrap();
            assert_index_kind(mapped, kind, 0);
            assert_eq!(collection.get_f64(*mapped).unwrap().name(), &name);
            assert_eq!(collection.get_f64_by_name(&name).unwrap().name(), &name);
            assert_eq!(collection.get_f64_index_by_name(&name).as_ref(), Some(mapped));
            assert!(!maps.parameters_u64.contains_key(&name));
            assert!(!maps.parameters_multi.contains_key(&name));
        }
        for (name, kind) in [
            ("u64-const", TestBuildKind::Const),
            ("u64-simple", TestBuildKind::Simple),
            ("u64-general", TestBuildKind::General),
        ] {
            let name: ParameterName = name.into();
            let mapped = maps.parameters_u64.get(&name).unwrap();
            assert_index_kind(mapped, kind, 0);
            assert_eq!(collection.get_u64(*mapped).unwrap().name(), &name);
            assert_eq!(collection.get_u64_by_name(&name).unwrap().name(), &name);
            assert_eq!(collection.get_u64_index_by_name(&name).as_ref(), Some(mapped));
            assert!(!maps.parameters_f64.contains_key(&name));
            assert!(!maps.parameters_multi.contains_key(&name));
        }
        for (name, kind) in [
            ("multi-const", TestBuildKind::Const),
            ("multi-simple", TestBuildKind::Simple),
            ("multi-general", TestBuildKind::General),
        ] {
            let name: ParameterName = name.into();
            let mapped = maps.parameters_multi.get(&name).unwrap();
            assert_index_kind(mapped, kind, 0);
            assert_eq!(collection.get_multi(mapped).unwrap().name(), &name);
            assert_eq!(collection.get_multi_by_name(&name).unwrap().name(), &name);
            assert_eq!(collection.get_multi_index_by_name(&name).as_ref(), Some(mapped));
            assert!(!maps.parameters_f64.contains_key(&name));
            assert!(!maps.parameters_u64.contains_key(&name));
        }
    }

    #[test]
    fn duplicate_names_return_exact_error_for_all_type_combinations() {
        for (first, second) in [
            (TestValueType::F64, TestValueType::F64),
            (TestValueType::U64, TestValueType::U64),
            (TestValueType::Multi, TestValueType::Multi),
            (TestValueType::F64, TestValueType::U64),
            (TestValueType::F64, TestValueType::Multi),
            (TestValueType::U64, TestValueType::Multi),
        ] {
            let mut builder = ParameterCollectionBuilder::default();
            add_test_parameter_builder(
                &mut builder,
                first,
                TestParameterBuilder::new("duplicate", TestBuildKind::Const),
            );
            add_test_parameter_builder(
                &mut builder,
                second,
                TestParameterBuilder::new("duplicate", TestBuildKind::Simple),
            );

            assert!(matches!(
                builder.build(&mut ResolutionMaps::new(default_domain())),
                Err(ParameterCollectionBuilderError::DuplicateParameterName { name })
                    if name == ParameterName::from("duplicate")
            ));
        }
    }

    #[test]
    fn duplicate_unresolved_builders_are_reported_as_duplicates() {
        let mut builder = ParameterCollectionBuilder::default();
        builder
            .f64(Box::new(
                TestParameterBuilder::new("duplicate", TestBuildKind::General).depending_on("missing"),
            ))
            .f64(Box::new(
                TestParameterBuilder::new("duplicate", TestBuildKind::General).depending_on("missing"),
            ));

        assert!(matches!(
            builder.build(&mut ResolutionMaps::new(default_domain())),
            Err(ParameterCollectionBuilderError::DuplicateParameterName { name })
                if name == ParameterName::from("duplicate")
        ));
    }

    #[test]
    fn forward_reference_builds_after_dependency() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let dependent = TestParameterBuilder::with_build_order("dependent", TestBuildKind::General, order.clone())
            .depending_on("source");
        let dependent_attempts = dependent.attempts();
        let source = TestParameterBuilder::with_build_order("source", TestBuildKind::Const, order.clone());
        let source_attempts = source.attempts();
        let mut builder = ParameterCollectionBuilder::default();
        builder.f64(Box::new(dependent)).f64(Box::new(source));

        let mut maps = ResolutionMaps::new(default_domain());
        let collection = builder.build(&mut maps).unwrap();

        assert_eq!(order.lock().unwrap().as_slice(), ["source", "dependent"]);
        assert_eq!(dependent_attempts.load(Ordering::Relaxed), 2);
        assert_eq!(source_attempts.load(Ordering::Relaxed), 1);
        assert!(collection.has_name(&"source".into()));
        assert!(collection.has_name(&"dependent".into()));
    }

    #[test]
    fn reverse_order_dependency_chain_is_topologically_built() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let a = TestParameterBuilder::with_build_order("a", TestBuildKind::General, order.clone()).depending_on("b");
        let b = TestParameterBuilder::with_build_order("b", TestBuildKind::General, order.clone()).depending_on("c");
        let c = TestParameterBuilder::with_build_order("c", TestBuildKind::Const, order.clone());
        let a_attempts = a.attempts();
        let b_attempts = b.attempts();
        let c_attempts = c.attempts();
        let mut builder = ParameterCollectionBuilder::default();
        builder.f64(Box::new(a)).f64(Box::new(b)).f64(Box::new(c));

        builder.build(&mut ResolutionMaps::new(default_domain())).unwrap();

        assert_eq!(order.lock().unwrap().as_slice(), ["c", "b", "a"]);
        assert_eq!(a_attempts.load(Ordering::Relaxed), 3);
        assert_eq!(b_attempts.load(Ordering::Relaxed), 2);
        assert_eq!(c_attempts.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn dependencies_can_progress_across_typed_builder_vectors() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let f64_builder =
            TestParameterBuilder::with_build_order("f64", TestBuildKind::General, order.clone()).depending_on("u64");
        let u64_builder =
            TestParameterBuilder::with_build_order("u64", TestBuildKind::General, order.clone()).depending_on("multi");
        let multi_builder = TestParameterBuilder::with_build_order("multi", TestBuildKind::Const, order.clone());
        let f64_attempts = f64_builder.attempts();
        let u64_attempts = u64_builder.attempts();
        let multi_attempts = multi_builder.attempts();
        let mut builder = ParameterCollectionBuilder::default();
        builder
            .f64(Box::new(f64_builder))
            .u64(Box::new(u64_builder))
            .multi(Box::new(multi_builder));

        let mut maps = ResolutionMaps::new(default_domain());
        let collection = builder.build(&mut maps).unwrap();

        assert_eq!(order.lock().unwrap().as_slice(), ["multi", "u64", "f64"]);
        assert_eq!(f64_attempts.load(Ordering::Relaxed), 3);
        assert_eq!(u64_attempts.load(Ordering::Relaxed), 2);
        assert_eq!(multi_attempts.load(Ordering::Relaxed), 1);
        assert!(collection.get_f64_by_name(&"f64".into()).is_some());
        assert!(collection.get_u64_by_name(&"u64".into()).is_some());
        assert!(collection.get_multi_by_name(&"multi".into()).is_some());
    }

    #[test]
    fn missing_dependency_returns_exact_missing_name_for_all_types() {
        for value_type in [TestValueType::F64, TestValueType::U64, TestValueType::Multi] {
            let mut builder = ParameterCollectionBuilder::default();
            add_test_parameter_builder(
                &mut builder,
                value_type,
                TestParameterBuilder::new("dependent", TestBuildKind::General).depending_on("missing"),
            );

            assert!(matches!(
                builder.build(&mut ResolutionMaps::new(default_domain())),
                Err(ParameterCollectionBuilderError::ParameterNotFound { name })
                    if name == ParameterName::from("missing")
            ));
        }
    }

    #[test]
    fn self_reference_returns_circular_reference() {
        let mut builder = ParameterCollectionBuilder::default();
        builder.f64(Box::new(
            TestParameterBuilder::new("self", TestBuildKind::General).depending_on("self"),
        ));

        match builder.build(&mut ResolutionMaps::new(default_domain())) {
            Err(ParameterCollectionBuilderError::CircularParameterReference { names }) => {
                assert_eq!(names, vec![ParameterName::from("self")]);
            }
            result => panic!("expected a circular reference error, got {result:?}"),
        }
    }

    #[test]
    fn three_parameter_cycle_returns_all_cycle_names() {
        let mut builder = ParameterCollectionBuilder::default();
        builder
            .f64(Box::new(
                TestParameterBuilder::new("a", TestBuildKind::General).depending_on("b"),
            ))
            .f64(Box::new(
                TestParameterBuilder::new("b", TestBuildKind::General).depending_on("c"),
            ))
            .f64(Box::new(
                TestParameterBuilder::new("c", TestBuildKind::General).depending_on("a"),
            ));

        match builder.build(&mut ResolutionMaps::new(default_domain())) {
            Err(ParameterCollectionBuilderError::CircularParameterReference { names }) => {
                assert_eq!(names, ["a", "b", "c"].map(ParameterName::from));
            }
            result => panic!("expected a circular reference error, got {result:?}"),
        }
    }

    #[test]
    fn mixed_cycle_and_missing_dependency_prefers_missing_error() {
        let mut builder = ParameterCollectionBuilder::default();
        builder
            .f64(Box::new(
                TestParameterBuilder::new("a", TestBuildKind::General).depending_on("b"),
            ))
            .f64(Box::new(
                TestParameterBuilder::new("b", TestBuildKind::General).depending_on("a"),
            ))
            .f64(Box::new(
                TestParameterBuilder::new("c", TestBuildKind::General).depending_on("missing"),
            ));

        assert!(matches!(
            builder.build(&mut ResolutionMaps::new(default_domain())),
            Err(ParameterCollectionBuilderError::ParameterNotFound { name })
                if name == ParameterName::from("missing")
        ));
    }

    #[test]
    fn parameter_build_error_preserves_parameter_name_and_source() {
        let mut builder = ParameterCollectionBuilder::default();
        builder.f64(Box::new(
            TestParameterBuilder::new("broken", TestBuildKind::General).failing("intentional failure"),
        ));

        let error = builder
            .build(&mut ResolutionMaps::new(default_domain()))
            .expect_err("the scripted builder should fail");
        let display = error.to_string();
        match error {
            ParameterCollectionBuilderError::ParameterBuildError { name, source } => {
                assert_eq!(name, ParameterName::from("broken"));
                assert!(matches!(
                    source.as_ref(),
                    ParameterBuildError::NoCalculationPhase { detail } if detail == "intentional failure"
                ));
            }
            other => panic!("expected a wrapped parameter build error, got {other:?}"),
        }
        assert!(display.contains("broken"));
        assert!(display.contains("intentional failure"));
    }

    #[test]
    fn size_counts_every_storage_and_general_output_category() {
        let mut collection = ParameterCollection::default();

        collection.push_const_f64(Box::new(TestParameter::named("f64-const")));
        collection.push_simple_f64(Box::new(TestParameter::named("f64-simple")));
        collection.push_general_f64(GeneralParameterEntry::before(TestParameter::named("f64-before")));
        collection.push_general_f64(GeneralParameterEntry::after(TestParameter::named("f64-after")));
        collection.push_general_f64(GeneralParameterEntry::both(TestParameter::named("f64-both")));
        collection.push_general_f64(GeneralParameterEntry::before_with_after_hook(TestParameter::named(
            "f64-hook",
        )));

        collection.push_const_u64(Box::new(TestParameter::named("u64-const")));
        collection.push_simple_u64(Box::new(TestParameter::named("u64-simple")));
        collection.push_general_u64(GeneralParameterEntry::before(TestParameter::named("u64-before")));
        collection.push_general_u64(GeneralParameterEntry::after(TestParameter::named("u64-after")));
        collection.push_general_u64(GeneralParameterEntry::both(TestParameter::named("u64-both")));
        collection.push_general_u64(GeneralParameterEntry::before_with_after_hook(TestParameter::named(
            "u64-hook",
        )));

        collection.push_const_multi(Box::new(TestParameter::named("multi-const")));
        collection.push_simple_multi(Box::new(TestParameter::named("multi-simple")));
        collection.push_general_multi(GeneralParameterEntry::before(TestParameter::named("multi-before")));
        collection.push_general_multi(GeneralParameterEntry::after(TestParameter::named("multi-after")));
        collection.push_general_multi(GeneralParameterEntry::both(TestParameter::named("multi-both")));
        collection.push_general_multi(GeneralParameterEntry::before_with_after_hook(TestParameter::named(
            "multi-hook",
        )));

        let size = collection.size();
        assert_eq!(size.const_f64, 1);
        assert_eq!(size.const_u64, 1);
        assert_eq!(size.const_multi, 1);
        assert_eq!(size.simple_f64, 1);
        assert_eq!(size.simple_u64, 1);
        assert_eq!(size.simple_multi, 1);
        assert_eq!(size.general_before_f64, 3);
        assert_eq!(size.general_before_u64, 3);
        assert_eq!(size.general_before_multi, 3);
        assert_eq!(size.general_after_f64, 2);
        assert_eq!(size.general_after_u64, 2);
        assert_eq!(size.general_after_multi, 2);
    }

    #[test]
    fn typed_lookup_round_trips_indices_names_and_phase_registrations() {
        let mut collection = ParameterCollection::default();

        let f64_const = collection.push_const_f64(Box::new(TestParameter::named("f64-const")));
        let f64_simple = collection.push_simple_f64(Box::new(TestParameter::named("f64-simple")));
        let f64_before = collection.push_general_f64(GeneralParameterEntry::before(TestParameter::named("f64-before")));
        let f64_after = collection.push_general_f64(GeneralParameterEntry::after(TestParameter::named("f64-after")));
        let f64_both = collection.push_general_f64(GeneralParameterEntry::both(TestParameter::named("f64-both")));
        let f64_hook = collection.push_general_f64(GeneralParameterEntry::before_with_after_hook(
            TestParameter::named("f64-hook"),
        ));

        let u64_const = collection.push_const_u64(Box::new(TestParameter::named("u64-const")));
        let u64_simple = collection.push_simple_u64(Box::new(TestParameter::named("u64-simple")));
        let u64_before = collection.push_general_u64(GeneralParameterEntry::before(TestParameter::named("u64-before")));
        let u64_after = collection.push_general_u64(GeneralParameterEntry::after(TestParameter::named("u64-after")));
        let u64_both = collection.push_general_u64(GeneralParameterEntry::both(TestParameter::named("u64-both")));
        let u64_hook = collection.push_general_u64(GeneralParameterEntry::before_with_after_hook(
            TestParameter::named("u64-hook"),
        ));

        let multi_const = collection.push_const_multi(Box::new(TestParameter::named("multi-const")));
        let multi_simple = collection.push_simple_multi(Box::new(TestParameter::named("multi-simple")));
        let multi_before =
            collection.push_general_multi(GeneralParameterEntry::before(TestParameter::named("multi-before")));
        let multi_after =
            collection.push_general_multi(GeneralParameterEntry::after(TestParameter::named("multi-after")));
        let multi_both = collection.push_general_multi(GeneralParameterEntry::both(TestParameter::named("multi-both")));
        let multi_hook = collection.push_general_multi(GeneralParameterEntry::before_with_after_hook(
            TestParameter::named("multi-hook"),
        ));

        assert_f64_lookup(&collection, f64_const, "f64-const");
        assert_f64_lookup(&collection, f64_simple, "f64-simple");
        assert_f64_lookup(&collection, f64_before, "f64-before");
        assert_f64_lookup(&collection, f64_after, "f64-after");
        assert_f64_lookup(&collection, f64_both, "f64-both");
        assert_f64_lookup(&collection, f64_hook, "f64-hook");

        assert_u64_lookup(&collection, u64_const, "u64-const");
        assert_u64_lookup(&collection, u64_simple, "u64-simple");
        assert_u64_lookup(&collection, u64_before, "u64-before");
        assert_u64_lookup(&collection, u64_after, "u64-after");
        assert_u64_lookup(&collection, u64_both, "u64-both");
        assert_u64_lookup(&collection, u64_hook, "u64-hook");

        assert_multi_lookup(&collection, &multi_const, "multi-const");
        assert_multi_lookup(&collection, &multi_simple, "multi-simple");
        assert_multi_lookup(&collection, &multi_before, "multi-before");
        assert_multi_lookup(&collection, &multi_after, "multi-after");
        assert_multi_lookup(&collection, &multi_both, "multi-both");
        assert_multi_lookup(&collection, &multi_hook, "multi-hook");

        for (index, has_before, has_after) in [
            (&f64_before, true, false),
            (&f64_after, false, true),
            (&f64_both, true, true),
            (&f64_hook, true, false),
        ] {
            assert_general_registration(index, has_before, has_after);
        }
        for (index, has_before, has_after) in [
            (&u64_before, true, false),
            (&u64_after, false, true),
            (&u64_both, true, true),
            (&u64_hook, true, false),
        ] {
            assert_general_registration(index, has_before, has_after);
        }
        for (index, has_before, has_after) in [
            (&multi_before, true, false),
            (&multi_after, false, true),
            (&multi_both, true, true),
            (&multi_hook, true, false),
        ] {
            assert_general_registration(index, has_before, has_after);
        }
    }

    #[test]
    fn parameter_setup_initializes_state_for_all_kinds_and_types() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut collection = ParameterCollection::default();

        let f64_const_probe = TestParameter::<f64>::new("f64-const-state", events.clone());
        let f64_const_calls = f64_const_probe.setup_calls();
        let f64_const = expect_const_index(collection.push_const_f64(Box::new(f64_const_probe)));
        let f64_simple_probe = TestParameter::<f64>::new("f64-simple-state", events.clone());
        let f64_simple_calls = f64_simple_probe.setup_calls();
        let f64_simple = expect_simple_index(collection.push_simple_f64(Box::new(f64_simple_probe)));
        let f64_general_probe = TestParameter::<f64>::new("f64-general-state", events.clone());
        let f64_general_calls = f64_general_probe.setup_calls();
        let f64_general =
            expect_general_registration(collection.push_general_f64(GeneralParameterEntry::before(f64_general_probe)));

        let u64_const_probe = TestParameter::<u64>::new("u64-const-state", events.clone());
        let u64_const_calls = u64_const_probe.setup_calls();
        let u64_const = expect_const_index(collection.push_const_u64(Box::new(u64_const_probe)));
        let u64_simple_probe = TestParameter::<u64>::new("u64-simple-state", events.clone());
        let u64_simple_calls = u64_simple_probe.setup_calls();
        let u64_simple = expect_simple_index(collection.push_simple_u64(Box::new(u64_simple_probe)));
        let u64_general_probe = TestParameter::<u64>::new("u64-general-state", events.clone());
        let u64_general_calls = u64_general_probe.setup_calls();
        let u64_general =
            expect_general_registration(collection.push_general_u64(GeneralParameterEntry::before(u64_general_probe)));

        let multi_const_probe = TestParameter::<MultiValue>::new("multi-const-state", events.clone());
        let multi_const_calls = multi_const_probe.setup_calls();
        let multi_const = expect_const_index(collection.push_const_multi(Box::new(multi_const_probe)));
        let multi_simple_probe = TestParameter::<MultiValue>::new("multi-simple-state", events.clone());
        let multi_simple_calls = multi_simple_probe.setup_calls();
        let multi_simple = expect_simple_index(collection.push_simple_multi(Box::new(multi_simple_probe)));
        let multi_general_probe = TestParameter::<MultiValue>::new("multi-general-state", events);
        let multi_general_calls = multi_general_probe.setup_calls();
        let multi_general = expect_general_registration(
            collection.push_general_multi(GeneralParameterEntry::before(multi_general_probe)),
        );

        let domain = default_domain();
        let timesteps = domain.time().timesteps();
        let scenario = ScenarioIndex::default();
        let mut states = ParameterStates::from_collection(&collection, timesteps, &scenario).unwrap();

        assert_test_parameter_state(
            states.get_const_f64_state(f64_const).unwrap(),
            "f64-const-state",
            timesteps.len(),
            0,
            0,
        );
        assert_test_parameter_state(
            states.get_simple_f64_state(f64_simple).unwrap(),
            "f64-simple-state",
            timesteps.len(),
            0,
            0,
        );
        assert_test_parameter_state(
            states.get_general_f64_state(f64_general.parameter).unwrap(),
            "f64-general-state",
            timesteps.len(),
            0,
            0,
        );
        assert_test_parameter_state(
            states.get_const_mut_u64_state(u64_const).unwrap(),
            "u64-const-state",
            timesteps.len(),
            0,
            0,
        );
        assert_test_parameter_state(
            states.get_simple_mut_u64_state(u64_simple).unwrap(),
            "u64-simple-state",
            timesteps.len(),
            0,
            0,
        );
        assert_test_parameter_state(
            states.get_general_mut_u64_state(u64_general.parameter).unwrap(),
            "u64-general-state",
            timesteps.len(),
            0,
            0,
        );
        assert_test_parameter_state(
            states.get_const_mut_multi_state(multi_const).unwrap(),
            "multi-const-state",
            timesteps.len(),
            0,
            0,
        );
        assert_test_parameter_state(
            states.get_simple_mut_multi_state(multi_simple).unwrap(),
            "multi-simple-state",
            timesteps.len(),
            0,
            0,
        );
        assert_test_parameter_state(
            states.get_general_mut_multi_state(multi_general.parameter).unwrap(),
            "multi-general-state",
            timesteps.len(),
            0,
            0,
        );

        for calls in [
            f64_const_calls,
            f64_simple_calls,
            f64_general_calls,
            u64_const_calls,
            u64_simple_calls,
            u64_general_calls,
            multi_const_calls,
            multi_simple_calls,
            multi_general_calls,
        ] {
            assert_eq!(calls.load(Ordering::Relaxed), 1);
        }
    }

    #[test]
    fn parameter_setup_error_reports_parameter_name_for_each_kind() {
        pyo3::Python::initialize();
        for kind in [TestBuildKind::Const, TestBuildKind::Simple, TestBuildKind::General] {
            let expected_name = format!("broken-{kind:?}");
            let probe = TestParameter::<f64>::new(&expected_name, Arc::new(Mutex::new(Vec::new())))
                .failing(TestParameterFailure::Setup);
            let mut collection = ParameterCollection::default();
            match kind {
                TestBuildKind::Const => {
                    collection.push_const_f64(Box::new(probe));
                }
                TestBuildKind::Simple => {
                    collection.push_simple_f64(Box::new(probe));
                }
                TestBuildKind::General => {
                    collection.push_general_f64(GeneralParameterEntry::before(probe));
                }
            }

            let domain = default_domain();
            let error = match ParameterStates::from_collection(
                &collection,
                domain.time().timesteps(),
                &ScenarioIndex::default(),
            ) {
                Err(error) => error,
                Ok(_) => panic!("setup should fail"),
            };
            let ParameterCollectionSetupError { name, source } = error;
            assert_eq!(*name, ParameterName::from(expected_name.as_str()));
            assert!(matches!(source.as_ref(),
                ParameterSetupError::TestError(msg) if msg == "lifecycle-probe"));
        }
    }

    #[test]
    fn compute_const_covers_all_types_dependency_order_and_internal_state() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut collection = ParameterCollection::default();
        let f64_source = expect_const_index(
            collection.push_const_f64(Box::new(TestParameter::<f64>::new("const-f64-source", events.clone()))),
        );
        let u64_index = expect_const_index(
            collection.push_const_u64(Box::new(TestParameter::<u64>::new("const-u64", events.clone()))),
        );
        let multi_index = expect_const_index(collection.push_const_multi(Box::new(TestParameter::<MultiValue>::new(
            "const-multi",
            events.clone(),
        ))));
        let f64_dependent = expect_const_index(collection.push_const_f64(Box::new(
            TestParameter::<f64>::new("const-f64-dependent", events.clone()).with_const_dependency(f64_source),
        )));

        let domain = default_domain();
        let timesteps = domain.time().timesteps();
        let scenario = ScenarioIndex::default();
        let mut internal_states = ParameterStates::from_collection(&collection, timesteps, &scenario).unwrap();
        let mut state = StateBuilder::new(Vec::new(), 0).with_parameters(&collection).build();
        collection
            .compute_const(&scenario, &mut state, &mut internal_states)
            .unwrap();

        assert_eq!(
            events.lock().unwrap().as_slice(),
            [
                "const-f64-source:const",
                "const-u64:const",
                "const-multi:const",
                "const-f64-dependent:const"
            ]
        );
        let values = state.get_const_parameter_values();
        assert_eq!(values.get_f64(f64_source).unwrap(), 11.0);
        assert_eq!(values.get_f64(f64_dependent).unwrap(), 12.0);
        assert_eq!(values.get_u64(u64_index).unwrap(), 12);
        assert_eq!(values.get_multi_f64(multi_index, "value").unwrap(), 13.0);
        assert_eq!(values.get_multi_u64(multi_index, "index").unwrap(), 14);
        assert_test_parameter_state(
            internal_states.get_const_f64_state(f64_source).unwrap(),
            "const-f64-source",
            timesteps.len(),
            0,
            1,
        );
        assert_test_parameter_state(
            internal_states.get_const_f64_state(f64_dependent).unwrap(),
            "const-f64-dependent",
            timesteps.len(),
            0,
            1,
        );
        assert_test_parameter_state(
            internal_states.get_const_mut_u64_state(u64_index).unwrap(),
            "const-u64",
            timesteps.len(),
            0,
            1,
        );
        assert_test_parameter_state(
            internal_states.get_const_mut_multi_state(multi_index).unwrap(),
            "const-multi",
            timesteps.len(),
            0,
            1,
        );
    }

    #[test]
    fn compute_simple_covers_all_types_dependency_order_and_internal_state() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut collection = ParameterCollection::default();
        let f64_source = expect_simple_index(
            collection.push_simple_f64(Box::new(TestParameter::<f64>::new("simple-f64-source", events.clone()))),
        );
        let u64_index = expect_simple_index(
            collection.push_simple_u64(Box::new(TestParameter::<u64>::new("simple-u64", events.clone()))),
        );
        let multi_index = expect_simple_index(collection.push_simple_multi(Box::new(
            TestParameter::<MultiValue>::new("simple-multi", events.clone()),
        )));
        let f64_dependent = expect_simple_index(collection.push_simple_f64(Box::new(
            TestParameter::<f64>::new("simple-f64-dependent", events.clone()).with_simple_dependency(f64_source),
        )));

        let domain = default_domain();
        let timesteps = domain.time().timesteps();
        let scenario = ScenarioIndex::default();
        let mut internal_states = ParameterStates::from_collection(&collection, timesteps, &scenario).unwrap();
        let mut state = StateBuilder::new(Vec::new(), 0).with_parameters(&collection).build();
        collection
            .compute_simple(&timesteps[0], &scenario, &mut state, &mut internal_states)
            .unwrap();
        collection
            .compute_simple(&timesteps[1], &scenario, &mut state, &mut internal_states)
            .unwrap();

        assert_eq!(
            events.lock().unwrap().as_slice(),
            [
                "simple-f64-source:simple",
                "simple-u64:simple",
                "simple-multi:simple",
                "simple-f64-dependent:simple",
                "simple-f64-source:simple",
                "simple-u64:simple",
                "simple-multi:simple",
                "simple-f64-dependent:simple",
            ]
        );
        let values = state.get_simple_parameter_values();
        assert_eq!(values.get_f64(f64_source).unwrap(), 11.0);
        assert_eq!(values.get_f64(f64_dependent).unwrap(), 12.0);
        assert_eq!(values.get_u64(u64_index).unwrap(), 12);
        assert_eq!(values.get_multi_f64(multi_index, "value").unwrap(), 13.0);
        assert_eq!(values.get_multi_u64(multi_index, "index").unwrap(), 14);
        assert_test_parameter_state(
            internal_states.get_simple_f64_state(f64_source).unwrap(),
            "simple-f64-source",
            timesteps.len(),
            0,
            2,
        );
        assert_test_parameter_state(
            internal_states.get_simple_f64_state(f64_dependent).unwrap(),
            "simple-f64-dependent",
            timesteps.len(),
            0,
            2,
        );
        assert_test_parameter_state(
            internal_states.get_simple_mut_u64_state(u64_index).unwrap(),
            "simple-u64",
            timesteps.len(),
            0,
            2,
        );
        assert_test_parameter_state(
            internal_states.get_simple_mut_multi_state(multi_index).unwrap(),
            "simple-multi",
            timesteps.len(),
            0,
            2,
        );
    }

    #[test]
    fn compute_const_and_simple_wrap_calculation_errors_with_name() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let domain = default_domain();
        let timesteps = domain.time().timesteps();
        let scenario = ScenarioIndex::default();

        let mut const_collection = ParameterCollection::default();
        const_collection.push_const_f64(Box::new(
            TestParameter::<f64>::new("broken-const", events.clone()).failing(TestParameterFailure::Const),
        ));
        const_collection.push_const_f64(Box::new(TestParameter::<f64>::new("not-run-const", events.clone())));
        let mut const_states = ParameterStates::from_collection(&const_collection, timesteps, &scenario).unwrap();
        let mut const_state = StateBuilder::new(Vec::new(), 0)
            .with_parameters(&const_collection)
            .build();
        match const_collection.compute_const(&scenario, &mut const_state, &mut const_states) {
            Err(ParameterCollectionConstCalculationError::CalculationError { name, source }) => {
                assert_eq!(name, ParameterName::from("broken-const"));
                assert!(matches!(source, ConstCalculationError::ConstantMetricF64Error(_)));
            }
            result => panic!("expected a constant calculation error, got {result:?}"),
        }

        let mut simple_collection = ParameterCollection::default();
        simple_collection.push_simple_f64(Box::new(
            TestParameter::<f64>::new("broken-simple", events.clone()).failing(TestParameterFailure::Simple),
        ));
        simple_collection.push_simple_f64(Box::new(TestParameter::<f64>::new("not-run-simple", events.clone())));
        let mut simple_states = ParameterStates::from_collection(&simple_collection, timesteps, &scenario).unwrap();
        let mut simple_state = StateBuilder::new(Vec::new(), 0)
            .with_parameters(&simple_collection)
            .build();
        match simple_collection.compute_simple(&timesteps[0], &scenario, &mut simple_state, &mut simple_states) {
            Err(ParameterCollectionSimpleCalculationError::CalculationError { name, source }) => {
                assert_eq!(name, ParameterName::from("broken-simple"));
                assert!(matches!(
                    source,
                    SimpleCalculationError::Internal { message } if message == "intentional simple failure"
                ));
            }
            result => panic!("expected a simple calculation error, got {result:?}"),
        }
        assert!(events.lock().unwrap().is_empty());
    }

    #[test]
    fn compute_rejects_mismatched_parameter_states_and_model_state() {
        let domain = default_domain();
        let timesteps = domain.time().timesteps();
        let scenario = ScenarioIndex::default();
        let timestep = &timesteps[0];

        let mut f64_collection = ParameterCollection::default();
        let f64_index = expect_simple_index(f64_collection.push_simple_f64(Box::new(TestParameter::<f64>::new(
            "simple-f64-mismatch",
            Arc::new(Mutex::new(Vec::new())),
        ))));
        let empty_collection = ParameterCollection::default();
        let mut empty_states = ParameterStates::from_collection(&empty_collection, timesteps, &scenario).unwrap();
        let mut f64_state = StateBuilder::new(Vec::new(), 0)
            .with_parameters(&f64_collection)
            .build();
        assert!(matches!(
            f64_collection.compute_simple(timestep, &scenario, &mut f64_state, &mut empty_states),
            Err(ParameterCollectionSimpleCalculationError::F64IndexNotFound(index)) if index == f64_index
        ));

        let mut u64_collection = ParameterCollection::default();
        let u64_index = expect_simple_index(u64_collection.push_simple_u64(Box::new(TestParameter::<u64>::new(
            "simple-u64-mismatch",
            Arc::new(Mutex::new(Vec::new())),
        ))));
        let mut empty_states = ParameterStates::from_collection(&empty_collection, timesteps, &scenario).unwrap();
        let mut u64_state = StateBuilder::new(Vec::new(), 0)
            .with_parameters(&u64_collection)
            .build();
        assert!(matches!(
            u64_collection.compute_simple(timestep, &scenario, &mut u64_state, &mut empty_states),
            Err(ParameterCollectionSimpleCalculationError::U64IndexNotFound(index)) if index == u64_index
        ));

        let mut multi_collection = ParameterCollection::default();
        let multi_index = expect_simple_index(multi_collection.push_simple_multi(Box::new(
            TestParameter::<MultiValue>::new("simple-multi-mismatch", Arc::new(Mutex::new(Vec::new()))),
        )));
        let mut empty_states = ParameterStates::from_collection(&empty_collection, timesteps, &scenario).unwrap();
        let mut multi_state = StateBuilder::new(Vec::new(), 0)
            .with_parameters(&multi_collection)
            .build();
        assert!(matches!(
            multi_collection.compute_simple(timestep, &scenario, &mut multi_state, &mut empty_states),
            Err(ParameterCollectionSimpleCalculationError::MultiIndexNotFound(index)) if index == multi_index
        ));

        let mut missing_internal = ParameterStates::from_collection(&f64_collection, timesteps, &scenario).unwrap();
        *missing_internal.get_simple_mut_f64_state(f64_index).unwrap() = None;
        let mut correctly_sized_state = StateBuilder::new(Vec::new(), 0)
            .with_parameters(&f64_collection)
            .build();
        assert!(matches!(
            f64_collection.compute_simple(timestep, &scenario, &mut correctly_sized_state, &mut missing_internal),
            Err(ParameterCollectionSimpleCalculationError::CalculationError { name, source })
                if name == ParameterName::from("simple-f64-mismatch")
                    && matches!(&source, SimpleCalculationError::Internal { message } if message == "missing or invalid probe state")
        ));

        let mut correct_internal = ParameterStates::from_collection(&f64_collection, timesteps, &scenario).unwrap();
        let mut empty_model_state = StateBuilder::new(Vec::new(), 0).build();
        assert!(matches!(
            f64_collection.compute_simple(timestep, &scenario, &mut empty_model_state, &mut correct_internal),
            Err(ParameterCollectionSimpleCalculationError::F64SetStateError { name, source })
                if name == ParameterName::from("simple-f64-mismatch")
                    && matches!(source, SetStateError::IndexNotFound(index) if index == f64_index)
        ));
    }

    #[test]
    fn general_schedule_preserves_registration_order_across_types() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut collection = ParameterCollection::default();
        let f64_before = expect_general_registration(collection.push_general_f64(GeneralParameterEntry::before(
            TestParameter::<f64>::new("f64-before-order", events.clone()),
        )));
        let u64_both = expect_general_registration(collection.push_general_u64(GeneralParameterEntry::both(
            TestParameter::<u64>::new("u64-both-order", events.clone()),
        )));
        let multi_after = expect_general_registration(collection.push_general_multi(GeneralParameterEntry::after(
            TestParameter::<MultiValue>::new("multi-after-order", events.clone()),
        )));
        let multi_before = expect_general_registration(collection.push_general_multi(GeneralParameterEntry::before(
            TestParameter::<MultiValue>::new("multi-before-order", events.clone()),
        )));
        let f64_both = expect_general_registration(collection.push_general_f64(GeneralParameterEntry::both(
            TestParameter::<f64>::new("f64-both-order", events.clone()),
        )));
        let u64_after = expect_general_registration(collection.push_general_u64(GeneralParameterEntry::after(
            TestParameter::<u64>::new("u64-after-order", events.clone()),
        )));

        let domain = default_domain();
        let timesteps = domain.time().timesteps();
        let scenario = ScenarioIndex::default();
        let network = Network::default();
        let mut internal_states = ParameterStates::from_collection(&collection, timesteps, &scenario).unwrap();
        let mut state = StateBuilder::new(Vec::new(), 0).with_parameters(&collection).build();
        collection
            .before_general(
                &timesteps[0],
                &scenario,
                &network,
                &mut state,
                &mut internal_states,
                None,
            )
            .unwrap();
        assert_eq!(
            events.lock().unwrap().as_slice(),
            [
                "f64-before-order:before",
                "u64-both-order:before",
                "multi-before-order:before",
                "f64-both-order:before"
            ]
        );
        assert_eq!(
            state
                .get_general_parameter_f64_before(f64_before.before.unwrap())
                .unwrap(),
            11.0
        );
        assert_eq!(
            state
                .get_general_parameter_u64_before(u64_both.before.unwrap())
                .unwrap(),
            12
        );
        assert_eq!(
            state
                .get_general_parameter_multi_before(multi_before.before.unwrap())
                .unwrap()
                .get_value("value"),
            Some(&13.0)
        );
        assert_eq!(
            state
                .get_general_parameter_f64_before(f64_both.before.unwrap())
                .unwrap(),
            11.0
        );

        collection
            .after_general(
                &timesteps[0],
                &scenario,
                &network,
                &mut state,
                &mut internal_states,
                None,
            )
            .unwrap();
        assert_eq!(
            events.lock().unwrap().as_slice(),
            [
                "f64-before-order:before",
                "u64-both-order:before",
                "multi-before-order:before",
                "f64-both-order:before",
                "u64-both-order:after",
                "multi-after-order:after",
                "f64-both-order:after",
                "u64-after-order:after",
            ]
        );
        assert_eq!(
            state.get_general_parameter_u64_after(u64_both.after.unwrap()).unwrap(),
            22
        );
        assert_eq!(
            state
                .get_general_parameter_multi_after(multi_after.after.unwrap())
                .unwrap()
                .get_index("index"),
            Some(&24)
        );
        assert_eq!(
            state.get_general_parameter_f64_after(f64_both.after.unwrap()).unwrap(),
            21.0
        );
        assert_eq!(
            state.get_general_parameter_u64_after(u64_after.after.unwrap()).unwrap(),
            22
        );
    }

    #[test]
    fn general_before_and_after_share_internal_state() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut collection = ParameterCollection::default();
        let registration = expect_general_registration(collection.push_general_f64(GeneralParameterEntry::both(
            TestParameter::<f64>::new("shared-general-state", events.clone()),
        )));
        let domain = default_domain();
        let timesteps = domain.time().timesteps();
        let scenario = ScenarioIndex::default();
        let network = Network::default();
        let mut internal_states = ParameterStates::from_collection(&collection, timesteps, &scenario).unwrap();
        let mut state = StateBuilder::new(Vec::new(), 0).with_parameters(&collection).build();

        collection
            .before_general(
                &timesteps[0],
                &scenario,
                &network,
                &mut state,
                &mut internal_states,
                None,
            )
            .unwrap();
        collection
            .after_general(
                &timesteps[0],
                &scenario,
                &network,
                &mut state,
                &mut internal_states,
                None,
            )
            .unwrap();

        assert_eq!(
            events.lock().unwrap().as_slice(),
            ["shared-general-state:before", "shared-general-state:after"]
        );
        assert_eq!(
            state
                .get_general_parameter_f64_before(registration.before.unwrap())
                .unwrap(),
            11.0
        );
        assert_eq!(
            state
                .get_general_parameter_f64_after(registration.after.unwrap())
                .unwrap(),
            21.0
        );
        assert_test_parameter_state(
            internal_states.get_general_f64_state(registration.parameter).unwrap(),
            "shared-general-state",
            timesteps.len(),
            0,
            2,
        );
    }

    #[test]
    fn general_calculation_errors_identify_before_after_and_hook_parameters() {
        let domain = default_domain();
        let timesteps = domain.time().timesteps();
        let scenario = ScenarioIndex::default();
        let network = Network::default();

        let mut before_collection = ParameterCollection::default();
        before_collection.push_general_f64(GeneralParameterEntry::before(
            TestParameter::<f64>::new("broken-before", Arc::new(Mutex::new(Vec::new())))
                .failing(TestParameterFailure::GeneralBefore),
        ));
        let mut before_states = ParameterStates::from_collection(&before_collection, timesteps, &scenario).unwrap();
        let mut before_state = StateBuilder::new(Vec::new(), 0)
            .with_parameters(&before_collection)
            .build();
        let error = before_collection
            .before_general(
                &timesteps[0],
                &scenario,
                &network,
                &mut before_state,
                &mut before_states,
                None,
            )
            .unwrap_err();
        assert_general_calculation_error(error, "broken-before", "intentional general before failure");

        let mut after_collection = ParameterCollection::default();
        after_collection.push_general_f64(GeneralParameterEntry::after(
            TestParameter::<f64>::new("broken-after", Arc::new(Mutex::new(Vec::new())))
                .failing(TestParameterFailure::GeneralAfter),
        ));
        let mut after_states = ParameterStates::from_collection(&after_collection, timesteps, &scenario).unwrap();
        let mut after_state = StateBuilder::new(Vec::new(), 0)
            .with_parameters(&after_collection)
            .build();
        let error = after_collection
            .after_general(
                &timesteps[0],
                &scenario,
                &network,
                &mut after_state,
                &mut after_states,
                None,
            )
            .unwrap_err();
        assert_general_calculation_error(error, "broken-after", "intentional general after failure");

        let mut hook_collection = ParameterCollection::default();
        let hook_registration = expect_general_registration(
            hook_collection.push_general_f64(GeneralParameterEntry::before_with_after_hook(
                TestParameter::<f64>::new("broken-hook", Arc::new(Mutex::new(Vec::new())))
                    .failing(TestParameterFailure::GeneralHook),
            )),
        );
        assert!(hook_registration.after.is_none());
        let mut hook_states = ParameterStates::from_collection(&hook_collection, timesteps, &scenario).unwrap();
        let mut hook_state = StateBuilder::new(Vec::new(), 0)
            .with_parameters(&hook_collection)
            .build();
        hook_collection
            .before_general(
                &timesteps[0],
                &scenario,
                &network,
                &mut hook_state,
                &mut hook_states,
                None,
            )
            .unwrap();
        let error = hook_collection
            .after_general(
                &timesteps[0],
                &scenario,
                &network,
                &mut hook_state,
                &mut hook_states,
                None,
            )
            .unwrap_err();
        assert_general_calculation_error(error, "broken-hook", "intentional general hook failure");
    }

    #[test]
    fn timings_reject_another_parameter_collection() {
        let mut source_collection = ParameterCollection::default();
        source_collection.push_general_f64(GeneralParameterEntry::both(TestParameter::<f64>::new(
            "timing-source",
            Arc::new(Mutex::new(Vec::new())),
        )));
        let mut other_collection = ParameterCollection::default();
        other_collection.push_general_f64(GeneralParameterEntry::both(TestParameter::<f64>::new(
            "timing-other",
            Arc::new(Mutex::new(Vec::new())),
        )));
        let mut timings = ParameterTimings::from_collection(&source_collection);
        let domain = default_domain();
        let timesteps = domain.time().timesteps();
        let scenario = ScenarioIndex::default();
        let network = Network::default();
        let mut internal_states = ParameterStates::from_collection(&other_collection, timesteps, &scenario).unwrap();
        let mut state = StateBuilder::new(Vec::new(), 0)
            .with_parameters(&other_collection)
            .build();

        assert!(matches!(
            other_collection.before_general(
                &timesteps[0],
                &scenario,
                &network,
                &mut state,
                &mut internal_states,
                Some(&mut timings),
            ),
            Err(ParameterCollectionGeneralCalculationError::TimingsFromAnotherCollection)
        ));
        assert!(matches!(
            other_collection.after_general(
                &timesteps[0],
                &scenario,
                &network,
                &mut state,
                &mut internal_states,
                Some(&mut timings),
            ),
            Err(ParameterCollectionGeneralCalculationError::TimingsFromAnotherCollection)
        ));

        let error = match timings.slowest_parameters_named(1, &other_collection) {
            Err(error) => error,
            Ok(_) => panic!("timings from another collection should be rejected"),
        };
        assert_eq!(error.expected, other_collection.id);
        assert_eq!(error.actual, source_collection.id);
        assert!(error.context.contains("same ID"));
    }

    #[test]
    fn simple_parameters_run() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut collection = ParameterCollection::default();

        let before_index = expect_simple_index(
            collection.push_simple_f64(Box::new(TestParameter::<f64>::phase("simple-before", events.clone()))),
        );

        let domain = default_domain();
        let timesteps = domain.time().timesteps();
        let timestep = &timesteps[0];
        let scenario_index = ScenarioIndex::default();

        let mut state = StateBuilder::new(Vec::new(), 0).with_parameters(&collection).build();

        let mut internal_states = ParameterStates::from_collection(&collection, timesteps, &scenario_index).unwrap();

        collection
            .compute_simple(timestep, &scenario_index, &mut state, &mut internal_states)
            .unwrap();

        // Only entries with a before implementation should have run.
        assert_eq!(events.lock().unwrap().as_slice(), ["simple-before:before",]);

        let values = state.get_simple_parameter_values();

        assert_eq!(values.get_f64(before_index).unwrap(), 11.0);

        assert_eq!(events.lock().unwrap().as_slice(), ["simple-before:before",]);

        // SimpleParameterValues exposes before-phase values. These should not be
        // changed by after-phase calculations.
        let values = state.get_simple_parameter_values();

        assert_eq!(values.get_f64(before_index).unwrap(), 11.0);
    }

    #[test]
    fn general_parameters_run_their_configured_phases() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut collection = ParameterCollection::default();

        let before_reg = expect_general_registration(collection.push_general_f64(GeneralParameterEntry::before(
            TestParameter::<f64>::phase("general-before", events.clone()),
        )));

        let after_reg = expect_general_registration(collection.push_general_f64(GeneralParameterEntry::after(
            TestParameter::<f64>::phase("general-after", events.clone()),
        )));

        let both_index = expect_general_registration(collection.push_general_f64(GeneralParameterEntry::both(
            TestParameter::<f64>::phase("general-both", events.clone()),
        )));

        let hook_reg = expect_general_registration(collection.push_general_f64(
            GeneralParameterEntry::before_with_after_hook(TestParameter::<f64>::phase("general-hook", events.clone())),
        ));

        let domain = default_domain();
        let timesteps = domain.time().timesteps();
        let timestep = &timesteps[0];
        let scenario_index = ScenarioIndex::default();
        let network = Network::default();

        let mut state = StateBuilder::new(Vec::new(), 0).with_parameters(&collection).build();

        let mut internal_states = ParameterStates::from_collection(&collection, timesteps, &scenario_index).unwrap();

        collection
            .before_general(
                timestep,
                &scenario_index,
                &network,
                &mut state,
                &mut internal_states,
                None,
            )
            .unwrap();

        assert_eq!(
            events.lock().unwrap().as_slice(),
            ["general-before:before", "general-both:before", "general-hook:before",]
        );

        assert_eq!(
            state
                .get_general_parameter_f64_before(before_reg.before.unwrap())
                .unwrap(),
            11.0
        );

        assert_eq!(
            state
                .get_general_parameter_f64_before(both_index.before.unwrap())
                .unwrap(),
            11.0
        );
        assert_eq!(
            state
                .get_general_parameter_f64_before(hook_reg.before.unwrap())
                .unwrap(),
            11.0
        );
        // After-only entries should not be registered for the before phase.
        assert!(after_reg.before.is_none());

        collection
            .after_general(
                timestep,
                &scenario_index,
                &network,
                &mut state,
                &mut internal_states,
                None,
            )
            .unwrap();

        assert_eq!(
            events.lock().unwrap().as_slice(),
            [
                "general-before:before",
                "general-both:before",
                "general-hook:before",
                "general-after:after",
                "general-both:after",
                "general-hook:hook",
            ]
        );

        // Value-producing after operations write their result.
        assert_eq!(
            state.get_general_parameter_f64_after(after_reg.after.unwrap()).unwrap(),
            22.0
        );
        assert_eq!(
            state
                .get_general_parameter_f64_after(both_index.after.unwrap())
                .unwrap(),
            22.0
        );

        // Before-only entries and after hooks should not be registered for the after phase.
        assert!(before_reg.after.is_none());
        assert!(hook_reg.after.is_none());
    }
}
