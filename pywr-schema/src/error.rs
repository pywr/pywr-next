use crate::data_tables::TableError;
use crate::nodes::NodeAttribute;
use pyo3::exceptions::PyRuntimeError;
use pyo3::PyErr;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SchemaError {
    #[error("IO error: {0}")]
    IO(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("node with name {0} not found")]
    NodeNotFound(String),
    #[error("node ({ty}) with name {name} does not support attribute {attr}")]
    NodeAttributeNotSupported {
        ty: String,
        name: String,
        attr: NodeAttribute,
    },
    #[error("parameter {0} not found")]
    ParameterNotFound(String),
    #[error("network {0} not found")]
    NetworkNotFound(String),
    #[error("missing initial volume for node: {0}")]
    MissingInitialVolume(String),
    #[error("Pywr core error: {0}")]
    PywrCore(#[from] pywr_core::PywrError),
    #[error("data table error: {0}")]
    DataTable(#[from] TableError),
    #[error("Circular node reference(s) found.")]
    CircularNodeReference,
    #[error("Circular parameters reference(s) found.")]
    CircularParameterReference,
    #[error("unsupported file format")]
    UnsupportedFileFormat,
    #[error("Python error: {0}")]
    PythonError(String),
    #[error("hdf5 error: {0}")]
    HDF5Error(String),
    #[error("Missing metric set: {0}")]
    MissingMetricSet(String),
    #[error("unexpected parameter type: {0}")]
    UnexpectedParameterType(String),
    #[error("mismatch in the length of data provided. expected: {expected}, found: {found}")]
    DataLengthMismatch { expected: usize, found: usize },
    #[error("Failed to estimate epsilon for use in the radial basis function.")]
    RbfEpsilonEstimation,
    #[error("Scenario group with name {0} not found")]
    ScenarioGroupNotFound(String),
    #[error("Inter-network transfer with name {0} not found")]
    InterNetworkTransferNotFound(String),
    #[error("Invalid rolling window definition on parameter {name}. Must convert to a positive integer.")]
    InvalidRollingWindow { name: String },
    #[error("Failed to load parameter {name}: {error}")]
    LoadParameter { name: String, error: String },
}

impl From<SchemaError> for PyErr {
    fn from(err: SchemaError) -> PyErr {
        PyRuntimeError::new_err(err.to_string())
    }
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum ConversionError {
    #[error("Error converting {attr:?} on node {name:?}")]
    NodeAttribute {
        attr: String,
        name: String,
        source: Box<ConversionError>,
    },
    #[error("Constant float value cannot be a parameter reference.")]
    ConstantFloatReferencesParameter,
    #[error("Constant float value cannot be an inline parameter.")]
    ConstantFloatInlineParameter,
    #[error("Missing one of the following attributes {attrs:?} on parameter {name:?}.")]
    MissingAttribute { attrs: Vec<String>, name: String },
    #[error("Unexpected the following attributes {attrs:?} on parameter {name:?}.")]
    UnexpectedAttribute { attrs: Vec<String>, name: String },
    #[error("Can not convert a float constant to an index constant.")]
    FloatToIndex,
    #[error("Attribute {attr:?} on node {name:?} is not allowed .")]
    ExtraNodeAttribute { attr: String, name: String },
    #[error("Custom node of type {ty:?} on node {name:?} is not supported .")]
    CustomNodeNotSupported { ty: String, name: String },
    #[error("Integer table indices are not supported.")]
    IntegerTableIndicesNotSupported,
    #[error("Conversion of one of the following attributes {attrs:?} is not supported on parameter {name:?}.")]
    UnsupportedAttribute { attrs: Vec<String>, name: String },
    #[error("Conversion of one of the following feature is not supported on parameter {name:?}: {feature}")]
    UnsupportedFeature { feature: String, name: String },
    #[error("Parameter {name:?} of type `{ty:?}` are not supported in Pywr v2. {instead:?}")]
    DeprecatedParameter { ty: String, name: String, instead: String },
    #[error("Unexpected type for attribute {attr} on parameter {name}. Expected `{expected}`, found `{actual}`")]
    UnexpectedType {
        attr: String,
        name: String,
        expected: String,
        actual: String,
    },
    #[error("'{0}' could not be parsed into a NaiveDate")]
    UnparseableDate(String),
    #[error("Chrono out of range error: {0}")]
    OutOfRange(#[from] chrono::OutOfRange),
}
