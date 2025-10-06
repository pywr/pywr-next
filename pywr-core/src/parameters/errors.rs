use crate::agg_funcs::AggFuncError;
use crate::metric::{
    ConstantMetricF64Error, ConstantMetricU64Error, MetricF64Error, MetricU64Error, SimpleMetricF64Error,
    SimpleMetricU64Error,
};
use crate::parameters::InterpolationError;
use thiserror::Error;

/// Errors returned during parameter setup.
#[derive(Error, Debug)]
pub enum ParameterSetupError {
    #[cfg(feature = "pyo3")]
    #[error("Error with Python parameter `{name}` (`{object}`): {py_error}")]
    PythonError {
        name: String,
        object: String,
        #[source]
        py_error: Box<pyo3::PyErr>,
    },
}

/// Errors returned by parameter calculations.
#[derive(Error, Debug)]
pub enum ParameterCalculationError {
    #[error("F64 metric error: {0}")]
    MetricF64Error(#[from] MetricF64Error),
    #[error("U64 metric error: {0}")]
    MetricU64Error(#[from] MetricU64Error),
    #[error("Out of bounds error at index {index} for array of length {length} on axis {axis}")]
    OutOfBoundsError { index: usize, length: usize, axis: usize },
    #[error("Division by zero error")]
    DivisionByZeroError,
    #[error("Interpolation error: {0}")]
    InterpolationError(#[from] InterpolationError),
    #[error("Internal error: {message}")]
    Internal { message: String },
    #[cfg(feature = "pyo3")]
    #[error("Error with Python parameter `{name}` (`{object}`): {py_error}")]
    PythonError {
        name: String,
        object: String,
        #[source]
        py_error: Box<pyo3::PyErr>,
    },
    #[error("Aggregation error: {0}")]
    AggFuncError(#[from] AggFuncError),
}

#[derive(Error, Debug)]
pub enum SimpleCalculationError {
    #[error("Simple f64 metric error: {0}")]
    SimpleMetricF64Error(#[from] SimpleMetricF64Error),
    #[error("Simple u64 metric error: {0}")]
    SimpleMetricU64Error(#[from] SimpleMetricU64Error),
    #[error("Out of bounds error at index {index} for array of length {length} on axis {axis}")]
    OutOfBoundsError { index: usize, length: usize, axis: usize },
    #[error("Internal error: {message}")]
    Internal { message: String },
    #[error("Aggregation error: {0}")]
    AggFuncError(#[from] AggFuncError),
}

#[allow(clippy::enum_variant_names)]
#[derive(Error, Debug)]
pub enum ConstCalculationError {
    #[error("Constant f64 metric error: {0}")]
    ConstantMetricF64Error(#[from] ConstantMetricF64Error),
    #[error("Constant u64 metric error: {0}")]
    ConstantMetricU64Error(#[from] ConstantMetricU64Error),
    #[error("Aggregation error: {0}")]
    AggFuncError(#[from] AggFuncError),
}
