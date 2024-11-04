//! Conversion traits for converting from Pywr v1 to v2 schema.
//!
//! This module contains traits for converting from Pywr v1 schema to Pywr v2 schema.
//! The traits are implemented for all types that can be converted. Due to differences in
//! the schemas some conversions may extract [`Parameter`]s and [`Timeseries`] from the
//! original data. This is primarily due to the fact that the v2 schema does not support
//! inline (and unnamed) parameters, and includes a separate timeseries section.
//!
//! The struct [`ConversionData`] is used to store these extracted parameters and timeseries.
//! It also tacks a count of unnamed parameters and timeseries. This is used during conversion
//! of meta-data to provide a unique name for unnamed parameters and timeseries.

use crate::parameters::{Parameter, ParameterMeta};
use crate::timeseries::Timeseries;
use pywr_v1_schema::parameters::ParameterMeta as ParameterMetaV1;

/// Counters for unnamed parameters and timeseries.
#[derive(Default)]
pub struct ConversionData {
    unnamed_count: usize,
    pub parameters: Vec<Parameter>,
    pub timeseries: Vec<Timeseries>,
}

impl ConversionData {
    pub fn reset_count(&mut self) {
        self.unnamed_count = 0;
    }
}

pub trait FromV1<T>: Sized {
    fn from_v1(v1: T, parent_node: Option<&str>, conversion_data: &mut ConversionData) -> Self;
}

pub trait IntoV1<T> {
    fn into_v2(self, parent_node: Option<&str>, conversion_data: &mut ConversionData) -> T;
}

// FromV1Parameter implies IntoV2Parameter
impl<T, U> IntoV1<U> for T
where
    U: FromV1<T>,
{
    fn into_v2(self, parent_node: Option<&str>, conversion_data: &mut ConversionData) -> U {
        U::from_v1(self, parent_node, conversion_data)
    }
}

pub trait TryFromV1<T>: Sized {
    type Error;
    fn try_from_v1(v1: T, parent_node: Option<&str>, conversion_data: &mut ConversionData)
        -> Result<Self, Self::Error>;
}

pub trait TryIntoV2<T> {
    type Error;
    fn try_into_v2(self, parent_node: Option<&str>, conversion_data: &mut ConversionData) -> Result<T, Self::Error>;
}

// TryFromV1Parameter implies TryIntoV2Parameter
impl<T, U> TryIntoV2<U> for T
where
    U: TryFromV1<T>,
{
    type Error = U::Error;

    fn try_into_v2(self, parent_node: Option<&str>, conversion_data: &mut ConversionData) -> Result<U, Self::Error> {
        U::try_from_v1(self, parent_node, conversion_data)
    }
}

impl FromV1<ParameterMetaV1> for ParameterMeta {
    fn from_v1(v1: ParameterMetaV1, parent_node: Option<&str>, conversion_data: &mut ConversionData) -> Self {
        Self {
            name: v1.name.unwrap_or_else(|| {
                let pname = match parent_node {
                    Some(pn) => format!("{pn}-p{}", conversion_data.unnamed_count),
                    None => format!("unnamed-{}", conversion_data.unnamed_count),
                };
                conversion_data.unnamed_count += 1;
                pname
            }),
            comment: v1.comment,
        }
    }
}

impl FromV1<Option<ParameterMetaV1>> for ParameterMeta {
    fn from_v1(v1: Option<ParameterMetaV1>, parent_node: Option<&str>, conversion_data: &mut ConversionData) -> Self {
        match v1 {
            Some(meta) => meta.into_v2(parent_node, conversion_data),
            None => {
                let meta = Self {
                    name: format!("unnamed-{}", conversion_data.unnamed_count),
                    comment: None,
                };
                conversion_data.unnamed_count += 1;
                meta
            }
        }
    }
}
