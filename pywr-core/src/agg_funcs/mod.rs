#[cfg(feature = "pyo3")]
mod py;

#[cfg(feature = "pyo3")]
pub use py::PyAggFunc;

use crate::recorders::PeriodValue;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AggFuncError {
    /// Error occurred in a Python aggregation function.
    #[cfg(feature = "pyo3")]
    #[error("Error in Python aggregation function '{object}': {py_error}")]
    PythonError {
        object: String,
        #[source]
        py_error: Box<pyo3::PyErr>,
    },
}

/// Aggregation functions that can be applied to a set of f64 values.
#[derive(Clone, Debug)]
pub enum AggFuncF64 {
    Sum,
    Mean,
    Product,
    Min,
    Max,
    CountNonZero,
    CountFunc {
        func: fn(f64) -> bool,
    },
    AnyNonZero {
        tolerance: f64,
    },
    #[cfg(feature = "pyo3")]
    Python(PyAggFunc),
}

impl AggFuncF64 {
    /// Calculate the aggregation of the given values.
    pub fn calc_period_values(&self, values: &[PeriodValue<f64>]) -> Option<f64> {
        match self {
            Self::Sum => Some(values.iter().map(|v| v.value).sum()),
            Self::Mean => {
                let ndays: f64 = values.iter().map(|v| v.duration.fractional_days()).sum();
                if ndays == 0.0 {
                    None
                } else {
                    let sum: f64 = values.iter().map(|v| v.value * v.duration.fractional_days()).sum();

                    Some(sum / ndays)
                }
            }
            Self::Product => Some(values.iter().map(|v| v.value).product()),
            Self::Min => values.iter().map(|v| v.value).min_by(|a, b| {
                a.partial_cmp(b)
                    .expect("Failed to calculate minimum of values containing a NaN.")
            }),
            Self::Max => values.iter().map(|v| v.value).max_by(|a, b| {
                a.partial_cmp(b)
                    .expect("Failed to calculate maximum of values containing a NaN.")
            }),
            Self::CountNonZero => {
                let count = values.iter().filter(|v| v.value != 0.0).count();
                Some(count as f64)
            }
            Self::CountFunc { func } => {
                let count = values.iter().filter(|v| func(v.value)).count();
                Some(count as f64)
            }
            Self::AnyNonZero { tolerance } => {
                let any = values.iter().any(|v| v.value.abs() > *tolerance);
                Some(any as u8 as f64)
            }
            #[cfg(feature = "pyo3")]
            Self::Python(py_func) => {
                let vals: Vec<f64> = values.iter().map(|v| v.value).collect();
                match py_func.call_f64(vals) {
                    Ok(result) => Some(result),
                    Err(e) => panic!("Error in Python aggregation function: {}", e),
                }
            }
        }
    }

    /// Calculate the aggregation of the given iterator of values.
    pub fn calc_iter_f64<'a, V>(&self, values: V) -> Result<f64, AggFuncError>
    where
        V: IntoIterator<Item = &'a f64>,
    {
        let agg_value = match self {
            Self::Sum => values.into_iter().sum(),
            Self::Mean => {
                let iter = values.into_iter();
                let count = iter.size_hint().0;
                if count == 0 {
                    0.0
                } else {
                    let total: f64 = iter.sum();
                    total / count as f64
                }
            }
            Self::Max => {
                let mut total = f64::MIN;
                for v in values {
                    total = total.max(*v);
                }
                total
            }
            Self::Min => {
                let mut total = f64::MAX;
                for v in values {
                    total = total.min(*v);
                }
                total
            }
            Self::Product => values.into_iter().product(),
            Self::CountNonZero => {
                let count = values.into_iter().filter(|&v| *v != 0.0).count();
                count as f64
            }
            Self::CountFunc { func } => {
                let count = values.into_iter().filter(|&v| func(*v)).count();
                count as f64
            }
            Self::AnyNonZero { tolerance } => {
                let any = values.into_iter().any(|&v| v.abs() > *tolerance);
                any as u8 as f64
            }
            #[cfg(feature = "pyo3")]
            Self::Python(py_func) => {
                let vals: Vec<f64> = values.into_iter().cloned().collect();
                py_func.call_f64(vals)?
            }
        };

        Ok(agg_value)
    }
}

/// Aggregation functions for aggregated index parameters.
#[derive(Clone, Debug)]
pub enum AggFuncU64 {
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
    #[cfg(feature = "pyo3")]
    Python(py::PyAggFunc),
}

impl AggFuncU64 {
    /// Calculate the aggregation of the given slice of values.
    pub fn calc_iter_u64<'a, V>(&self, values: V) -> Result<u64, AggFuncError>
    where
        V: IntoIterator<Item = &'a u64>,
    {
        let value = match self {
            AggFuncU64::Sum => values.into_iter().sum(),
            AggFuncU64::Product => values.into_iter().product(),
            AggFuncU64::Max => *values.into_iter().max().unwrap_or(&u64::MIN),
            AggFuncU64::Min => *values.into_iter().min().unwrap_or(&u64::MAX),
            AggFuncU64::Any => values.into_iter().any(|&b| b != 0) as u64,
            AggFuncU64::All => values.into_iter().all(|&b| b != 0) as u64,
            #[cfg(feature = "pyo3")]
            Self::Python(py_func) => {
                let vals: Vec<u64> = values.into_iter().cloned().collect();
                py_func.call_u64(vals)?
            }
        };

        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use crate::agg_funcs::{AggFuncF64, AggFuncU64, py};
    use float_cmp::assert_approx_eq;
    use pyo3::{Py, PyAny};
    #[cfg(feature = "pyo3")]
    use pyo3::{
        Python,
        ffi::c_str,
        prelude::{PyAnyMethods, PyModule},
        types::{PyDict, PyTuple},
    };

    #[test]
    fn test_f64_sum() {
        let result = AggFuncF64::Sum.calc_iter_f64(&[1.0, 2.0, 3.0]).unwrap();
        assert_approx_eq!(f64, result, 6.0);
    }

    #[test]
    fn test_f64_product() {
        let result = AggFuncF64::Product.calc_iter_f64(&[2.0, 3.0, 4.0]).unwrap();
        assert_approx_eq!(f64, result, 24.0);
    }

    #[test]
    fn test_f64_mean() {
        let result = AggFuncF64::Mean.calc_iter_f64(&[2.0, 4.0, 6.0]).unwrap();
        assert_approx_eq!(f64, result, 4.0);
    }

    #[test]
    fn test_f64_min() {
        let result = AggFuncF64::Min.calc_iter_f64(&[5.0, 2.0, 8.0]).unwrap();
        assert_approx_eq!(f64, result, 2.0);
    }

    #[test]
    fn test_f64_max() {
        let result = AggFuncF64::Max.calc_iter_f64(&[5.0, 2.0, 8.0]).unwrap();
        assert_approx_eq!(f64, result, 8.0);
    }

    #[test]
    fn test_f64_empty_mean() {
        let result = AggFuncF64::Mean.calc_iter_f64(&[]).unwrap();
        assert_approx_eq!(f64, result, 0.0);
    }

    #[test]
    fn test_f64_any_nonzero_true() {
        let result = AggFuncF64::AnyNonZero { tolerance: 1e-6 }
            .calc_iter_f64(&[1e-5, 0.0, 0.0])
            .unwrap();
        assert_approx_eq!(f64, result, 1.0);
    }

    #[test]
    fn test_f64_any_nonzero_false() {
        let result = AggFuncF64::AnyNonZero { tolerance: 1e-6 }
            .calc_iter_f64(&[1e-7, 0.0, 0.0])
            .unwrap();
        assert_approx_eq!(f64, result, 0.0);
    }

    #[test]
    fn test_u64_sum() {
        let result = AggFuncU64::Sum.calc_iter_u64(&[1u64, 2u64, 3u64]).unwrap();
        assert_eq!(result, 6u64);
    }

    #[test]
    fn test_u64_product() {
        let result = AggFuncU64::Product.calc_iter_u64(&[2u64, 3u64, 4u64]).unwrap();
        assert_eq!(result, 24u64);
    }

    #[test]
    fn test_u64_min() {
        let result = AggFuncU64::Min.calc_iter_u64(&[5u64, 2u64, 8u64]).unwrap();
        assert_eq!(result, 2u64);
    }

    #[test]
    fn test_u64_max() {
        let result = AggFuncU64::Max.calc_iter_u64(&[5u64, 2u64, 8u64]).unwrap();
        assert_eq!(result, 8u64);
    }

    #[test]
    fn test_u64_any_true() {
        let result = AggFuncU64::Any.calc_iter_u64(&[0u64, 0u64, 5u64]).unwrap();
        assert_eq!(result, 1u64);
    }

    #[test]
    fn test_u64_any_false() {
        let result = AggFuncU64::Any.calc_iter_u64(&[0u64, 0u64, 0u64]).unwrap();
        assert_eq!(result, 0u64);
    }

    #[test]
    fn test_u64_all_true() {
        let result = AggFuncU64::All.calc_iter_u64(&[1u64, 2u64, 3u64]).unwrap();
        assert_eq!(result, 1u64);
    }

    #[test]
    fn test_u64_all_false() {
        let result = AggFuncU64::All.calc_iter_u64(&[1u64, 0u64, 3u64]).unwrap();
        assert_eq!(result, 0u64);
    }

    /// Create a simple Python function that sums a list of values and adds an offset.
    #[cfg(feature = "pyo3")]
    fn make_py_sum_function() -> Py<PyAny> {
        Python::attach(|py| {
            let test_module = PyModule::from_code(
                py,
                c_str!(
                    r#"
def my_sum(values, offset):
    return sum(values) + offset
"#
                ),
                c_str!(""),
                c_str!(""),
            )
            .unwrap();

            test_module.getattr("my_sum").unwrap().into()
        })
    }

    #[test]
    #[cfg(feature = "pyo3")]
    fn test_f64_py() {
        Python::initialize();

        let py_func = make_py_sum_function();

        let args = Python::attach(|py| PyTuple::new(py, [5]).unwrap().unbind());
        let kwargs = Python::attach(|py| PyDict::new(py).unbind());

        let agg_func = AggFuncF64::Python(py::PyAggFunc::new(py_func, args, kwargs));

        let result = agg_func.calc_iter_f64(&[1.0, 2.0, 3.0]).unwrap();
        assert_approx_eq!(f64, result, 6.0 + 5.0);
    }

    #[test]
    #[cfg(feature = "pyo3")]
    fn test_u64_py() {
        Python::initialize();

        let py_func = make_py_sum_function();

        let args = Python::attach(|py| PyTuple::new(py, [5]).unwrap().unbind());
        let kwargs = Python::attach(|py| PyDict::new(py).unbind());

        let agg_func = AggFuncU64::Python(py::PyAggFunc::new(py_func, args, kwargs));

        let result = agg_func.calc_iter_u64(&[1u64, 2u64, 3u64]).unwrap();
        assert_eq!(result, 6u64 + 5u64);
    }
}
