#[cfg(feature = "core")]
use crate::data_tables::{TableCollectionError, TableDataRef};
use crate::digest::ChecksumError;
use crate::nodes::{NodeAttribute, NodeComponent, NodeSlot};
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
    // Use this error when a node is not found in the schema (i.e. while parsing the schema).
    #[error("Node with name {name} not found in the schema.")]
    NodeNotFound { name: String },
    #[error("Virtual node with name {name} not found in the schema.")]
    VirtualNodeNotFound { name: String },
    // Use this error when a node is not found in a pywr-core network (i.e. during building the network).
    #[error("Node with name `{name}` and sub-name `{}` not found in the network.", .sub_name.as_deref().unwrap_or("None"))]
    CoreNodeNotFound { name: String, sub_name: Option<String> },
    #[error("Attribute `{attr}` not supported.")]
    NodeAttributeNotSupported { attr: NodeAttribute },
    #[error("Component `{attr}` not supported.")]
    NodeComponentNotSupported { attr: NodeComponent },
    #[error("Input slot `{slot}` not supported.")]
    InputNodeSlotNotSupported { slot: NodeSlot },
    #[error("Output slot `{slot}` not supported.")]
    OutputNodeSlotNotSupported { slot: NodeSlot },
    // Use this error when a parameter is not found in the schema (i.e. while parsing the schema).
    #[error("Parameter `{name}` not found in the schema.")]
    ParameterNotFound { name: String, key: Option<String> },
    // Use this error when a parameter is not found in a pywr-core network (i.e. during building the network).
    #[error("Parameter `{name}` not found in the network.")]
    CoreParameterNotFound { name: String, key: Option<String> },
    #[error("Expected an index parameter, but found a regular parameter: {0}")]
    IndexParameterExpected(String),
    #[error("Loading a local parameter reference (name: {0}) requires a parent name space.")]
    LocalParameterReferenceRequiresParent(String),
    #[error("network {0} not found")]
    NetworkNotFound(String),
    #[error("Edge from `{from_node}` to `{to_node}` not found")]
    EdgeNotFound { from_node: String, to_node: String },
    #[error("Pywr core network error: {0}")]
    #[cfg(feature = "core")]
    CoreNetworkError(#[from] pywr_core::NetworkError),
    #[error("Pywr model domain error: {0}")]
    #[cfg(feature = "core")]
    CoreModelDomainError(#[from] pywr_core::models::ModelDomainError),
    #[error("Multi-network model error: {0}")]
    #[cfg(feature = "core")]
    CoreMultiNetworkModelError(#[from] pywr_core::models::MultiNetworkModelError),
    #[error("Metric F64 error: {0}")]
    #[cfg(feature = "core")]
    CoreMetricF64Error(#[from] pywr_core::metric::MetricF64Error),
    #[error("Error loading data from table `{0}` (column: `{1:?}`, row: `{2:?}`) error: {source}", table_ref.table, table_ref.column, table_ref.row)]
    #[cfg(feature = "core")]
    TableRefLoad {
        table_ref: TableDataRef,
        #[source]
        source: Box<TableCollectionError>,
    },
    #[cfg(feature = "pyo3")]
    #[error("Python error: {0}")]
    PythonError(#[from] PyErr),
    #[error("hdf5 error: {0}")]
    HDF5Error(String),
    #[error("Missing metric set: {0}")]
    MissingMetricSet(String),
    #[error("Mismatch in the length of data provided. expected: {expected}, found: {found}")]
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
    #[error("Placeholder parameter `{name}` cannot be added to a model.")]
    PlaceholderParameterNotAllowed { name: String },
    #[error("Node cannot be used in a flow constraint.")]
    NodeNotAllowedInFlowConstraint,
    #[error("Node cannot be used in a storage constraint.")]
    NodeNotAllowedInStorageConstraint,
    #[error("{msg}")]
    InvalidNodeAttributes { msg: String },
    #[error("'{node}' does not have a slot named '{slot}'")]
    NodeConnectionSlotNotFound { node: String, slot: NodeSlot },
    #[error("{msg}")]
    NodeConnectionSlotRequired { msg: String },
    #[error("Checksum error: {0}")]
    ChecksumError(#[from] ChecksumError),
    #[error(
        "Number of values ({values}) for parameter '{name}' does not match the size ({scenarios}) of the specified scenario group '{group}'."
    )]
    ScenarioValuesLengthMismatch {
        values: usize,
        name: String,
        scenarios: usize,
        group: String,
    },
}

#[cfg(all(feature = "core", feature = "pyo3"))]
impl TryFrom<SchemaError> for PyErr {
    type Error = ();
    fn try_from(err: SchemaError) -> Result<Self, Self::Error> {
        match err {
            SchemaError::PythonError(py_err) => Ok(py_err),
            SchemaError::Timeseries(err) => err.try_into(),
            _ => Err(()),
        }
    }
}

#[derive(Error, Debug, PartialEq, Eq, Clone)]
#[cfg_attr(feature = "pyo3", pyclass)]
#[allow(clippy::large_enum_variant)]
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
    #[error("Failed to convert scenario: {error}")]
    Scenarios { error: ConversionError },
    #[error("Failed to convert table: {error}")]
    Table {
        name: String,
        url: PathBuf,
        json: Option<String>,
        error: ConversionError,
    },
    #[error("Failed to convert edge from `{from_node}` to `{to_node}`: {error}")]
    Edge {
        from_node: String,
        to_node: String,
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
    #[error("Scenario slice is invalid: length is {length}, expected 1 or 2.")]
    InvalidScenarioSlice { length: usize },
    #[error("Table conversion is not currently supported: {name}")]
    TableConversionNotSupported { name: String },
    #[error("Invalid slot: {slot}")]
    InvalidSlot { slot: String },
}
