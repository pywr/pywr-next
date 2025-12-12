#[cfg(feature = "core")]
use crate::SchemaError;
use crate::py_utils::PythonSource;
#[cfg(all(feature = "core", feature = "pyo3"))]
use crate::py_utils::{try_load_optional_py_args, try_load_optional_py_kwargs};
#[cfg(all(feature = "core", feature = "pyo3"))]
use pyo3::{Python, types::PyAnyMethods};
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::parameters::{AggFunc as AggFuncV1, IndexAggFunc as IndexAggFuncV1};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[cfg(feature = "core")]
use std::path::Path;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};

/// An aggregation function implemented in Python.
#[derive(Deserialize, Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct PythonAggFunc {
    pub source: PythonSource,
    /// The name of the Python object from the module to use. This should be a callable object.
    pub object: String,
    /// Position arguments to pass to the object during setup.
    pub args: Option<Vec<serde_json::Value>>,
    /// Keyword arguments to pass to the object during setup.
    pub kwargs: Option<HashMap<String, serde_json::Value>>,
}

#[cfg(all(feature = "core", feature = "pyo3"))]
impl PythonAggFunc {
    /// Load the Python aggregation function.
    fn load(&self, data_path: Option<&Path>) -> Result<pywr_core::agg_funcs::PyAggFunc, SchemaError> {
        Python::initialize();

        let function = Python::attach(|py| {
            let module = self.source.load_module(py, data_path)?;
            let obj = module.getattr(&self.object)?;

            Ok::<_, SchemaError>(obj.unbind())
        })?;

        let args = Python::attach(|py| try_load_optional_py_args(py, &self.args))?;
        let kwargs = Python::attach(|py| try_load_optional_py_kwargs(py, &self.kwargs))?;

        Ok(pywr_core::agg_funcs::PyAggFunc::new(function, args, kwargs))
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct AnyNonZero {
    tolerance: Option<f64>,
}

#[cfg(feature = "core")]
impl AnyNonZero {
    fn load(&self) -> Result<pywr_core::agg_funcs::AggFuncF64, SchemaError> {
        let tolerance = self.tolerance.unwrap_or(1e-6);

        Ok(pywr_core::agg_funcs::AggFuncF64::AnyNonZero { tolerance })
    }
}

// TODO complete these
/// Aggregation functions for float values.
///
/// This enum defines the possible aggregation functions that can be applied to index metrics.
/// They are mapped to the corresponding functions in the `pywr_core::parameters::AggFunc` enum
/// when used in the core library.
#[derive(Deserialize, Serialize, Debug, Clone, JsonSchema, PywrVisitAll, Display, EnumDiscriminants)]
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
    AnyNonZero(AnyNonZero),
    Python(PythonAggFunc),
}

#[cfg(feature = "core")]
impl AggFunc {
    #[cfg_attr(not(feature = "pyo3"), allow(unused_variables))]
    pub fn load(&self, data_path: Option<&Path>) -> Result<pywr_core::agg_funcs::AggFuncF64, SchemaError> {
        match self {
            Self::Sum => Ok(pywr_core::agg_funcs::AggFuncF64::Sum),
            Self::Max => Ok(pywr_core::agg_funcs::AggFuncF64::Max),
            Self::Min => Ok(pywr_core::agg_funcs::AggFuncF64::Min),
            Self::Product => Ok(pywr_core::agg_funcs::AggFuncF64::Product),
            Self::Mean => Ok(pywr_core::agg_funcs::AggFuncF64::Mean),
            Self::CountNonZero => Ok(pywr_core::agg_funcs::AggFuncF64::CountNonZero),
            Self::AnyNonZero(agg_func) => Ok(agg_func.load()?),
            #[cfg(feature = "pyo3")]
            Self::Python(py_func) => Ok(pywr_core::agg_funcs::AggFuncF64::Python(py_func.load(data_path)?)),
            #[cfg(not(feature = "pyo3"))]
            Self::Python(_) => Err(SchemaError::FeatureNotEnabled("pyo3".to_string())),
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
#[derive(Deserialize, Serialize, Debug, Clone, JsonSchema, PywrVisitAll, Display, EnumDiscriminants)]
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
    Python(PythonAggFunc),
}

#[cfg(feature = "core")]
impl IndexAggFunc {
    #[cfg_attr(not(feature = "pyo3"), allow(unused_variables))]
    pub fn load(&self, data_path: Option<&Path>) -> Result<pywr_core::agg_funcs::AggFuncU64, SchemaError> {
        match self {
            Self::Sum => Ok(pywr_core::agg_funcs::AggFuncU64::Sum),
            Self::Product => Ok(pywr_core::agg_funcs::AggFuncU64::Product),
            Self::Max => Ok(pywr_core::agg_funcs::AggFuncU64::Max),
            Self::Min => Ok(pywr_core::agg_funcs::AggFuncU64::Min),
            Self::Any => Ok(pywr_core::agg_funcs::AggFuncU64::Any),
            Self::All => Ok(pywr_core::agg_funcs::AggFuncU64::All),
            #[cfg(feature = "pyo3")]
            Self::Python(py_func) => Ok(pywr_core::agg_funcs::AggFuncU64::Python(py_func.load(data_path)?)),
            #[cfg(not(feature = "pyo3"))]
            Self::Python(_) => Err(SchemaError::FeatureNotEnabled("pyo3".to_string())),
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
