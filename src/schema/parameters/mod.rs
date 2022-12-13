mod aggregated;
mod asymmetric_switch;
mod control_curves;
mod core;
mod indexed_array;
mod polynomial;
mod profiles;
mod tables;
mod thresholds;

use super::parameters::aggregated::{AggregatedIndexParameter, AggregatedParameter};
use super::parameters::asymmetric_switch::AsymmetricSwitchIndexParameter;
use super::parameters::control_curves::{
    ControlCurveIndexParameter, ControlCurveInterpolatedParameter, ControlCurveParameter,
    ControlCurvePiecewiseInterpolatedParameter,
};
pub use super::parameters::core::{ConstantParameter, MaxParameter, NegativeParameter};
use super::parameters::indexed_array::IndexedArrayParameter;
use super::parameters::polynomial::Polynomial1DParameter;
use super::parameters::profiles::{DailyProfileParameter, MonthlyProfileParameter};
use super::parameters::tables::TablesArrayParameter;
use super::parameters::thresholds::ParameterThresholdParameter;

use crate::parameters::{FloatValue, IndexValue, ParameterType};
use crate::schema::data_tables::{LoadedTableCollection, TableDataRef};
use crate::{IndexParameterIndex, ParameterIndex, PywrError};
use pywr_schema::parameters::{
    CoreParameter, ExternalDataRef as ExternalDataRefV1, Parameter as ParameterV1, ParameterMeta as ParameterMetaV1,
    ParameterValue as ParameterValueV1, TableIndex as TableIndexV1,
};
use std::collections::HashMap;
use std::path::Path;

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
                    Some(pn) => format!("{}-p{}", pn, unnamed_count),
                    None => format!("unnamed-{}", unnamed_count),
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
                    name: format!("unnamed-{}", unnamed_count),
                    comment: None,
                };
                *unnamed_count += 1;
                meta
            }
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(tag = "type")]
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
    Max(MaxParameter),
    Negative(NegativeParameter),
    Polynomial1D(Polynomial1DParameter),
    ParameterThreshold(ParameterThresholdParameter),
    TablesArray(TablesArrayParameter),
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
            Self::Max(p) => p.meta.name.as_str(),
            Self::Negative(p) => p.meta.name.as_str(),
            Self::Polynomial1D(p) => p.meta.name.as_str(),
            Self::ParameterThreshold(p) => p.meta.name.as_str(),
            Self::TablesArray(p) => p.meta.name.as_str(),
        }
    }

    fn node_references(&self) -> HashMap<&str, &str> {
        match self {
            Self::Constant(p) => p.node_references(),
            Self::ControlCurveInterpolated(p) => p.node_references(),
            Self::Aggregated(p) => p.node_references(),
            Self::AggregatedIndex(p) => p.node_references(),
            Self::AsymmetricSwitchIndex(p) => p.node_references(),
            Self::ControlCurvePiecewiseInterpolated(p) => p.node_references(),
            Self::ControlCurveIndex(p) => p.node_references(),
            Self::ControlCurve(p) => p.node_references(),
            Self::DailyProfile(p) => p.node_references(),
            Self::IndexedArray(p) => p.node_references(),
            Self::MonthlyProfile(p) => p.node_references(),
            Self::Max(p) => p.node_references(),
            Self::Negative(p) => p.node_references(),
            Self::Polynomial1D(p) => p.node_references(),
            Self::ParameterThreshold(p) => p.node_references(),
            Self::TablesArray(p) => p.node_references(),
        }
    }

    // pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
    //     match self {
    //         Self::Constant(p) => p.parameters(),
    //         Self::ControlCurveInterpolated(p) => p.parameters(),
    //         Self::Aggregated(p) => p.parameters(),
    //         Self::AggregatedIndex(p) => p.parameters(),
    //         Self::ControlCurvePiecewiseInterpolated(p) => p.parameters(),
    //         Self::ControlCurveIndex(p) => p.parameters(),
    //         Self::ControlCurve(p) => p.parameters(),
    //         Self::DailyProfile(p) => p.parameters(),
    //         Self::IndexedArray(p) => p.parameters(),
    //         Self::MonthlyProfile(p) => p.parameters(),
    //         Self::Max(p) => p.parameters(),
    //         Self::Negative(p) => p.parameters(),
    //         Self::Polynomial1D(p) => p.parameters(),
    //         Self::TablesArray(p) => p.parameters(),
    //     }
    // }

    pub fn ty(&self) -> &str {
        match self {
            Self::Constant(_) => "Constant",
            Self::ControlCurveInterpolated(_) => "ControlCurveInterpolated",
            Self::Aggregated(_) => "Aggregated",
            Self::AggregatedIndex(_) => "AggregatedIndex",
            Self::AsymmetricSwitchIndex(_) => "AsymmetricSwitchIndex",
            Self::ControlCurvePiecewiseInterpolated(_) => "ControlCurvePiecewiseInterpolated",
            Self::ControlCurveIndex(_) => "ControlCurveIndex",
            Self::ControlCurve(_) => "ControlCurve",
            Self::DailyProfile(_) => "DailyProfile",
            Self::IndexedArray(_) => "IndexedArray",
            Self::MonthlyProfile(_) => "MonthlyProfile",
            Self::Max(_) => "Max",
            Self::Negative(_) => "Negative",
            Self::Polynomial1D(_) => "Polynomial1D",
            Self::ParameterThreshold(_) => "ParameterThreshold",
            Self::TablesArray(_) => "TablesArray",
        }
    }

    pub fn add_to_model(
        &self,
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<ParameterType, PywrError> {
        let ty = match self {
            Self::Constant(p) => ParameterType::Parameter(p.add_to_model(model, tables)?),
            Self::ControlCurveInterpolated(p) => ParameterType::Parameter(p.add_to_model(model, tables, data_path)?),
            Self::Aggregated(p) => ParameterType::Parameter(p.add_to_model(model, tables, data_path)?),
            Self::AggregatedIndex(p) => ParameterType::Index(p.add_to_model(model, tables, data_path)?),
            Self::AsymmetricSwitchIndex(p) => ParameterType::Index(p.add_to_model(model, tables, data_path)?),
            Self::ControlCurvePiecewiseInterpolated(p) => {
                ParameterType::Parameter(p.add_to_model(model, tables, data_path)?)
            }
            Self::ControlCurveIndex(p) => ParameterType::Index(p.add_to_model(model, tables, data_path)?),
            Self::ControlCurve(p) => ParameterType::Parameter(p.add_to_model(model, tables, data_path)?),
            Self::DailyProfile(p) => ParameterType::Parameter(p.add_to_model(model, tables)?),
            Self::IndexedArray(p) => ParameterType::Parameter(p.add_to_model(model, tables, data_path)?),
            Self::MonthlyProfile(p) => ParameterType::Parameter(p.add_to_model(model, tables)?),
            Self::Max(p) => ParameterType::Parameter(p.add_to_model(model, tables, data_path)?),
            Self::Negative(p) => ParameterType::Parameter(p.add_to_model(model, tables, data_path)?),
            Self::Polynomial1D(p) => ParameterType::Parameter(p.add_to_model(model)?),
            Self::ParameterThreshold(p) => ParameterType::Index(p.add_to_model(model, tables, data_path)?),
            Self::TablesArray(p) => ParameterType::Parameter(p.add_to_model(model, data_path)?),
        };

        Ok(ty)
    }
}

impl TryFromV1Parameter<ParameterV1> for Parameter {
    type Error = PywrError;

    fn try_from_v1_parameter(
        v1: ParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let p = match v1 {
            ParameterV1::Core(v1) => match v1 {
                CoreParameter::Aggregated(p) => {
                    Parameter::Aggregated(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::AggregatedIndex(p) => {
                    Parameter::AggregatedIndex(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::AsymmetricSwitchIndex(p) => {
                    Parameter::AsymmetricSwitchIndex(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::Constant(p) => Parameter::Constant(p.try_into_v2_parameter(parent_node, unnamed_count)?),
                CoreParameter::ControlCurvePiecewiseInterpolated(p) => {
                    Parameter::ControlCurvePiecewiseInterpolated(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::ControlCurveInterpolated(p) => {
                    Parameter::ControlCurveInterpolated(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::ControlCurveIndex(p) => {
                    Parameter::ControlCurveIndex(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::ControlCurve(p) => {
                    Parameter::ControlCurve(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::DailyProfile(p) => {
                    Parameter::DailyProfile(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::IndexedArray(p) => {
                    Parameter::IndexedArray(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::MonthlyProfile(p) => {
                    Parameter::MonthlyProfile(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::Max(p) => Parameter::Max(p.try_into_v2_parameter(parent_node, unnamed_count)?),
                CoreParameter::Negative(p) => Parameter::Negative(p.try_into_v2_parameter(parent_node, unnamed_count)?),
                CoreParameter::Polynomial1D(p) => {
                    Parameter::Polynomial1D(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::ParameterThreshold(p) => {
                    Parameter::ParameterThreshold(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::TablesArray(p) => {
                    Parameter::TablesArray(p.try_into_v2_parameter(parent_node, unnamed_count)?)
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

impl ConstantValue<f64> {
    /// Return the value loading from a table if required.
    pub fn load(&self, tables: &LoadedTableCollection) -> Result<f64, PywrError> {
        match self {
            Self::Literal(v) => Ok(*v),
            Self::Table(tbl_ref) => Ok(tables.get_scalar_f64(tbl_ref)?),
            Self::External(_) => todo!("Load the float from the external source!"),
        }
    }
}

impl ConstantValue<usize> {
    /// Return the value loading from a table if required.
    pub fn load(&self, tables: &LoadedTableCollection) -> Result<usize, PywrError> {
        match self {
            Self::Literal(v) => Ok(*v),
            Self::Table(tbl_ref) => Ok(tables.get_scalar_usize(tbl_ref)?),
            Self::External(_) => todo!("Load the float from the external source!"),
        }
    }
}

impl TryFrom<ParameterValueV1> for ConstantValue<f64> {
    type Error = PywrError;

    fn try_from(v1: ParameterValueV1) -> Result<Self, Self::Error> {
        match v1 {
            ParameterValueV1::Constant(v) => Ok(Self::Literal(v)),
            ParameterValueV1::Reference(_) => Err(PywrError::V1SchemaConversion(
                "Constant float value cannot be a parameter reference.".to_string(),
            )),
            ParameterValueV1::Table(tbl) => Ok(Self::Table(tbl.into())),
            ParameterValueV1::Inline(_) => Err(PywrError::V1SchemaConversion(
                "Constant float value cannot be an inline parameter.".to_string(),
            )),
        }
    }
}

/// A floating-point(f64) value from another parameter
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum ParameterFloatValue {
    Reference(String),
    Inline(Box<Parameter>),
}

impl ParameterFloatValue {
    pub fn load(
        &self,
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<ParameterIndex, PywrError> {
        match self {
            ParameterFloatValue::Reference(name) => {
                // This should be an existing parameter
                model.get_parameter_index_by_name(name)
            }
            ParameterFloatValue::Inline(parameter) => {
                // Inline parameter needs to be added
                match parameter.add_to_model(model, tables, data_path)? {
                    ParameterType::Parameter(idx) => Ok(idx),
                    ParameterType::Index(_) => Err(PywrError::UnexpectedParameterType(format!(
                        "Found index parameter of type '{}' with name '{}' where an float parameter was expected.",
                        parameter.ty(),
                        parameter.name(),
                    ))),
                }
            }
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
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<IndexParameterIndex, PywrError> {
        match self {
            Self::Reference(name) => {
                // This should be an existing parameter
                model.get_index_parameter_index_by_name(name)
            }
            Self::Inline(parameter) => {
                // Inline parameter needs to be added
                match parameter.add_to_model(model, tables, data_path)? {
                    ParameterType::Index(idx) => Ok(idx),
                    ParameterType::Parameter(_) => Err(PywrError::UnexpectedParameterType(format!(
                        "Found float parameter of type '{}' with name '{}' where an index parameter was expected.",
                        parameter.ty(),
                        parameter.name(),
                    ))),
                }
            }
        }
    }
}

/// A potentially dynamic floating-point (f64) value
///
/// This value can be a constant (literal or otherwise) or a dynamic value provided
/// by another parameter.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum DynamicFloatValue {
    Constant(ConstantValue<f64>),
    Dynamic(ParameterFloatValue),
}

impl DynamicFloatValue {
    pub fn from_f64(v: f64) -> Self {
        Self::Constant(ConstantValue::Literal(v))
    }

    pub fn load(
        &self,
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<FloatValue, PywrError> {
        let parameter_ref = match self {
            DynamicFloatValue::Constant(v) => FloatValue::Constant(v.load(tables)?),
            DynamicFloatValue::Dynamic(v) => FloatValue::Dynamic(v.load(model, tables, data_path)?),
        };
        Ok(parameter_ref)
    }
}

impl TryFromV1Parameter<ParameterValueV1> for DynamicFloatValue {
    type Error = PywrError;

    fn try_from_v1_parameter(
        v1: ParameterValueV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let p = match v1 {
            ParameterValueV1::Constant(v) => Self::Constant(ConstantValue::Literal(v)),
            ParameterValueV1::Reference(p_name) => Self::Dynamic(ParameterFloatValue::Reference(p_name)),
            ParameterValueV1::Table(tbl) => Self::Constant(ConstantValue::Table(tbl.into())),
            ParameterValueV1::Inline(param) => Self::Dynamic(ParameterFloatValue::Inline(Box::new(
                (*param).try_into_v2_parameter(parent_node, unnamed_count)?,
            ))),
        };
        Ok(p)
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
    pub fn load(
        &self,
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<IndexValue, PywrError> {
        let parameter_ref = match self {
            DynamicIndexValue::Constant(v) => IndexValue::Constant(v.load(tables)?),
            DynamicIndexValue::Dynamic(v) => IndexValue::Dynamic(v.load(model, tables, data_path)?),
        };
        Ok(parameter_ref)
    }
}

impl TryFromV1Parameter<ParameterValueV1> for DynamicIndexValue {
    type Error = PywrError;

    fn try_from_v1_parameter(
        v1: ParameterValueV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let p = match v1 {
            // There was no such thing as s constant index in Pywr v1
            // TODO this could print a warning and do a cast to usize instead.
            ParameterValueV1::Constant(_) => {
                return Err(PywrError::V1SchemaConversion(
                    "Not possible to convert a float constant to an index constant".to_string(),
                ))
            }
            ParameterValueV1::Reference(p_name) => Self::Dynamic(ParameterIndexValue::Reference(p_name)),
            ParameterValueV1::Table(tbl) => Self::Constant(ConstantValue::Table(tbl.into())),
            ParameterValueV1::Inline(param) => Self::Dynamic(ParameterIndexValue::Inline(Box::new(
                (*param).try_into_v2_parameter(parent_node, unnamed_count)?,
            ))),
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
    pub fn load(&self, tables: &LoadedTableCollection) -> Result<Vec<f64>, PywrError> {
        match self {
            Self::Literal(v) => Ok(v.clone()),
            Self::Table(tbl_ref) => Ok(tables.get_vec_f64(tbl_ref)?.clone()),
            Self::External(_) => todo!("Load the float vector from the external source!"),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct ExternalDataRef {
    url: String,
    column: Option<TableIndex>,
    index: Option<TableIndex>,
}

impl From<ExternalDataRefV1> for ExternalDataRef {
    fn from(v1: ExternalDataRefV1) -> Self {
        Self {
            url: v1.url,
            column: v1.column.map(|i| i.into()),
            index: v1.index.map(|i| i.into()),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum TableIndex {
    Single(String),
    Multi(Vec<String>),
}

impl From<TableIndexV1> for TableIndex {
    fn from(v1: TableIndexV1) -> Self {
        match v1 {
            TableIndexV1::Single(s) => Self::Single(s),
            TableIndexV1::Multi(s) => Self::Multi(s),
        }
    }
}

pub enum DynamicFloatValueType<'a> {
    Single(&'a DynamicFloatValue),
    List(&'a Vec<DynamicFloatValue>),
}

impl<'a> From<&'a DynamicFloatValue> for DynamicFloatValueType<'a> {
    fn from(v: &'a DynamicFloatValue) -> Self {
        Self::Single(v)
    }
}

impl<'a> From<&'a Vec<DynamicFloatValue>> for DynamicFloatValueType<'a> {
    fn from(v: &'a Vec<DynamicFloatValue>) -> Self {
        Self::List(v)
    }
}
