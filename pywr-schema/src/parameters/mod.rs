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
mod indexed_array;
mod interpolated;
mod offset;
mod polynomial;
mod profiles;
mod python;
mod tables;
mod thresholds;

pub use super::data_tables::{LoadedTableCollection, TableDataRef};
pub use super::parameters::aggregated::{AggFunc, AggregatedIndexParameter, AggregatedParameter, IndexAggFunc};
pub use super::parameters::asymmetric_switch::AsymmetricSwitchIndexParameter;
pub use super::parameters::control_curves::{
    ControlCurveIndexParameter, ControlCurveInterpolatedParameter, ControlCurveParameter,
    ControlCurvePiecewiseInterpolatedParameter,
};
pub use super::parameters::core::{
    ActivationFunction, ConstantParameter, MaxParameter, MinParameter, NegativeMaxParameter, NegativeMinParameter,
    NegativeParameter, VariableSettings,
};
pub use super::parameters::delay::DelayParameter;
pub use super::parameters::discount_factor::DiscountFactorParameter;
pub use super::parameters::indexed_array::IndexedArrayParameter;
pub use super::parameters::polynomial::Polynomial1DParameter;
pub use super::parameters::profiles::{
    DailyProfileParameter, MonthlyProfileParameter, RadialBasisFunction, RbfProfileParameter,
    RbfProfileVariableSettings, UniformDrawdownProfileParameter, WeeklyProfileParameter,
};
pub use super::parameters::python::{PythonModule, PythonParameter, PythonReturnType};
pub use super::parameters::tables::TablesArrayParameter;
pub use super::parameters::thresholds::ParameterThresholdParameter;
use crate::error::{ConversionError, SchemaError};
use crate::metric::Metric;
use crate::model::LoadArgs;
use crate::parameters::core::DivisionParameter;
use crate::parameters::interpolated::InterpolatedParameter;
pub use offset::OffsetParameter;
use pywr_core::metric::MetricUsize;
use pywr_core::parameters::ParameterIndex;
use pywr_v1_schema::parameters::{
    CoreParameter, DataFrameParameter as DataFrameParameterV1, ExternalDataRef as ExternalDataRefV1,
    Parameter as ParameterV1, ParameterMeta as ParameterMetaV1, ParameterValue as ParameterValueV1, ParameterVec,
    TableIndex as TableIndexV1, TableIndexEntry as TableIndexEntryV1,
};
use std::path::PathBuf;
use strum_macros::{Display, EnumDiscriminants, EnumString, IntoStaticStr, VariantNames};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct ParameterMeta {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

pub trait FromV1Parameter<T>: Sized {
    fn from_v1_parameter(v1: T, parent_node: Option<&str>, unnamed_count: &mut usize) -> Self;
}

pub trait IntoV2Parameter<T> {
    fn into_v2_parameter(self, parent_node: Option<&str>, unnamed_count: &mut usize) -> T;
}

// FromV1Parameter implies IntoV2Parameter
impl<T, U> IntoV2Parameter<U> for T
where
    U: FromV1Parameter<T>,
{
    fn into_v2_parameter(self, parent_node: Option<&str>, unnamed_count: &mut usize) -> U {
        U::from_v1_parameter(self, parent_node, unnamed_count)
    }
}

pub trait TryFromV1Parameter<T>: Sized {
    type Error;
    fn try_from_v1_parameter(v1: T, parent_node: Option<&str>, unnamed_count: &mut usize) -> Result<Self, Self::Error>;
}

pub trait TryIntoV2Parameter<T> {
    type Error;
    fn try_into_v2_parameter(self, parent_node: Option<&str>, unnamed_count: &mut usize) -> Result<T, Self::Error>;
}

// TryFromV1Parameter implies TryIntoV2Parameter
impl<T, U> TryIntoV2Parameter<U> for T
where
    U: TryFromV1Parameter<T>,
{
    type Error = U::Error;

    fn try_into_v2_parameter(self, parent_node: Option<&str>, unnamed_count: &mut usize) -> Result<U, Self::Error> {
        U::try_from_v1_parameter(self, parent_node, unnamed_count)
    }
}

impl FromV1Parameter<ParameterMetaV1> for ParameterMeta {
    fn from_v1_parameter(v1: ParameterMetaV1, parent_node: Option<&str>, unnamed_count: &mut usize) -> Self {
        Self {
            name: v1.name.unwrap_or_else(|| {
                let pname = match parent_node {
                    Some(pn) => format!("{pn}-p{unnamed_count}"),
                    None => format!("unnamed-{unnamed_count}"),
                };
                *unnamed_count += 1;
                pname
            }),
            comment: v1.comment,
        }
    }
}

impl FromV1Parameter<Option<ParameterMetaV1>> for ParameterMeta {
    fn from_v1_parameter(v1: Option<ParameterMetaV1>, parent_node: Option<&str>, unnamed_count: &mut usize) -> Self {
        match v1 {
            Some(meta) => meta.into_v2_parameter(parent_node, unnamed_count),
            None => {
                let meta = Self {
                    name: format!("unnamed-{unnamed_count}"),
                    comment: None,
                };
                *unnamed_count += 1;
                meta
            }
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, EnumDiscriminants, Clone)]
#[serde(tag = "type")]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, VariantNames))]
// This creates a separate enum called `NodeType` that is available in this module.
#[strum_discriminants(name(ParameterType))]
pub enum Parameter {
    Aggregated(AggregatedParameter),
    AggregatedIndex(AggregatedIndexParameter),
    AsymmetricSwitchIndex(AsymmetricSwitchIndexParameter),
    Constant(ConstantParameter),
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
    Negative(NegativeParameter),
    NegativeMax(NegativeMaxParameter),
    NegativeMin(NegativeMinParameter),
    Polynomial1D(Polynomial1DParameter),
    ParameterThreshold(ParameterThresholdParameter),
    TablesArray(TablesArrayParameter),
    Python(PythonParameter),
    Delay(DelayParameter),
    Division(DivisionParameter),
    Offset(OffsetParameter),
    DiscountFactor(DiscountFactorParameter),
    Interpolated(InterpolatedParameter),
    RbfProfile(RbfProfileParameter),
}

impl Parameter {
    pub fn name(&self) -> &str {
        match self {
            Self::Constant(p) => p.meta.name.as_str(),
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
            Self::Negative(p) => p.meta.name.as_str(),
            Self::Polynomial1D(p) => p.meta.name.as_str(),
            Self::ParameterThreshold(p) => p.meta.name.as_str(),
            Self::TablesArray(p) => p.meta.name.as_str(),
            Self::Python(p) => p.meta.name.as_str(),
            Self::Division(p) => p.meta.name.as_str(),
            Self::Delay(p) => p.meta.name.as_str(),
            Self::Offset(p) => p.meta.name.as_str(),
            Self::DiscountFactor(p) => p.meta.name.as_str(),
            Self::Interpolated(p) => p.meta.name.as_str(),
            Self::RbfProfile(p) => p.meta.name.as_str(),
            Self::NegativeMax(p) => p.meta.name.as_str(),
            Self::NegativeMin(p) => p.meta.name.as_str(),
        }
    }

    pub fn parameter_type(&self) -> ParameterType {
        // Implementation provided by the `EnumDiscriminants` derive macro.
        self.into()
    }

    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<pywr_core::parameters::ParameterType, SchemaError> {
        let ty = match self {
            Self::Constant(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args)?),
            Self::ControlCurveInterpolated(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args)?)
            }
            Self::Aggregated(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args)?),
            Self::AggregatedIndex(p) => pywr_core::parameters::ParameterType::Index(p.add_to_model(network, args)?),
            Self::AsymmetricSwitchIndex(p) => {
                pywr_core::parameters::ParameterType::Index(p.add_to_model(network, args)?)
            }
            Self::ControlCurvePiecewiseInterpolated(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args)?)
            }
            Self::ControlCurveIndex(p) => pywr_core::parameters::ParameterType::Index(p.add_to_model(network, args)?),
            Self::ControlCurve(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args)?),
            Self::DailyProfile(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args)?),
            Self::IndexedArray(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args)?),
            Self::MonthlyProfile(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args)?),
            Self::WeeklyProfile(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args)?),
            Self::UniformDrawdownProfile(p) => {
                pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args)?)
            }
            Self::Max(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args)?),
            Self::Min(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args)?),
            Self::Negative(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args)?),
            Self::Polynomial1D(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network)?),
            Self::ParameterThreshold(p) => pywr_core::parameters::ParameterType::Index(p.add_to_model(network, args)?),
            Self::TablesArray(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args)?),
            Self::Python(p) => p.add_to_model(network, args)?,
            Self::Delay(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args)?),
            Self::Division(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args)?),
            Self::Offset(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args)?),
            Self::DiscountFactor(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args)?),
            Self::Interpolated(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args)?),
            Self::RbfProfile(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network)?),
            Self::NegativeMax(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args)?),
            Self::NegativeMin(p) => pywr_core::parameters::ParameterType::Parameter(p.add_to_model(network, args)?),
        };

        Ok(ty)
    }
}

pub fn convert_parameter_v1_to_v2(
    v1_parameters: ParameterVec,
    unnamed_count: &mut usize,
    errors: &mut Vec<ConversionError>,
) -> (Vec<Parameter>, Vec<TimeseriesV1Data>) {
    let param_or_ts: Vec<ParameterOrTimeseries> = v1_parameters
        .into_iter()
        .filter_map(|p| match p.try_into_v2_parameter(None, unnamed_count) {
            Ok(pt) => Some(pt),
            Err(e) => {
                errors.push(e);
                None
            }
        })
        .collect::<Vec<_>>();

    let parameters = param_or_ts
        .clone()
        .into_iter()
        .filter_map(|pot| match pot {
            ParameterOrTimeseries::Parameter(p) => Some(p),
            ParameterOrTimeseries::Timeseries(_) => None,
        })
        .collect();

    let timeseries = param_or_ts
        .into_iter()
        .filter_map(|pot| match pot {
            ParameterOrTimeseries::Parameter(_) => None,
            ParameterOrTimeseries::Timeseries(t) => Some(t),
        })
        .collect();

    (parameters, timeseries)
}

#[derive(Clone)]
pub enum ParameterOrTimeseries {
    Parameter(Parameter),
    Timeseries(TimeseriesV1Data),
}

#[derive(Clone, Debug)]
pub struct TimeseriesV1Data {
    pub name: Option<String>,
    pub source: TimeseriesV1Source,
    pub time_col: Option<String>,
    pub column: Option<String>,
    pub scenario: Option<String>,
}

impl From<DataFrameParameterV1> for TimeseriesV1Data {
    fn from(p: DataFrameParameterV1) -> Self {
        let source = if let Some(url) = p.url {
            TimeseriesV1Source::Url(url)
        } else if let Some(tbl) = p.table {
            TimeseriesV1Source::Table(tbl)
        } else {
            panic!("DataFrameParameter must have a url or table attribute.")
        };

        let name = p.meta.and_then(|m| m.name);
        let time_col = match p.pandas_kwargs.get("index_col") {
            Some(v) => v.as_str().map(|s| s.to_string()),
            None => None,
        };

        Self {
            name,
            source,
            time_col,
            column: p.column,
            scenario: p.scenario,
        }
    }
}

#[derive(Clone, Debug)]
pub enum TimeseriesV1Source {
    Url(PathBuf),
    Table(String),
}

impl From<Parameter> for ParameterOrTimeseries {
    fn from(p: Parameter) -> Self {
        Self::Parameter(p)
    }
}

impl From<TimeseriesV1Data> for ParameterOrTimeseries {
    fn from(t: TimeseriesV1Data) -> Self {
        Self::Timeseries(t)
    }
}

impl TryFromV1Parameter<ParameterV1> for ParameterOrTimeseries {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: ParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let p: ParameterOrTimeseries = match v1 {
            ParameterV1::Core(v1) => match v1 {
                CoreParameter::Aggregated(p) => {
                    Parameter::Aggregated(p.try_into_v2_parameter(parent_node, unnamed_count)?).into()
                }
                CoreParameter::AggregatedIndex(p) => {
                    Parameter::AggregatedIndex(p.try_into_v2_parameter(parent_node, unnamed_count)?).into()
                }
                CoreParameter::AsymmetricSwitchIndex(p) => {
                    Parameter::AsymmetricSwitchIndex(p.try_into_v2_parameter(parent_node, unnamed_count)?).into()
                }
                CoreParameter::Constant(p) => {
                    Parameter::Constant(p.try_into_v2_parameter(parent_node, unnamed_count)?).into()
                }
                CoreParameter::ControlCurvePiecewiseInterpolated(p) => {
                    Parameter::ControlCurvePiecewiseInterpolated(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                        .into()
                }
                CoreParameter::ControlCurveInterpolated(p) => {
                    Parameter::ControlCurveInterpolated(p.try_into_v2_parameter(parent_node, unnamed_count)?).into()
                }
                CoreParameter::ControlCurveIndex(p) => {
                    Parameter::ControlCurveIndex(p.try_into_v2_parameter(parent_node, unnamed_count)?).into()
                }
                CoreParameter::ControlCurve(p) => match p.clone().try_into_v2_parameter(parent_node, unnamed_count) {
                    Ok(p) => Parameter::ControlCurve(p).into(),
                    Err(_) => Parameter::ControlCurveIndex(p.try_into_v2_parameter(parent_node, unnamed_count)?).into(),
                },
                CoreParameter::DailyProfile(p) => {
                    Parameter::DailyProfile(p.try_into_v2_parameter(parent_node, unnamed_count)?).into()
                }
                CoreParameter::IndexedArray(p) => {
                    Parameter::IndexedArray(p.try_into_v2_parameter(parent_node, unnamed_count)?).into()
                }
                CoreParameter::MonthlyProfile(p) => {
                    Parameter::MonthlyProfile(p.try_into_v2_parameter(parent_node, unnamed_count)?).into()
                }
                CoreParameter::UniformDrawdownProfile(p) => {
                    Parameter::UniformDrawdownProfile(p.try_into_v2_parameter(parent_node, unnamed_count)?).into()
                }
                CoreParameter::Max(p) => Parameter::Max(p.try_into_v2_parameter(parent_node, unnamed_count)?).into(),
                CoreParameter::Negative(p) => {
                    Parameter::Negative(p.try_into_v2_parameter(parent_node, unnamed_count)?).into()
                }
                CoreParameter::Polynomial1D(p) => {
                    Parameter::Polynomial1D(p.try_into_v2_parameter(parent_node, unnamed_count)?).into()
                }
                CoreParameter::ParameterThreshold(p) => {
                    Parameter::ParameterThreshold(p.try_into_v2_parameter(parent_node, unnamed_count)?).into()
                }
                CoreParameter::TablesArray(p) => {
                    Parameter::TablesArray(p.try_into_v2_parameter(parent_node, unnamed_count)?).into()
                }
                CoreParameter::Min(p) => Parameter::Min(p.try_into_v2_parameter(parent_node, unnamed_count)?).into(),
                CoreParameter::Division(p) => {
                    Parameter::Division(p.try_into_v2_parameter(parent_node, unnamed_count)?).into()
                }
                CoreParameter::DataFrame(p) => {
                    let ts_data: TimeseriesV1Data = p.into();
                    ts_data.into()
                }
                CoreParameter::Deficit(p) => {
                    return Err(ConversionError::DeprecatedParameter {
                        ty: "DeficitParameter".to_string(),
                        name: p.meta.and_then(|m| m.name).unwrap_or("unnamed".to_string()),
                        instead: "Use a derived metric instead.".to_string(),
                    })
                }
                CoreParameter::DiscountFactor(p) => {
                    Parameter::DiscountFactor(p.try_into_v2_parameter(parent_node, unnamed_count)?).into()
                }
                CoreParameter::InterpolatedVolume(p) => {
                    Parameter::Interpolated(p.try_into_v2_parameter(parent_node, unnamed_count)?).into()
                }
                CoreParameter::InterpolatedFlow(p) => {
                    Parameter::Interpolated(p.try_into_v2_parameter(parent_node, unnamed_count)?).into()
                }
                CoreParameter::HydropowerTarget(_) => todo!("Implement HydropowerTargetParameter"),
                CoreParameter::WeeklyProfile(p) => {
                    Parameter::WeeklyProfile(p.try_into_v2_parameter(parent_node, unnamed_count)?).into()
                }
                CoreParameter::Storage(p) => {
                    return Err(ConversionError::DeprecatedParameter {
                        ty: "StorageParameter".to_string(),
                        name: p.meta.and_then(|m| m.name).unwrap_or("unnamed".to_string()),
                        instead: "Use a derived metric instead.".to_string(),
                    })
                }
                CoreParameter::RollingMeanFlowNode(_) => todo!("Implement RollingMeanFlowNodeParameter"),
                CoreParameter::ScenarioWrapper(_) => todo!("Implement ScenarioWrapperParameter"),
                CoreParameter::Flow(p) => {
                    return Err(ConversionError::DeprecatedParameter {
                        ty: "FlowParameter".to_string(),
                        name: p.meta.and_then(|m| m.name).unwrap_or("unnamed".to_string()),
                        instead: "Use a derived metric instead.".to_string(),
                    })
                }
                CoreParameter::RbfProfile(p) => {
                    Parameter::RbfProfile(p.try_into_v2_parameter(parent_node, unnamed_count)?).into()
                }
                CoreParameter::NegativeMax(p) => {
                    Parameter::NegativeMax(p.try_into_v2_parameter(parent_node, unnamed_count)?).into()
                }
                CoreParameter::NegativeMin(p) => {
                    Parameter::NegativeMin(p.try_into_v2_parameter(parent_node, unnamed_count)?).into()
                }
            },
            ParameterV1::Custom(p) => {
                println!("Custom parameter: {:?} ({})", p.meta.name, p.ty);
                // TODO do something better with custom parameters

                let mut comment = format!("V1 CUSTOM PARAMETER ({}) UNCONVERTED!", p.ty);
                if let Some(c) = p.meta.comment {
                    comment.push_str(" ORIGINAL COMMENT: ");
                    comment.push_str(c.as_str());
                }

                Parameter::Constant(ConstantParameter {
                    meta: ParameterMeta {
                        name: p.meta.name.unwrap_or_else(|| "unnamed-custom-parameter".to_string()),
                        comment: Some(comment),
                    },
                    value: ConstantValue::Literal(0.0),
                })
                .into()
            }
        };

        Ok(p)
    }
}

/// An non-variable constant floating-point (f64) value
///
/// This value can be a literal float or an external reference to an input table.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum ConstantValue<T> {
    Literal(T),
    Table(TableDataRef),
    External(ExternalDataRef),
}

impl Default for ConstantValue<f64> {
    fn default() -> Self {
        Self::Literal(0.0)
    }
}

impl ConstantValue<f64> {
    /// Return the value loading from a table if required.
    pub fn load(&self, tables: &LoadedTableCollection) -> Result<f64, SchemaError> {
        match self {
            Self::Literal(v) => Ok(*v),
            Self::Table(tbl_ref) => Ok(tables.get_scalar_f64(tbl_ref)?),
            Self::External(_) => todo!("Load the float from the external source!"),
        }
    }
}

impl ConstantValue<usize> {
    /// Return the value loading from a table if required.
    pub fn load(&self, tables: &LoadedTableCollection) -> Result<usize, SchemaError> {
        match self {
            Self::Literal(v) => Ok(*v),
            Self::Table(tbl_ref) => Ok(tables.get_scalar_usize(tbl_ref)?),
            Self::External(_) => todo!("Load the float from the external source!"),
        }
    }
}

impl TryFrom<ParameterValueV1> for ConstantValue<f64> {
    type Error = ConversionError;

    fn try_from(v1: ParameterValueV1) -> Result<Self, Self::Error> {
        match v1 {
            ParameterValueV1::Constant(v) => Ok(Self::Literal(v)),
            ParameterValueV1::Reference(_) => Err(ConversionError::ConstantFloatReferencesParameter),
            ParameterValueV1::Table(tbl) => Ok(Self::Table(tbl.try_into()?)),
            ParameterValueV1::Inline(_) => Err(ConversionError::ConstantFloatInlineParameter),
        }
    }
}

/// An integer (i64) value from another parameter
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum ParameterIndexValue {
    Reference(String),
    Inline(Box<Parameter>),
}

impl ParameterIndexValue {
    pub fn load(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<ParameterIndex<usize>, SchemaError> {
        match self {
            Self::Reference(name) => {
                // This should be an existing parameter
                Ok(network.get_index_parameter_index_by_name(name)?)
            }
            Self::Inline(parameter) => {
                // Inline parameter needs to be added
                match parameter.add_to_model(network, args)? {
                    pywr_core::parameters::ParameterType::Index(idx) => Ok(idx),
                    pywr_core::parameters::ParameterType::Parameter(_) => Err(SchemaError::UnexpectedParameterType(format!(
                        "Found float parameter of type '{}' with name '{}' where an index parameter was expected.",
                        parameter.parameter_type(),
                        parameter.name(),
                    ))),
                    pywr_core::parameters::ParameterType::Multi(_) => Err(SchemaError::UnexpectedParameterType(format!(
                        "Found an inline definition of a multi valued parameter of type '{}' with name '{}' where an index parameter was expected. Multi valued parameters cannot be defined inline.",
                        parameter.parameter_type(),
                        parameter.name(),
                    ))),
                }
            }
        }
    }
}

/// A potentially dynamic integer (usize) value
///
/// This value can be a constant (literal or otherwise) or a dynamic value provided
/// by another parameter.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum DynamicIndexValue {
    Constant(ConstantValue<usize>),
    Dynamic(ParameterIndexValue),
}

impl DynamicIndexValue {
    pub fn from_usize(v: usize) -> Self {
        Self::Constant(ConstantValue::Literal(v))
    }

    ///
    pub fn load(&self, network: &mut pywr_core::network::Network, args: &LoadArgs) -> Result<MetricUsize, SchemaError> {
        let parameter_ref = match self {
            DynamicIndexValue::Constant(v) => MetricUsize::Constant(v.load(args.tables)?),
            DynamicIndexValue::Dynamic(v) => MetricUsize::IndexParameterValue(v.load(network, args)?),
        };
        Ok(parameter_ref)
    }
}

impl TryFromV1Parameter<ParameterValueV1> for DynamicIndexValue {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: ParameterValueV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let p = match v1 {
            // There was no such thing as s constant index in Pywr v1
            // TODO this could print a warning and do a cast to usize instead.
            ParameterValueV1::Constant(_) => return Err(ConversionError::FloatToIndex),
            ParameterValueV1::Reference(p_name) => Self::Dynamic(ParameterIndexValue::Reference(p_name)),
            ParameterValueV1::Table(tbl) => Self::Constant(ConstantValue::Table(tbl.try_into()?)),
            ParameterValueV1::Inline(param) => {
                let definition: ParameterOrTimeseries = (*param).try_into_v2_parameter(parent_node, unnamed_count)?;
                match definition {
                    ParameterOrTimeseries::Parameter(p) => Self::Dynamic(ParameterIndexValue::Inline(Box::new(p))),
                    ParameterOrTimeseries::Timeseries(_) => {
                        // TODO create an error for this
                        panic!("Timeseries do not support indexes yet")
                    }
                }
            }
        };
        Ok(p)
    }
}

/// An non-variable vector of constant floating-point (f64) values
///
/// This value can be a literal vector of floats or an external reference to an input table.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum ConstantFloatVec {
    Literal(Vec<f64>),
    Table(TableDataRef),
    External(ExternalDataRef),
}

impl ConstantFloatVec {
    /// Return the value loading from a table if required.
    pub fn load(&self, tables: &LoadedTableCollection) -> Result<Vec<f64>, SchemaError> {
        match self {
            Self::Literal(v) => Ok(v.clone()),
            Self::Table(tbl_ref) => Ok(tables.get_vec_f64(tbl_ref)?.clone()),
            Self::External(_) => todo!("Load the float vector from the external source!"),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct ExternalDataRef {
    url: PathBuf,
    column: Option<TableIndex>,
    index: Option<TableIndex>,
}

impl TryFrom<ExternalDataRefV1> for ExternalDataRef {
    type Error = ConversionError;
    fn try_from(v1: ExternalDataRefV1) -> Result<Self, Self::Error> {
        let column = match v1.column {
            None => None,
            Some(c) => Some(c.try_into()?),
        };
        let index = match v1.index {
            None => None,
            Some(i) => Some(i.try_into()?),
        };
        Ok(Self {
            url: v1.url,
            column,
            index,
        })
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum TableIndex {
    Single(String),
    Multi(Vec<String>),
}

impl TryFrom<TableIndexV1> for TableIndex {
    type Error = ConversionError;

    fn try_from(v1: TableIndexV1) -> Result<Self, Self::Error> {
        match v1 {
            TableIndexV1::Single(s) => match s {
                TableIndexEntryV1::Name(s) => Ok(TableIndex::Single(s)),
                TableIndexEntryV1::Index(_) => Err(ConversionError::IntegerTableIndicesNotSupported),
            },
            TableIndexV1::Multi(s) => {
                let names = s
                    .into_iter()
                    .map(|e| match e {
                        TableIndexEntryV1::Name(s) => Ok(s),
                        TableIndexEntryV1::Index(_) => Err(ConversionError::IntegerTableIndicesNotSupported),
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

    /// Test all of the documentation examples successfully deserialize.
    #[test]
    fn test_doc_examples() {
        let mut doc_examples = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        doc_examples.push("src/parameters/doc_examples");

        for entry in fs::read_dir(doc_examples).unwrap() {
            let p = entry.unwrap().path();
            if p.is_file() {
                let data = fs::read_to_string(p).unwrap();
                let _: Parameter = serde_json::from_str(&data).unwrap();
            }
        }
    }
}
