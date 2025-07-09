use crate::data_tables::{DataTable, TableDataRef, TableError};
use crate::nodes::NodeAttribute;
use crate::timeseries::TimeseriesError;
#[cfg(feature = "core")]
use ndarray::ShapeError;
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SchemaError {
    // Catch infallible errors here rather than unwrapping at call site. This should be safer
    // in the long run if an infallible error is changed to a fallible one.
    #[error("Infallible error: {0}")]
    Infallible(#[from] std::convert::Infallible),
    #[error("IO error on path `{path}`: {error}")]
    IO { path: PathBuf, error: std::io::Error },
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
    #[error("Parameter `{0}` not found")]
    ParameterNotFound(String),
    #[error("Expected an index parameter, but found a regular parameter: {0}")]
    IndexParameterExpected(String),
    #[error("Loading a local parameter reference (name: {0}) requires a parent name space.")]
    LocalParameterReferenceRequiresParent(String),
    #[error("network {0} not found")]
    NetworkNotFound(String),
    #[error("missing initial volume for node: {0}")]
    MissingInitialVolume(String),
    #[error("Pywr core error: {0}")]
    #[cfg(feature = "core")]
    PywrCore(#[from] pywr_core::PywrError),
    #[error("Error loading data from table `{0}` (column: `{1:?}`, index: `{2:?}`) error: {error}", table_ref.table, table_ref.column, table_ref.index)]
    TableRefLoad { table_ref: TableDataRef, error: TableError },
    #[error("Error loading table `{table_def:?}` error: {error}")]
    TableLoad { table_def: DataTable, error: TableError },
    #[error("Circular node reference(s) found.")]
    CircularNodeReference,
    #[error("Circular parameters reference(s) found. Unable to load the following parameters: {0:?}")]
    CircularParameterReference(Vec<String>),
    #[error("unsupported file format")]
    UnsupportedFileFormat,
    #[cfg(feature = "pyo3")]
    #[error("Python error: {0}")]
    PythonError(#[from] PyErr),
    #[error("hdf5 error: {0}")]
    HDF5Error(String),
    #[error("Missing metric set: {0}")]
    MissingMetricSet(String),
    #[error("mismatch in the length of data provided. expected: {expected}, found: {found}")]
    DataLengthMismatch { expected: usize, found: usize },
    #[error("Failed to estimate epsilon for use in the radial basis function.")]
    RbfEpsilonEstimation,
    #[error("Scenario error: {0}")]
    #[cfg(feature = "core")]
    Scenario(#[from] pywr_core::scenario::ScenarioError),
    #[error("Inter-network transfer with name {0} not found")]
    InterNetworkTransferNotFound(String),
    #[error("Invalid rolling window definition on parameter {name}. Must convert to a positive integer.")]
    InvalidRollingWindow { name: String },
    #[error("Failed to load parameter {name}: {error}")]
    LoadParameter { name: String, error: String },
    #[error("Timeseries error: {0}")]
    Timeseries(#[from] TimeseriesError),
    #[error(
        "The output of literal constant values is not supported. This is because they do not have a unique identifier such as a name. If you would like to output a constant value please use a `Constant` parameter."
    )]
    LiteralConstantOutputNotSupported,
    #[error("Chrono out of range error: {0}")]
    OutOfRange(#[from] chrono::OutOfRange),
    #[error("The metric set with name '{0}' contains no metrics")]
    EmptyMetricSet(String),
    #[error("Missing the following attribute {attr:?} on node {name:?}.")]
    MissingNodeAttribute { attr: String, name: String },
    #[error("The feature '{0}' must be enabled to use this functionality.")]
    FeatureNotEnabled(String),
    #[cfg(feature = "core")]
    #[error("Shape error: {0}")]
    NdarrayShape(#[from] ShapeError),
    #[cfg(feature = "core")]
    #[error("Placeholder node `{name}` cannot be added to a model.")]
    PlaceholderNodeNotAllowed { name: String },
}

#[cfg(all(feature = "core", feature = "pyo3"))]
impl From<SchemaError> for PyErr {
    fn from(err: SchemaError) -> PyErr {
        pyo3::exceptions::PyRuntimeError::new_err(err.to_string())
    }
}

#[derive(Error, Debug, PartialEq, Eq, Clone)]
#[cfg_attr(feature = "pyo3", pyclass)]
pub enum ComponentConversionError {
    #[error("Failed to convert `{attr}` on node `{name}`: {error}")]
    Node {
        attr: String,
        name: String,
        error: ConversionError,
    },
    #[error("Failed to convert `{attr}` on parameter `{name}`: {error}")]
    Parameter {
        attr: String,
        name: String,
        error: ConversionError,
    },
}

#[derive(Error, Debug, PartialEq, Eq, Clone)]
#[cfg_attr(feature = "pyo3", pyclass)]
pub enum ConversionError {
    #[error("Constant float value cannot be a parameter reference.")]
    ConstantFloatReferencesParameter {},
    #[error("Constant float value cannot be an inline parameter.")]
    ConstantFloatInlineParameter {},
    #[error("Missing one of the following attributes {attrs:?}.")]
    MissingAttribute { attrs: Vec<String> },
    #[error("The following attributes are unexpected {attrs:?}.")]
    UnexpectedAttribute { attrs: Vec<String> },
    #[error("The following attributes are defined {attrs:?}. Only 1 is allowed.")]
    AmbiguousAttributes { attrs: Vec<String> },
    #[error("Can not convert a float constant to an index constant.")]
    FloatToIndex {},
    #[error("Attribute {attr:?} on is not allowed .")]
    ExtraAttribute { attr: String },
    #[error("Custom node of type {ty:?} is not supported .")]
    CustomTypeNotSupported { ty: String },
    #[error("Conversion of one of the following attributes {attrs:?} is not supported.")]
    UnsupportedAttribute { attrs: Vec<String> },
    #[error("Conversion of the following feature is not supported: {feature}")]
    UnsupportedFeature { feature: String },
    #[error("Type `{ty:?}` are not supported in Pywr v2. {instead:?}")]
    DeprecatedParameter { ty: String, instead: String },
    #[error("Expected `{expected}`, found `{actual}`")]
    UnexpectedType { expected: String, actual: String },
    #[error("Failed to convert `{attr}` on table `{name}`: {error}")]
    TableRef { attr: String, name: String, error: String },
    #[error("Unrecognised type: {ty}")]
    UnrecognisedType { ty: String },
    #[error("Non-constant value cannot be converted automatically.")]
    NonConstantValue {},
    #[error("{found:?} value(s) found, {expected:?} were expected")]
    IncorrectNumberOfValues { expected: usize, found: usize },
}
