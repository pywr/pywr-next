use crate::agg_funcs::AggFuncError;
use pyo3::prelude::{PySequenceMethods, PyTupleMethods};
use pyo3::types::{PyDict, PyList, PyTuple};
use pyo3::{Bound, Py, PyAny, Python};
use std::sync::Arc;

// Wrapper around a Python aggregation function to allow it to be cloned and shared.
// This is necessary because Py<PyAny> does not implement Clone.
#[derive(Clone, Debug)]
pub struct PyAggFunc(Arc<PyAggFuncInner>);

impl PyAggFunc {
    pub fn new(function: Py<PyAny>, args: Py<PyTuple>, kwargs: Py<PyDict>) -> Self {
        Self(Arc::new(PyAggFuncInner::new(function, args, kwargs)))
    }

    pub fn call_f64(&self, values: Vec<f64>) -> Result<f64, AggFuncError> {
        self.0.call_f64(values)
    }

    pub fn call_u64(&self, values: Vec<u64>) -> Result<u64, AggFuncError> {
        self.0.call_u64(values)
    }
}

/// A Python aggregation function with arguments and keyword arguments.
///
/// The function is stored as a `Py<PyAny>` to allow for any callable Python object.
/// This object will be called with a first argument being a list of values to aggregate,
/// followed by any additional positional and keyword arguments specified in `args` and `kwargs`.
/// It is expected that the function will return a single float.
#[derive(Debug)]
struct PyAggFuncInner {
    function: Py<PyAny>,
    args: Py<PyTuple>,
    kwargs: Py<PyDict>,
}

impl PyAggFuncInner {
    fn new(function: Py<PyAny>, args: Py<PyTuple>, kwargs: Py<PyDict>) -> Self {
        Self { function, args, kwargs }
    }

    fn call_f64(&self, values: Vec<f64>) -> Result<f64, AggFuncError> {
        let value = Python::attach(|py| {
            let values_py: Bound<PyList> = PyList::new(py, values).map_err(|py_error| AggFuncError::PythonError {
                object: self.function.to_string(),
                py_error: Box::new(py_error),
            })?;

            let args = PyTuple::new(py, [values_py]).map_err(|py_error| AggFuncError::PythonError {
                object: self.function.to_string(),
                py_error: Box::new(py_error),
            })?;

            // Concatenate the user defined args with the info arg.
            let args = args
                .into_sequence()
                .concat(self.args.bind(py).as_sequence())
                .map_err(|py_error| AggFuncError::PythonError {
                    object: self.function.to_string(),
                    py_error: Box::new(py_error),
                })?;

            let args = args.to_tuple().map_err(|py_error| AggFuncError::PythonError {
                object: self.function.to_string(),
                py_error: Box::new(py_error),
            })?;

            let kwargs = self.kwargs.bind(py);

            let result = self
                .function
                .call(py, args, Some(kwargs))
                .map_err(|py_error| AggFuncError::PythonError {
                    object: self.function.to_string(),
                    py_error: Box::new(py_error),
                })?
                .extract(py)
                .map_err(|py_error| AggFuncError::PythonError {
                    object: self.function.to_string(),
                    py_error: Box::new(py_error),
                })?;

            Ok(result)
        })?;

        Ok(value)
    }

    fn call_u64(&self, values: Vec<u64>) -> Result<u64, AggFuncError> {
        let value = Python::attach(|py| {
            let values_py: Bound<PyList> = PyList::new(py, values).map_err(|py_error| AggFuncError::PythonError {
                object: self.function.to_string(),
                py_error: Box::new(py_error),
            })?;

            let args = PyTuple::new(py, [values_py]).map_err(|py_error| AggFuncError::PythonError {
                object: self.function.to_string(),
                py_error: Box::new(py_error),
            })?;

            // Concatenate the user defined args with the info arg.
            let args = args
                .into_sequence()
                .concat(self.args.bind(py).as_sequence())
                .map_err(|py_error| AggFuncError::PythonError {
                    object: self.function.to_string(),
                    py_error: Box::new(py_error),
                })?;

            let args = args.to_tuple().map_err(|py_error| AggFuncError::PythonError {
                object: self.function.to_string(),
                py_error: Box::new(py_error),
            })?;

            let kwargs = self.kwargs.bind(py);

            let result = self
                .function
                .call(py, args, Some(kwargs))
                .map_err(|py_error| AggFuncError::PythonError {
                    object: self.function.to_string(),
                    py_error: Box::new(py_error),
                })?
                .extract(py)
                .map_err(|py_error| AggFuncError::PythonError {
                    object: self.function.to_string(),
                    py_error: Box::new(py_error),
                })?;

            Ok(result)
        })?;

        Ok(value)
    }
}
