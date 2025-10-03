//! Parameter schema definitions.
//!
//! The enum [`Parameter`] contains all of the valid Pywr parameter schemas. The parameter
//! variants define separate schemas for different parameter types. When a network is generated
//! from a schema the parameter schemas are added to the network using [`Parameter::add_to_model`].
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
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::error::{ComponentConversionError, ConversionError};
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::timeseries::ConvertedTimeseriesReference;
use crate::v1::{ConversionData, IntoV2, TryFromV1, TryIntoV2};
use crate::visit::{VisitMetrics, VisitPaths};
pub use aggregated::{AggregatedIndexParameter, AggregatedParameter};
pub use asymmetric_switch::AsymmetricSwitchIndexParameter;
pub use control_curves::{
    ControlCurveIndexParameter, ControlCurveInterpolatedParameter, ControlCurveParameter,
    ControlCurvePiecewiseInterpolatedParameter,
};
pub use core::{
    ActivationFunction, ConstantParameter, ConstantScenarioParameter, DivisionParameter, MaxParameter, MinParameter,
    NegativeMaxParameter, NegativeMinParameter, NegativeParameter, VariableSettings,
};
pub use delay::{DelayIndexParameter, DelayParameter};
pub use discount_factor::DiscountFactorParameter;
pub use hydropower::HydropowerTargetParameter;
pub use indexed_array::IndexedArrayParameter;
pub use interpolated::InterpolatedParameter;
pub use offset::OffsetParameter;
pub use placeholder::PlaceholderParameter;
pub use polynomial::Polynomial1DParameter;
pub use profiles::{
    DailyProfileParameter, DirunalProfileParameter, MonthlyInterpDay, MonthlyProfileParameter, RadialBasisFunction,
    RbfProfileParameter, RbfProfileVariableSettings, UniformDrawdownProfileParameter, WeeklyProfileParameter,
};
pub use python::{PythonObject, PythonParameter, PythonReturnType};
use pywr_schema_macros::{PywrVisitAll, skip_serializing_none};
use pywr_v1_schema::parameters::{
    CoreParameter, DataFrameParameter as DataFrameParameterV1, Parameter as ParameterV1,
    ParameterValue as ParameterValueV1, TableIndex as TableIndexV1, TableIndexEntry as TableIndexEntryV1,
};
pub use rolling::{RollingIndexParameter, RollingParameter};
use schemars::JsonSchema;
use std::path::{Path, PathBuf};
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};
pub use tables::TablesArrayParameter;
pub use thresholds::{MultiThresholdParameter, Predicate, ThresholdParameter};

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
pub struct ParameterMeta {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
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
    HydropowerTarget(HydropowerTargetParameter),
    Polynomial1D(Polynomial1DParameter),
    Threshold(ThresholdParameter),
    TablesArray(TablesArrayParameter),
    Python(PythonParameter),
    Delay(DelayParameter),
    DelayIndex(DelayIndexParameter),
    Division(DivisionParameter),
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
        match self {
            Self::Constant(p) => p.meta.name.as_str(),
            Self::ConstantScenario(p) => p.meta.name.as_str(),
            Self::ControlCurveInterpolated(p) => p.meta.name.as_str(),
            Self::Aggregated(p) => p.meta.name.as_str(),
            Self::AggregatedIndex(p) => p.meta.name.as_str(),
            Self::AsymmetricSwitchIndex(p) => p.meta.name.as_str(),
            Self::ControlCurvePiecewiseInterpolated(p) => p.meta.name.as_str(),
            Self::ControlCurveIndex(p) => p.meta.name.as_str(),
            Self::ControlCurve(p) => p.meta.name.as_str(),
            Self::DailyProfile(p) => p.meta.name.as_str(),
            Self::IndexedArray(p) => p.meta.name.as_str(),
            Self::MonthlyProfile(p) => p.meta.name.as_str(),
            Self::WeeklyProfile(p) => p.meta.name.as_str(),
            Self::UniformDrawdownProfile(p) => p.meta.name.as_str(),
            Self::Max(p) => p.meta.name.as_str(),
            Self::Min(p) => p.meta.name.as_str(),
            Self::MultiThreshold(p) => p.meta.name.as_str(),
            Self::Negative(p) => p.meta.name.as_str(),
            Self::Polynomial1D(p) => p.meta.name.as_str(),
            Self::Threshold(p) => p.meta.name.as_str(),
            Self::TablesArray(p) => p.meta.name.as_str(),
            Self::Python(p) => p.meta.name.as_str(),
            Self::Division(p) => p.meta.name.as_str(),
            Self::Delay(p) => p.meta.name.as_str(),
            Self::DelayIndex(p) => p.meta.name.as_str(),
            Self::Offset(p) => p.meta.name.as_str(),
            Self::DiscountFactor(p) => p.meta.name.as_str(),
            Self::Interpolated(p) => p.meta.name.as_str(),
            Self::HydropowerTarget(p) => p.meta.name.as_str(),
            Self::RbfProfile(p) => p.meta.name.as_str(),
            Self::NegativeMax(p) => p.meta.name.as_str(),
            Self::NegativeMin(p) => p.meta.name.as_str(),
            Self::Rolling(p) => p.meta.name.as_str(),
            Self::RollingIndex(p) => p.meta.name.as_str(),
            Self::Placeholder(p) => p.meta.name.as_str(),
            Self::DiurnalProfile(p) => p.meta.name.as_str(),
        }
    }

    pub fn parameter_type(&self) -> ParameterType {
        // Implementation provided by the `EnumDiscriminants` derive macro.
        self.into()
    }
}

#[cfg(feature = "core")]
impl Parameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<pywr_core::parameters::ParameterType, SchemaError> {
        let ty = match self {
            Self::Constant(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?)
            }
            Self::ConstantScenario(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?)
            }
            Self::ControlCurveInterpolated(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?)
            }
            Self::Aggregated(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?)
            }
            Self::AggregatedIndex(p) => {
                pywr_core::parameters::ParameterType::Index(p.add_to_model(network, args, parent)?)
            }
            Self::AsymmetricSwitchIndex(p) => {
                pywr_core::parameters::ParameterType::Index(p.add_to_model(network, args, parent)?)
            }
            Self::ControlCurvePiecewiseInterpolated(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?)
            }
            Self::ControlCurveIndex(p) => {
                pywr_core::parameters::ParameterType::Index(p.add_to_model(network, args, parent)?)
            }
            Self::ControlCurve(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?)
            }
            Self::DailyProfile(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?)
            }
            Self::IndexedArray(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?)
            }
            Self::MonthlyProfile(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?)
            }
            Self::WeeklyProfile(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?)
            }
            Self::UniformDrawdownProfile(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?)
            }
            Self::Max(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?),
            Self::Min(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?),
            Self::Negative(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?)
            }
            Self::Polynomial1D(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?)
            }
            Self::Threshold(p) => p.add_to_model(network, args, parent)?,
            Self::TablesArray(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?)
            }
            Self::Python(p) => p.add_to_model(network, args, parent)?,
            Self::Delay(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?),
            Self::DelayIndex(p) => pywr_core::parameters::ParameterType::Index(p.add_to_model(network, args, parent)?),
            Self::Division(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?)
            }
            Self::Offset(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?),
            Self::DiscountFactor(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?)
            }
            Self::Interpolated(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?)
            }
            Self::RbfProfile(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, parent)?),
            Self::NegativeMax(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?)
            }
            Self::NegativeMin(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?)
            }
            Self::HydropowerTarget(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?)
            }
            Self::Rolling(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?),
            Self::RollingIndex(p) => {
                pywr_core::parameters::ParameterType::Index(p.add_to_model(network, args, parent)?)
            }
            Self::Placeholder(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model()?),
            Self::MultiThreshold(p) => p.add_to_model(network, args, parent)?,
            Self::DiurnalProfile(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args, parent)?)
            }
        };

        Ok(ty)
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

#[derive(Clone)]
pub enum ParameterOrTimeseriesRef {
    // Boxed due to large size difference.
    Parameter(Box<Parameter>),
    Timeseries(ConvertedTimeseriesReference),
}

impl From<Parameter> for ParameterOrTimeseriesRef {
    fn from(p: Parameter) -> Self {
        Self::Parameter(Box::new(p))
    }
}

impl From<ConvertedTimeseriesReference> for ParameterOrTimeseriesRef {
    fn from(t: ConvertedTimeseriesReference) -> Self {
        Self::Timeseries(t)
    }
}

impl TryFromV1<ParameterV1> for ParameterOrTimeseriesRef {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: ParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let p: ParameterOrTimeseriesRef = match v1 {
            ParameterV1::Core(v1) => match v1 {
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
                    Parameter::UniformDrawdownProfile(p.into_v2(parent_node, conversion_data)).into()
                }
                CoreParameter::Max(p) => Parameter::Max(p.try_into_v2(parent_node, conversion_data)?).into(),
                CoreParameter::Negative(p) => Parameter::Negative(p.try_into_v2(parent_node, conversion_data)?).into(),
                CoreParameter::Polynomial1D(p) => {
                    Parameter::Polynomial1D(p.into_v2(parent_node, conversion_data)).into()
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
                    <DataFrameParameterV1 as TryIntoV2<ConvertedTimeseriesReference>>::try_into_v2(
                        p,
                        parent_node,
                        conversion_data,
                    )?
                    .into()
                }
                CoreParameter::Deficit(p) => {
                    return Err(ComponentConversionError::Parameter {
                        name: p.meta.and_then(|m| m.name).unwrap_or("unnamed".to_string()),
                        attr: "".to_string(),
                        error: ConversionError::DeprecatedParameter {
                            ty: "DeficitParameter".to_string(),
                            instead: "Use a derived metric instead.".to_string(),
                        },
                    });
                }
                CoreParameter::DiscountFactor(p) => {
                    Parameter::DiscountFactor(p.into_v2(parent_node, conversion_data)).into()
                }
                CoreParameter::InterpolatedVolume(p) => {
                    Parameter::Interpolated(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::InterpolatedFlow(p) => {
                    Parameter::Interpolated(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::HydropowerTarget(p) => {
                    Parameter::HydropowerTarget(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::WeeklyProfile(p) => {
                    Parameter::WeeklyProfile(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::Storage(p) => {
                    return Err(ComponentConversionError::Parameter {
                        name: p.meta.and_then(|m| m.name).unwrap_or("unnamed".to_string()),
                        attr: "".to_string(),
                        error: ConversionError::DeprecatedParameter {
                            ty: "StorageParameter".to_string(),
                            instead: "Use a derived metric instead.".to_string(),
                        },
                    });
                }
                CoreParameter::RollingMeanFlowNode(p) => {
                    Parameter::Rolling(p.try_into_v2(parent_node, conversion_data)?).into()
                }
                CoreParameter::ScenarioWrapper(_) => todo!("Implement ScenarioWrapperParameter"),
                CoreParameter::Flow(p) => {
                    return Err(ComponentConversionError::Parameter {
                        name: p.meta.and_then(|m| m.name).unwrap_or("unnamed".to_string()),
                        attr: "".to_string(),
                        error: ConversionError::DeprecatedParameter {
                            ty: "FlowParameter".to_string(),
                            instead: "Use a derived metric instead.".to_string(),
                        },
                    });
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
                return Err(ComponentConversionError::Parameter {
                    name: p.meta.name.unwrap_or_else(|| "unnamed".to_string()),
                    attr: "".to_string(),
                    error: ConversionError::UnrecognisedType { ty: p.ty },
                });
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

// The Derive does not work for the generic type T
mod constant_value_visit_metrics {
    use super::*;
    use crate::metric::Metric;
    use crate::visit::VisitMetrics;
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
}
mod constant_value_visit_paths {
    use super::*;
    use crate::visit::VisitPaths;
    use std::path::{Path, PathBuf};
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
    serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll, Display, EnumDiscriminants, PartialEq,
)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(ConstantFloatVecType))]
pub enum ConstantFloatVec {
    Literal { values: Vec<f64> },
    Table(TableDataRef),
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
    use crate::parameters::Parameter;
    use std::fs;
    use std::path::PathBuf;

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
