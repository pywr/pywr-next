mod aggregated;
mod control_curves;
mod core;
mod indexed_array;
mod polynomial;
mod profiles;
mod tables;

use super::parameters::aggregated::{AggregatedIndexParameter, AggregatedParameter};
use super::parameters::control_curves::{
    ControlCurveIndexParameter, ControlCurveInterpolatedParameter, ControlCurveParameter,
    ControlCurvePiecewiseInterpolatedParameter,
};
pub use super::parameters::core::{ConstantParameter, MaxParameter, NegativeParameter};
use super::parameters::indexed_array::IndexedArrayParameter;
use super::parameters::polynomial::Polynomial1DParameter;
use super::parameters::profiles::{DailyProfileParameter, MonthlyProfileParameter};
use super::parameters::tables::TablesArrayParameter;

use std::collections::HashMap;

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct ParameterMeta {
    pub name: String,
    pub comment: Option<String>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(tag = "type")]
pub enum Parameter {
    Aggregated(AggregatedParameter),
    AggregatedIndex(AggregatedIndexParameter),
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
    TablesArray(TablesArrayParameter),
}

impl Parameter {
    pub fn name(&self) -> &str {
        match self {
            Self::Constant(p) => p.meta.name.as_str(),
            Self::ControlCurveInterpolated(p) => p.meta.name.as_str(),
            Self::Aggregated(p) => p.meta.name.as_str(),
            Self::AggregatedIndex(p) => p.meta.name.as_str(),
            Self::ControlCurvePiecewiseInterpolated(p) => p.meta.name.as_str(),
            Self::ControlCurveIndex(p) => p.meta.name.as_str(),
            Self::ControlCurve(p) => p.meta.name.as_str(),
            Self::DailyProfile(p) => p.meta.name.as_str(),
            Self::IndexedArray(p) => p.meta.name.as_str(),
            Self::MonthlyProfile(p) => p.meta.name.as_str(),
            Self::Max(p) => p.meta.name.as_str(),
            Self::Negative(p) => p.meta.name.as_str(),
            Self::Polynomial1D(p) => p.meta.name.as_str(),
            Self::TablesArray(p) => p.meta.name.as_str(),
        }
    }

    fn node_references(&self) -> HashMap<&str, &str> {
        match self {
            Self::Constant(p) => p.node_references(),
            Self::ControlCurveInterpolated(p) => p.node_references(),
            Self::Aggregated(p) => p.node_references(),
            Self::AggregatedIndex(p) => p.node_references(),
            Self::ControlCurvePiecewiseInterpolated(p) => p.node_references(),
            Self::ControlCurveIndex(p) => p.node_references(),
            Self::ControlCurve(p) => p.node_references(),
            Self::DailyProfile(p) => p.node_references(),
            Self::IndexedArray(p) => p.node_references(),
            Self::MonthlyProfile(p) => p.node_references(),
            Self::Max(p) => p.node_references(),
            Self::Negative(p) => p.node_references(),
            Self::Polynomial1D(p) => p.node_references(),
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
            Self::ControlCurvePiecewiseInterpolated(_) => "ControlCurvePiecewiseInterpolated",
            Self::ControlCurveIndex(_) => "ControlCurveIndex",
            Self::ControlCurve(_) => "ControlCurve",
            Self::DailyProfile(_) => "DailyProfile",
            Self::IndexedArray(_) => "IndexedArray",
            Self::MonthlyProfile(_) => "MonthlyProfile",
            Self::Max(_) => "Max",
            Self::Negative(_) => "Negative",
            Self::Polynomial1D(_) => "Polynomial1D",
            Self::TablesArray(_) => "TablesArray",
        }
    }
}

/// An non-variable constant floating-point (f64) value
///
/// This value can be a literal float or an external reference to an input table.
#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(untagged)]
pub enum ConstantFloatValue {
    Literal(f64),
    Table(TableDataRef),
}

impl ConstantFloatValue {
    /// Return the value loading from a table if required.
    pub fn load(&self) -> f64 {
        match self {
            Self::Literal(v) => *v,
            Self::Table(_) => {
                todo!("Load the float from the external table!")
            }
        }
    }
}

/// A floating-point(f64) value from another parameter
///
/// This value can be a constant (literal or otherwise) or a dynamic value provided
/// by another parameter.
#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(untagged)]
pub enum ParameterFloatValue {
    Reference(String),
    Inline(Box<Parameter>),
}

// impl ParameterFloatValue {
//     /// Return the value loading from a table if required.
//     pub fn load(&self) -> f64 {
//         match self {
//             Self::Literal(v) => *v,
//             Self::Table(_) => {
//                 todo!("Load the float from the external table!")
//             }
//         }
//     }
// }

/// A potentially dynamic floating-point (f64) value
///
/// This value can be a constant (literal or otherwise) or a dynamic value provided
/// by another parameter.
#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(untagged)]
pub enum DynamicFloatValue {
    Constant(ConstantFloatValue),
    Dynamic(ParameterFloatValue),
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct ExternalDataRef {
    url: String,
    column: Option<String>,
    index: Option<String>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct TableDataRef {
    table: String,
    column: Option<String>,
    index: Option<String>,
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
