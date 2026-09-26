#[cfg(feature = "core")]
use crate::data_tables::{TableCollectionError, TableDataRef};
use crate::digest::ChecksumError;
use crate::edge::Edge;
use crate::nodes::{NodeAttribute, NodeComponent, NodeSlot, NodeType, VirtualNodeType};
#[cfg(feature = "core")]
use crate::time_series::LoadedTimeSeriesCollectionError;
use jiff::civil::DateTime;
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

/// The subject of a message about a reference, naming the network holding it when the model has
/// more than one.
fn reference_subject(network: Option<&str>, owner: &str) -> String {
    match network {
        Some(network) => format!("The {owner} in the network `{network}`"),
        None => format!("The {owner}"),
    }
}

/// A problem with a model's scenarios, found by
/// [`ScenarioDomain::validate`](crate::model::ScenarioDomain::validate), or with a reference to
/// one of its groups, found by [`crate::ModelSchema::validate`].
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ScenarioProblem {
    /// A name used by more than one entry of `groups`.
    #[error("The name `{name}` is used by {count} scenario groups, but each name must be unique.")]
    DuplicateGroupName { name: String, count: usize },
    /// A group whose `size` is zero.
    #[error("The scenario group `{group}` has a size of zero, but a group must have at least one scenario.")]
    EmptyGroup { group: String },
    /// A group whose `labels` do not number its `size`.
    #[error(
        "The scenario group `{group}` has {found} label(s) for its {expected} scenario(s). A group with labels must have one for each of its scenarios."
    )]
    IncorrectNumberOfLabels {
        group: String,
        found: usize,
        expected: usize,
    },
    /// A label used by more than one scenario of a group. A label resolves to the first scenario
    /// holding it, so the rest could never be named.
    #[error(
        "The scenario group `{group}` uses the label `{label}` for {count} of its scenarios, but each label must be unique within its group."
    )]
    DuplicateLabel { group: String, label: String, count: usize },
    /// A `Slice` subset whose `end` is not after its `start`, so it names no scenarios.
    #[error(
        "The `Slice` subset of the scenario group `{group}` runs from {start} to {end}, but a slice must start before it ends."
    )]
    EmptySlice { group: String, start: usize, end: usize },
    /// A `Slice` subset that reaches past the end of its group.
    #[error(
        "The `Slice` subset of the scenario group `{group}` ends at {end}, but the group has only {size} scenario(s)."
    )]
    SliceOutOfRange { group: String, size: usize, end: usize },
    /// A subset that names nothing.
    #[error("The subset of the scenario group `{group}` is empty, but a subset must name at least one scenario.")]
    EmptySubset { group: String },
    /// An `Indices` subset entry that is not a scenario of its group.
    #[error(
        "The `Indices` subset of the scenario group `{group}` names the scenario {index}, but the group has only {size} scenario(s)."
    )]
    SubsetIndexOutOfRange { group: String, size: usize, index: usize },
    /// An `Indices` subset naming one scenario more than once.
    #[error(
        "The `Indices` subset of the scenario group `{group}` names the scenario {index} {count} times, but a subset must name each scenario at most once."
    )]
    DuplicateSubsetIndex { group: String, index: usize, count: usize },
    /// A `Labels` subset entry that is not one of its group's labels.
    #[error(
        "The `Labels` subset of the scenario group `{group}` names the label `{label}`, which the group does not have."
    )]
    SubsetLabelNotFound { group: String, label: String },
    /// A `Labels` subset naming one label more than once.
    #[error(
        "The `Labels` subset of the scenario group `{group}` names the label `{label}` {count} times, but a subset must name each scenario at most once."
    )]
    DuplicateSubsetLabel { group: String, label: String, count: usize },
    /// A `Labels` subset on a group that has no `labels`.
    #[error("The scenario group `{group}` has a `Labels` subset, but the group has no `labels` for it to name.")]
    SubsetNeedsGroupLabels { group: String },
    /// A subset given alongside `combinations`. Either can constrain the domain, but not both.
    #[error(
        "The scenarios have `combinations` as well as a subset on the scenario group `{group}`. Only one of the two can constrain the domain."
    )]
    CombinationsAndSubset { group: String },
    /// `combinations` given with no groups to combine.
    #[error("The scenarios have `combinations`, but no `groups` for them to combine.")]
    CombinationsWithoutGroups,
    /// An empty list of `combinations`.
    #[error("The scenarios have an empty list of `combinations`, but a model must simulate at least one scenario.")]
    EmptyCombinations,
    /// A combination that does not have one entry for each group.
    #[error(
        "The combination at index {combination} has {found} entry(s) for the model's {expected} scenario group(s). Each combination must have one entry for each group, in the order the groups are defined."
    )]
    IncorrectCombinationLength {
        combination: usize,
        found: usize,
        expected: usize,
    },
    /// A combination entry that is not a scenario of its group.
    #[error(
        "The combination at index {combination} names the scenario {index} of the scenario group `{group}`, but the group has only {size} scenario(s)."
    )]
    CombinationIndexOutOfRange {
        combination: usize,
        group: String,
        size: usize,
        index: usize,
    },
    /// A combination entry naming a label its group does not have.
    #[error(
        "The combination at index {combination} names the label `{label}` of the scenario group `{group}`, which the group does not have."
    )]
    CombinationLabelNotFound {
        combination: usize,
        group: String,
        label: String,
    },
    /// A combination entry naming a label of a group that has no `labels`.
    #[error(
        "The combination at index {combination} names the label `{label}` of the scenario group `{group}`, but the group has no `labels`. Name its scenarios by index instead."
    )]
    CombinationNeedsGroupLabels {
        combination: usize,
        group: String,
        label: String,
    },
    /// A reference to a group that `scenarios.groups` does not define.
    #[error(
        "{} refers to the scenario group `{group}`, which the model's scenarios do not define.", reference_subject(.network.as_deref(), .owner)
    )]
    UnknownGroupReference {
        /// The network holding the reference, for a [`crate::MultiNetworkModelSchema`]. `None`
        /// for a [`crate::ModelSchema`], which has only one network.
        network: Option<String>,
        /// The component holding it, as an [`Owner`](crate::visit::Owner) displays itself.
        owner: String,
        /// The group it names.
        group: String,
    },
}

/// The problems [`ScenarioDomain::validate`](crate::model::ScenarioDomain::validate) found, as an
/// error, so that building a model can carry them as a [`source`](std::error::Error::source).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioValidationError {
    /// Never empty, and ordered as
    /// [`ScenarioDomain::validate`](crate::model::ScenarioDomain::validate) documents.
    pub problems: Vec<ScenarioProblem>,
}

impl ScenarioValidationError {
    /// A multi-line report, as [`NetworkValidationError::report`].
    pub fn report(&self) -> impl std::fmt::Display {
        struct Report<'a>(&'a ScenarioValidationError);

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
        write!(f, "The scenarios have {} problem(s)", self.problems.len())
    }
}

impl std::fmt::Display for ScenarioValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.write_summary(f)?;
        write!(f, ".")
    }
}

impl std::error::Error for ScenarioValidationError {}

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
    /// The problems with the model's scenarios, including references to groups it does not define.
    pub scenarios: Vec<ScenarioProblem>,
    /// The networks that have problems.
    pub networks: Vec<NetworkValidationError>,
}

impl ValidationError {
    /// `Ok` if no problems were found, or `Err` with them all.
    pub(crate) fn into_result(self) -> Result<(), Self> {
        if self.model.is_empty() && self.scenarios.is_empty() && self.networks.is_empty() {
            Ok(())
        } else {
            Err(self)
        }
    }

    /// As [`NetworkValidationError::report`], with the scenarios' problems and then each
    /// network's listed under the model's, and named with the network unless it is the model's
    /// only one.
    pub fn report(&self) -> impl std::fmt::Display {
        struct Report<'a>(&'a ValidationError);

        impl std::fmt::Display for Report<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.write_summary(f)?;
                write!(f, ":")?;

                let model = self.0.model.iter().map(ToString::to_string);
                let scenarios = self.0.scenarios.iter().map(ToString::to_string);

                let networks = self.0.networks.iter().flat_map(|network| {
                    network.problems.iter().map(move |problem| match &network.name {
                        Some(name) => format!("Network `{name}`: {problem}"),
                        None => problem.to_string(),
                    })
                });

                write_problem_lines(f, model.chain(scenarios).chain(networks))
            }
        }

        Report(self)
    }

    /// Write the summary, without the punctuation that ends it.
    fn write_summary(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count =
            self.model.len() + self.scenarios.len() + self.networks.iter().map(|n| n.problems.len()).sum::<usize>();

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
    #[error(transparent)]
    Infallible(#[from] std::convert::Infallible),
    #[error("IO error on path `{path}`.")]
    IO {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
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
    #[error("Error loading data from table `{0}` (column: `{1:?}`, row: `{2:?}`).", table_ref.table, table_ref.column, table_ref.row)]
    #[cfg(feature = "core")]
    TableRefLoad {
        table_ref: TableDataRef,
        #[source]
        source: Box<TableCollectionError>,
    },
    #[cfg(feature = "pyo3")]
    #[error("Python error.")]
    PythonError(#[from] PyErr),
    #[cfg(feature = "hdf5")]
    #[error("HDF5 error with file at `{path}`.")]
    HDF5Error {
        path: PathBuf,
        #[source]
        source: hdf5_metno::Error,
    },
    #[error("Multiple metric-sets not supported. {0}")]
    MultipleMetricSetsNotSupported(String),
    #[error("Mismatch in the length of data provided. expected: {expected}, found: {found}")]
    DataLengthMismatch { expected: usize, found: usize },
    #[error("Failed to estimate epsilon for use in the radial basis function.")]
    RbfEpsilonEstimation,
    #[error("Inter-network transfer with name {0} not found")]
    InterNetworkTransferNotFound(String),
    #[error("Invalid rolling window definition on parameter {name}. Must convert to a positive integer.")]
    InvalidRollingWindow { name: String },
    #[error("Failed to load parameter {name}: {error}")]
    LoadParameter { name: String, error: String },
    #[cfg(feature = "core")]
    #[error("Loaded time-series error.")]
    TimeSeries(#[from] LoadedTimeSeriesCollectionError),
    #[error(
        "The output of literal constant values is not supported. This is because they do not have a unique identifier such as a name. If you would like to output a constant value please use a `Constant` parameter."
    )]
    LiteralConstantOutputNotSupported,
    #[error("The metric set with name '{0}' contains no metrics")]
    EmptyMetricSet(String),
    #[error("Missing the following attribute {attr:?} on node {name:?}.")]
    MissingNodeAttribute { attr: String, name: String },
    #[error("The feature '{0}' must be enabled to use this functionality.")]
    FeatureNotEnabled(String),
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
    #[error("Checksum error.")]
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
