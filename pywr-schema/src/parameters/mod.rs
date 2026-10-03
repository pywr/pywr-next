//! Parameter schema definitions.
//!
//! The enum [`Parameter`] contains all of the valid Pywr parameter schemas. The parameter
//! variants define separate schemas for different parameter types. When a network is generated
//! from a schema the parameter schemas are added to the network using [`Parameter::add_to_network`].
//! This typically adds a struct from [`crate::parameters`] to the network using the data
//! defined in the schema.
//!
//! Serializing and deserializing is accomplished using [`serde`].
mod aggregated;
mod asymmetric_switch;
mod control_curves;
mod core;
mod delay;
mod discount_factor;
mod hydropower;
mod indexed_array;
mod interpolated;

mod difference;
mod offset;
mod placeholder;
mod polynomial;
mod profiles;
mod python;
mod rolling;
mod tables;
mod thresholds;

#[cfg(feature = "core")]
pub use super::data_tables::LoadedTableCollection;
pub use super::data_tables::TableDataRef;
use crate::data_tables::DataTableValueType;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::error::{ComponentConversionError, ConversionError};
use crate::meta::NamedMeta;
use crate::metric::{Metric, MetricValueType, ParameterReturnValue};
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::time_series::ConvertedTimeSeriesReference;
use crate::v1::{ConversionData, TryFromV1, TryIntoV2};
use crate::validation::{ParameterProblem, ParameterReferenceProblem};
use crate::visit::{Reference, ReferenceMut, VisitMetrics, VisitPaths, VisitReferences};
pub use aggregated::{AggregatedIndexParameter, AggregatedParameter};
pub use asymmetric_switch::AsymmetricSwitchIndexParameter;
pub use control_curves::{
    ControlCurveIndexParameter, ControlCurveInterpolatedParameter, ControlCurveParameter,
    ControlCurvePiecewiseInterpolatedParameter,
};
pub use core::{
    ActivationFunction, ActivationFunctionType, ConstantParameter, ConstantScenarioParameter, DivisionParameter,
    MaxParameter, MinParameter, NegativeMaxParameter, NegativeMinParameter, NegativeParameter, VariableSettings,
};
pub use delay::{DEFAULT_DELAY, DelayIndexParameter, DelayParameter};
pub use difference::DifferenceParameter;
pub use discount_factor::DiscountFactorParameter;
pub use hydropower::HydropowerTargetParameter;
pub use indexed_array::IndexedArrayParameter;
pub use interpolated::InterpolatedParameter;
pub use offset::OffsetParameter;
pub use placeholder::PlaceholderParameter;
pub use polynomial::Polynomial1DParameter;
pub use profiles::{
    DailyProfileParameter, DirunalProfileParameter, MonthlyInterpDay, MonthlyProfileParameter, RadialBasisFunction,
    RadialBasisFunctionType, RbfProfileParameter, RbfProfileVariableSettings, UniformDrawdownProfileParameter,
    WeeklyInterpDay, WeeklyProfileParameter,
};
pub use python::{PythonObject, PythonObjectType, PythonParameter, PythonReturnType};
#[cfg(feature = "core")]
use pywr_core::parameters::ParameterName;
use pywr_schema_macros::{PywrVisitAll, PywrVisitMetrics, PywrVisitPaths};
use pywr_v1_schema::parameters::{
    CoreParameter, DataFrameParameter as DataFrameParameterV1, Parameter as ParameterV1,
    ParameterValue as ParameterValueV1, TableIndex as TableIndexV1, TableIndexEntry as TableIndexEntryV1,
};
pub use rolling::{RollingIndexParameter, RollingParameter};
use schemars::JsonSchema;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};
pub use tables::TablesArrayParameter;
pub use thresholds::{MultiThresholdParameter, Predicate, ThresholdParameter};

#[derive(
    serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq, Eq, JsonSchema, PywrVisitAll, Display, EnumIter,
)]
pub enum ParameterPhase {
    Before,
    After,
    Both,
}

impl ParameterPhase {
    /// Whether a parameter in this phase calculates the value `return_value` asks for. `Both`
    /// counts as calculated, since whether core gives it depends on how it builds the parameter
    /// and where it is read.
    pub fn calculates(&self, return_value: ParameterReturnValue) -> bool {
        match return_value {
            ParameterReturnValue::Before => matches!(self, Self::Before | Self::Both),
            ParameterReturnValue::After | ParameterReturnValue::AfterOrElseInitial => {
                matches!(self, Self::After | Self::Both)
            }
            ParameterReturnValue::Both => true,
        }
    }
}

/// The type of value a parameter gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterValueType {
    Float,
    Index,
    Multi,
}

impl ParameterValueType {
    /// Whether a `metric` can read the parameter: a float metric reads an index as a float, but
    /// an index metric reads only indices.
    pub fn is_readable_by(self, metric: MetricValueType) -> bool {
        match self {
            Self::Float => metric == MetricValueType::Float,
            Self::Index | Self::Multi => true,
        }
    }

    /// Whether a reference must give a key, which only a multi-valued parameter takes.
    pub fn needs_key(self) -> bool {
        self == Self::Multi
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, EnumDiscriminants, Clone, JsonSchema, Display)]
#[serde(tag = "type")]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
// This creates a separate enum called `ParameterType` that is available in this module.
#[strum_discriminants(name(ParameterType))]
pub enum Parameter {
    Aggregated(AggregatedParameter),
    AggregatedIndex(AggregatedIndexParameter),
    AsymmetricSwitchIndex(AsymmetricSwitchIndexParameter),
    Constant(ConstantParameter),
    ConstantScenario(ConstantScenarioParameter),
    ControlCurvePiecewiseInterpolated(ControlCurvePiecewiseInterpolatedParameter),
    ControlCurveInterpolated(ControlCurveInterpolatedParameter),
    ControlCurveIndex(ControlCurveIndexParameter),
    ControlCurve(ControlCurveParameter),
    DailyProfile(DailyProfileParameter),
    IndexedArray(IndexedArrayParameter),
    MonthlyProfile(MonthlyProfileParameter),
    WeeklyProfile(WeeklyProfileParameter),
    UniformDrawdownProfile(UniformDrawdownProfileParameter),
    Max(MaxParameter),
    Min(MinParameter),
    MultiThreshold(MultiThresholdParameter),
    Negative(NegativeParameter),
    NegativeMax(NegativeMaxParameter),
    NegativeMin(NegativeMinParameter),
    HydropowerTarget(Box<HydropowerTargetParameter>),
    Polynomial1D(Polynomial1DParameter),
    Threshold(ThresholdParameter),
    TablesArray(TablesArrayParameter),
    Python(PythonParameter),
    Delay(DelayParameter),
    DelayIndex(DelayIndexParameter),
    Division(DivisionParameter),
    Difference(DifferenceParameter),
    Offset(OffsetParameter),
    DiscountFactor(DiscountFactorParameter),
    Interpolated(InterpolatedParameter),
    RbfProfile(RbfProfileParameter),
    Rolling(RollingParameter),
    RollingIndex(RollingIndexParameter),
    Placeholder(PlaceholderParameter),
    DiurnalProfile(DirunalProfileParameter),
}

impl Parameter {
    pub fn name(&self) -> &str {
        self.meta().name.as_str()
    }

    pub fn is_placeholder(&self) -> bool {
        matches!(self, Self::Placeholder(_))
    }

    pub fn meta(&self) -> &NamedMeta {
        match self {
            Self::Constant(p) => &p.meta,
            Self::ConstantScenario(p) => &p.meta,
            Self::ControlCurveInterpolated(p) => &p.meta,
            Self::Aggregated(p) => &p.meta,
            Self::AggregatedIndex(p) => &p.meta,
            Self::AsymmetricSwitchIndex(p) => &p.meta,
            Self::ControlCurvePiecewiseInterpolated(p) => &p.meta,
            Self::ControlCurveIndex(p) => &p.meta,
            Self::ControlCurve(p) => &p.meta,
            Self::DailyProfile(p) => &p.meta,
            Self::IndexedArray(p) => &p.meta,
            Self::MonthlyProfile(p) => &p.meta,
            Self::WeeklyProfile(p) => &p.meta,
            Self::UniformDrawdownProfile(p) => &p.meta,
            Self::Max(p) => &p.meta,
            Self::Min(p) => &p.meta,
            Self::MultiThreshold(p) => &p.meta,
            Self::Negative(p) => &p.meta,
            Self::Polynomial1D(p) => &p.meta,
            Self::Threshold(p) => &p.meta,
            Self::TablesArray(p) => &p.meta,
            Self::Python(p) => &p.meta,
            Self::Division(p) => &p.meta,
            Self::Difference(p) => &p.meta,
            Self::Delay(p) => &p.meta,
            Self::DelayIndex(p) => &p.meta,
            Self::Offset(p) => &p.meta,
            Self::DiscountFactor(p) => &p.meta,
            Self::Interpolated(p) => &p.meta,
            Self::HydropowerTarget(p) => &p.meta,
            Self::RbfProfile(p) => &p.meta,
            Self::NegativeMax(p) => &p.meta,
            Self::NegativeMin(p) => &p.meta,
            Self::Rolling(p) => &p.meta,
            Self::RollingIndex(p) => &p.meta,
            Self::Placeholder(p) => &p.meta,
            Self::DiurnalProfile(p) => &p.meta,
        }
    }

    /// Get a mutable reference to the parameter's metadata.
    pub fn meta_mut(&mut self) -> &mut NamedMeta {
        match self {
            Self::Constant(p) => &mut p.meta,
            Self::ConstantScenario(p) => &mut p.meta,
            Self::ControlCurveInterpolated(p) => &mut p.meta,
            Self::Aggregated(p) => &mut p.meta,
            Self::AggregatedIndex(p) => &mut p.meta,
            Self::AsymmetricSwitchIndex(p) => &mut p.meta,
            Self::ControlCurvePiecewiseInterpolated(p) => &mut p.meta,
            Self::ControlCurveIndex(p) => &mut p.meta,
            Self::ControlCurve(p) => &mut p.meta,
            Self::DailyProfile(p) => &mut p.meta,
            Self::IndexedArray(p) => &mut p.meta,
            Self::MonthlyProfile(p) => &mut p.meta,
            Self::WeeklyProfile(p) => &mut p.meta,
            Self::UniformDrawdownProfile(p) => &mut p.meta,
            Self::Max(p) => &mut p.meta,
            Self::Min(p) => &mut p.meta,
            Self::MultiThreshold(p) => &mut p.meta,
            Self::Negative(p) => &mut p.meta,
            Self::Polynomial1D(p) => &mut p.meta,
            Self::Threshold(p) => &mut p.meta,
            Self::TablesArray(p) => &mut p.meta,
            Self::Python(p) => &mut p.meta,
            Self::Division(p) => &mut p.meta,
            Self::Difference(p) => &mut p.meta,
            Self::Delay(p) => &mut p.meta,
            Self::DelayIndex(p) => &mut p.meta,
            Self::Offset(p) => &mut p.meta,
            Self::DiscountFactor(p) => &mut p.meta,
            Self::Interpolated(p) => &mut p.meta,
            Self::HydropowerTarget(p) => &mut p.meta,
            Self::RbfProfile(p) => &mut p.meta,
            Self::NegativeMax(p) => &mut p.meta,
            Self::NegativeMin(p) => &mut p.meta,
            Self::Rolling(p) => &mut p.meta,
            Self::RollingIndex(p) => &mut p.meta,
            Self::Placeholder(p) => &mut p.meta,
            Self::DiurnalProfile(p) => &mut p.meta,
        }
    }

    pub fn parameter_type(&self) -> ParameterType {
        // Implementation provided by the `EnumDiscriminants` derive macro.
        self.into()
    }

    /// The phase(s) the parameter is calculated in, or `None` for a Python class, whose methods
    /// decide its phases when it is built.
    pub fn phase(&self) -> Option<ParameterPhase> {
        let phase = match self {
            Self::Aggregated(p) => p.phase.clone(),
            Self::AggregatedIndex(p) => p.phase.clone(),
            Self::AsymmetricSwitchIndex(_) => ParameterPhase::Before,
            Self::Constant(_) => ParameterPhase::Before,
            Self::ConstantScenario(_) => ParameterPhase::Before,
            Self::ControlCurvePiecewiseInterpolated(p) => p.phase.clone(),
            Self::ControlCurveInterpolated(p) => p.phase.clone(),
            Self::ControlCurveIndex(p) => p.phase.clone(),
            Self::ControlCurve(p) => p.phase.clone(),
            Self::DailyProfile(_) => ParameterPhase::Before,
            Self::IndexedArray(p) => p.phase.clone(),
            Self::MonthlyProfile(_) => ParameterPhase::Before,
            Self::WeeklyProfile(_) => ParameterPhase::Before,
            Self::UniformDrawdownProfile(_) => ParameterPhase::Before,
            Self::Max(p) => p.phase.clone(),
            Self::Min(p) => p.phase.clone(),
            Self::MultiThreshold(p) => p.phase.clone(),
            Self::Negative(p) => p.phase.clone(),
            Self::Polynomial1D(p) => p.phase.clone(),
            Self::Threshold(p) => p.phase.clone(),
            Self::TablesArray(_) => ParameterPhase::Before,
            Self::Python(p) => match p.object {
                PythonObject::Class { .. } => return None,
                PythonObject::Function { .. } => ParameterPhase::Before,
            },
            Self::Delay(_) => ParameterPhase::Before,
            Self::DelayIndex(_) => ParameterPhase::Before,
            Self::Division(p) => p.phase.clone(),
            Self::Difference(p) => p.phase.clone(),
            Self::Offset(p) => p.phase.clone(),
            Self::DiscountFactor(_) => ParameterPhase::Before,
            Self::Interpolated(p) => p.phase.clone(),
            // Core calculates it before with a target and after with an actual flow.
            Self::HydropowerTarget(p) => match (p.target.is_some(), p.actual_flow.is_some()) {
                (true, true) => ParameterPhase::Both,
                (false, true) => ParameterPhase::After,
                _ => ParameterPhase::Before,
            },
            Self::RbfProfile(_) => ParameterPhase::Before,
            Self::NegativeMax(p) => p.phase.clone(),
            Self::NegativeMin(p) => p.phase.clone(),
            Self::Rolling(_) => ParameterPhase::Before,
            Self::RollingIndex(_) => ParameterPhase::Before,
            Self::Placeholder(_) => ParameterPhase::Before,
            Self::DiurnalProfile(_) => ParameterPhase::Before,
        };

        Some(phase)
    }

    /// The type of value the parameter gives, or `None` for a placeholder, which builds nothing.
    pub fn value_type(&self) -> Option<ParameterValueType> {
        use ParameterValueType::{Float, Index, Multi};

        let value_type = match self {
            Self::AggregatedIndex(_)
            | Self::AsymmetricSwitchIndex(_)
            | Self::ControlCurveIndex(_)
            | Self::DelayIndex(_)
            | Self::RollingIndex(_) => Index,
            // With returned metrics it gives one of them, not the index that picks it.
            Self::Threshold(p) if p.returned_metrics.is_some() => Float,
            Self::MultiThreshold(p) if p.returned_metrics.is_some() => Float,
            Self::Threshold(_) | Self::MultiThreshold(_) => Index,
            Self::Python(p) => match p.return_type {
                PythonReturnType::Float => Float,
                PythonReturnType::Int => Index,
                PythonReturnType::Dict => Multi,
            },
            Self::Placeholder(_) => return None,
            Self::Aggregated(_)
            | Self::Constant(_)
            | Self::ConstantScenario(_)
            | Self::ControlCurvePiecewiseInterpolated(_)
            | Self::ControlCurveInterpolated(_)
            | Self::ControlCurve(_)
            | Self::DailyProfile(_)
            | Self::IndexedArray(_)
            | Self::MonthlyProfile(_)
            | Self::WeeklyProfile(_)
            | Self::UniformDrawdownProfile(_)
            | Self::Max(_)
            | Self::Min(_)
            | Self::Negative(_)
            | Self::NegativeMax(_)
            | Self::NegativeMin(_)
            | Self::HydropowerTarget(_)
            | Self::Polynomial1D(_)
            | Self::TablesArray(_)
            | Self::Delay(_)
            | Self::Division(_)
            | Self::Difference(_)
            | Self::Offset(_)
            | Self::DiscountFactor(_)
            | Self::Interpolated(_)
            | Self::RbfProfile(_)
            | Self::Rolling(_)
            | Self::DiurnalProfile(_) => Float,
        };

        Some(value_type)
    }

    /// Check that a reference with `key`, read by a `metric` and asking for `return_value`, can
    /// read this parameter: its key, then the kind of value, then the phase. A placeholder takes
    /// any reference, and a Python class any phase.
    pub fn validate_reference(
        &self,
        key: Option<&str>,
        metric: MetricValueType,
        return_value: ParameterReturnValue,
    ) -> Result<(), ParameterReferenceProblem> {
        let Some(value_type) = self.value_type() else {
            return Ok(());
        };

        match (value_type.needs_key(), key) {
            (true, None) => Err(ParameterReferenceProblem::KeyMissing),
            (false, Some(key)) => Err(ParameterReferenceProblem::KeyNotAllowed { key: key.to_string() }),
            _ if !value_type.is_readable_by(metric) => Err(ParameterReferenceProblem::NotAnIndex),
            _ => match self.phase() {
                Some(phase) if !phase.calculates(return_value) => {
                    Err(ParameterReferenceProblem::ValueNotCalculated { return_value, phase })
                }
                _ => Ok(()),
            },
        }
    }

    /// Check the parameter's own fields, such as a control curve's count of values, and return
    /// every problem found. A value in a table is not loaded, so it is not checked.
    pub fn validate(&self) -> Result<(), Vec<ParameterProblem>> {
        match self {
            Self::Aggregated(p) => p.validate(),
            Self::AggregatedIndex(p) => p.validate(),
            Self::ControlCurvePiecewiseInterpolated(p) => p.validate(),
            Self::ControlCurveInterpolated(p) => p.validate(),
            Self::ControlCurveIndex(p) => p.validate(),
            Self::ControlCurve(p) => p.validate(),
            Self::DailyProfile(p) => p.validate(),
            Self::IndexedArray(p) => p.validate(),
            Self::MonthlyProfile(p) => p.validate(),
            Self::WeeklyProfile(p) => p.validate(),
            Self::UniformDrawdownProfile(p) => p.validate(),
            Self::MultiThreshold(p) => p.validate(),
            Self::Polynomial1D(p) => p.validate(),
            Self::Division(p) => p.validate(),
            Self::Difference(p) => p.validate(),
            Self::DiscountFactor(p) => p.validate(),
            Self::Interpolated(p) => p.validate(),
            Self::HydropowerTarget(p) => p.validate(),
            Self::RbfProfile(p) => p.validate(),
            Self::Rolling(p) => p.validate(),
            Self::RollingIndex(p) => p.validate(),
            Self::DiurnalProfile(p) => p.validate(),
            Self::AsymmetricSwitchIndex(_)
            | Self::Constant(_)
            | Self::ConstantScenario(_)
            | Self::Max(_)
            | Self::Min(_)
            | Self::Negative(_)
            | Self::Threshold(_)
            | Self::TablesArray(_)
            | Self::Python(_)
            | Self::Delay(_)
            | Self::DelayIndex(_)
            | Self::Offset(_)
            | Self::NegativeMax(_)
            | Self::NegativeMin(_)
            | Self::Placeholder(_) => Ok(()),
        }
    }
}

/// The problems [`Parameter::validate`] finds in each of `parameters`, with the parameter's name,
/// in the order listed.
pub(crate) fn validate_each_parameter(parameters: &[Parameter]) -> impl Iterator<Item = (&str, ParameterProblem)> {
    parameters.iter().flat_map(|parameter| {
        parameter
            .validate()
            .err()
            .unwrap_or_default()
            .into_iter()
            .map(move |problem| (parameter.name(), problem))
    })
}

#[cfg(feature = "core")]
impl Parameter {
    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<(), SchemaError> {
        match self {
            Self::Constant(p) => p.add_to_network(network, args, parent),
            Self::ConstantScenario(p) => p.add_to_network(network, args, parent),
            Self::ControlCurveInterpolated(p) => p.add_to_network(network, args, parent),
            Self::Aggregated(p) => p.add_to_network(network, args, parent),
            Self::AggregatedIndex(p) => p.add_to_network(network, args, parent),
            Self::AsymmetricSwitchIndex(p) => p.add_to_network(network, args, parent),
            Self::ControlCurvePiecewiseInterpolated(p) => p.add_to_network(network, args, parent),
            Self::ControlCurveIndex(p) => p.add_to_network(network, args, parent),
            Self::ControlCurve(p) => p.add_to_network(network, args, parent),
            Self::DailyProfile(p) => p.add_to_network(network, args, parent),
            Self::IndexedArray(p) => p.add_to_network(network, args, parent),
            Self::MonthlyProfile(p) => p.add_to_network(network, args, parent),
            Self::WeeklyProfile(p) => p.add_to_network(network, args, parent),
            Self::UniformDrawdownProfile(p) => p.add_to_network(network, args, parent),
            Self::Max(p) => p.add_to_network(network, args, parent),
            Self::Min(p) => p.add_to_network(network, args, parent),
            Self::Negative(p) => p.add_to_network(network, args, parent),
            Self::Polynomial1D(p) => p.add_to_network(network, args, parent),
            Self::Threshold(p) => p.add_to_network(network, args, parent),
            Self::TablesArray(p) => p.add_to_network(network, args, parent),
            Self::Python(p) => p.add_to_network(network, args, parent),
            Self::Delay(p) => p.add_to_network(network, args, parent),
            Self::DelayIndex(p) => p.add_to_network(network, args, parent),
            Self::Division(p) => p.add_to_network(network, args, parent),
            Self::Difference(p) => p.add_to_network(network, args, parent),
            Self::Offset(p) => p.add_to_network(network, args, parent),
            Self::DiscountFactor(p) => p.add_to_network(network, args, parent),
            Self::Interpolated(p) => p.add_to_network(network, args, parent),
            Self::RbfProfile(p) => p.add_to_network(network, parent),
            Self::NegativeMax(p) => p.add_to_network(network, args, parent),
            Self::NegativeMin(p) => p.add_to_network(network, args, parent),
            Self::HydropowerTarget(p) => p.add_to_network(network, args, parent),
            Self::Rolling(p) => p.add_to_network(network, args, parent),
            Self::RollingIndex(p) => p.add_to_network(network, args, parent),
            Self::Placeholder(p) => p.add_to_network(),
            Self::MultiThreshold(p) => p.add_to_network(network, args, parent),
            Self::DiurnalProfile(p) => p.add_to_network(network, args, parent),
        }?;

        // `validate` checks references against `value_type`, so it must match the build.
        debug_assert_eq!(
            self.added_value_type(network, parent),
            self.value_type(),
            "The parameter `{}`",
            self.name()
        );

        Ok(())
    }

    /// The value type of the network's list that holds the parameter.
    fn added_value_type(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        parent: Option<&str>,
    ) -> Option<ParameterValueType> {
        let name = ParameterName::new(self.name(), parent);
        let parameters = network.parameters();

        if parameters.f64.iter().any(|p| p.name() == &name) {
            Some(ParameterValueType::Float)
        } else if parameters.u64.iter().any(|p| p.name() == &name) {
            Some(ParameterValueType::Index)
        } else if parameters.multi.iter().any(|p| p.name() == &name) {
            Some(ParameterValueType::Multi)
        } else {
            None
        }
    }
}

impl VisitMetrics for Parameter {
    fn visit_metrics<F: FnMut(&Metric)>(&self, visitor: &mut F) {
        match self {
            Self::Constant(p) => p.visit_metrics(visitor),
            Self::ConstantScenario(p) => p.visit_metrics(visitor),
            Self::ControlCurveInterpolated(p) => p.visit_metrics(visitor),
            Self::Aggregated(p) => p.visit_metrics(visitor),
            Self::AggregatedIndex(p) => p.visit_metrics(visitor),
            Self::AsymmetricSwitchIndex(p) => p.visit_metrics(visitor),
            Self::ControlCurvePiecewiseInterpolated(p) => p.visit_metrics(visitor),
            Self::ControlCurveIndex(p) => p.visit_metrics(visitor),
            Self::ControlCurve(p) => p.visit_metrics(visitor),
            Self::DailyProfile(p) => p.visit_metrics(visitor),
            Self::IndexedArray(p) => p.visit_metrics(visitor),
            Self::MonthlyProfile(p) => p.visit_metrics(visitor),
            Self::WeeklyProfile(p) => p.visit_metrics(visitor),
            Self::UniformDrawdownProfile(p) => p.visit_metrics(visitor),
            Self::Max(p) => p.visit_metrics(visitor),
            Self::Min(p) => p.visit_metrics(visitor),
            Self::MultiThreshold(p) => p.visit_metrics(visitor),
            Self::Negative(p) => p.visit_metrics(visitor),
            Self::Polynomial1D(p) => p.visit_metrics(visitor),
            Self::Threshold(p) => p.visit_metrics(visitor),
            Self::TablesArray(p) => p.visit_metrics(visitor),
            Self::Python(p) => p.visit_metrics(visitor),
            Self::Delay(p) => p.visit_metrics(visitor),
            Self::DelayIndex(p) => p.visit_metrics(visitor),
            Self::Division(p) => p.visit_metrics(visitor),
            Self::Difference(p) => p.visit_metrics(visitor),
            Self::Offset(p) => p.visit_metrics(visitor),
            Self::DiscountFactor(p) => p.visit_metrics(visitor),
            Self::Interpolated(p) => p.visit_metrics(visitor),
            Self::RbfProfile(p) => p.visit_metrics(visitor),
            Self::NegativeMax(p) => p.visit_metrics(visitor),
            Self::NegativeMin(p) => p.visit_metrics(visitor),
            Self::HydropowerTarget(p) => p.visit_metrics(visitor),
            Self::Rolling(p) => p.visit_metrics(visitor),
            Self::RollingIndex(p) => p.visit_metrics(visitor),
            Self::Placeholder(p) => p.visit_metrics(visitor),
            Self::DiurnalProfile(p) => p.visit_metrics(visitor),
        }
    }

    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, visitor: &mut F) {
        match self {
            Self::Constant(p) => p.visit_metrics_mut(visitor),
            Self::ConstantScenario(p) => p.visit_metrics_mut(visitor),
            Self::ControlCurveInterpolated(p) => p.visit_metrics_mut(visitor),
            Self::Aggregated(p) => p.visit_metrics_mut(visitor),
            Self::AggregatedIndex(p) => p.visit_metrics_mut(visitor),
            Self::AsymmetricSwitchIndex(p) => p.visit_metrics_mut(visitor),
            Self::ControlCurvePiecewiseInterpolated(p) => p.visit_metrics_mut(visitor),
            Self::ControlCurveIndex(p) => p.visit_metrics_mut(visitor),
            Self::ControlCurve(p) => p.visit_metrics_mut(visitor),
            Self::DailyProfile(p) => p.visit_metrics_mut(visitor),
            Self::IndexedArray(p) => p.visit_metrics_mut(visitor),
            Self::MonthlyProfile(p) => p.visit_metrics_mut(visitor),
            Self::WeeklyProfile(p) => p.visit_metrics_mut(visitor),
            Self::UniformDrawdownProfile(p) => p.visit_metrics_mut(visitor),
            Self::Max(p) => p.visit_metrics_mut(visitor),
            Self::Min(p) => p.visit_metrics_mut(visitor),
            Self::MultiThreshold(p) => p.visit_metrics_mut(visitor),
            Self::Negative(p) => p.visit_metrics_mut(visitor),
            Self::Polynomial1D(p) => p.visit_metrics_mut(visitor),
            Self::Threshold(p) => p.visit_metrics_mut(visitor),
            Self::TablesArray(p) => p.visit_metrics_mut(visitor),
            Self::Python(p) => p.visit_metrics_mut(visitor),
            Self::Delay(p) => p.visit_metrics_mut(visitor),
            Self::DelayIndex(p) => p.visit_metrics_mut(visitor),
            Self::Division(p) => p.visit_metrics_mut(visitor),
            Self::Difference(p) => p.visit_metrics_mut(visitor),
            Self::Offset(p) => p.visit_metrics_mut(visitor),
            Self::DiscountFactor(p) => p.visit_metrics_mut(visitor),
            Self::Interpolated(p) => p.visit_metrics_mut(visitor),
            Self::RbfProfile(p) => p.visit_metrics_mut(visitor),
            Self::NegativeMax(p) => p.visit_metrics_mut(visitor),
            Self::NegativeMin(p) => p.visit_metrics_mut(visitor),
            Self::HydropowerTarget(p) => p.visit_metrics_mut(visitor),
            Self::Rolling(p) => p.visit_metrics_mut(visitor),
            Self::RollingIndex(p) => p.visit_metrics_mut(visitor),
            Self::Placeholder(p) => p.visit_metrics_mut(visitor),
            Self::DiurnalProfile(p) => p.visit_metrics_mut(visitor),
        }
    }
}

impl VisitPaths for Parameter {
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        match self {
            Self::Constant(p) => p.visit_paths(visitor),
            Self::ConstantScenario(p) => p.visit_paths(visitor),
            Self::ControlCurveInterpolated(p) => p.visit_paths(visitor),
            Self::Aggregated(p) => p.visit_paths(visitor),
            Self::AggregatedIndex(p) => p.visit_paths(visitor),
            Self::AsymmetricSwitchIndex(p) => p.visit_paths(visitor),
            Self::ControlCurvePiecewiseInterpolated(p) => p.visit_paths(visitor),
            Self::ControlCurveIndex(p) => p.visit_paths(visitor),
            Self::ControlCurve(p) => p.visit_paths(visitor),
            Self::DailyProfile(p) => p.visit_paths(visitor),
            Self::IndexedArray(p) => p.visit_paths(visitor),
            Self::MonthlyProfile(p) => p.visit_paths(visitor),
            Self::WeeklyProfile(p) => p.visit_paths(visitor),
            Self::UniformDrawdownProfile(p) => p.visit_paths(visitor),
            Self::Max(p) => p.visit_paths(visitor),
            Self::Min(p) => p.visit_paths(visitor),
            Self::MultiThreshold(p) => p.visit_paths(visitor),
            Self::Negative(p) => p.visit_paths(visitor),
            Self::Polynomial1D(p) => p.visit_paths(visitor),
            Self::Threshold(p) => p.visit_paths(visitor),
            Self::TablesArray(p) => p.visit_paths(visitor),
            Self::Python(p) => p.visit_paths(visitor),
            Self::Delay(p) => p.visit_paths(visitor),
            Self::DelayIndex(p) => p.visit_paths(visitor),
            Self::Division(p) => p.visit_paths(visitor),
            Self::Difference(p) => p.visit_paths(visitor),
            Self::Offset(p) => p.visit_paths(visitor),
            Self::DiscountFactor(p) => p.visit_paths(visitor),
            Self::Interpolated(p) => p.visit_paths(visitor),
            Self::RbfProfile(p) => p.visit_paths(visitor),
            Self::NegativeMax(p) => p.visit_paths(visitor),
            Self::NegativeMin(p) => p.visit_paths(visitor),
            Self::HydropowerTarget(p) => p.visit_paths(visitor),
            Self::Rolling(p) => p.visit_paths(visitor),
            Self::RollingIndex(p) => p.visit_paths(visitor),
            Self::Placeholder(p) => p.visit_paths(visitor),
            Self::DiurnalProfile(p) => p.visit_paths(visitor),
        }
    }

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        match self {
            Self::Constant(p) => p.visit_paths_mut(visitor),
            Self::ConstantScenario(p) => p.visit_paths_mut(visitor),
            Self::ControlCurveInterpolated(p) => p.visit_paths_mut(visitor),
            Self::Aggregated(p) => p.visit_paths_mut(visitor),
            Self::AggregatedIndex(p) => p.visit_paths_mut(visitor),
            Self::AsymmetricSwitchIndex(p) => p.visit_paths_mut(visitor),
            Self::ControlCurvePiecewiseInterpolated(p) => p.visit_paths_mut(visitor),
            Self::ControlCurveIndex(p) => p.visit_paths_mut(visitor),
            Self::ControlCurve(p) => p.visit_paths_mut(visitor),
            Self::DailyProfile(p) => p.visit_paths_mut(visitor),
            Self::IndexedArray(p) => p.visit_paths_mut(visitor),
            Self::MonthlyProfile(p) => p.visit_paths_mut(visitor),
            Self::WeeklyProfile(p) => p.visit_paths_mut(visitor),
            Self::UniformDrawdownProfile(p) => p.visit_paths_mut(visitor),
            Self::Max(p) => p.visit_paths_mut(visitor),
            Self::Min(p) => p.visit_paths_mut(visitor),
            Self::MultiThreshold(p) => p.visit_paths_mut(visitor),
            Self::Negative(p) => p.visit_paths_mut(visitor),
            Self::Polynomial1D(p) => p.visit_paths_mut(visitor),
            Self::Threshold(p) => p.visit_paths_mut(visitor),
            Self::TablesArray(p) => p.visit_paths_mut(visitor),
            Self::Python(p) => p.visit_paths_mut(visitor),
            Self::Delay(p) => p.visit_paths_mut(visitor),
            Self::DelayIndex(p) => p.visit_paths_mut(visitor),
            Self::Division(p) => p.visit_paths_mut(visitor),
            Self::Difference(p) => p.visit_paths_mut(visitor),
            Self::Offset(p) => p.visit_paths_mut(visitor),
            Self::DiscountFactor(p) => p.visit_paths_mut(visitor),
            Self::Interpolated(p) => p.visit_paths_mut(visitor),
            Self::RbfProfile(p) => p.visit_paths_mut(visitor),
            Self::NegativeMax(p) => p.visit_paths_mut(visitor),
            Self::NegativeMin(p) => p.visit_paths_mut(visitor),
            Self::HydropowerTarget(p) => p.visit_paths_mut(visitor),
            Self::Rolling(p) => p.visit_paths_mut(visitor),
            Self::RollingIndex(p) => p.visit_paths_mut(visitor),
            Self::Placeholder(p) => p.visit_paths_mut(visitor),
            Self::DiurnalProfile(p) => p.visit_paths_mut(visitor),
        }
    }
}

impl VisitReferences for Parameter {
    fn visit_references<F: FnMut(Reference<'_>)>(&self, visitor: &mut F) {
        match self {
            Self::Constant(p) => p.visit_references(visitor),
            Self::ConstantScenario(p) => p.visit_references(visitor),
            Self::ControlCurveInterpolated(p) => p.visit_references(visitor),
            Self::Aggregated(p) => p.visit_references(visitor),
            Self::AggregatedIndex(p) => p.visit_references(visitor),
            Self::AsymmetricSwitchIndex(p) => p.visit_references(visitor),
            Self::ControlCurvePiecewiseInterpolated(p) => p.visit_references(visitor),
            Self::ControlCurveIndex(p) => p.visit_references(visitor),
            Self::ControlCurve(p) => p.visit_references(visitor),
            Self::DailyProfile(p) => p.visit_references(visitor),
            Self::IndexedArray(p) => p.visit_references(visitor),
            Self::MonthlyProfile(p) => p.visit_references(visitor),
            Self::WeeklyProfile(p) => p.visit_references(visitor),
            Self::UniformDrawdownProfile(p) => p.visit_references(visitor),
            Self::Max(p) => p.visit_references(visitor),
            Self::Min(p) => p.visit_references(visitor),
            Self::MultiThreshold(p) => p.visit_references(visitor),
            Self::Negative(p) => p.visit_references(visitor),
            Self::Polynomial1D(p) => p.visit_references(visitor),
            Self::Threshold(p) => p.visit_references(visitor),
            Self::TablesArray(p) => p.visit_references(visitor),
            Self::Python(p) => p.visit_references(visitor),
            Self::Delay(p) => p.visit_references(visitor),
            Self::DelayIndex(p) => p.visit_references(visitor),
            Self::Division(p) => p.visit_references(visitor),
            Self::Difference(p) => p.visit_references(visitor),
            Self::Offset(p) => p.visit_references(visitor),
            Self::DiscountFactor(p) => p.visit_references(visitor),
            Self::Interpolated(p) => p.visit_references(visitor),
            Self::RbfProfile(p) => p.visit_references(visitor),
            Self::NegativeMax(p) => p.visit_references(visitor),
            Self::NegativeMin(p) => p.visit_references(visitor),
            Self::HydropowerTarget(p) => p.visit_references(visitor),
            Self::Rolling(p) => p.visit_references(visitor),
            Self::RollingIndex(p) => p.visit_references(visitor),
            Self::Placeholder(p) => p.visit_references(visitor),
            Self::DiurnalProfile(p) => p.visit_references(visitor),
        }
    }

    fn visit_references_mut<F: FnMut(ReferenceMut<'_>)>(&mut self, visitor: &mut F) {
        match self {
            Self::Constant(p) => p.visit_references_mut(visitor),
            Self::ConstantScenario(p) => p.visit_references_mut(visitor),
            Self::ControlCurveInterpolated(p) => p.visit_references_mut(visitor),
            Self::Aggregated(p) => p.visit_references_mut(visitor),
            Self::AggregatedIndex(p) => p.visit_references_mut(visitor),
            Self::AsymmetricSwitchIndex(p) => p.visit_references_mut(visitor),
            Self::ControlCurvePiecewiseInterpolated(p) => p.visit_references_mut(visitor),
            Self::ControlCurveIndex(p) => p.visit_references_mut(visitor),
            Self::ControlCurve(p) => p.visit_references_mut(visitor),
            Self::DailyProfile(p) => p.visit_references_mut(visitor),
            Self::IndexedArray(p) => p.visit_references_mut(visitor),
            Self::MonthlyProfile(p) => p.visit_references_mut(visitor),
            Self::WeeklyProfile(p) => p.visit_references_mut(visitor),
            Self::UniformDrawdownProfile(p) => p.visit_references_mut(visitor),
            Self::Max(p) => p.visit_references_mut(visitor),
            Self::Min(p) => p.visit_references_mut(visitor),
            Self::MultiThreshold(p) => p.visit_references_mut(visitor),
            Self::Negative(p) => p.visit_references_mut(visitor),
            Self::Polynomial1D(p) => p.visit_references_mut(visitor),
            Self::Threshold(p) => p.visit_references_mut(visitor),
            Self::TablesArray(p) => p.visit_references_mut(visitor),
            Self::Python(p) => p.visit_references_mut(visitor),
            Self::Delay(p) => p.visit_references_mut(visitor),
            Self::DelayIndex(p) => p.visit_references_mut(visitor),
            Self::Division(p) => p.visit_references_mut(visitor),
            Self::Difference(p) => p.visit_references_mut(visitor),
            Self::Offset(p) => p.visit_references_mut(visitor),
            Self::DiscountFactor(p) => p.visit_references_mut(visitor),
            Self::Interpolated(p) => p.visit_references_mut(visitor),
            Self::RbfProfile(p) => p.visit_references_mut(visitor),
            Self::NegativeMax(p) => p.visit_references_mut(visitor),
            Self::NegativeMin(p) => p.visit_references_mut(visitor),
            Self::HydropowerTarget(p) => p.visit_references_mut(visitor),
            Self::Rolling(p) => p.visit_references_mut(visitor),
            Self::RollingIndex(p) => p.visit_references_mut(visitor),
            Self::Placeholder(p) => p.visit_references_mut(visitor),
            Self::DiurnalProfile(p) => p.visit_references_mut(visitor),
        }
    }
}

#[derive(Clone)]
pub enum ParameterOrTimeSeriesRef {
    // Boxed due to large size difference.
    Parameter(Box<Parameter>),
    TimeSeries(ConvertedTimeSeriesReference),
}

impl From<Parameter> for ParameterOrTimeSeriesRef {
    fn from(p: Parameter) -> Self {
        Self::Parameter(Box::new(p))
    }
}

impl From<ConvertedTimeSeriesReference> for ParameterOrTimeSeriesRef {
    fn from(t: ConvertedTimeSeriesReference) -> Self {
        Self::TimeSeries(t)
    }
}

impl TryFromV1<ParameterV1> for ParameterOrTimeSeriesRef {
    type Error = Box<ComponentConversionError>;

    fn try_from_v1(
        v1: ParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let p: ParameterOrTimeSeriesRef = match v1 {
            ParameterV1::Core(v1) => match *v1 {
                CoreParameter::Aggregated(p) => {
                    Parameter::Aggregated(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::AggregatedIndex(p) => {
                    Parameter::AggregatedIndex(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::AsymmetricSwitchIndex(p) => {
                    Parameter::AsymmetricSwitchIndex(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::Constant(p) => Parameter::Constant(p.try_into_v2(parent_node, conversion_data)?).into(),
                CoreParameter::ConstantScenario(p) => {
                    Parameter::ConstantScenario(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::ControlCurvePiecewiseInterpolated(p) => {
                    Parameter::ControlCurvePiecewiseInterpolated(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::ControlCurveInterpolated(p) => {
                    Parameter::ControlCurveInterpolated(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::ControlCurveIndex(p) => {
                    Parameter::ControlCurveIndex(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::ControlCurve(p) => match p.clone().try_into_v2(parent_node, conversion_data) {
                    Ok(p) => Parameter::ControlCurve(p).into(),
                    Err(_) => Parameter::ControlCurveIndex(p.try_into_v2(parent_node, conversion_data)?).into(),
                },
                CoreParameter::DailyProfile(p) => {
                    Parameter::DailyProfile(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::IndexedArray(p) => {
                    Parameter::IndexedArray(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::MonthlyProfile(p) => {
                    Parameter::MonthlyProfile(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::UniformDrawdownProfile(p) => {
                    Parameter::UniformDrawdownProfile(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::Max(p) => Parameter::Max(p.try_into_v2(parent_node, conversion_data)?).into(),
                CoreParameter::Negative(p) => Parameter::Negative(p.try_into_v2(parent_node, conversion_data)?).into(),
                CoreParameter::Polynomial1D(p) => {
                    Parameter::Polynomial1D(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::ParameterThreshold(p) => {
                    Parameter::Threshold(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::NodeThreshold(p) => {
                    Parameter::Threshold(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::StorageThreshold(p) => {
                    Parameter::Threshold(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::MultipleThresholdIndex(p) => {
                    Parameter::MultiThreshold(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::MultipleThresholdParameterIndex(p) => {
                    Parameter::MultiThreshold(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::CurrentYearThreshold(_) => todo!(),
                CoreParameter::CurrentOrdinalDayThreshold(_) => todo!(),
                CoreParameter::TablesArray(p) => {
                    Parameter::TablesArray(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::Min(p) => Parameter::Min(p.try_into_v2(parent_node, conversion_data)?).into(),
                CoreParameter::Division(p) => Parameter::Division(p.try_into_v2(parent_node, conversion_data)?).into(),
                CoreParameter::DataFrame(p) => {
                    <DataFrameParameterV1 as TryIntoV2<ConvertedTimeSeriesReference>>::try_into_v2(
                        p,
                        parent_node,
                        conversion_data,
                    )?
                    .into()
                }
                CoreParameter::Deficit(p) => {
                    return Err(Box::new(ComponentConversionError::Parameter {
                        name: p.meta.and_then(|m| m.name).unwrap_or("unnamed".to_string()),
                        attr: "".to_string(),
                        error: ConversionError::DeprecatedParameter {
                            ty: "DeficitParameter".to_string(),
                            instead: "Use a derived metric instead.".to_string(),
                        },
                    }));
                }
                CoreParameter::DiscountFactor(p) => {
                    Parameter::DiscountFactor(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::InterpolatedVolume(p) => {
                    Parameter::Interpolated(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::InterpolatedFlow(p) => {
                    Parameter::Interpolated(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::HydropowerTarget(p) => {
                    Parameter::HydropowerTarget(Box::new(p.try_into_v2(parent_node, conversion_data)?)).into()
                }
                CoreParameter::WeeklyProfile(p) => {
                    Parameter::WeeklyProfile(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::Storage(p) => {
                    return Err(Box::new(ComponentConversionError::Parameter {
                        name: p.meta.and_then(|m| m.name).unwrap_or("unnamed".to_string()),
                        attr: "".to_string(),
                        error: ConversionError::DeprecatedParameter {
                            ty: "StorageParameter".to_string(),
                            instead: "Use a derived metric instead.".to_string(),
                        },
                    }));
                }
                CoreParameter::RollingMeanFlowNode(p) => {
                    Parameter::Rolling(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::ScenarioWrapper(_) => todo!("Implement ScenarioWrapperParameter"),
                CoreParameter::Flow(p) => {
                    return Err(Box::new(ComponentConversionError::Parameter {
                        name: p.meta.and_then(|m| m.name).unwrap_or("unnamed".to_string()),
                        attr: "".to_string(),
                        error: ConversionError::DeprecatedParameter {
                            ty: "FlowParameter".to_string(),
                            instead: "Use a derived metric instead.".to_string(),
                        },
                    }));
                }
                CoreParameter::RbfProfile(p) => {
                    Parameter::RbfProfile(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::NegativeMax(p) => {
                    Parameter::NegativeMax(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::NegativeMin(p) => {
                    Parameter::NegativeMin(p.try_into_v2(parent_node, conversion_data)?).into()
                }
            },
            ParameterV1::Custom(p) => {
                return Err(Box::new(ComponentConversionError::Parameter {
                    name: p.meta.name.unwrap_or_else(|| "unnamed".to_string()),
                    attr: "".to_string(),
                    error: ConversionError::UnrecognisedType { ty: p.ty },
                }));
            }
        };

        Ok(p)
    }
}

/// A non-variable constant floating-point (f64) value
///
/// This value can be a literal float or an external reference to an input table.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, Display, EnumDiscriminants)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(ConstantValueType))]
pub enum ConstantValue<T> {
    /// A literal value.
    Literal { value: T },
    /// A reference to a constant value in a table.
    Table(TableDataRef),
}

impl From<f64> for ConstantValue<f64> {
    fn from(v: f64) -> Self {
        Self::Literal { value: v }
    }
}

impl From<u64> for ConstantValue<u64> {
    fn from(v: u64) -> Self {
        Self::Literal { value: v }
    }
}

impl From<u32> for ConstantValue<u64> {
    fn from(v: u32) -> Self {
        Self::Literal { value: v as u64 }
    }
}

impl From<u16> for ConstantValue<u64> {
    fn from(v: u16) -> Self {
        Self::Literal { value: v as u64 }
    }
}

impl From<u8> for ConstantValue<u64> {
    fn from(v: u8) -> Self {
        Self::Literal { value: v as u64 }
    }
}

impl From<NonZeroU64> for ConstantValue<NonZeroU64> {
    fn from(v: NonZeroU64) -> Self {
        Self::Literal { value: v }
    }
}

impl Default for ConstantValue<f64> {
    fn default() -> Self {
        0.0.into()
    }
}

impl Default for ConstantValue<u64> {
    fn default() -> Self {
        0_u64.into()
    }
}

// The derive does not work for the generic type T, so these are written out by hand.
impl<T> VisitMetrics for ConstantValue<T>
where
    T: VisitMetrics,
{
    fn visit_metrics<F: FnMut(&Metric)>(&self, visitor: &mut F) {
        match self {
            Self::Literal { value } => value.visit_metrics(visitor),
            Self::Table(v) => v.visit_metrics(visitor),
        }
    }

    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, visitor: &mut F) {
        match self {
            Self::Literal { value } => value.visit_metrics_mut(visitor),
            Self::Table(v) => v.visit_metrics_mut(visitor),
        }
    }
}

impl<T> VisitPaths for ConstantValue<T>
where
    T: VisitPaths,
{
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        match self {
            Self::Literal { value } => value.visit_paths(visitor),
            Self::Table(v) => v.visit_paths(visitor),
        }
    }

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        match self {
            Self::Literal { value } => value.visit_paths_mut(visitor),
            Self::Table(v) => v.visit_paths_mut(visitor),
        }
    }
}

impl<T> VisitReferences for ConstantValue<T>
where
    T: VisitReferences,
{
    fn visit_references<F: FnMut(Reference<'_>)>(&self, visitor: &mut F) {
        match self {
            Self::Literal { value } => value.visit_references(visitor),
            Self::Table(v) => visitor(Reference::Table {
                table_ref: v,
                expected: DataTableValueType::Scalar,
            }),
        }
    }

    fn visit_references_mut<F: FnMut(ReferenceMut<'_>)>(&mut self, visitor: &mut F) {
        match self {
            Self::Literal { value } => value.visit_references_mut(visitor),
            Self::Table(v) => visitor(ReferenceMut::Table(&mut v.table)),
        }
    }
}

#[cfg(feature = "core")]
impl ConstantValue<f64> {
    /// Return the value loading from a table if required.
    pub fn load(&self, tables: &LoadedTableCollection) -> Result<f64, SchemaError> {
        match self {
            Self::Literal { value } => Ok(*value),
            Self::Table(tbl_ref) => tables
                .get_scalar_f64(tbl_ref)
                .map_err(|source| SchemaError::TableRefLoad {
                    table_ref: tbl_ref.clone(),
                    source: Box::new(source),
                }),
        }
    }
}

#[cfg(feature = "core")]
impl ConstantValue<u64> {
    /// Return the value loading from a table if required.
    pub fn load(&self, tables: &LoadedTableCollection) -> Result<u64, SchemaError> {
        match self {
            Self::Literal { value } => Ok(*value),
            Self::Table(tbl_ref) => tables
                .get_scalar_u64(tbl_ref)
                .map_err(|source| SchemaError::TableRefLoad {
                    table_ref: tbl_ref.clone(),
                    source: Box::new(source),
                }),
        }
    }
}

#[cfg(feature = "core")]
impl ConstantValue<NonZeroU64> {
    /// Return the value loading from a table if required.
    ///
    /// A table holds a plain integer, so a zero loaded from one is refused here.
    pub fn load(&self, tables: &LoadedTableCollection) -> Result<NonZeroU64, SchemaError> {
        match self {
            Self::Literal { value } => Ok(*value),
            Self::Table(tbl_ref) => {
                let value = tables
                    .get_scalar_u64(tbl_ref)
                    .map_err(|source| SchemaError::TableRefLoad {
                        table_ref: tbl_ref.clone(),
                        source: Box::new(source),
                    })?;

                NonZeroU64::new(value).ok_or_else(|| SchemaError::TableRefZero {
                    table_ref: tbl_ref.clone(),
                })
            }
        }
    }
}

impl TryFrom<ParameterValueV1> for ConstantValue<f64> {
    type Error = ConversionError;

    fn try_from(v1: ParameterValueV1) -> Result<Self, Self::Error> {
        match v1 {
            ParameterValueV1::Constant(value) => Ok(Self::Literal { value }),
            ParameterValueV1::Reference(_) => Err(ConversionError::ConstantFloatReferencesParameter {}),
            ParameterValueV1::Table(tbl) => Ok(Self::Table(tbl.try_into()?)),
            ParameterValueV1::Inline(_) => Err(ConversionError::ConstantFloatInlineParameter {}),
        }
    }
}

/// An non-variable vector of constant floating-point (f64) values
///
/// This value can be a literal vector of floats or an external reference to an input table.
#[derive(
    serde::Deserialize,
    serde::Serialize,
    Debug,
    Clone,
    JsonSchema,
    PywrVisitMetrics,
    PywrVisitPaths,
    Display,
    EnumDiscriminants,
    PartialEq,
)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(ConstantFloatVecType))]
pub enum ConstantFloatVec {
    Literal { values: Vec<f64> },
    Table(TableDataRef),
}

// Written out by hand: the derive cannot say that a table here must hold arrays.
impl VisitReferences for ConstantFloatVec {
    fn visit_references<F: FnMut(Reference<'_>)>(&self, visitor: &mut F) {
        if let Self::Table(table_ref) = self {
            visitor(Reference::Table {
                table_ref,
                expected: DataTableValueType::Array,
            });
        }
    }

    fn visit_references_mut<F: FnMut(ReferenceMut<'_>)>(&mut self, visitor: &mut F) {
        if let Self::Table(table_ref) = self {
            visitor(ReferenceMut::Table(&mut table_ref.table));
        }
    }
}

#[cfg(feature = "core")]
impl ConstantFloatVec {
    /// Return the value loading from a table if required.
    pub fn load(&self, tables: &LoadedTableCollection) -> Result<Vec<f64>, SchemaError> {
        match self {
            Self::Literal { values } => Ok(values.clone()),
            Self::Table(tbl_ref) => {
                tables
                    .get_vec_f64(tbl_ref)
                    .map(|v| v.to_vec())
                    .map_err(|source| SchemaError::TableRefLoad {
                        table_ref: tbl_ref.clone(),
                        source: Box::new(source),
                    })
            }
        }
    }
}

#[derive(
    serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll, Display, PartialEq, EnumDiscriminants,
)]
#[serde(untagged)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(TableIndexType))]
pub enum TableIndex {
    Single(String),
    Multi(Vec<String>),
}

impl TryFrom<TableIndexV1> for TableIndex {
    type Error = String;

    fn try_from(v1: TableIndexV1) -> Result<Self, Self::Error> {
        match v1 {
            TableIndexV1::Single(s) => match s {
                TableIndexEntryV1::Name(s) => Ok(TableIndex::Single(s)),
                TableIndexEntryV1::Index(_) => Err("Integer table indices not supported".to_string()),
            },
            TableIndexV1::Multi(s) => {
                let names = s
                    .into_iter()
                    .map(|e| match e {
                        TableIndexEntryV1::Name(s) => Ok(s),
                        TableIndexEntryV1::Index(_) => Err("Integer table indices not supported".to_string()),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Self::Multi(names))
            }
        }
    }
}

pub enum DynamicFloatValueType<'a> {
    Single(&'a Metric),
    List(&'a Vec<Metric>),
}

impl<'a> From<&'a Metric> for DynamicFloatValueType<'a> {
    fn from(v: &'a Metric) -> Self {
        Self::Single(v)
    }
}

impl<'a> From<&'a Vec<Metric>> for DynamicFloatValueType<'a> {
    fn from(v: &'a Vec<Metric>) -> Self {
        Self::List(v)
    }
}

#[cfg(test)]
mod tests {
    use crate::parameters::ParameterValueType::{Float, Index, Multi};
    use crate::parameters::{Parameter, ParameterPhase};
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;

    /// The mutable metadata accessor should return the same metadata as [`Parameter::meta`].
    #[test]
    fn test_parameter_meta_mut() {
        let data = r#"
        {
            "meta": { "name": "a-parameter" },
            "type": "Constant",
            "value": { "type": "Literal", "value": 1.0 }
        }
        "#;

        let mut parameter: Parameter = serde_json::from_str(data).unwrap();
        assert_eq!(parameter.name(), "a-parameter");

        parameter.meta_mut().name = "renamed".to_string();
        assert_eq!(parameter.name(), "renamed");
    }

    /// [`Parameter::phase`] should follow a HydropowerTarget's target and actual flow.
    #[test]
    fn test_hydropower_target_phase() {
        let metric = json!({ "type": "Literal", "value": 1.0 });
        let cases = [
            (json!({ "target": metric }), ParameterPhase::Before),
            (json!({ "actual_flow": metric }), ParameterPhase::After),
            (json!({ "target": metric, "actual_flow": metric }), ParameterPhase::Both),
        ];

        for (mut data, expected) in cases {
            data["meta"] = json!({ "name": "a-parameter" });
            data["type"] = json!("HydropowerTarget");
            let parameter: Parameter = serde_json::from_value(data.clone()).unwrap();
            assert_eq!(parameter.phase(), Some(expected), "{data}");
        }
    }

    /// [`Parameter::value_type`] should follow the fields that decide a Threshold's, a
    /// MultiThreshold's and a Python parameter's value type.
    #[test]
    fn test_value_type_follows_fields() {
        let value_type =
            |data: &serde_json::Value| serde_json::from_value::<Parameter>(data.clone()).unwrap().value_type();
        let metric = json!({ "type": "Literal", "value": 1.0 });

        let mut threshold = json!({
            "meta": { "name": "a-parameter" },
            "type": "Threshold",
            "phase": "Before",
            "metric": metric,
            "threshold": metric,
            "predicate": "GT"
        });
        assert_eq!(value_type(&threshold), Some(Index));
        threshold["returned_metrics"] = json!([metric, metric]);
        assert_eq!(value_type(&threshold), Some(Float));

        let mut multi_threshold = json!({
            "meta": { "name": "a-parameter" },
            "type": "MultiThreshold",
            "phase": "Before",
            "metric": metric,
            "thresholds": [metric],
            "predicate": "GT"
        });
        assert_eq!(value_type(&multi_threshold), Some(Index));
        multi_threshold["returned_metrics"] = json!([metric]);
        assert_eq!(value_type(&multi_threshold), Some(Float));

        let mut python = json!({
            "meta": { "name": "a-parameter" },
            "type": "Python",
            "source": { "type": "Path", "path": "p.py" },
            "object": { "type": "Class", "class": "P" },
            "return_type": "Int"
        });
        assert_eq!(value_type(&python), Some(Index));
        python["return_type"] = json!("Dict");
        assert_eq!(value_type(&python), Some(Multi));
    }

    /// [`Parameter::validate_reference`] should check the key, then the kind of value, then the
    /// phase, and pass a placeholder and a Python class's phase.
    #[test]
    fn test_validate_reference_checks_each_rule() {
        use crate::metric::MetricValueType;
        use crate::metric::ParameterReturnValue::{After, Before};
        use crate::validation::ParameterReferenceProblem::*;

        let parameter = |data: serde_json::Value| serde_json::from_value::<Parameter>(data).unwrap();

        let constant = parameter(json!({
            "meta": { "name": "constant" },
            "type": "Constant",
            "value": { "type": "Literal", "value": 1.0 }
        }));
        let python_dict = parameter(json!({
            "meta": { "name": "python" },
            "type": "Python",
            "source": { "type": "Path", "path": "p.py" },
            "object": { "type": "Class", "class": "P" },
            "return_type": "Dict"
        }));
        let placeholder = parameter(json!({ "meta": { "name": "placeholder" }, "type": "Placeholder" }));

        // The `KeyNotAllowed` and `NotAnIndex` cases also break the rules checked after them.
        let cases = [
            (&constant, None, MetricValueType::Float, Before, Ok(())),
            (
                &constant,
                Some("a"),
                MetricValueType::Index,
                After,
                Err(KeyNotAllowed { key: "a".to_string() }),
            ),
            (&constant, None, MetricValueType::Index, After, Err(NotAnIndex)),
            (
                &constant,
                None,
                MetricValueType::Float,
                After,
                Err(ValueNotCalculated {
                    return_value: After,
                    phase: ParameterPhase::Before,
                }),
            ),
            (&python_dict, None, MetricValueType::Float, Before, Err(KeyMissing)),
            (&python_dict, Some("a"), MetricValueType::Index, After, Ok(())),
            (&placeholder, Some("a"), MetricValueType::Index, After, Ok(())),
        ];

        for (parameter, key, metric, return_value, expected) in cases {
            assert_eq!(
                parameter.validate_reference(key, metric, return_value),
                expected,
                "{} with key {key:?}, read as {metric:?} for {return_value}",
                parameter.name()
            );
        }
    }

    /// [`Parameter::validate`] should refuse a parameter breaking each rule, and pass one where a
    /// rule could be too strict.
    #[test]
    fn test_validate_checks_each_rule() {
        use crate::validation::ParameterProblem::*;
        use crate::validation::PointsProblem::{LengthMismatch, NotIncreasing, TooFewPoints};

        let x = |value: f64| json!({ "type": "Literal", "value": value });
        let n = |value: u64| json!({ "type": "Literal", "value": value });
        let values = |count: usize| json!({ "type": "Literal", "values": vec![0.0; count] });
        let table = json!({ "type": "Table", "table": "t" });
        let not_literal = json!({ "type": "Parameter", "name": "p" });
        let control_curve = |kind: &str, values: serde_json::Value| json!({ "type": kind, "phase": "Before", "control_curves": [x(0.5)], "storage_metric": x(0.5), "values": values });
        let interpolated = |xp: serde_json::Value, fp: usize| json!({ "type": "Interpolated", "phase": "Before", "x": x(0.5), "xp": xp, "fp": vec![x(0.0); fp] });

        let cases = [
            (
                control_curve("ControlCurve", json!([x(1.0)])),
                vec![ControlCurveValues { required: 2, found: 1 }],
            ),
            (
                control_curve("ControlCurveInterpolated", json!([x(1.0), x(0.0)])),
                vec![ControlCurveValues { required: 3, found: 2 }],
            ),
            (
                control_curve("ControlCurvePiecewiseInterpolated", json!([[0.0, 1.0]])),
                vec![ControlCurveValues { required: 2, found: 1 }],
            ),
            (
                interpolated(json!([x(0.0), x(1.0)]), 1),
                vec![Interpolation(LengthMismatch { x: 2, y: 1 })],
            ),
            (interpolated(json!([x(0.0)]), 1), vec![Interpolation(TooFewPoints(1))]),
            (
                interpolated(json!([x(0.0), x(1.0), x(1.0)]), 3),
                vec![Interpolation(NotIncreasing { index: 2 })],
            ),
            // Only neighbouring literals are compared.
            (interpolated(json!([x(1.0), not_literal, x(0.5)]), 3), vec![]),
            // 2016 is a leap year.
            (
                json!({ "type": "UniformDrawdownProfile", "reset_day": n(29), "reset_month": n(2) }),
                vec![],
            ),
            (
                json!({ "type": "UniformDrawdownProfile", "reset_day": n(30), "reset_month": n(2) }),
                vec![NotADate { day: 30, month: 2 }],
            ),
            (
                json!({ "type": "UniformDrawdownProfile", "reset_month": n(13) }),
                vec![NotADate { day: 1, month: 13 }],
            ),
            // Core casts these with `as i8` and `as u8`, which would wrap them round.
            (
                json!({ "type": "UniformDrawdownProfile", "reset_day": n(257), "reset_month": n(1), "residual_days": n(256) }),
                vec![NotADate { day: 257, month: 1 }, ResidualDaysTooLarge(256)],
            ),
            (
                json!({ "type": "UniformDrawdownProfile", "reset_day": table, "reset_month": n(13) }),
                vec![],
            ),
            (
                json!({ "type": "DailyProfile", "values": values(364) }),
                vec![ProfileValues {
                    allowed: &[365, 366],
                    found: 364,
                }],
            ),
            (
                json!({ "type": "MonthlyProfile", "values": values(11) }),
                vec![ProfileValues {
                    allowed: &[12],
                    found: 11,
                }],
            ),
            (
                json!({ "type": "WeeklyProfile", "values": values(54) }),
                vec![ProfileValues {
                    allowed: &[52, 53],
                    found: 54,
                }],
            ),
            (
                json!({ "type": "DiurnalProfile", "values": values(23) }),
                vec![ProfileValues {
                    allowed: &[24],
                    found: 23,
                }],
            ),
            (json!({ "type": "DailyProfile", "values": table }), vec![]),
            (
                json!({ "type": "RbfProfile", "points": [], "function": { "type": "Gaussian" } }),
                vec![NoPointsForEpsilon],
            ),
            (
                json!({ "type": "RbfProfile", "points": [], "function": { "type": "Gaussian", "epsilon": 1.0 } }),
                vec![],
            ),
            (
                json!({ "type": "Division", "phase": "Before", "numerator": x(1.0), "denominator": x(0.0) }),
                vec![ZeroDenominator],
            ),
            (
                json!({ "type": "IndexedArray", "phase": "Before", "metrics": [], "index_metric": not_literal }),
                vec![NoMetrics],
            ),
            (
                json!({ "type": "IndexedArray", "phase": "Before", "metrics": [x(0.0), x(0.0)], "index_metric": { "type": "Constant", "value": 2 } }),
                vec![IndexOutOfRange { index: 2, count: 2 }],
            ),
            (json!({ "type": "HydropowerTarget" }), vec![NoTargetOrActualFlow]),
            (
                json!({ "type": "HydropowerTarget", "actual_flow": x(1.0), "min_flow": x(0.0), "max_flow": x(2.0) }),
                vec![FlowBoundWithoutTarget("min_flow"), FlowBoundWithoutTarget("max_flow")],
            ),
            (
                json!({ "type": "HydropowerTarget", "target": x(1.0), "min_flow": x(0.0), "max_flow": x(2.0) }),
                vec![],
            ),
            (
                json!({ "type": "HydropowerTarget", "target": x(1.0), "efficiency": 0.0, "energy_unit_conversion": -1.0 }),
                vec![NotPositive("efficiency"), NotPositive("energy_unit_conversion")],
            ),
            (
                json!({ "type": "RbfProfile", "points": [[1, 1.0], [100, 2.0]], "function": { "type": "Gaussian", "epsilon": 0.0 } }),
                vec![ZeroEpsilon],
            ),
            // Core repeats each point 365 days before and after.
            (
                json!({ "type": "RbfProfile", "points": [[10, 1.0], [10, 2.0], [375, 3.0]], "function": { "type": "Linear" } }),
                vec![
                    PointsCoincide { first: 0, second: 1 },
                    PointsCoincide { first: 0, second: 2 },
                    PointsCoincide { first: 1, second: 2 },
                ],
            ),
            (
                json!({ "type": "Aggregated", "phase": "Before", "agg_func": { "type": "AnyNonZero", "tolerance": -1.0 }, "metrics": [] }),
                vec![NegativeTolerance, NoMetrics],
            ),
            (
                json!({ "type": "AggregatedIndex", "phase": "Before", "agg_func": { "type": "Sum" }, "metrics": [] }),
                vec![NoMetrics],
            ),
            (
                json!({ "type": "Rolling", "metric": x(1.0), "window_size": 0, "initial_value": 0.0, "agg_func": { "type": "Mean" } }),
                vec![ZeroWindowSize],
            ),
            (
                json!({ "type": "RollingIndex", "metric": { "type": "Constant", "value": 1 }, "window_size": 3, "min_values": 4, "initial_value": 0, "agg_func": { "type": "Max" } }),
                vec![MinValuesAboveWindow {
                    min_values: 4,
                    window_size: 3,
                }],
            ),
            (
                json!({ "type": "Difference", "phase": "Before", "a": x(1.0), "b": x(0.0), "min": x(2.0), "max": x(1.0) }),
                vec![MinAboveMax],
            ),
            (
                json!({ "type": "DiscountFactor", "discount_rate": x(-1.0), "base_year": 2020 }),
                vec![DiscountRateTooLow],
            ),
            (
                json!({ "type": "MultiThreshold", "phase": "Before", "metric": x(1.0), "thresholds": [x(0.5)], "predicate": "GT", "returned_metrics": [x(0.0), x(1.0), x(2.0)] }),
                vec![ReturnedMetricsCount { required: 2, found: 3 }],
            ),
            (
                json!({ "type": "ControlCurveIndex", "phase": "Before", "control_curves": [], "storage_metric": x(0.5) }),
                vec![NoControlCurves],
            ),
            (
                json!({ "type": "Polynomial1D", "phase": "Before", "metric": x(1.0), "coefficients": [] }),
                vec![NoCoefficients],
            ),
        ];

        for (mut data, problems) in cases {
            data["meta"] = json!({ "name": "a-parameter" });
            let parameter: Parameter = serde_json::from_value(data.clone()).unwrap();

            let expected = if problems.is_empty() { Ok(()) } else { Err(problems) };
            assert_eq!(parameter.validate(), expected, "{data}");
        }
    }

    /// Test all the documentation examples successfully deserialize.
    #[test]
    fn test_doc_examples() {
        let mut doc_examples = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        doc_examples.push("src/parameters/doc_examples");

        for entry in fs::read_dir(doc_examples).unwrap() {
            let p = entry.unwrap().path();
            if p.is_file() {
                let data = fs::read_to_string(&p).unwrap_or_else(|_| panic!("Failed to read file: {p:?}",));

                let value: serde_json::Value =
                    serde_json::from_str(&data).unwrap_or_else(|_| panic!("Failed to deserialize: {p:?}",));

                match value {
                    serde_json::Value::Object(_) => {
                        let _ = serde_json::from_value::<Parameter>(value)
                            .unwrap_or_else(|e| panic!("Failed to deserialize `{p:?}`: {e}",));
                    }
                    serde_json::Value::Array(_) => {
                        let _ = serde_json::from_value::<Vec<Parameter>>(value)
                            .unwrap_or_else(|e| panic!("Failed to deserialize: `{p:?}`: {e}",));
                    }
                    _ => panic!("Expected JSON object or array: {p:?}",),
                }
            }
        }
    }
}
