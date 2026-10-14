//! Wrapping Pywr errors for use in Python.

use anyhow::anyhow;
use pyo3::exceptions::PyException;
use pyo3::{PyErr, create_exception};
use pywr_core::models::{ModelBuilderError, ModelRunError, MultiNetworkModelBuilderError, MultiNetworkModelRunError};
use pywr_core::recorders::RecorderAggregationError;
use pywr_schema::{ModelSchemaBuildError, ModelSchemaReadError, MultiNetworkModelSchemaBuildError};

// Base exception for all Pywr errors
create_exception!(pywr, PywrError, PyException);

pub(crate) struct PyModelSchemaReadError {
    inner: ModelSchemaReadError,
}

impl From<ModelSchemaReadError> for PyModelSchemaReadError {
    fn from(error: ModelSchemaReadError) -> Self {
        PyModelSchemaReadError { inner: error }
    }
}

create_exception!(pywr, SchemaReadError, PywrError);

impl From<PyModelSchemaReadError> for PyErr {
    fn from(error: PyModelSchemaReadError) -> Self {
        PyErr::new::<SchemaReadError, _>(format!("{:?}", anyhow!(error.inner)))
    }
}

pub(crate) struct PyModelSchemaBuildError {
    inner: ModelSchemaBuildError,
}

impl From<ModelSchemaBuildError> for PyModelSchemaBuildError {
    fn from(error: ModelSchemaBuildError) -> Self {
        PyModelSchemaBuildError { inner: error }
    }
}

create_exception!(pywr, SchemaBuildError, PywrError);

impl From<PyModelSchemaBuildError> for PyErr {
    fn from(error: PyModelSchemaBuildError) -> Self {
        PyErr::new::<SchemaBuildError, _>(format!("{:?}", anyhow!(error.inner)))
    }
}

pub(crate) struct PyMultiNetworkModelSchemaBuildError {
    inner: MultiNetworkModelSchemaBuildError,
}

impl From<MultiNetworkModelSchemaBuildError> for PyMultiNetworkModelSchemaBuildError {
    fn from(error: MultiNetworkModelSchemaBuildError) -> Self {
        PyMultiNetworkModelSchemaBuildError { inner: error }
    }
}

create_exception!(pywr, MultiNetworkSchemaBuildError, PywrError);

impl From<PyMultiNetworkModelSchemaBuildError> for PyErr {
    fn from(error: PyMultiNetworkModelSchemaBuildError) -> Self {
        PyErr::new::<MultiNetworkSchemaBuildError, _>(format!("{:?}", anyhow!(error.inner)))
    }
}

pub(crate) struct PyModelBuilderError {
    inner: ModelBuilderError,
}

impl From<ModelBuilderError> for PyModelBuilderError {
    fn from(error: ModelBuilderError) -> Self {
        PyModelBuilderError { inner: error }
    }
}

create_exception!(pywr, NetworkBuildError, PywrError);

impl From<PyModelBuilderError> for PyErr {
    fn from(error: PyModelBuilderError) -> Self {
        match error.inner {
            ModelBuilderError::NetworkBuildError(_) => {
                PyErr::new::<NetworkBuildError, _>(format!("{:?}", anyhow!(error.inner)))
            }
        }
    }
}

pub(crate) struct PyMultiNetworkModelBuilderError {
    inner: MultiNetworkModelBuilderError,
}

impl From<MultiNetworkModelBuilderError> for PyMultiNetworkModelBuilderError {
    fn from(error: MultiNetworkModelBuilderError) -> Self {
        PyMultiNetworkModelBuilderError { inner: error }
    }
}

create_exception!(pywr, NetworkTransferError, PywrError);

impl From<PyMultiNetworkModelBuilderError> for PyErr {
    fn from(error: PyMultiNetworkModelBuilderError) -> Self {
        match error.inner {
            MultiNetworkModelBuilderError::NetworkBuilderError { .. }
            | MultiNetworkModelBuilderError::DuplicateNetworkName { .. } => {
                PyErr::new::<NetworkBuildError, _>(format!("{:?}", anyhow!(error.inner)))
            }
            MultiNetworkModelBuilderError::NetworkNotFoundForTransfer { .. }
            | MultiNetworkModelBuilderError::ResolveMetricF64ForTransferError { .. }
            | MultiNetworkModelBuilderError::DuplicateTransferName { .. }
            | MultiNetworkModelBuilderError::TransferToSelf { .. } => {
                PyErr::new::<NetworkTransferError, _>(format!("{:?}", anyhow!(error.inner)))
            }
        }
    }
}

pub(crate) struct PyModelRunError {
    inner: ModelRunError,
}

impl From<ModelRunError> for PyModelRunError {
    fn from(error: ModelRunError) -> Self {
        PyModelRunError { inner: error }
    }
}

create_exception!(pywr, SetupError, PywrError);
create_exception!(pywr, StepError, PywrError);
create_exception!(pywr, FinaliseError, PywrError);

impl From<PyModelRunError> for PyErr {
    fn from(error: PyModelRunError) -> Self {
        match error.inner {
            ModelRunError::SetupError(_) => PyErr::new::<SetupError, _>(format!("{:?}", anyhow!(error.inner))),
            ModelRunError::StepError(_) => PyErr::new::<StepError, _>(format!("{:?}", anyhow!(error.inner))),
            ModelRunError::FinaliseError(_) => PyErr::new::<FinaliseError, _>(format!("{:?}", anyhow!(error.inner))),
        }
    }
}

pub(crate) struct PyMultiNetworkModelRunError {
    error: MultiNetworkModelRunError,
}

impl From<MultiNetworkModelRunError> for PyMultiNetworkModelRunError {
    fn from(error: MultiNetworkModelRunError) -> Self {
        PyMultiNetworkModelRunError { error }
    }
}

impl From<PyMultiNetworkModelRunError> for PyErr {
    fn from(error: PyMultiNetworkModelRunError) -> Self {
        match error.error {
            MultiNetworkModelRunError::SetupError(_) => {
                PyErr::new::<SetupError, _>(format!("{:?}", anyhow!(error.error)))
            }
            MultiNetworkModelRunError::StepError(_) => {
                PyErr::new::<StepError, _>(format!("{:?}", anyhow!(error.error)))
            }
            MultiNetworkModelRunError::FinaliseError(_) => {
                PyErr::new::<FinaliseError, _>(format!("{:?}", anyhow!(error.error)))
            }
        }
    }
}

pub(crate) struct PyRecorderAggregationError {
    inner: RecorderAggregationError,
}

impl From<RecorderAggregationError> for PyRecorderAggregationError {
    fn from(error: RecorderAggregationError) -> Self {
        PyRecorderAggregationError { inner: error }
    }
}

create_exception!(pywr, RecorderDoesNotSupportAggregation, PywrError);
create_exception!(pywr, AggregationError, PywrError);

impl From<PyRecorderAggregationError> for PyErr {
    fn from(error: PyRecorderAggregationError) -> Self {
        match error.inner {
            RecorderAggregationError::RecorderDoesNotSupportAggregation => {
                PyErr::new::<RecorderDoesNotSupportAggregation, _>(format!("{:?}", anyhow!(error.inner)))
            }
            RecorderAggregationError::AggregationError { .. } => {
                PyErr::new::<AggregationError, _>(format!("{:?}", anyhow!(error.inner)))
            }
        }
    }
}
