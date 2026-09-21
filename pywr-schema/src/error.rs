#[cfg(feature = "core")]
use crate::data_tables::{TableCollectionError, TableDataRef};
use crate::digest::ChecksumError;
use crate::edge::Edge;
use crate::nodes::{NodeAttribute, NodeComponent, NodeSlot, NodeType, VirtualNodeType};
#[cfg(feature = "core")]
use crate::time_series::LoadedTimeSeriesCollectionError;
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

/// The `"input"` or `"output"` slots a node has, for the end of a message about one it does not.
/// `None` is a node type that never has such slots, as opposed to a node configured without any.
fn slot_list_message(end: &str, slots: Option<&[NodeSlot]>) -> String {
    match slots {
        None => format!("Nodes of this type have no {end} slots."),
        Some([]) => format!("As configured, it has no {end} slots."),
        Some(slots) => {
            let slots: Vec<String> = slots.iter().map(|slot| format!("`{slot}`")).collect();
            format!("Its {end} slots are: {}.", slots.join(", "))
        }
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
    /// The `from_node` is a virtual node, which an edge cannot connect.
    #[error(
        "The `{node_type}` virtual node `{name}` cannot be connected from. Only nodes in `nodes` can be connected by edges."
    )]
    VirtualFromNode { name: String, node_type: VirtualNodeType },
    /// The `to_node` is a virtual node, which an edge cannot connect.
    #[error(
        "The `{node_type}` virtual node `{name}` cannot be connected to. Only nodes in `nodes` can be connected by edges."
    )]
    VirtualToNode { name: String, node_type: VirtualNodeType },
    /// Both ends are the same node.
    #[error("The node `{0}` cannot be connected to itself.")]
    SelfEdge(String),
    /// The `from_slot` is not one of the `from_node`'s output slots.
    #[error("The `{node_type}` node `{name}` has no output slot `{slot}`. {}", slot_list_message("output", .valid.as_deref()))]
    UnknownFromSlot {
        name: String,
        node_type: NodeType,
        slot: NodeSlot,
        valid: Option<Vec<NodeSlot>>,
    },
    /// The `to_slot` is not one of the `to_node`'s input slots.
    #[error("The `{node_type}` node `{name}` has no input slot `{slot}`. {}", slot_list_message("input", .valid.as_deref()))]
    UnknownToSlot {
        name: String,
        node_type: NodeType,
        slot: NodeSlot,
        valid: Option<Vec<NodeSlot>>,
    },
    /// The `to_node` is a node that cannot be the receiving end of an edge.
    #[error("The `{node_type}` node `{name}` cannot receive flow.")]
    NoInflow { name: String, node_type: NodeType },
    /// The `from_node` is a node that cannot be the providing end of an edge.
    #[error("The `{node_type}` node `{name}` cannot provide flow.")]
    NoOutflow { name: String, node_type: NodeType },
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
///
/// A name must be unique within its list, not across lists: `nodes` and `virtual_nodes` share one
/// name-space, and `parameters`, `tables`, `time_series` and `metric_sets` each have their own.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum NetworkProblem {
    /// A name used by more than one entry of `nodes` or `virtual_nodes`.
    #[error("{0}")]
    DuplicateNodeName(DuplicateNodeName),
    /// A name used by more than one entry of `parameters`.
    #[error("The name `{name}` is used by {count} parameters, but each name must be unique.")]
    DuplicateParameterName { name: String, count: usize },
    /// A name used by more than one entry of `tables`.
    #[error("The name `{name}` is used by {count} tables, but each name must be unique.")]
    DuplicateTableName { name: String, count: usize },
    /// A name used by more than one entry of `time_series`.
    #[error("The name `{name}` is used by {count} time series, but each name must be unique.")]
    DuplicateTimeSeriesName { name: String, count: usize },
    /// A name used by more than one entry of `metric_sets`.
    #[error("The name `{name}` is used by {count} metric sets, but each name must be unique.")]
    DuplicateMetricSetName { name: String, count: usize },
    /// An edge that could not connect the nodes it names.
    #[error("{0}")]
    InvalidEdge(EdgeValidationError),
}

/// Write one bullet per problem, each on its own line, under a summary written by the caller.
fn write_problem_lines(f: &mut std::fmt::Formatter<'_>, problems: impl Iterator<Item = String>) -> std::fmt::Result {
    for problem in problems {
        write!(f, "\n- {problem}")?;
    }

    Ok(())
}

/// Every problem [`crate::NetworkSchema::validate`] found with one network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkValidationError {
    /// The network's name in a [`crate::MultiNetworkModelSchema`]. `None` for a network validated
    /// on its own, or as part of a [`crate::ModelSchema`].
    pub name: Option<String>,
    /// Never empty. Duplicate names first, list by list in the order nodes, parameters, tables,
    /// time series, metric sets, each sorted by name; then invalid edges in the order listed.
    pub problems: Vec<NetworkProblem>,
}

impl NetworkValidationError {
    /// A multi-line report: the summary, then one line for each problem.
    ///
    /// [`Display`](std::fmt::Display) writes the summary alone, so that this error reads well
    /// inside a caller's own message. Use the report where every problem should be shown, such as
    /// when printing to a terminal.
    pub fn report(&self) -> impl std::fmt::Display {
        struct Report<'a>(&'a NetworkValidationError);

        impl std::fmt::Display for Report<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.write_summary(f)?;
                write!(f, ":")?;
                write_problem_lines(f, self.0.problems.iter().map(ToString::to_string))
            }
        }

        Report(self)
    }

    /// Write the summary, without the punctuation that ends it.
    fn write_summary(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.name {
            Some(name) => write!(f, "The network `{name}`")?,
            None => write!(f, "The network")?,
        }

        write!(f, " has {} problem(s)", self.problems.len())
    }
}

impl std::fmt::Display for NetworkValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.write_summary(f)?;
        write!(f, ".")
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

    /// As [`NetworkValidationError::report`], with each network's problems listed under the
    /// model's, and named with the network unless it is the model's only one.
    pub fn report(&self) -> impl std::fmt::Display {
        struct Report<'a>(&'a ValidationError);

        impl std::fmt::Display for Report<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.write_summary(f)?;
                write!(f, ":")?;

                let model = self.0.model.iter().map(ToString::to_string);

                let networks = self.0.networks.iter().flat_map(|network| {
                    network.problems.iter().map(move |problem| match &network.name {
                        Some(name) => format!("Network `{name}`: {problem}"),
                        None => problem.to_string(),
                    })
                });

                write_problem_lines(f, model.chain(networks))
            }
        }

        Report(self)
    }

    /// Write the summary, without the punctuation that ends it.
    fn write_summary(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.model.len() + self.networks.iter().map(|n| n.problems.len()).sum::<usize>();

        write!(f, "The model has {count} problem(s)")
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.write_summary(f)?;
        write!(f, ".")
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
    #[error(
        "Loading a local parameter reference (name: {0}) requires a either specifying a \"node\" or being used in a node context."
    )]
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
    #[cfg(feature = "core")]
    #[error("TimeSeries error: {0}")]
    TimeSeries(#[from] LoadedTimeSeriesCollectionError),
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
            SchemaError::TimeSeries(err) => err.try_into(),
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
