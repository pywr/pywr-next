use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::parameters::{AggFunc as AggFuncV1, IndexAggFunc as IndexAggFuncV1};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};

// TODO complete these
/// Aggregation functions for float values.
///
/// This enum defines the possible aggregation functions that can be applied to index metrics.
/// They are mapped to the corresponding functions in the `pywr_core::parameters::AggFunc` enum
/// when used in the core library.
#[derive(Deserialize, Serialize, Debug, Copy, Clone, JsonSchema, PywrVisitAll, Display, EnumDiscriminants)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(AggFuncType))]
pub enum AggFunc {
    Sum,
    Max,
    Min,
    Product,
    Mean,
    CountNonZero,
}

#[cfg(feature = "core")]
impl From<AggFunc> for pywr_core::agg_funcs::AggFuncF64 {
    fn from(value: AggFunc) -> Self {
        match value {
            AggFunc::Sum => pywr_core::agg_funcs::AggFuncF64::Sum,
            AggFunc::Max => pywr_core::agg_funcs::AggFuncF64::Max,
            AggFunc::Min => pywr_core::agg_funcs::AggFuncF64::Min,
            AggFunc::Product => pywr_core::agg_funcs::AggFuncF64::Product,
            AggFunc::Mean => pywr_core::agg_funcs::AggFuncF64::Mean,
            AggFunc::CountNonZero => pywr_core::agg_funcs::AggFuncF64::CountNonZero,
        }
    }
}
impl From<AggFuncV1> for AggFunc {
    fn from(v1: AggFuncV1) -> Self {
        match v1 {
            AggFuncV1::Sum => Self::Sum,
            AggFuncV1::Product => Self::Product,
            AggFuncV1::Max => Self::Max,
            AggFuncV1::Min => Self::Min,
        }
    }
}

// TODO complete these
/// Aggregation functions for index (integer) values.
///
/// This enum defines the possible aggregation functions that can be applied to index metrics.
/// They are mapped to the corresponding functions in the `pywr_core::parameters::AggIndexFunc` enum
/// when used in the core library.
#[derive(Deserialize, Serialize, Debug, Copy, Clone, JsonSchema, PywrVisitAll, Display, EnumDiscriminants)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(IndexAggFuncType))]
pub enum IndexAggFunc {
    /// Sum of all values.
    Sum,
    /// Product of all values.
    Product,
    /// Minimum value among all values.
    Min,
    /// Maximum value among all values.
    Max,
    /// Returns 1 if any value is non-zero, otherwise 0.
    Any,
    /// Returns 1 if all values are non-zero, otherwise 0.
    All,
}

#[cfg(feature = "core")]
impl From<IndexAggFunc> for pywr_core::agg_funcs::AggFuncU64 {
    fn from(value: IndexAggFunc) -> Self {
        match value {
            IndexAggFunc::Sum => pywr_core::agg_funcs::AggFuncU64::Sum,
            IndexAggFunc::Product => pywr_core::agg_funcs::AggFuncU64::Product,
            IndexAggFunc::Max => pywr_core::agg_funcs::AggFuncU64::Max,
            IndexAggFunc::Min => pywr_core::agg_funcs::AggFuncU64::Min,
            IndexAggFunc::Any => pywr_core::agg_funcs::AggFuncU64::Any,
            IndexAggFunc::All => pywr_core::agg_funcs::AggFuncU64::All,
        }
    }
}

impl From<IndexAggFuncV1> for IndexAggFunc {
    fn from(v1: IndexAggFuncV1) -> Self {
        match v1 {
            IndexAggFuncV1::Sum => Self::Sum,
            IndexAggFuncV1::Product => Self::Product,
            IndexAggFuncV1::Max => Self::Max,
            IndexAggFuncV1::Min => Self::Min,
            IndexAggFuncV1::Any => Self::Any,
            IndexAggFuncV1::All => Self::All,
        }
    }
}
