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
mod threshold;
mod vector;

use std::any::Any;
// Re-imports
use crate::metric::{
    ConstantMetricF64, ConstantMetricU64, MetricF64, MetricF64Error, MetricF64ResolutionError, MetricU64,
    MetricU64ResolutionError, SimpleMetricF64, SimpleMetricU64,
};
use crate::network::{Network, ResolutionMaps};
use crate::scenario::{ScenarioGroupNotFound, ScenarioIndex};
use crate::state::{
    ConstParameterValues, MultiValue, ParameterReturnValue, SetStateError, SimpleParameterValues, State,
};
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
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use thiserror::Error;
pub use threshold::{Predicate, ThresholdParameter, ThresholdParameterBuilder};
pub use vector::{VectorParameter, VectorParameterBuilder};

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

impl ParameterIndex<f64> {
    /// Convert the parameter index into a metric.
    pub fn into_metric_f64(self, return_value: ParameterReturnValue) -> MetricF64 {
        match self {
            ParameterIndex::Const(idx) => ConstantMetricF64::ParameterValue(idx).into(),
            ParameterIndex::Simple(idx) => SimpleMetricF64::ParameterValue(idx).into(),
            ParameterIndex::General(index) => MetricF64::ParameterValue { index, return_value },
        }
    }

    /// Convert the parameter index into a metric that returns the "before" value.
    ///
    /// This is a convenience method for `into_metric_f64(ParameterReturnValue::Before)`.
    pub fn into_metric_f64_before(self) -> MetricF64 {
        self.into_metric_f64(ParameterReturnValue::Before)
    }

    /// Convert the parameter index into a metric that returns the "after" value.
    ///
    /// This is a convenience method for `into_metric_f64(ParameterReturnValue::After)`.
    pub fn into_metric_f64_after(self) -> MetricF64 {
        self.into_metric_f64(ParameterReturnValue::After)
    }
}

impl ParameterIndex<u64> {
    /// Convert the parameter index into a metric.
    pub fn into_metric_f64(self, return_value: ParameterReturnValue) -> MetricF64 {
        match self {
            ParameterIndex::Const(idx) => ConstantMetricF64::IndexParameterValue(idx).into(),
            ParameterIndex::Simple(idx) => SimpleMetricF64::IndexParameterValue(idx).into(),
            ParameterIndex::General(index) => MetricF64::IndexParameterValue { index, return_value },
        }
    }

    /// Convert the parameter index into a metric that returns the "before" value.
    ///
    /// This is a convenience method for `into_metric(ParameterReturnValue::Before)`.
    pub fn into_metric_f64_before(self) -> MetricF64 {
        self.into_metric_f64(ParameterReturnValue::Before)
    }

    /// Convert the parameter index into a metric.
    pub fn into_metric_u64(self, return_value: ParameterReturnValue) -> MetricU64 {
        match self {
            ParameterIndex::Const(idx) => ConstantMetricU64::IndexParameterValue(idx).into(),
            ParameterIndex::Simple(idx) => SimpleMetricU64::IndexParameterValue(idx).into(),
            ParameterIndex::General(index) => MetricU64::IndexParameterValue { index, return_value },
        }
    }

    /// Convert the parameter index into a metric that returns the "before" value.
    ///
    /// This is a convenience method for `into_metric(ParameterReturnValue::Before)`.
    pub fn into_metric_u64_before(self) -> MetricU64 {
        self.into_metric_u64(ParameterReturnValue::Before)
    }
}

impl ParameterIndex<MultiValue> {
    /// Convert the parameter index into a metric.
    pub fn into_metric_f64(self, key: &str, return_value: ParameterReturnValue) -> MetricF64 {
        let key = key.to_string();
        match self {
            ParameterIndex::Const(index) => ConstantMetricF64::MultiParameterValue { index, key }.into(),
            ParameterIndex::Simple(index) => SimpleMetricF64::MultiParameterValue { index, key }.into(),
            ParameterIndex::General(index) => MetricF64::MultiParameterValue {
                index,
                key,
                return_value,
            },
        }
    }

    /// Convert the parameter index into a metric that returns the "before" value.
    ///
    /// This is a convenience method for `into_metric_f64(key, ParameterReturnValue::Before)`.
    pub fn into_metric_f64_before(self, key: &str) -> MetricF64 {
        self.into_metric_f64(key, ParameterReturnValue::Before)
    }

    /// Convert the parameter index into a metric.
    pub fn into_metric_u64(self, key: &str, return_value: ParameterReturnValue) -> MetricU64 {
        let key = key.to_string();
        match self {
            ParameterIndex::Const(index) => ConstantMetricU64::MultiParameterValue { index, key }.into(),
            ParameterIndex::Simple(index) => SimpleMetricU64::MultiParameterValue { index, key }.into(),
            ParameterIndex::General(index) => MetricU64::MultiParameterValue {
                index,
                key,
                return_value,
            },
        }
    }

    /// Convert the parameter index into a metric that returns the "before" value.
    ///
    /// This is a convenience method for `into_metric_u64(key, ParameterReturnValue::Before)`.
    pub fn into_metric_u64_before(self, key: &str) -> MetricU64 {
        self.into_metric_u64(key, ParameterReturnValue::Before)
    }
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

/// A trait that defines a component that produces a value each time-step.
///
/// The trait is generic over the type of the value produced.
pub trait GeneralParameter<T>: Parameter {
    fn before(
        &self,
        #[allow(unused_variables)] timestep: &Timestep,
        #[allow(unused_variables)] scenario_index: &ScenarioIndex,
        #[allow(unused_variables)] model: &Network,
        #[allow(unused_variables)] state: &State,
        #[allow(unused_variables)] internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<T>, GeneralCalculationError> {
        Ok(None)
    }

    fn after(
        &self,
        #[allow(unused_variables)] timestep: &Timestep,
        #[allow(unused_variables)] scenario_index: &ScenarioIndex,
        #[allow(unused_variables)] model: &Network,
        #[allow(unused_variables)] state: &State,
        #[allow(unused_variables)] internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<T>, GeneralCalculationError> {
        Ok(None)
    }

    fn try_into_simple(&self) -> Option<Box<dyn SimpleParameter<T>>> {
        None
    }

    fn as_parameter(&self) -> &dyn Parameter;
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
}

pub enum BuiltParameter<T> {
    General(Box<dyn GeneralParameter<T>>),
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
/// let metric = resolve_metric_f64!(self, self.metric, resolution_maps, "metric");
/// ```
#[macro_export]
macro_rules! resolve_metric_f64 {
    ($self:ident, $unresolved:expr, $maps:expr, $attr:expr $(,)?) => {
        match $unresolved.resolve($maps) {
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
/// let metric = resolve_metric_u64!(self, self.metric, resolution_maps, "metric");
/// ```
#[macro_export]
macro_rules! resolve_metric_u64 {
    ($self:ident, $unresolved:expr, $maps:expr, $attr:expr $(,)?) => {
        match $unresolved.resolve($maps) {
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
    ($self:ident, $unresolved:expr, $maps:expr, $attr:expr $(,)?) => {
        match $unresolved {
            Some(u) => match u.resolve($maps) {
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
///     resolve_metric_f64_vec!(self, &self.control_curves, resolution_maps, "control_curves");
/// ```
#[macro_export]
macro_rules! resolve_metric_f64_vec {
    ($self:ident, $unresolved:expr, $maps:expr, $attr:expr $(,)?) => {{
        let unresolved = $unresolved;
        let mut resolved = Vec::with_capacity(unresolved.len());
        for m in unresolved.iter() {
            match m.resolve($maps) {
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
    ($self:ident, $unresolved:expr, $maps:expr, $attr:expr $(,)?) => {{
        let unresolved = $unresolved;
        let mut resolved = Vec::with_capacity(unresolved.len());
        for m in unresolved.iter() {
            match m.resolve($maps) {
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
    ($self:ident, $unresolved:expr, $maps:expr, $attr:expr $(,)?) => {{
        let unresolved = $unresolved;
        let mut resolved = HashMap::with_capacity(unresolved.len());
        for (k, m) in unresolved.iter() {
            match m.resolve($maps) {
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
    ($self:ident, $unresolved:expr, $maps:expr, $attr:expr $(,)?) => {{
        let unresolved = $unresolved;
        let mut resolved = HashMap::with_capacity(unresolved.len());
        for (k, m) in unresolved.iter() {
            match m.resolve($maps) {
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

/// A trait that defines a component that produces a value each time-step.
///
/// The trait is generic over the type of the value produced.
pub trait SimpleParameter<T>: Parameter {
    fn before(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        values: &SimpleParameterValues,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<T>, SimpleCalculationError>;

    fn after(
        &self,
        #[allow(unused_variables)] timestep: &Timestep,
        #[allow(unused_variables)] scenario_index: &ScenarioIndex,
        #[allow(unused_variables)] values: &SimpleParameterValues,
        #[allow(unused_variables)] internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<T>, SimpleCalculationError> {
        Ok(None)
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

// Unique ID for each parameter collection.
static PARAMETER_COLLECTION_ID: AtomicU64 = AtomicU64::new(0);

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
    ) -> Vec<(ParameterName, ParameterTiming)> {
        if self.id != collection.id {
            todo!("Return error with ID mismatch");
        }

        // SAFETY: The id of the timings must match the id of the collection to ensure that the
        // indices are correct. This is checked above and should be guaranteed by construction.
        unsafe {
            self.slowest_parameters(n)
                .into_iter()
                .map(|(idx, timing)| (collection.get_general_unchecked(idx).name().clone(), timing))
                .collect()
        }
    }
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
    #[error("Error setting state for general F64 parameter '{name}': {source}")]
    F64SetStateError {
        name: ParameterName,
        #[source]
        source: SetStateError<GeneralParameterIndex<f64>>,
    },
    #[error("Error setting state for general U64 parameter '{name}': {source}")]
    U64SetStateError {
        name: ParameterName,
        #[source]
        source: SetStateError<GeneralParameterIndex<u64>>,
    },
    #[error("Error setting state for general Multi parameter '{name}': {source}")]
    MultiSetStateError {
        name: ParameterName,
        #[source]
        source: SetStateError<GeneralParameterIndex<MultiValue>>,
    },
    #[error("The timing data was created with from a different parameter collection. ")]
    TimingsFromAnotherCollection,
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

    general_f64: Vec<Box<dyn GeneralParameter<f64>>>,
    general_u64: Vec<Box<dyn GeneralParameter<u64>>>,
    general_multi: Vec<Box<dyn GeneralParameter<MultiValue>>>,
    general_resolve_order: Vec<GeneralParameterType>,
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
            general_resolve_order: Vec::new(),
            id: PARAMETER_COLLECTION_ID.fetch_add(1, Ordering::Relaxed),
        }
    }
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
    fn push_general_f64(&mut self, p: Box<dyn GeneralParameter<f64>>) -> ParameterIndex<f64> {
        match p.try_into_simple() {
            Some(p) => self.push_simple_f64(p),
            None => {
                let index = GeneralParameterIndex::new(self.general_f64.len());

                self.general_resolve_order.push(GeneralParameterType::Parameter(index));
                self.general_f64.push(p);

                ParameterIndex::General(index)
            }
        }
    }

    /// Push a new simple parameter to the collection.
    ///
    /// The new parameter will be simplified as much as possible.
    ///
    /// SAFETY: This must remain a private function to maintain the indexing guarantees.
    fn push_simple_f64(&mut self, p: Box<dyn SimpleParameter<f64>>) -> ParameterIndex<f64> {
        match p.try_into_const() {
            Some(p) => self.push_const_f64(p),
            None => {
                let index = SimpleParameterIndex::new(self.simple_f64.len());

                self.simple_resolve_order.push(SimpleParameterType::Parameter(index));
                self.simple_f64.push(p);

                ParameterIndex::Simple(index)
            }
        }
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

    /// Push a new general parameter to the collection.
    ///
    /// The new parameter will be simplified as much as possible.
    ///
    /// SAFETY: This must remain a private function to maintain the indexing guarantees.
    fn push_general_u64(&mut self, p: Box<dyn GeneralParameter<u64>>) -> ParameterIndex<u64> {
        match p.try_into_simple() {
            Some(p) => self.push_simple_u64(p),
            None => {
                let index = GeneralParameterIndex::new(self.general_u64.len());

                self.general_resolve_order.push(GeneralParameterType::Index(index));
                self.general_u64.push(p);

                ParameterIndex::General(index)
            }
        }
    }

    /// Push a new simple parameter to the collection.
    ///
    /// The new parameter will be simplified as much as possible.
    ///
    /// SAFETY: This must remain a private function to maintain the indexing guarantees.
    fn push_simple_u64(&mut self, p: Box<dyn SimpleParameter<u64>>) -> ParameterIndex<u64> {
        match p.try_into_const() {
            Some(p) => self.push_const_u64(p),
            None => {
                let index = SimpleParameterIndex::new(self.simple_u64.len());

                self.simple_resolve_order.push(SimpleParameterType::Index(index));
                self.simple_u64.push(p);

                ParameterIndex::Simple(index)
            }
        }
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

    /// Push a new general parameter to the collection.
    ///
    /// The new parameter will be simplified as much as possible.
    ///
    /// SAFETY: This must remain a private function to maintain the indexing guarantees.
    fn push_general_multi(&mut self, p: Box<dyn GeneralParameter<MultiValue>>) -> ParameterIndex<MultiValue> {
        match p.try_into_simple() {
            Some(p) => self.push_simple_multi(p),
            None => {
                let index = GeneralParameterIndex::new(self.general_multi.len());

                self.general_resolve_order.push(GeneralParameterType::Multi(index));
                self.general_multi.push(p);

                ParameterIndex::General(index)
            }
        }
    }

    /// Push a new simple parameter to the collection.
    ///
    /// The new parameter will be simplified as much as possible.
    ///
    /// SAFETY: This must remain a private function to maintain the indexing guarantees.
    fn push_simple_multi(&mut self, p: Box<dyn SimpleParameter<MultiValue>>) -> ParameterIndex<MultiValue> {
        match p.try_into_const() {
            Some(p) => self.push_const_multi(p),
            None => {
                let index = SimpleParameterIndex::new(self.simple_multi.len());

                self.simple_resolve_order.push(SimpleParameterType::Multi(index));
                self.simple_multi.push(p);

                ParameterIndex::Simple(index)
            }
        }
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

    pub fn compute_general(
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

        for p in &self.general_resolve_order {
            let start = Instant::now();
            match p {
                GeneralParameterType::Parameter(idx) => {
                    // Find the parameter itself
                    let p = self
                        .general_f64
                        .get(*idx.deref())
                        .ok_or(ParameterCollectionGeneralCalculationError::F64IndexNotFound(*idx))?;
                    // .. and its internal state
                    let internal_state = internal_states
                        .get_general_mut_f64_state(*idx)
                        .ok_or(ParameterCollectionGeneralCalculationError::F64IndexNotFound(*idx))?;

                    let value = p
                        .before(timestep, scenario_index, network, state, internal_state)
                        .map_err(|source| ParameterCollectionGeneralCalculationError::CalculationError {
                            name: p.name().clone(),
                            source: Box::new(source),
                        })?;

                    state
                        .set_general_parameter_value_before(*idx, value)
                        .map_err(|source| ParameterCollectionGeneralCalculationError::F64SetStateError {
                            name: p.name().clone(),
                            source,
                        })?;

                    if let Some(timings) = timings.as_deref_mut() {
                        unsafe {
                            timings.general_f64.get_unchecked_mut(*idx.deref()).before += start.elapsed();
                        }
                    }
                }
                GeneralParameterType::Index(idx) => {
                    // Find the parameter itself
                    let p = self
                        .general_u64
                        .get(*idx.deref())
                        .ok_or(ParameterCollectionGeneralCalculationError::U64IndexNotFound(*idx))?;
                    // ... and its internal state
                    let internal_state = internal_states
                        .get_general_mut_u64_state(*idx)
                        .ok_or(ParameterCollectionGeneralCalculationError::U64IndexNotFound(*idx))?;

                    let value = p
                        .before(timestep, scenario_index, network, state, internal_state)
                        .map_err(|source| ParameterCollectionGeneralCalculationError::CalculationError {
                            name: p.name().clone(),
                            source: Box::new(source),
                        })?;

                    state
                        .set_general_parameter_index_before(*idx, value)
                        .map_err(|source| ParameterCollectionGeneralCalculationError::U64SetStateError {
                            name: p.name().clone(),
                            source,
                        })?;

                    if let Some(timings) = timings.as_deref_mut() {
                        unsafe {
                            timings.general_u64.get_unchecked_mut(*idx.deref()).before += start.elapsed();
                        }
                    }
                }
                GeneralParameterType::Multi(idx) => {
                    // Find the parameter itself
                    let p = self
                        .general_multi
                        .get(*idx.deref())
                        .ok_or(ParameterCollectionGeneralCalculationError::MultiIndexNotFound(*idx))?;
                    // ... and its internal state
                    let internal_state = internal_states
                        .get_general_mut_multi_state(*idx)
                        .ok_or(ParameterCollectionGeneralCalculationError::MultiIndexNotFound(*idx))?;

                    let value = p
                        .before(timestep, scenario_index, network, state, internal_state)
                        .map_err(|source| ParameterCollectionGeneralCalculationError::CalculationError {
                            name: p.name().clone(),
                            source: Box::new(source),
                        })?;

                    state
                        .set_general_multi_parameter_value_before(*idx, value)
                        .map_err(
                            |source| ParameterCollectionGeneralCalculationError::MultiSetStateError {
                                name: p.name().clone(),
                                source,
                            },
                        )?;

                    if let Some(timings) = timings.as_deref_mut() {
                        unsafe {
                            timings.general_multi.get_unchecked_mut(*idx.deref()).before += start.elapsed();
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

        for p in &self.general_resolve_order {
            let start = Instant::now();
            match p {
                GeneralParameterType::Parameter(idx) => {
                    // Find the parameter itself
                    let p = self
                        .general_f64
                        .get(*idx.deref())
                        .ok_or(ParameterCollectionGeneralCalculationError::F64IndexNotFound(*idx))?;
                    // .. and its internal state
                    let internal_state = internal_states
                        .get_general_mut_f64_state(*idx)
                        .ok_or(ParameterCollectionGeneralCalculationError::F64IndexNotFound(*idx))?;

                    let value = p
                        .after(timestep, scenario_index, network, state, internal_state)
                        .map_err(|source| ParameterCollectionGeneralCalculationError::CalculationError {
                            name: p.name().clone(),
                            source: Box::new(source),
                        })?;

                    state.set_general_parameter_value_after(*idx, value).map_err(|source| {
                        ParameterCollectionGeneralCalculationError::F64SetStateError {
                            name: p.name().clone(),
                            source,
                        }
                    })?;

                    if let Some(timings) = timings.as_deref_mut() {
                        unsafe {
                            timings.general_f64.get_unchecked_mut(*idx.deref()).after += start.elapsed();
                        }
                    }
                }
                GeneralParameterType::Index(idx) => {
                    // Find the parameter itself
                    let p = self
                        .general_u64
                        .get(*idx.deref())
                        .ok_or(ParameterCollectionGeneralCalculationError::U64IndexNotFound(*idx))?;
                    // .. and its internal state
                    let internal_state = internal_states
                        .get_general_mut_u64_state(*idx)
                        .ok_or(ParameterCollectionGeneralCalculationError::U64IndexNotFound(*idx))?;

                    let value = p
                        .after(timestep, scenario_index, network, state, internal_state)
                        .map_err(|source| ParameterCollectionGeneralCalculationError::CalculationError {
                            name: p.name().clone(),
                            source: Box::new(source),
                        })?;

                    state.set_general_parameter_index_after(*idx, value).map_err(|source| {
                        ParameterCollectionGeneralCalculationError::U64SetStateError {
                            name: p.name().clone(),
                            source,
                        }
                    })?;

                    if let Some(timings) = timings.as_deref_mut() {
                        unsafe {
                            timings.general_u64.get_unchecked_mut(*idx.deref()).after += start.elapsed();
                        }
                    }
                }
                GeneralParameterType::Multi(idx) => {
                    // Find the parameter itself
                    let p = self
                        .general_multi
                        .get(*idx.deref())
                        .ok_or(ParameterCollectionGeneralCalculationError::MultiIndexNotFound(*idx))?;
                    // .. and its internal state
                    let internal_state = internal_states
                        .get_general_mut_multi_state(*idx)
                        .ok_or(ParameterCollectionGeneralCalculationError::MultiIndexNotFound(*idx))?;

                    let value = p
                        .after(timestep, scenario_index, network, state, internal_state)
                        .map_err(|source| ParameterCollectionGeneralCalculationError::CalculationError {
                            name: p.name().clone(),
                            source: Box::new(source),
                        })?;

                    state
                        .set_general_multi_parameter_value_after(*idx, value)
                        .map_err(
                            |source| ParameterCollectionGeneralCalculationError::MultiSetStateError {
                                name: p.name().clone(),
                                source,
                            },
                        )?;

                    if let Some(timings) = timings.as_deref_mut() {
                        unsafe {
                            timings.general_multi.get_unchecked_mut(*idx.deref()).after += start.elapsed();
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

                    let value = p
                        .before(
                            timestep,
                            scenario_index,
                            &state.get_simple_parameter_values(),
                            internal_state,
                        )
                        .map_err(|source| ParameterCollectionSimpleCalculationError::CalculationError {
                            name: p.name().clone(),
                            source,
                        })?;

                    state.set_simple_parameter_value_before(*idx, value).map_err(|source| {
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
                        .before(
                            timestep,
                            scenario_index,
                            &state.get_simple_parameter_values(),
                            internal_state,
                        )
                        .map_err(|source| ParameterCollectionSimpleCalculationError::CalculationError {
                            name: p.name().clone(),
                            source,
                        })?;

                    state.set_simple_parameter_index_before(*idx, value).map_err(|source| {
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
                        .before(
                            timestep,
                            scenario_index,
                            &state.get_simple_parameter_values(),
                            internal_state,
                        )
                        .map_err(|source| ParameterCollectionSimpleCalculationError::CalculationError {
                            name: p.name().clone(),
                            source,
                        })?;

                    state
                        .set_simple_multi_parameter_value_before(*idx, value)
                        .map_err(|source| ParameterCollectionSimpleCalculationError::MultiSetStateError {
                            name: p.name().clone(),
                            source,
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
    use super::{
        BuiltParameter, ConstParameter, GeneralCalculationError, GeneralParameter, MaybeBuiltParameter, Parameter,
        ParameterBuildError, ParameterBuilder, ParameterCollectionBuilder, ParameterMeta, ParameterName,
        ParameterState, SimpleParameter,
    };
    use crate::network::ResolutionMaps;
    use crate::parameters::errors::{ConstCalculationError, SimpleCalculationError};
    use crate::scenario::ScenarioIndex;
    use crate::state::{ConstParameterValues, MultiValue};
    use crate::test_utils::default_domain;

    #[derive(Debug)]
    struct TestParameterBuilder {
        meta: ParameterMeta,
    }

    impl Default for TestParameterBuilder {
        fn default() -> Self {
            Self {
                meta: ParameterMeta::new("test-parameter".into()),
            }
        }
    }

    impl ParameterBuilder<f64> for TestParameterBuilder {
        fn name(&self) -> &ParameterName {
            &self.meta.name
        }

        fn build(
            self: Box<Self>,
            _resolution_maps: &ResolutionMaps,
        ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
            let p = TestParameter { meta: self.meta };
            Ok(MaybeBuiltParameter::Built(BuiltParameter::Const(Box::new(p))))
        }
    }

    impl ParameterBuilder<u64> for TestParameterBuilder {
        fn name(&self) -> &ParameterName {
            &self.meta.name
        }

        fn build(
            self: Box<Self>,
            _resolution_maps: &ResolutionMaps,
        ) -> Result<MaybeBuiltParameter<u64>, ParameterBuildError> {
            let p = TestParameter { meta: self.meta };
            Ok(MaybeBuiltParameter::Built(BuiltParameter::Const(Box::new(p))))
        }
    }

    /// Parameter for testing purposes
    #[derive(Debug)]
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
        fn before(
            &self,
            _timestep: &crate::timestep::Timestep,
            _scenario_index: &ScenarioIndex,
            _values: &crate::state::SimpleParameterValues,
            _internal_state: &mut Option<Box<dyn ParameterState>>,
        ) -> Result<Option<T>, SimpleCalculationError> {
            Ok(Some(T::from(1)))
        }

        fn as_parameter(&self) -> &dyn Parameter {
            self
        }
    }

    impl SimpleParameter<MultiValue> for TestParameter {
        fn before(
            &self,
            _timestep: &crate::timestep::Timestep,
            _scenario_index: &ScenarioIndex,
            _values: &crate::state::SimpleParameterValues,
            _internal_state: &mut Option<Box<dyn ParameterState>>,
        ) -> Result<Option<MultiValue>, SimpleCalculationError> {
            Ok(Some(MultiValue::default()))
        }

        fn as_parameter(&self) -> &dyn Parameter {
            self
        }
    }
    impl<T> GeneralParameter<T> for TestParameter
    where
        T: From<u8>,
    {
        fn before(
            &self,
            _timestep: &crate::timestep::Timestep,
            _scenario_index: &ScenarioIndex,
            _model: &crate::network::Network,
            _state: &crate::state::State,
            _internal_state: &mut Option<Box<dyn ParameterState>>,
        ) -> Result<Option<T>, GeneralCalculationError> {
            Ok(Some(T::from(1)))
        }

        fn as_parameter(&self) -> &dyn Parameter {
            self
        }
    }

    impl GeneralParameter<MultiValue> for TestParameter {
        fn before(
            &self,
            _timestep: &crate::timestep::Timestep,
            _scenario_index: &ScenarioIndex,
            _model: &crate::network::Network,
            _state: &crate::state::State,
            _internal_state: &mut Option<Box<dyn ParameterState>>,
        ) -> Result<Option<MultiValue>, GeneralCalculationError> {
            Ok(Some(MultiValue::default()))
        }

        fn as_parameter(&self) -> &dyn Parameter {
            self
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
}
