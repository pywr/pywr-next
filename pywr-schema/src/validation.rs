//! The problems found by validating a schema, and the messages that describe them.

use crate::data_tables::{CsvDataTableLookup, DataTableValueType};
use crate::edge::Edge;
use crate::metric::ParameterReturnValue;
use crate::nodes::{NodeComponent, NodeSlot, NodeType, VirtualNodeType};
use crate::parameters::ParameterPhase;
use jiff::civil::DateTime;
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

/// The reason a table reference does not fit the table it names, found by
/// [`DataTable::validate_reference`](crate::data_tables::DataTable::validate_reference).
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum TableReferenceProblem {
    /// The table holds arrays where a single value is read, or the other way round.
    #[error("The table holds `{found}` values, but `{expected}` values are read from it.")]
    WrongValueType {
        expected: DataTableValueType,
        found: DataTableValueType,
    },
    /// The reference gives more or fewer labels than the table's key takes.
    #[error("The table's key takes {expected} label(s), but the reference gives {found}.")]
    WrongKeySize { expected: usize, found: usize },
    /// A label that is empty. `index` is the first one's position in the key, which holds the
    /// `row` labels and then the `column` labels.
    #[error("The reference contains an empty label at index {index} of its key.")]
    EmptyLabel { index: usize },
}

/// Why a list of points cannot be interpolated between.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PointsProblem {
    #[error("There are {x} x value(s) but {y} y value(s), and each point needs one of each.")]
    LengthMismatch { x: usize, y: usize },
    #[error("There are {0} point(s), but interpolating needs at least two.")]
    TooFewPoints(usize),
    /// The first x value that is not greater than the one before it.
    #[error("The x values must strictly increase, but the one at index {index} does not.")]
    NotIncreasing { index: usize },
}

/// Check interpolation points from their x values, `None` where one is not a literal, and their
/// number of y values. Only adjacent literal x values are compared, as core compares them.
pub(crate) fn check_interpolation_points(x: &[Option<f64>], y_count: usize) -> Result<(), Vec<PointsProblem>> {
    let mut problems = Vec::new();

    if x.len() != y_count {
        problems.push(PointsProblem::LengthMismatch { x: x.len(), y: y_count });
    } else if y_count < 2 {
        problems.push(PointsProblem::TooFewPoints(y_count));
    }

    if let Some(index) = x
        .windows(2)
        .position(|pair| matches!(pair, [Some(a), Some(b)] if b <= a))
    {
        problems.push(PointsProblem::NotIncreasing { index: index + 1 });
    }

    if problems.is_empty() { Ok(()) } else { Err(problems) }
}

/// A problem with a parameter's own fields, found by
/// [`Parameter::validate`](crate::parameters::Parameter::validate).
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ParameterProblem {
    #[error("`values` has {found} entry(s), but the control curves require {required}.")]
    ControlCurveValues { required: usize, found: usize },
    #[error("The points in `xp` and `fp` cannot be interpolated between. {0}")]
    Interpolation(PointsProblem),
    /// The day and month given, or their defaults.
    #[error("`reset_day` {day} and `reset_month` {month} do not make a date.")]
    NotADate { day: u64, month: u64 },
    #[error("`residual_days` is {0}, but it can be at most 255.")]
    ResidualDaysTooLarge(u64),
    #[error("`values` has {found} entry(s), but the profile takes {}.", counts_message(.allowed))]
    ProfileValues { allowed: &'static [usize], found: usize },
    #[error("`points` is empty, so `epsilon` cannot be estimated from them.")]
    NoPointsForEpsilon,
    #[error("`epsilon` is 0.")]
    ZeroEpsilon,
    #[error(
        "The points at index {first} and {second} fall on the same day of the year, so they cannot be interpolated between."
    )]
    PointsCoincide { first: usize, second: usize },
    #[error("`denominator` is 0.")]
    ZeroDenominator,
    #[error("`metrics` is empty.")]
    NoMetrics,
    #[error("`index_metric` is {index}, but `metrics` has only {count} entry(s).")]
    IndexOutOfRange { index: u64, count: usize },
    #[error("Neither `target` nor `actual_flow` is set, so it is never calculated.")]
    NoTargetOrActualFlow,
    /// `min_flow` or `max_flow`, which bound only a `target`.
    #[error("`{0}` is set, but it applies only with a `target`.")]
    FlowBoundWithoutTarget(&'static str),
    /// A constant of a hydropower calculation, such as `efficiency`.
    #[error("`{0}` must be above 0.")]
    NotPositive(&'static str),
    #[error("`window_size` is 0.")]
    ZeroWindowSize,
    #[error("`min_values` is {min_values}, but the window holds only {window_size}, so it never fills.")]
    MinValuesAboveWindow { min_values: u64, window_size: u64 },
    #[error("The `AnyNonZero` `tolerance` must not be negative.")]
    NegativeTolerance,
    #[error("`min` is above `max`.")]
    MinAboveMax,
    #[error("`discount_rate` must be above -1.")]
    DiscountRateTooLow,
    #[error("`returned_metrics` has {found} entry(s), but the thresholds require {required}.")]
    ReturnedMetricsCount { required: usize, found: usize },
    #[error("`control_curves` is empty.")]
    NoControlCurves,
    #[error("`coefficients` is empty.")]
    NoCoefficients,
}

/// Counts for a message, such as "12" or "365 or 366".
fn counts_message(counts: &[usize]) -> String {
    counts.iter().map(ToString::to_string).collect::<Vec<_>>().join(" or ")
}

/// Why a storage's initial volume is outside its bounds.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum InitialVolumeProblem {
    #[error("The initial volume is below `min_volume`.")]
    BelowMin,
    #[error("The initial volume is above `max_volume`.")]
    AboveMax,
}

/// A problem with a node's own fields, found by [`Node::validate`](crate::nodes::Node::validate).
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum NodeProblem {
    #[error("{0}")]
    InitialVolume(InitialVolumeProblem),
    /// The field, `rainfall` or `evaporation`.
    #[error("`{0}` is set, but it needs a `surface_area`.")]
    NoSurfaceArea(&'static str),
    #[error("The `control_curve` of the step at index {step} must be from 0 to 1.")]
    ControlCurveOutOfRange { step: usize },
    /// The first control curve below the one before it.
    #[error("The `control_curve` of the step at index {step} is below the one before it.")]
    ControlCurveDecreases { step: usize },
    #[error("The `factor` of the split at index {split} must be between 0 and 1, both excluded.")]
    SplitFactorOutOfRange { split: usize },
    #[error("The slot name `{slot}` is used by {count} splits, but each must be unique.")]
    DuplicateSlotName { slot: String, count: usize },
    #[error("A `Gross` `loss_factor` must be from 0 up to 1, 1 excluded.")]
    GrossLossFactorOutOfRange,
    #[error("A `Net` `loss_factor` must not be negative.")]
    NegativeNetLossFactor,
}

/// A problem with a virtual node's own fields, found by
/// [`VirtualNode::validate`](crate::nodes::VirtualNode::validate).
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum VirtualNodeProblem {
    #[error("{0}")]
    InitialVolume(InitialVolumeProblem),
    #[error("`{day_field}` {day} and `{month_field}` {month} do not make a date.")]
    NotADate {
        day_field: &'static str,
        month_field: &'static str,
        day: i8,
        month: i8,
    },
    #[error(
        "The `Proportion` relationship has {factors} factor(s) for {members} member(s), but needs one fewer factor than members."
    )]
    ProportionFactorCount { factors: usize, members: usize },
    #[error(
        "The `Ratio` relationship has {factors} factor(s) for {members} member(s), but needs one factor per member, and at least one member."
    )]
    RatioFactorCount { factors: usize, members: usize },
    #[error(
        "The `Coefficients` relationship has {factors} factor(s) for {members} member(s), but needs exactly two members, with a factor each."
    )]
    CoefficientsFactorCount { factors: usize, members: usize },
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

/// The components a node builds, for the end of a message about one it does not.
fn component_list_message(built: &[NodeComponent]) -> String {
    if built.is_empty() {
        "As configured, it builds no components.".to_string()
    } else {
        let built: Vec<String> = built.iter().map(|component| format!("`{component}`")).collect();
        format!("It builds: {}.", built.join(", "))
    }
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
    /// A member of an `Aggregated` or `VirtualStorage` node naming a node with no components.
    #[error(
        "The virtual node `{virtual_node}` names the `{node_type}` node `{node}`, but nodes of this type have no components for it to take."
    )]
    MemberWithoutComponents {
        virtual_node: String,
        node: String,
        node_type: NodeType,
    },
    /// A member of an `Aggregated` or `VirtualStorage` node taking a component its node does not
    /// build.
    #[error(
        "The virtual node `{virtual_node}` takes the {}component `{component}` of the `{node_type}` node `{node}`, but that node does not build it. {}", if *.default { "default " } else { "" }, component_list_message(.built)
    )]
    MemberComponentNotBuilt {
        virtual_node: String,
        node: String,
        node_type: NodeType,
        /// The component the member names, or its node's default.
        component: NodeComponent,
        /// Whether the member names no component.
        default: bool,
        /// The components the node does build.
        built: Vec<NodeComponent>,
    },
    /// A member of an `AggregatedStorage` node that is not a storage.
    #[error(
        "The virtual node `{virtual_node}` names the `{node_type}` node `{node}`, but an `AggregatedStorage` node takes only storage nodes."
    )]
    MemberNotStorage {
        virtual_node: String,
        node: String,
        node_type: NodeType,
    },
    /// An index metric naming a parameter that gives a float value.
    #[error(
        "The {owner} uses the {} as an index, but it gives a float value.", named_parameter(.parameter, .node.as_deref())
    )]
    ParameterNotAnIndex {
        owner: String,
        parameter: String,
        node: Option<String>,
    },
    /// A reference without a key naming a multi-valued parameter.
    #[error(
        "The {owner} refers to the {} without a key, but it gives several values, one per key.", named_parameter(.parameter, .node.as_deref())
    )]
    ParameterKeyMissing {
        owner: String,
        parameter: String,
        node: Option<String>,
    },
    /// A reference with a key naming a parameter that gives a single value.
    #[error(
        "The {owner} names the key `{key}` of the {}, but it gives a single value and takes no key.", named_parameter(.parameter, .node.as_deref())
    )]
    ParameterKeyNotAllowed {
        owner: String,
        parameter: String,
        node: Option<String>,
        key: String,
    },
    /// A reference asking a parameter for a value it does not calculate.
    #[error(
        "The {owner} asks {} for its `{return_value}` value, but it is calculated only in the `{phase}` phase.", named_parameter(.parameter, .node.as_deref())
    )]
    ParameterValueNotCalculated {
        owner: String,
        parameter: String,
        node: Option<String>,
        return_value: ParameterReturnValue,
        phase: ParameterPhase,
    },
    /// A CSV table whose lookup pywr cannot load with its type of values.
    #[error("The table `{table}` cannot be loaded. {}", unsupported_lookup_message(.value_type, .lookup))]
    UnsupportedTableLookup {
        table: String,
        value_type: DataTableValueType,
        lookup: CsvDataTableLookup,
    },
    /// A table reference that does not fit the table it names.
    #[error("The {owner} has an invalid reference to the table `{table}`. {problem}")]
    InvalidTableReference {
        owner: String,
        table: String,
        problem: TableReferenceProblem,
    },
    /// A node whose own fields are invalid, found by [`Node::validate`](crate::nodes::Node::validate).
    #[error("The node `{node}` is invalid. {problem}")]
    InvalidNode { node: String, problem: NodeProblem },
    /// A virtual node whose own fields are invalid, found by
    /// [`VirtualNode::validate`](crate::nodes::VirtualNode::validate).
    #[error("The virtual node `{virtual_node}` is invalid. {problem}")]
    InvalidVirtualNode {
        virtual_node: String,
        problem: VirtualNodeProblem,
    },
    /// A parameter whose own fields are invalid, found by
    /// [`Parameter::validate`](crate::parameters::Parameter::validate).
    #[error("The {} is invalid. {problem}", named_parameter(.parameter, .node.as_deref()))]
    InvalidParameter {
        parameter: String,
        /// The node or virtual node holding it, for a local parameter.
        node: Option<String>,
        problem: ParameterProblem,
    },
}

/// Why pywr cannot load a table of `value_type` with `lookup`, for the end of a message.
fn unsupported_lookup_message(value_type: &DataTableValueType, lookup: &CsvDataTableLookup) -> String {
    match (value_type, lookup) {
        (DataTableValueType::Array, CsvDataTableLookup::Both { .. }) => {
            "A table of `Array` values must have a `Row` or `Col` lookup, not `Both`.".to_string()
        }
        (_, CsvDataTableLookup::Row { cols }) => {
            format!("A `Row` lookup's `cols`, the number of index columns, must be 1 to 4, not {cols}.")
        }
        (_, CsvDataTableLookup::Col { rows }) => {
            format!("A `Col` lookup's `rows`, the number of index rows, must be 1 to 4, not {rows}.")
        }
        (_, CsvDataTableLookup::Both { rows, cols }) => format!(
            "A `Both` lookup's `rows` and `cols`, the numbers of index rows and columns, must each be 1 or 2, not {rows} and {cols}."
        ),
    }
}

/// How a message names a parameter, after "the".
fn named_parameter(parameter: &str, node: Option<&str>) -> String {
    match node {
        Some(node) => format!("local parameter `{parameter}` of `{node}`"),
        None => format!("parameter `{parameter}`"),
    }
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
    /// Never empty, and in the order [`crate::NetworkSchema::validate`] lists its checks.
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
