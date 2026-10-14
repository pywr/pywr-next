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
    #[error("Error with Python parameter `{name}` (`{object}`).")]
    PythonError {
        name: String,
        object: String,
        #[source]
        py_error: Box<pyo3::PyErr>,
    },
    #[cfg(test)]
    #[error("Test error: {0}")]
    TestError(String),
}

/// Errors returned by parameter calculations.
#[derive(Error, Debug)]
pub enum GeneralCalculationError {
    #[error("F64 metric error.")]
    MetricF64Error(#[from] MetricF64Error),
    #[error("U64 metric error.")]
    MetricU64Error(#[from] MetricU64Error),
    #[error("Out of bounds error at index {index} for array of length {length} on axis {axis}")]
    OutOfBoundsError { index: usize, length: usize, axis: usize },
    #[error("Division by zero error")]
    DivisionByZeroError,
    #[error("Interpolation error.")]
    InterpolationError(#[from] InterpolationError),
    #[error("Internal error: {message}")]
    Internal { message: String },
    #[cfg(feature = "pyo3")]
    #[error("Error with Python parameter `{name}` (`{object}`).")]
    PythonError {
        name: String,
        object: String,
        #[source]
        py_error: Box<pyo3::PyErr>,
    },
    #[error("Aggregation error.")]
    AggFuncError(#[from] AggFuncError),
    #[error(
        "Calculation phase not enabled for parameter. This is a runtime error for parameter type `{ty}` that does support '{phase}', but has failed to calculate for the following reason: {message}"
    )]
    PhaseNotEnabled { ty: String, phase: String, message: String },
}

#[derive(Error, Debug)]
pub enum SimpleCalculationError {
    #[error("Simple f64 metric error.")]
    SimpleMetricF64Error(#[from] SimpleMetricF64Error),
    #[error("Simple u64 metric error.")]
    SimpleMetricU64Error(#[from] SimpleMetricU64Error),
    #[error("Out of bounds error at index {index} for array of length {length} on axis {axis}")]
    OutOfBoundsError { index: usize, length: usize, axis: usize },
    #[error("Internal error: {message}")]
    Internal { message: String },
    #[error("Aggregation error.")]
    AggFuncError(#[from] AggFuncError),
}

#[allow(clippy::enum_variant_names)]
#[derive(Error, Debug)]
pub enum ConstCalculationError {
    #[error("Constant f64 metric error.")]
    ConstantMetricF64Error(#[from] ConstantMetricF64Error),
    #[error("Constant u64 metric error.")]
    ConstantMetricU64Error(#[from] ConstantMetricU64Error),
    #[error("Aggregation error.")]
    AggFuncError(#[from] AggFuncError),
}
