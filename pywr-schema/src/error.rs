#[cfg(feature = "core")]
use crate::data_tables::{TableCollectionError, TableDataRef};
use crate::digest::ChecksumError;
use crate::edge::Edge;
use crate::nodes::{NodeAttribute, NodeComponent, NodeSlot, NodeType};
use crate::timeseries::TimeseriesError;
use jiff::civil::DateTime;
#[cfg(feature = "core")]
use ndarray::ShapeError;
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
use std::path::PathBuf;
use thiserror::Error;

/// A node name that is used by more than one node in a network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateNodeName {
    /// The duplicated name.
    pub name: String,
    /// The number of nodes with this name.
    pub nodes: usize,
    /// The number of virtual nodes with this name.
    pub virtual_nodes: usize,
}

impl std::fmt::Display for DuplicateNodeName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "The name `{}` is used by {} node(s) and {} virtual node(s), but each name must be unique.",
            self.name, self.nodes, self.virtual_nodes
        )
    }
}

/// The reason an [`Edge`] is invalid.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum EdgeProblem {
    /// The `from_node` is not an entry of `nodes`.
    #[error("There is no node named `{0}` to connect from.")]
    UnknownFromNode(String),
    /// The `to_node` is not an entry of `nodes`.
    #[error("There is no node named `{0}` to connect to.")]
    UnknownToNode(String),
    #[error("A node cannot be connected to itself.")]
    SelfEdge,
    #[error("The `{node_type}` node has no output slot `{slot}`.")]
    UnknownFromSlot { node_type: NodeType, slot: NodeSlot },
    #[error("The `{node_type}` node has no input slot `{slot}`.")]
    UnknownToSlot { node_type: NodeType, slot: NodeSlot },
    #[error("The `{0}` node cannot receive flow.")]
    NoInflow(NodeType),
    #[error("The `{0}` node cannot provide flow.")]
    NoOutflow(NodeType),
}

/// An edge that [`crate::NetworkSchema::validate`] rejected, and why.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[error("The edge `{edge}` is invalid. {problem}")]
pub struct EdgeValidationError {
    /// The invalid edge.
    pub edge: Edge,
    /// The first problem [`crate::NetworkSchema::validate_edge`] found with it.
    pub problem: EdgeProblem,
}

/// A problem with a model that is not about any one of its networks, found by
/// [`crate::model::TimeDomain::validate`].
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ModelProblem {
    /// The simulation period ends before it starts.
    #[error("The simulation period ends before it starts: `end` ({end}) precedes `start` ({start}).")]
    EndBeforeStart { start: DateTime, end: DateTime },
    /// A timestep frequency string that cannot be parsed as a [`jiff::Span`].
    #[error("The timestep frequency `{freq}` could not be parsed as a duration: {error}")]
    UnparsableFrequency { freq: String, error: String },
    /// A timestep frequency string that parses, but is zero or negative.
    #[error("The timestep frequency `{freq}` is not a positive duration.")]
    NonPositiveFrequency { freq: String },
}

/// A problem with one network, found by [`crate::NetworkSchema::validate`].
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum NetworkProblem {
    /// A name used by more than one entry of `nodes` or `virtual_nodes`.
    #[error("{0}")]
    DuplicateNodeName(DuplicateNodeName),
    /// An edge that could not connect the nodes it names.
    #[error("{0}")]
    InvalidEdge(EdgeValidationError),
}

/// The most problems the message of a [`NetworkValidationError`] or a [`ValidationError`] lists.
///
/// The message counts the rest; the error itself always holds every problem.
pub const MAX_PROBLEMS_IN_MESSAGE: usize = 10;

/// Write a heading that counts the problems, then one line for each of the first
/// [`MAX_PROBLEMS_IN_MESSAGE`], then a count of any left over.
fn write_problems(
    f: &mut std::fmt::Formatter<'_>,
    subject: &str,
    count: usize,
    problems: impl Iterator<Item = String>,
) -> std::fmt::Result {
    write!(f, "{subject} has {count} problem(s):")?;

    for problem in problems.take(MAX_PROBLEMS_IN_MESSAGE) {
        write!(f, "\n- {problem}")?;
    }

    if count > MAX_PROBLEMS_IN_MESSAGE {
        write!(f, "\n- ... and {} more.", count - MAX_PROBLEMS_IN_MESSAGE)?;
    }

    Ok(())
}

/// Every problem [`crate::NetworkSchema::validate`] found with one network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkValidationError {
    /// The network's name in a [`crate::MultiNetworkModelSchema`]. `None` for a network validated
    /// on its own, or as part of a [`crate::ModelSchema`].
    pub name: Option<String>,
    /// Never empty. Duplicate names first, sorted by name, then invalid edges in the order listed.
    pub problems: Vec<NetworkProblem>,
}

impl std::fmt::Display for NetworkValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let subject = match &self.name {
            Some(name) => format!("The network `{name}`"),
            None => "The network".to_string(),
        };

        write_problems(
            f,
            &subject,
            self.problems.len(),
            self.problems.iter().map(ToString::to_string),
        )
    }
}

impl std::error::Error for NetworkValidationError {}

/// Every problem [`crate::ModelSchema::validate`] or [`crate::MultiNetworkModelSchema::validate`]
/// found, with the model's own problems kept apart from those of its networks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// The problems that are not about any one network.
    pub model: Vec<ModelProblem>,
    /// The networks that have problems.
    pub networks: Vec<NetworkValidationError>,
}

impl ValidationError {
    /// `Ok` if no problems were found, or `Err` with them all.
    pub(crate) fn into_result(self) -> Result<(), Self> {
        if self.model.is_empty() && self.networks.is_empty() {
            Ok(())
        } else {
            Err(self)
        }
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.model.len() + self.networks.iter().map(|n| n.problems.len()).sum::<usize>();

        let model = self.model.iter().map(ToString::to_string);

        let networks = self.networks.iter().flat_map(|network| {
            network.problems.iter().map(move |problem| match &network.name {
                Some(name) => format!("Network `{name}`: {problem}"),
                None => problem.to_string(),
            })
        });

        write_problems(f, "The model", count, model.chain(networks))
    }
}

impl std::error::Error for ValidationError {}

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
    Scenario(#[from] pywr_core::scenario::ScenarioDomainBuilderError),
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
    #[error("Placeholder output `{name}` cannot be added to a model.")]
    PlaceholderOutputNotAllowed { name: String },
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
#[cfg_attr(feature = "pyo3", pyclass(from_py_object))]
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
#[cfg_attr(feature = "pyo3", pyclass(from_py_object))]
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
    #[error("Scenario combinations defined without any groups.")]
    ScenarioCombinationsWithoutGroups {},
}
