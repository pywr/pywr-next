#[cfg(feature = "core")]
use crate::data_tables::LoadedTableCollection;
use crate::error::{ComponentConversionError, ModelProblem, ScenarioProblem, ValidationError};
#[cfg(feature = "core")]
use crate::error::{ScenarioValidationError, SchemaError};
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::network::{LoadArgs, NetworkSchemaBuildError, NetworkSchemaReadError};
#[cfg(feature = "core")]
use crate::time_series::LoadedTimeSeriesCollection;
use crate::util::duplicates;
use crate::visit::{Owner, Reference, ReferenceMut, VisitMetrics, VisitPaths, VisitReferences};
use crate::{ConversionError, NetworkSchema, NetworkSchemaRef};
use jiff::Span;
use jiff::civil::{DateTime, date};
#[cfg(feature = "core")]
use pywr_core::{
    models::{
        ModelBuilder, ModelDomainBuilder, ModelDomainBuilderError, MultiNetworkEntryBuilder, MultiNetworkModelBuilder,
        MultiNetworkTransferBuilder,
    },
    timestep::TimestepDuration,
};
use pywr_schema_macros::skip_serializing_none;
use schemars::JsonSchema;
use std::collections::BTreeSet;
#[cfg(feature = "core")]
use std::collections::HashMap;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};
use thiserror::Error;

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema)]
pub struct Metadata {
    pub title: String,
    pub description: Option<String>,
    pub minimum_version: Option<String>,
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            title: "Untitled model".to_string(),
            description: None,
            minimum_version: None,
        }
    }
}

impl From<pywr_v1_schema::model::Metadata> for Metadata {
    fn from(v1: pywr_v1_schema::model::Metadata) -> Self {
        Self {
            title: v1
                .title
                .unwrap_or("Model converted from Pywr v1.x with no title.".to_string()),
            description: v1.description,
            minimum_version: v1.minimum_version,
        }
    }
}

/// A timestep defines the time interval between each step in the model.
///
/// The timestep can be defined in three ways:
/// - A fixed number of non-zero hours.
/// - A fixed number of non-zero days.
/// - A frequency string that can be parsed as a [`jiff::Span`] (e.g. '7d' or 'P7D').
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, Display, EnumDiscriminants)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(TimestepType))]
pub enum Timestep {
    /// A fixed number of hours.
    Hours { hours: NonZeroU64 },
    /// A fixed number of days.
    Days { days: NonZeroU64 },
    /// A frequency string that can be parsed as a [`jiff::Span`].
    Frequency { freq: String },
}

impl From<pywr_v1_schema::model::Timestep> for Timestep {
    fn from(v1: pywr_v1_schema::model::Timestep) -> Self {
        match v1 {
            pywr_v1_schema::model::Timestep::Days(d) => Self::Days {
                days: NonZeroU64::new(d).expect("days must be non-zero"),
            },
            pywr_v1_schema::model::Timestep::Frequency(freq) => Self::Frequency { freq },
        }
    }
}

impl Default for Timestep {
    fn default() -> Self {
        Self::Days {
            days: NonZeroU64::new(1).expect("1 is non-zero"),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema)]
pub struct TimeDomain {
    pub start: DateTime,
    pub end: DateTime,
    pub timestep: Timestep,
}

impl Default for TimeDomain {
    fn default() -> Self {
        Self {
            start: date(2000, 1, 1).at(0, 0, 0, 0),
            end: date(2000, 12, 31).at(0, 0, 0, 0),
            timestep: Timestep::default(),
        }
    }
}

impl TimeDomain {
    /// Validate the time domain.
    ///
    /// This checks that the simulation period does not end before it starts, and that a
    /// [`Timestep::Frequency`] string is a duration that `pywr-core` can use. These are the
    /// problems that would otherwise only appear when the model is built.
    ///
    /// Both are checked, and every problem found is returned.
    pub fn validate(&self) -> Result<(), Vec<ModelProblem>> {
        let mut problems = Vec::new();

        // The same instant is a period, if a short one, so `>` rather than `>=`.
        if self.start > self.end {
            problems.push(ModelProblem::EndBeforeStart {
                start: self.start,
                end: self.end,
            });
        }

        if let Timestep::Frequency { freq } = &self.timestep {
            // The same parse that `pywr_core::timestep::TimeDomainBuilder` makes.
            match freq.parse::<Span>() {
                Err(error) => problems.push(ModelProblem::UnparsableFrequency {
                    freq: freq.clone(),
                    error: error.to_string(),
                }),
                Ok(span) if span.is_zero() || span.is_negative() => {
                    problems.push(ModelProblem::NonPositiveFrequency { freq: freq.clone() })
                }
                Ok(_) => {}
            }
        }

        if problems.is_empty() { Ok(()) } else { Err(problems) }
    }
}

impl From<pywr_v1_schema::model::Timestepper> for TimeDomain {
    fn from(v1: pywr_v1_schema::model::Timestepper) -> Self {
        Self {
            start: v1.start,
            end: v1.end,
            timestep: v1.timestep.into(),
        }
    }
}

#[cfg(feature = "core")]
impl From<TimeDomain> for pywr_core::timestep::TimeDomainBuilder {
    fn from(ts: TimeDomain) -> Self {
        let timestep = match ts.timestep {
            Timestep::Hours { hours } => TimestepDuration::Hours(hours),
            Timestep::Days { days } => TimestepDuration::Days(days),
            Timestep::Frequency { freq } => TimestepDuration::Frequency(freq),
        };

        Self::new(ts.start, ts.end, timestep)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScenarioGroupSlice {
    pub start: usize,
    pub end: usize,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScenarioGroupIndices {
    pub indices: Vec<usize>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScenarioGroupLabels {
    pub labels: Vec<String>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema, Display, EnumDiscriminants)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(ScenarioGroupSubsetType))]
pub enum ScenarioGroupSubset {
    Slice(ScenarioGroupSlice),
    Indices(ScenarioGroupIndices),
    Labels(ScenarioGroupLabels),
}

/// A scenario group defines a set of scenarios that can be run in a model.
///
/// A scenario group is defined by a name and a size. The size is the number of scenarios in the group.
/// Optional labels can be defined for the group. These labels are used in output data
/// to identify the scenario group. A subset can be defined to simulate only part of the group.
///
/// See also the examples in the [`ScenarioDomain`] documentation.
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema)]
pub struct ScenarioGroup {
    pub name: String,
    pub size: usize,
    pub labels: Option<Vec<String>>,
    pub subset: Option<ScenarioGroupSubset>,
}

impl ScenarioGroup {
    /// Every problem with this group: its size, then its labels, then its subset.
    ///
    /// Its name is not checked here; a name is only a problem beside the other groups'.
    fn problems(&self) -> Vec<ScenarioProblem> {
        let mut problems = Vec::new();

        if self.size == 0 {
            problems.push(ScenarioProblem::EmptyGroup {
                group: self.name.clone(),
            });
        }

        if let Some(labels) = &self.labels {
            if labels.len() != self.size {
                problems.push(ScenarioProblem::IncorrectNumberOfLabels {
                    group: self.name.clone(),
                    found: labels.len(),
                    expected: self.size,
                });
            }

            problems.extend(duplicates(labels, String::as_str).into_iter().map(|(label, count)| {
                ScenarioProblem::DuplicateLabel {
                    group: self.name.clone(),
                    label: label.to_string(),
                    count,
                }
            }));
        }

        match &self.subset {
            Some(ScenarioGroupSubset::Slice(slice)) => {
                // A `start` past the group is always caught by one of these two checks.
                if slice.start >= slice.end {
                    problems.push(ScenarioProblem::EmptySlice {
                        group: self.name.clone(),
                        start: slice.start,
                        end: slice.end,
                    });
                }

                if slice.end > self.size {
                    problems.push(ScenarioProblem::SliceOutOfRange {
                        group: self.name.clone(),
                        size: self.size,
                        end: slice.end,
                    });
                }
            }
            Some(ScenarioGroupSubset::Indices(subset)) => {
                if subset.indices.is_empty() {
                    problems.push(ScenarioProblem::EmptySubset {
                        group: self.name.clone(),
                    });
                }

                // A set, so that an entry repeated out of range is reported once.
                let out_of_range: BTreeSet<usize> =
                    subset.indices.iter().copied().filter(|i| *i >= self.size).collect();

                problems.extend(
                    out_of_range
                        .into_iter()
                        .map(|index| ScenarioProblem::SubsetIndexOutOfRange {
                            group: self.name.clone(),
                            size: self.size,
                            index,
                        }),
                );

                problems.extend(
                    duplicates(&subset.indices, |index| *index)
                        .into_iter()
                        .map(|(index, count)| ScenarioProblem::DuplicateSubsetIndex {
                            group: self.name.clone(),
                            index,
                            count,
                        }),
                );
            }
            Some(ScenarioGroupSubset::Labels(subset)) => {
                if subset.labels.is_empty() {
                    problems.push(ScenarioProblem::EmptySubset {
                        group: self.name.clone(),
                    });
                }

                match &self.labels {
                    None => problems.push(ScenarioProblem::SubsetNeedsGroupLabels {
                        group: self.name.clone(),
                    }),
                    Some(labels) => {
                        let missing: BTreeSet<&String> = subset.labels.iter().filter(|l| !labels.contains(l)).collect();

                        problems.extend(missing.into_iter().map(|label| ScenarioProblem::SubsetLabelNotFound {
                            group: self.name.clone(),
                            label: label.clone(),
                        }));
                    }
                }

                problems.extend(
                    duplicates(&subset.labels, String::as_str)
                        .into_iter()
                        .map(|(label, count)| ScenarioProblem::DuplicateSubsetLabel {
                            group: self.name.clone(),
                            label: label.to_string(),
                            count,
                        }),
                );
            }
            None => {}
        }

        problems
    }

    /// The problem with `entry`, if it does not name a scenario of this group. `combination` is
    /// the entry's combination's position in `combinations`.
    fn combination_entry_problem(&self, combination: usize, entry: &ScenarioLabelOrIndex) -> Option<ScenarioProblem> {
        match entry {
            ScenarioLabelOrIndex::Index(index) => {
                (*index >= self.size).then(|| ScenarioProblem::CombinationIndexOutOfRange {
                    combination,
                    group: self.name.clone(),
                    size: self.size,
                    index: *index,
                })
            }
            ScenarioLabelOrIndex::Label(label) => match &self.labels {
                None => Some(ScenarioProblem::CombinationNeedsGroupLabels {
                    combination,
                    group: self.name.clone(),
                    label: label.clone(),
                }),
                Some(labels) => (!labels.contains(label)).then(|| ScenarioProblem::CombinationLabelNotFound {
                    combination,
                    group: self.name.clone(),
                    label: label.clone(),
                }),
            },
        }
    }
}

#[cfg(feature = "core")]
impl From<ScenarioGroup> for pywr_core::scenario::ScenarioGroupBuilder {
    fn from(value: ScenarioGroup) -> Self {
        let mut builder = pywr_core::scenario::ScenarioGroupBuilder::new(&value.name, value.size);

        if let Some(labels) = value.labels {
            builder.with_labels(&labels);
        }

        if let Some(subset) = value.subset {
            match subset {
                ScenarioGroupSubset::Slice(slice) => {
                    builder.with_subset_slice(slice.start, slice.end);
                }
                ScenarioGroupSubset::Indices(indices) => {
                    builder.with_subset_indices(indices.indices);
                }
                ScenarioGroupSubset::Labels(labels) => {
                    builder.with_subset_labels(&labels.labels);
                }
            }
        }

        builder
    }
}

impl TryFrom<pywr_v1_schema::model::Scenario> for ScenarioGroup {
    type Error = ConversionError;

    fn try_from(v1: pywr_v1_schema::model::Scenario) -> Result<Self, Self::Error> {
        let subset = v1
            .slice
            .map(|s| match s.len() {
                1 => {
                    let start = 0;
                    let end = v1.size;
                    Ok(ScenarioGroupSubset::Slice(ScenarioGroupSlice { start, end }))
                }
                2 => {
                    let start = s[0].unwrap_or_default();
                    let end = match s[1] {
                        Some(v) => v,
                        None => v1.size,
                    };
                    Ok(ScenarioGroupSubset::Slice(ScenarioGroupSlice { start, end }))
                }
                3 => {
                    let start = s[0].unwrap_or_default();
                    let end = match s[1] {
                        Some(v) => v,
                        None => v1.size,
                    };
                    match s[2] {
                        Some(step) => {
                            let indices = (start..end).step_by(step).collect();
                            Ok(ScenarioGroupSubset::Indices(ScenarioGroupIndices { indices }))
                        }
                        None => Ok(ScenarioGroupSubset::Slice(ScenarioGroupSlice { start, end })),
                    }
                }
                _ => Err(ConversionError::InvalidScenarioSlice { length: s.len() }),
            })
            .transpose()?;

        Ok(Self {
            name: v1.name,
            size: v1.size,
            labels: v1.ensemble_names,
            subset,
        })
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema, Display, EnumDiscriminants)]
#[serde(untagged)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(ScenarioLabelOrIndexType))]
pub enum ScenarioLabelOrIndex {
    Label(String),
    Index(usize),
}

#[cfg(feature = "core")]
impl From<ScenarioLabelOrIndex> for pywr_core::scenario::ScenarioLabelOrIndex {
    fn from(val: ScenarioLabelOrIndex) -> pywr_core::scenario::ScenarioLabelOrIndex {
        match val {
            ScenarioLabelOrIndex::Label(label) => pywr_core::scenario::ScenarioLabelOrIndex::Label(label),
            ScenarioLabelOrIndex::Index(index) => pywr_core::scenario::ScenarioLabelOrIndex::Index(index),
        }
    }
}

/// A scenario domain is a collection of scenario groups that define the possible scenarios that
/// can be run in a model.
///
/// Each scenario group has a name and size. The full space of the domain is defined as the
/// cartesian product of the sizes of each group. For simulation purposes, the domain can be
/// constrained (or "subsetted") by defining a subset for each group. A subset can be defined
/// using specific labels or indices of the group, or using slice of the group. The slice is a contiguous
/// subset of the group that will be used in the simulation. The slice is defined by the `start`
/// and `end` indices of the group. The `start` index is inclusive and the `end` index is exclusive.
///
/// Alternatively, the domain can be constrained by defining a list of combinations of the groups
/// that will be used in the simulation. The combinations are defined as a list of lists of indices
/// of the groups.
///
/// It is an error if both a `slice`(s) and `combinations` are defined.
///
/// # JSON Examples
///
/// The examples below show how a scenario group can be defined in JSON.
///
/// ```json
#[doc = include_str!("doc_examples/scenario_domain1.json")]
/// ```
///
/// The example below shows how a scenario group can be defined with custom labels. In this
/// case Roman numerals are used to identify the individual scenarios.
///
/// ```json
#[doc = include_str!("doc_examples/scenario_domain2.json")]
/// ```
///
/// The example below shows how to define two scenario groups.
///
/// ```json
#[doc = include_str!("doc_examples/scenario_domain3.json")]
/// ```
#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScenarioDomain {
    /// The groups that define the scenario domain.
    pub groups: Vec<ScenarioGroup>,
    /// Optional combinations of the groups that allow simulation of specific scenarios.
    pub combinations: Option<Vec<Vec<ScenarioLabelOrIndex>>>,
}

impl ScenarioDomain {
    /// Validate the scenario domain, returning every problem found. See [`ScenarioProblem`] for
    /// the problems detected.
    ///
    /// The references a network makes to these groups are checked by [`ModelSchema::validate`].
    pub fn validate(&self) -> Result<(), Vec<ScenarioProblem>> {
        let mut problems: Vec<ScenarioProblem> = duplicates(&self.groups, |group| group.name.as_str())
            .into_iter()
            .map(|(name, count)| ScenarioProblem::DuplicateGroupName {
                name: name.to_string(),
                count,
            })
            .collect();

        problems.extend(self.groups.iter().flat_map(ScenarioGroup::problems));

        problems.extend(self.combination_problems());

        if problems.is_empty() { Ok(()) } else { Err(problems) }
    }

    /// Every problem with `combinations`, in the order they are listed.
    fn combination_problems(&self) -> Vec<ScenarioProblem> {
        let Some(combinations) = &self.combinations else {
            return Vec::new();
        };

        if self.groups.is_empty() {
            return vec![ScenarioProblem::CombinationsWithoutGroups];
        }

        // A subset and a combination are two ways to constrain the same domain, and `pywr-core`
        // refuses to apply both.
        let mut problems: Vec<ScenarioProblem> = self
            .groups
            .iter()
            .filter(|group| group.subset.is_some())
            .map(|group| ScenarioProblem::CombinationsAndSubset {
                group: group.name.clone(),
            })
            .collect();

        if combinations.is_empty() {
            problems.push(ScenarioProblem::EmptyCombinations);
        }

        for (combination, entries) in combinations.iter().enumerate() {
            if entries.len() != self.groups.len() {
                problems.push(ScenarioProblem::IncorrectCombinationLength {
                    combination,
                    found: entries.len(),
                    expected: self.groups.len(),
                });
            }

            // Entries past the last group have no group to be checked against; the length
            // problem above covers them.
            for (entry, group) in entries.iter().zip(&self.groups) {
                problems.extend(group.combination_entry_problem(combination, entry));
            }
        }

        problems
    }
}

#[cfg(feature = "core")]
impl ScenarioDomain {
    /// The builder for this domain, once [`validate`](Self::validate) accepts it, as
    /// `NetworkSchema::add_to_network` validates the network before building it.
    fn validated_builder(&self) -> Result<pywr_core::scenario::ScenarioDomainBuilder, ScenarioValidationError> {
        self.validate()
            .map_err(|problems| ScenarioValidationError { problems })?;

        Ok(self.clone().into())
    }
}

impl TryFrom<Vec<pywr_v1_schema::model::Scenario>> for ScenarioDomain {
    type Error = ConversionError;

    fn try_from(v1: Vec<pywr_v1_schema::model::Scenario>) -> Result<Self, Self::Error> {
        let groups = v1.into_iter().map(|g| g.try_into()).collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            groups,
            combinations: None,
        })
    }
}

#[cfg(feature = "core")]
impl From<ScenarioDomain> for pywr_core::scenario::ScenarioDomainBuilder {
    fn from(val: ScenarioDomain) -> Self {
        let mut builder = pywr_core::scenario::ScenarioDomainBuilder::default();

        for group in val.groups {
            builder.with_group(group.into());
        }

        if let Some(combinations) = val.combinations {
            builder.with_combinations(combinations.into_iter().collect());
        }

        builder
    }
}

/// Error type for reading a [`ModelSchema`] or [`MultiNetworkModelSchema`] network from a file or string.
#[derive(Error, Debug)]
pub enum ModelSchemaReadError {
    #[error("IO error on path `{path}`: {error}")]
    IO { path: PathBuf, error: std::io::Error },
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Error, Debug)]
#[cfg(feature = "core")]
pub enum ModelSchemaBuildError {
    #[error("Failed to construct the network: {source}")]
    NetworkBuildError {
        #[source]
        source: Box<NetworkSchemaBuildError>,
    },
    #[error("Scenario validation failed: {source}")]
    ScenarioValidation {
        #[from]
        source: ScenarioValidationError,
    },
    #[error("Error building model domain: {0}")]
    CoreModelDomainBuilderError(#[from] ModelDomainBuilderError),
}

/// The top-level schema for a Pywr model.
///
/// A Pywr model is defined by this top-level schema which is mostly conveniently loaded from a
/// JSON file. The schema is used to "build" a [`pywr_core::models::Model`] which can then be
/// "run" to produce results. The purpose of the schema is to provide a higher level and more
/// user friendly interface to model definition than the core model itself. This allows
/// abstractions, such as [`crate::nodes::WaterTreatmentWorksNode`], to be created and used in the
/// schema without the user needing to know the details of how this is implemented in the core
/// model.
///
///
/// # Example
///
/// The simplest model is given in the example below:
///
/// ```json
#[doc = include_str!("../tests/simple1.json")]
/// ```
///
///
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema, Default)]
pub struct ModelSchema {
    pub metadata: Metadata,
    pub time: TimeDomain,
    pub scenarios: Option<ScenarioDomain>,
    pub network: NetworkSchema,
}

impl FromStr for ModelSchema {
    type Err = ModelSchemaReadError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(serde_json::from_str(s)?)
    }
}

impl VisitPaths for ModelSchema {
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        self.network.visit_paths(visitor);
    }
    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        self.network.visit_paths_mut(visitor)
    }
}

impl VisitMetrics for ModelSchema {
    fn visit_metrics<F: FnMut(&Metric)>(&self, visitor: &mut F) {
        self.network.visit_metrics(visitor);
    }

    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, visitor: &mut F) {
        self.network.visit_metrics_mut(visitor);
    }
}

impl VisitReferences for ModelSchema {
    fn visit_references<F: FnMut(Reference<'_>)>(&self, visitor: &mut F) {
        self.network.visit_references(visitor);
    }

    fn visit_references_mut<F: FnMut(ReferenceMut<'_>)>(&mut self, visitor: &mut F) {
        self.network.visit_references_mut(visitor);
    }
}

/// Every problem with a model's `scenarios` and with the references its `networks` make to them,
/// as [`ValidationError::scenarios`] holds them. Each network is paired with its name in a
/// [`MultiNetworkModelSchema`], or `None` for a model with a single network.
fn scenario_problems<'a>(
    scenarios: Option<&ScenarioDomain>,
    networks: impl IntoIterator<Item = (Option<&'a str>, &'a NetworkSchema)>,
) -> Vec<ScenarioProblem> {
    let mut problems = scenarios.and_then(|domain| domain.validate().err()).unwrap_or_default();

    let groups: BTreeSet<&str> = scenarios
        .map(|domain| domain.groups.iter().map(|group| group.name.as_str()).collect())
        .unwrap_or_default();

    for (network_name, network) in networks {
        network.visit_owned_references(&mut |owner, reference| {
            if let Reference::ScenarioGroup(group) = reference {
                if !groups.contains(group) {
                    problems.push(ScenarioProblem::UnknownGroupReference {
                        network: network_name.map(ToString::to_string),
                        owner: owner.to_string(),
                        group: group.to_string(),
                    });
                }
            }
        });
    }

    problems
}

impl ModelSchema {
    pub fn visit_owned_references<F: FnMut(Owner<'_>, Reference<'_>)>(&self, visitor: &mut F) {
        self.network.visit_owned_references(visitor);
    }

    pub fn visit_owned_references_mut<F: FnMut(Owner<'_>, ReferenceMut<'_>)>(&mut self, visitor: &mut F) {
        self.network.visit_owned_references_mut(visitor);
    }

    pub fn new(title: &str, start: &DateTime, end: &DateTime) -> Self {
        Self {
            metadata: Metadata {
                title: title.to_string(),
                description: None,
                minimum_version: None,
            },
            time: TimeDomain {
                start: *start,
                end: *end,
                timestep: Timestep::default(),
            },
            scenarios: None,
            network: NetworkSchema::default(),
        }
    }

    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, ModelSchemaReadError> {
        let data = std::fs::read_to_string(&path).map_err(|error| ModelSchemaReadError::IO {
            path: path.as_ref().to_path_buf(),
            error,
        })?;
        Ok(serde_json::from_str(data.as_str())?)
    }

    /// Validate the model's schema. See [`TimeDomain::validate`], [`ScenarioDomain::validate`]
    /// and [`NetworkSchema::validate`]. The network's references to scenario groups are checked
    /// here too; see [`ValidationError::scenarios`].
    pub fn validate(&self) -> Result<(), ValidationError> {
        ValidationError {
            model: self.time.validate().err().unwrap_or_default(),
            scenarios: scenario_problems(self.scenarios.as_ref(), [(None, &self.network)]),
            networks: self.network.validate().err().into_iter().collect(),
        }
        .into_result()
    }

    /// Create a [`pywr_core::models::ModelBuilder`] from the schema.
    #[cfg(feature = "core")]
    pub fn create_model_builder(
        &self,
        data_path: Option<&Path>,
        output_path: Option<&Path>,
    ) -> Result<ModelBuilder, ModelSchemaBuildError> {
        let time_domain_builder = self.time.clone().into();

        let scenario_builder = match &self.scenarios {
            Some(scenarios) => scenarios.validated_builder()?,
            None => pywr_core::scenario::ScenarioDomainBuilder::default(),
        };

        let mut domain_builder = ModelDomainBuilder::new(time_domain_builder);
        domain_builder.scenario(scenario_builder);

        let domain = domain_builder.build()?;

        let mut network_builder = pywr_core::network::NetworkBuilder::default();

        self.network
            .add_to_network(&mut network_builder, &domain, data_path, output_path, &[])
            .map_err(|source| ModelSchemaBuildError::NetworkBuildError {
                source: Box::new(source),
            })?;

        let model_builder = ModelBuilder::new(domain, network_builder);

        Ok(model_builder)
    }

    /// Convert a v1 model to a v2 model.
    ///
    /// This function is used to convert a v1 model to a v2 model. The conversion is not always
    /// possible and may result in errors. The errors are returned as a vector of [`ComponentConversionError`]s.
    /// alongside the (partially) converted model. This may result in a model that will not
    /// function as expected. The user should check the errors and the converted model to ensure
    /// that the conversion has been successful.
    pub fn from_v1(v1: pywr_v1_schema::PywrModel) -> (Self, Vec<ComponentConversionError>) {
        let mut errors = Vec::new();

        let metadata = v1.metadata.into();
        let time = v1.timestepper.into();
        let mut scenarios: Option<ScenarioDomain> = match v1.scenarios.map(|s| s.try_into()) {
            Some(Ok(scenarios)) => Some(scenarios),
            Some(Err(err)) => {
                errors.push(ComponentConversionError::Scenarios { error: err });
                None
            }
            None => None,
        };

        if let Some(combinations) = v1.scenario_combinations {
            let combinations = combinations
                .into_iter()
                .map(|c| c.into_iter().map(ScenarioLabelOrIndex::Index).collect::<Vec<_>>())
                .collect::<Vec<_>>();

            if let Some(scenarios) = &mut scenarios {
                scenarios.combinations = Some(combinations);
            } else {
                errors.push(ComponentConversionError::Scenarios {
                    error: ConversionError::ScenarioCombinationsWithoutGroups {},
                });
            }
        }

        let (network, network_errors) = NetworkSchema::from_v1(v1.network);
        errors.extend(network_errors);

        (
            Self {
                metadata,
                time,
                scenarios,
                network,
            },
            errors,
        )
    }

    /// Convert a v1 JSON string to a v2 model.
    ///
    /// See [`ModelSchema::from_v1`] for more information.
    pub fn from_v1_str(v1: &str) -> Result<(Self, Vec<ComponentConversionError>), pywr_v1_schema::PywrSchemaError> {
        let v1_model = pywr_v1_schema::PywrModel::from_str(v1)?;

        Ok(Self::from_v1(v1_model))
    }
}

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct MultiNetworkTransfer {
    pub from_network: String,
    pub metric: Metric,
    pub name: String,
    pub initial_value: Option<f64>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct MultiNetworkEntry {
    pub name: String,
    pub network: NetworkSchemaRef,
    pub transfers: Vec<MultiNetworkTransfer>,
}

#[derive(Error, Debug)]
#[cfg(feature = "core")]
pub enum MultiNetworkModelSchemaBuildError {
    #[error("Error building model domain: {0}")]
    CoreModelDomainBuilderError(#[from] ModelDomainBuilderError),
    #[error("Scenario validation failed: {source}")]
    ScenarioValidation {
        #[from]
        source: ScenarioValidationError,
    },
    #[error("Failed to construct the network `{name}`: {source}")]
    NetworkBuildError {
        name: String,
        #[source]
        source: Box<NetworkSchemaBuildError>,
    },
    #[error("Failed to read Pywr network from path `{path}`: {source}")]
    NetworkReadError {
        path: PathBuf,
        #[source]
        source: NetworkSchemaReadError,
    },
    #[error("Failed to add node `{name}` to the model: {source}")]
    AddTransferError {
        name: String,
        #[source]
        source: Box<SchemaError>,
    },
}

/// A Pywr model containing multiple link networks.
///
/// This schema is used to define a model containing multiple linked networks. Each network
/// is self-contained and solved as like a single a model. However, the networks can be linked
/// together using [`PywrMultiNetworkTransfer`]s. These transfers allow the value of a metric
/// in one network to be used as the value of a parameter in another network. This allows complex
/// inter-model relationships to be defined.
///
/// The model is solved by iterating over the networks within each time-step. Inter-network
/// transfers are updated between each network solve. The networks are solved in the order
/// that they are defined. This means that the order of the networks is important. For example,
/// the 1st network will only be able to use the previous time-step's state from other networks.
/// Whereas the 2nd network can use metrics calculated in the current time-step of the 1st model.
///
/// The overall algorithm produces an single model run with interleaved solving of each network.
/// The pseudo-code for the algorithm is:
///
/// ```text
/// for time_step in time_steps {
///     for network in networks {
///         // Get the latest values from the other networks
///         network.update_inter_network_transfers();
///         // Solve this network's allocation routine / linear program
///         network.solve();
///     }
/// }
/// ```
///
/// # When to use
///
/// A [`MultiNetworkModelSchema`] should be used in cases where there is a strong separation between
/// the networks being simulated. The allocation routine (linear program) of each network is solved
/// independently each time-step. This means that the only way in which the networks can share
/// information and data is between the linear program solves via the user defined transfers.
///
/// Configuring a model like this maybe be beneficial in the following cases:
///   1. Represent separate systems with limited and/or prescribed connectivity. For example,
///     linking networks from two suppliers connected by a strategic transfer.
///   2. Have important validated behaviour of the allocation that should be retained. If the
///     networks (linear programs) were combined into a single model, the allocation routine could
///     produce different results (i.e. penalty costs from one model influencing another).
///   2. Are very large and/or complex to control model run times. The run time of a
///     [`MultiNetworkModelSchema`] is roughly the sum of the individual networks. Whereas the time
///     solve a large linear program combining all the networks could be significantly longer.
///
/// # Example
///
/// The following example shows a model with networks with the inflow to "supply2" in the second
/// network defined as the flow to "demand1" in the first network.
///
/// ```json5
/// // model.json
#[doc = include_str!("../tests/multi1/model.json")]
/// // network1.json
#[doc = include_str!("../tests/multi1/network1.json")]
/// // network2.json
#[doc = include_str!("../tests/multi1/network2.json")]
/// ```
///
///
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct MultiNetworkModelSchema {
    pub metadata: Metadata,
    pub time: TimeDomain,
    pub scenarios: Option<ScenarioDomain>,
    pub networks: Vec<MultiNetworkEntry>,
}

impl FromStr for MultiNetworkModelSchema {
    type Err = ModelSchemaReadError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(serde_json::from_str(s)?)
    }
}

impl MultiNetworkModelSchema {
    pub fn new(title: &str, start: &DateTime, end: &DateTime) -> Self {
        Self {
            metadata: Metadata {
                title: title.to_string(),
                description: None,
                minimum_version: None,
            },
            time: TimeDomain {
                start: *start,
                end: *end,
                timestep: Timestep::default(),
            },
            scenarios: None,
            networks: Vec::new(),
        }
    }
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, ModelSchemaReadError> {
        let data = std::fs::read_to_string(&path).map_err(|error| ModelSchemaReadError::IO {
            path: path.as_ref().to_path_buf(),
            error,
        })?;
        Ok(serde_json::from_str(data.as_str())?)
    }

    /// Validate the model's time domain, its scenarios, and the schema of each network in the
    /// model. See [`TimeDomain::validate`], [`ScenarioDomain::validate`] and
    /// [`NetworkSchema::validate`].
    ///
    /// Every problem found is returned. Each network's problems carry the network's name, as does
    /// each reference it makes to a scenario group the model does not have. Only inline networks
    /// are checked; a network given by path is not read.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let inline = || {
            self.networks.iter().filter_map(|entry| match &entry.network {
                NetworkSchemaRef::Inline(network) => Some((entry.name.as_str(), network)),
                NetworkSchemaRef::Path(_) => None,
            })
        };

        let networks = inline()
            .filter_map(|(name, network)| {
                network.validate().err().map(|mut error| {
                    error.name = Some(name.to_string());
                    error
                })
            })
            .collect();

        ValidationError {
            model: self.time.validate().err().unwrap_or_default(),
            scenarios: scenario_problems(
                self.scenarios.as_ref(),
                inline().map(|(name, network)| (Some(name), network)),
            ),
            networks,
        }
        .into_result()
    }

    #[cfg(feature = "core")]
    pub fn create_model_builder(
        &self,
        data_path: Option<&Path>,
        output_path: Option<&Path>,
    ) -> Result<MultiNetworkModelBuilder, MultiNetworkModelSchemaBuildError> {
        let time_builder = self.time.clone().into();

        let scenario_builder = match &self.scenarios {
            Some(scenarios) => scenarios.validated_builder()?,
            None => pywr_core::scenario::ScenarioDomainBuilder::default(),
        };

        let mut domain_builder = ModelDomainBuilder::new(time_builder);
        domain_builder.scenario(scenario_builder);
        let domain = domain_builder.build()?;

        let mut network_entry_builders = Vec::with_capacity(self.networks.len());
        let mut network_builder_map = HashMap::with_capacity(self.networks.len());
        let mut schemas: Vec<(NetworkSchema, LoadedTableCollection, LoadedTimeSeriesCollection)> =
            Vec::with_capacity(self.networks.len());

        // First load all the networks
        // These will contain any parameters that are referenced by the inter-model transfers
        // Because of potential circular references, we need to load all the networks first.
        for (i, network_entry) in self.networks.iter().enumerate() {
            // Load the network itself
            let mut network_builder = pywr_core::network::NetworkBuilder::default();

            let (schema, tables, time_series) = match &network_entry.network {
                NetworkSchemaRef::Path(path) => {
                    let pth = if let Some(dp) = data_path {
                        if path.is_relative() {
                            dp.join(path)
                        } else {
                            path.clone()
                        }
                    } else {
                        path.clone()
                    };

                    let network_schema = NetworkSchema::from_path(&pth)
                        .map_err(|source| MultiNetworkModelSchemaBuildError::NetworkReadError { path: pth, source })?;

                    let (tables, time_series) = network_schema
                        .add_to_network(
                            &mut network_builder,
                            &domain,
                            data_path,
                            output_path,
                            &network_entry.transfers,
                        )
                        .map_err(|source| MultiNetworkModelSchemaBuildError::NetworkBuildError {
                            name: network_entry.name.clone(),
                            source: Box::new(source),
                        })?;

                    (network_schema, tables, time_series)
                }
                NetworkSchemaRef::Inline(network_schema) => {
                    let (tables, time_series) = network_schema
                        .add_to_network(
                            &mut network_builder,
                            &domain,
                            data_path,
                            output_path,
                            &network_entry.transfers,
                        )
                        .map_err(|source| MultiNetworkModelSchemaBuildError::NetworkBuildError {
                            name: network_entry.name.clone(),
                            source: Box::new(source),
                        })?;

                    (network_schema.clone(), tables, time_series)
                }
            };

            schemas.push((schema, tables, time_series));

            network_entry_builders.push(network_builder);
            network_builder_map.insert(network_entry.name.clone(), i);
        }

        // Now load the inter-model transfer builders.
        let mut transfer_builders = Vec::with_capacity(self.networks.len());
        for network_entry in &self.networks {
            let mut t_builders = Vec::with_capacity(network_entry.transfers.len());
            for transfer in &network_entry.transfers {
                // Load the metric from the "from" network
                let from_network_idx = *network_builder_map.get(&transfer.from_network).ok_or_else(|| {
                    MultiNetworkModelSchemaBuildError::AddTransferError {
                        name: transfer.name.clone(),
                        source: Box::new(SchemaError::NetworkNotFound(transfer.from_network.clone())),
                    }
                })?;
                let from_network = &mut network_entry_builders[from_network_idx];

                // The transfer metric will fail to load if it is defined as an inter-model transfer itself.
                let (from_schema, from_tables, from_time_series) = &schemas[from_network_idx];

                let args = LoadArgs {
                    schema: from_schema,
                    domain: &domain,
                    tables: from_tables,
                    time_series: from_time_series,
                    data_path,
                    inter_network_transfers: &[],
                };

                let from_metric = transfer.metric.load(from_network, &args, None).map_err(|source| {
                    MultiNetworkModelSchemaBuildError::AddTransferError {
                        name: transfer.name.clone(),
                        source: Box::new(source),
                    }
                })?;

                let mut transfer_builder =
                    MultiNetworkTransferBuilder::new(&transfer.name, &transfer.from_network, from_metric);
                if let Some(iv) = transfer.initial_value {
                    transfer_builder.initial_value(iv);
                }

                t_builders.push(transfer_builder);
            }

            transfer_builders.push(t_builders);
        }

        // Now construct the model from the loaded components
        let mut model_builder = MultiNetworkModelBuilder::new(domain);

        for (network_entry, (network_builder, t_builders)) in self
            .networks
            .iter()
            .zip(network_entry_builders.into_iter().zip(transfer_builders))
        {
            let mut entry_builder = MultiNetworkEntryBuilder::new(&network_entry.name, network_builder);
            for t in t_builders {
                entry_builder.transfer(t);
            }

            model_builder.network(entry_builder);
        }

        Ok(model_builder)
    }
}

#[cfg(test)]
mod tests {
    use super::{ModelSchema, MultiNetworkModelSchema, ScenarioDomain};
    use crate::edge::Edge;
    use crate::error::{EdgeProblem, ModelProblem, NetworkProblem, ScenarioProblem, ValidationError};
    use crate::model::{TimeDomain, Timestep};
    use crate::visit::VisitPaths;
    use jiff::civil::date;
    use std::fs;
    use std::fs::read_to_string;
    use std::path::PathBuf;

    fn model_str() -> String {
        read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/simple1.json")).unwrap()
    }

    #[test]
    fn test_simple1_schema() {
        let data = model_str();
        let schema: ModelSchema = serde_json::from_str(&data).unwrap();

        assert_eq!(schema.network.nodes.len(), 3);
        assert_eq!(schema.network.edges.len(), 2);
    }

    #[test]
    fn test_date() {
        let timestepper_str = r#"
        {
            "start": "2015-01-01",
            "end": "2015-12-31",
            "timestep": {
              "type": "Days",
              "days": 1
            }
        }
        "#;

        let timestep: TimeDomain = serde_json::from_str(timestepper_str).unwrap();

        assert_eq!(timestep.start, date(2015, 1, 1).at(0, 0, 0, 0));
        assert_eq!(timestep.end, date(2015, 12, 31).at(0, 0, 0, 0));
    }

    #[test]
    fn test_datetime() {
        let timestepper_str = r#"
        {
            "start": "2015-01-01T12:30:00",
            "end": "2015-01-01T14:30:00",
            "timestep": {
                "type": "Hours",
                "hours": 1
            }
        }
        "#;

        let timestep: TimeDomain = serde_json::from_str(timestepper_str).unwrap();
        assert_eq!(timestep.start, date(2015, 1, 1).at(12, 30, 0, 0));
        assert_eq!(timestep.end, date(2015, 1, 1).at(14, 30, 0, 0));
    }

    /// Test that the visit_paths functions works as expected.
    #[test]
    fn test_visit_paths() {
        let mut model_fn = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        model_fn.push("tests/time-series.json");

        let mut schema = ModelSchema::from_path(model_fn.as_path()).unwrap();

        let expected_paths = vec![PathBuf::from("inflow.csv"), PathBuf::from("time-series-expected.csv")];

        let mut paths: Vec<PathBuf> = Vec::new();

        schema.visit_paths(&mut |p| {
            paths.push(p.to_path_buf());
        });

        assert_eq!(&paths, &expected_paths);

        schema.visit_paths_mut(&mut |p: &mut PathBuf| {
            *p = PathBuf::from("this-file-does-not-exist.csv");
        });

        // Expect this to file as the path has been updated to a missing file.
        #[cfg(feature = "core")]
        if schema.create_model_builder(model_fn.parent(), None).is_ok() {
            let str = serde_json::to_string_pretty(&schema).unwrap();
            panic!("Expected an error due to missing file: {str}");
        }
    }

    /// Return the default time domain with the timestep replaced.
    fn time_domain_with(timestep: Timestep) -> TimeDomain {
        TimeDomain {
            timestep,
            ..Default::default()
        }
    }

    #[test]
    fn test_validate_period() {
        let valid: ModelSchema = serde_json::from_str(&model_str()).unwrap();
        valid.validate().expect("The unmodified model should be valid");

        // A period that ends before it starts is rejected, naming both ends.
        let mut schema = valid.clone();
        std::mem::swap(&mut schema.time.start, &mut schema.time.end);
        assert_eq!(
            schema.validate(),
            Err(ValidationError {
                model: vec![ModelProblem::EndBeforeStart {
                    start: schema.time.start,
                    end: schema.time.end,
                }],
                scenarios: vec![],
                networks: vec![],
            })
        );

        // A single instant is a period, if a short one.
        let mut schema = valid.clone();
        schema.time.end = schema.time.start;
        assert_eq!(schema.validate(), Ok(()));
    }

    #[test]
    fn test_validate_frequency() {
        // The forms `jiff` reads: "friendly" and ISO 8601.
        for freq in ["7d", "1mo", "3h", "P7D"] {
            let time = time_domain_with(Timestep::Frequency { freq: freq.to_string() });
            assert_eq!(time.validate(), Ok(()), "`{freq}` should be a valid frequency");
        }

        // A string that is not a duration at all.
        for freq in ["every other tuesday", "7", "", "1q"] {
            let time = time_domain_with(Timestep::Frequency { freq: freq.to_string() });
            let problems = time
                .validate()
                .expect_err(&format!("`{freq}` should not parse as a frequency"));
            assert!(matches!(problems[..], [ModelProblem::UnparsableFrequency { .. }]));
        }

        // A duration that parses, but would never advance the clock.
        for freq in ["0d", "-7d"] {
            let time = time_domain_with(Timestep::Frequency { freq: freq.to_string() });
            assert_eq!(
                time.validate(),
                Err(vec![ModelProblem::NonPositiveFrequency { freq: freq.to_string() }])
            );
        }
    }

    /// The period and the frequency are both checked, so a time domain can report both.
    #[test]
    fn test_validate_time_domain_reports_every_problem() {
        let mut time = time_domain_with(Timestep::Frequency {
            freq: "-7d".to_string(),
        });
        std::mem::swap(&mut time.start, &mut time.end);

        assert_eq!(
            time.validate(),
            Err(vec![
                ModelProblem::EndBeforeStart {
                    start: time.start,
                    end: time.end,
                },
                ModelProblem::NonPositiveFrequency {
                    freq: "-7d".to_string()
                },
            ])
        );
    }

    /// A model's own problems, its scenarios' and its network's are reported together, but apart.
    #[test]
    fn test_validate_model_reports_model_and_network_problems_apart() {
        let mut schema: ModelSchema = serde_json::from_str(&model_str()).unwrap();
        std::mem::swap(&mut schema.time.start, &mut schema.time.end);
        schema.scenarios = Some(scenarios(r#"{ "groups": [{ "name": "A", "size": 0 }] }"#));
        schema.network.edges.push(Edge {
            from_node: "link1".to_string(),
            to_node: "missing".to_string(),
            from_slot: None,
            to_slot: None,
        });

        let error = schema.validate().unwrap_err();

        assert_eq!(
            error.model,
            vec![ModelProblem::EndBeforeStart {
                start: schema.time.start,
                end: schema.time.end,
            }]
        );
        assert_eq!(
            error.scenarios,
            vec![ScenarioProblem::EmptyGroup { group: "A".to_string() }]
        );
        assert_eq!(error.networks.len(), 1);
        assert_eq!(error.networks[0].name, None);
        assert!(matches!(
            error.networks[0].problems.as_slice(),
            [NetworkProblem::InvalidEdge(e)] if e.problem == EdgeProblem::UnknownToNode("missing".to_string())
        ));

        // The summary counts all three together.
        assert_eq!(error.to_string(), "The model has 3 problem(s).");

        // The scenarios' problems are listed between the model's and its network's, which need no
        // name for a single network.
        let report = error.report().to_string();
        assert!(report.starts_with("The model has 3 problem(s):\n- The simulation period ends before it starts"));
        assert!(
            report.contains(
                "\n- The scenario group `A` has a size of zero, but a group must have at least one scenario.\n"
            )
        );
        assert!(
            report
                .ends_with("\n- The edge `link1->missing` is invalid. There is no node named `missing` to connect to.")
        );
    }

    /// Each inline network of a multi-network model is checked, and a network's problems are
    /// reported under its name. A valid network is left out.
    #[test]
    fn test_validate_multi_network_model_names_each_network() {
        let schema: MultiNetworkModelSchema = r#"
        {
            "metadata": { "title": "Two networks" },
            "time": { "start": "2015-01-01", "end": "2015-12-31", "timestep": { "type": "Days", "days": 1 } },
            "networks": [
                {
                    "name": "valid",
                    "network": {
                        "nodes": [
                            { "meta": { "name": "supply" }, "type": "Input" },
                            { "meta": { "name": "demand" }, "type": "Output" }
                        ],
                        "edges": [{ "from_node": "supply", "to_node": "demand" }]
                    },
                    "transfers": []
                },
                {
                    "name": "north",
                    "network": {
                        "nodes": [
                            { "meta": { "name": "supply" }, "type": "Input" },
                            { "meta": { "name": "supply" }, "type": "Input" },
                            { "meta": { "name": "demand" }, "type": "Output" }
                        ],
                        "edges": [{ "from_node": "demand", "to_node": "supply" }]
                    },
                    "transfers": []
                }
            ]
        }
        "#
        .parse()
        .expect("Failed to parse test model JSON");

        let error = schema.validate().unwrap_err();

        assert!(error.model.is_empty());
        assert_eq!(error.networks.len(), 1);
        assert_eq!(error.networks[0].name.as_deref(), Some("north"));
        assert!(matches!(
            error.networks[0].problems.as_slice(),
            [NetworkProblem::DuplicateNodeName(_), NetworkProblem::InvalidEdge(_)]
        ));

        assert_eq!(
            error.report().to_string(),
            "The model has 2 problem(s):\n\
             - Network `north`: The name `supply` is used by 2 node(s) and 0 virtual node(s), but each name must be unique.\n\
             - Network `north`: The edge `demand->supply` is invalid. The `Output` node `demand` cannot provide flow."
        );
    }

    #[test]
    fn test_scenario_domain_doc_examples() {
        let mut doc_examples = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        doc_examples.push("src/doc_examples");

        for entry in fs::read_dir(doc_examples).unwrap() {
            let p = entry.unwrap().path();
            if p.is_file() && p.file_name().unwrap().to_str().unwrap().starts_with("scenario_domain") {
                let data = read_to_string(&p).unwrap_or_else(|e| panic!("Failed to read file: {p:?}: {e}",));

                let value: ScenarioDomain =
                    serde_json::from_str(&data).unwrap_or_else(|e| panic!("Failed to deserialize {p:?}: {e}",));

                // Every example the documentation shows must be one validation accepts.
                value
                    .validate()
                    .unwrap_or_else(|problems| panic!("Doc example {p:?} is not valid: {problems:?}"));
            }
        }
    }

    /// Deserialise a scenario domain, which every test below starts from.
    fn scenarios(json: &str) -> ScenarioDomain {
        serde_json::from_str(json).expect("Failed to deserialize the scenario domain")
    }

    /// The problems with a scenario domain, which must not be valid.
    fn problems_of(json: &str) -> Vec<ScenarioProblem> {
        scenarios(json)
            .validate()
            .expect_err("Expected the scenario domain to be invalid")
    }

    /// A group's labels must number its scenarios, and must be distinct.
    #[test]
    fn test_validate_scenarios_labels() {
        assert_eq!(
            problems_of(r#"{ "groups": [{ "name": "A", "size": 3, "labels": ["wet", "dry"] }] }"#),
            vec![ScenarioProblem::IncorrectNumberOfLabels {
                group: "A".to_string(),
                found: 2,
                expected: 3,
            }]
        );

        assert_eq!(
            problems_of(r#"{ "groups": [{ "name": "A", "size": 3, "labels": ["wet", "dry", "wet"] }] }"#),
            vec![ScenarioProblem::DuplicateLabel {
                group: "A".to_string(),
                label: "wet".to_string(),
                count: 2,
            }]
        );
    }

    /// A `Slice` subset must start before it ends, and must not reach past its group.
    #[test]
    fn test_validate_scenarios_slice_subset() {
        let problems_of_slice = |start: usize, end: usize| {
            problems_of(&format!(
                r#"{{ "groups": [{{ "name": "A", "size": 5, "subset": {{ "type": "Slice", "start": {start}, "end": {end} }} }}] }}"#
            ))
        };

        assert_eq!(
            problems_of_slice(2, 2),
            vec![ScenarioProblem::EmptySlice {
                group: "A".to_string(),
                start: 2,
                end: 2,
            }]
        );

        assert_eq!(
            problems_of_slice(0, 6),
            vec![ScenarioProblem::SliceOutOfRange {
                group: "A".to_string(),
                size: 5,
                end: 6,
            }]
        );

        // The two are checked apart, so one slice can break both.
        assert_eq!(
            problems_of_slice(7, 6),
            vec![
                ScenarioProblem::EmptySlice {
                    group: "A".to_string(),
                    start: 7,
                    end: 6,
                },
                ScenarioProblem::SliceOutOfRange {
                    group: "A".to_string(),
                    size: 5,
                    end: 6,
                },
            ]
        );
    }

    /// An `Indices` subset must not be empty, and must name scenarios its group has, each once.
    #[test]
    fn test_validate_scenarios_indices_subset() {
        assert_eq!(
            problems_of(
                r#"{ "groups": [{ "name": "A", "size": 3, "subset": { "type": "Indices", "indices": [] } }] }"#
            ),
            vec![ScenarioProblem::EmptySubset { group: "A".to_string() }]
        );

        // Each offending scenario is named once, however often the subset repeats it, and the
        // repetition is reported separately.
        assert_eq!(
            problems_of(
                r#"{ "groups": [{ "name": "A", "size": 3, "subset": { "type": "Indices", "indices": [1, 5, 5, 9] } }] }"#
            ),
            vec![
                ScenarioProblem::SubsetIndexOutOfRange {
                    group: "A".to_string(),
                    size: 3,
                    index: 5,
                },
                ScenarioProblem::SubsetIndexOutOfRange {
                    group: "A".to_string(),
                    size: 3,
                    index: 9,
                },
                ScenarioProblem::DuplicateSubsetIndex {
                    group: "A".to_string(),
                    index: 5,
                    count: 2,
                },
            ]
        );
    }

    /// A `Labels` subset must name labels the group has, and needs the group to have labels at
    /// all.
    #[test]
    fn test_validate_scenarios_labels_subset() {
        assert_eq!(
            problems_of(
                r#"{ "groups": [{ "name": "A", "size": 2, "subset": { "type": "Labels", "labels": ["wet"] } }] }"#
            ),
            vec![ScenarioProblem::SubsetNeedsGroupLabels { group: "A".to_string() }]
        );

        assert_eq!(
            problems_of(
                r#"{ "groups": [{ "name": "A", "size": 2, "labels": ["wet", "dry"], "subset": { "type": "Labels", "labels": ["damp", "dry", "dry"] } }] }"#
            ),
            vec![
                ScenarioProblem::SubsetLabelNotFound {
                    group: "A".to_string(),
                    label: "damp".to_string(),
                },
                ScenarioProblem::DuplicateSubsetLabel {
                    group: "A".to_string(),
                    label: "dry".to_string(),
                    count: 2,
                },
            ]
        );
    }

    /// Combinations cannot be given alongside a subset, and every group carrying one is named.
    /// They also need groups to combine, and must not be an empty list.
    #[test]
    fn test_validate_scenarios_combinations_against_the_domain() {
        assert_eq!(
            problems_of(
                r#"{
                    "groups": [
                        { "name": "A", "size": 3, "subset": { "type": "Slice", "start": 0, "end": 2 } },
                        { "name": "B", "size": 2, "subset": { "type": "Indices", "indices": [1] } }
                    ],
                    "combinations": [[0, 0]]
                }"#
            ),
            vec![
                ScenarioProblem::CombinationsAndSubset { group: "A".to_string() },
                ScenarioProblem::CombinationsAndSubset { group: "B".to_string() },
            ]
        );

        assert_eq!(
            problems_of(r#"{ "groups": [], "combinations": [[0]] }"#),
            vec![ScenarioProblem::CombinationsWithoutGroups]
        );

        assert_eq!(
            problems_of(r#"{ "groups": [{ "name": "A", "size": 2 }], "combinations": [] }"#),
            vec![ScenarioProblem::EmptyCombinations]
        );
    }

    /// Each combination must have one entry per group.
    #[test]
    fn test_validate_scenarios_combination_length() {
        assert_eq!(
            problems_of(
                r#"{
                    "groups": [{ "name": "A", "size": 2 }, { "name": "B", "size": 2 }],
                    "combinations": [[0], [0, 1, 1]]
                }"#
            ),
            vec![
                ScenarioProblem::IncorrectCombinationLength {
                    combination: 0,
                    found: 1,
                    expected: 2,
                },
                ScenarioProblem::IncorrectCombinationLength {
                    combination: 1,
                    found: 3,
                    expected: 2,
                },
            ]
        );
    }

    /// A combination's entries must name scenarios their group has, whether by index or by label.
    #[test]
    fn test_validate_scenarios_combination_entries() {
        assert_eq!(
            problems_of(
                r#"{
                    "groups": [{ "name": "A", "size": 2, "labels": ["wet", "dry"] }, { "name": "B", "size": 2 }],
                    "combinations": [[5, 0], ["damp", 1], ["wet", "second"]]
                }"#
            ),
            vec![
                ScenarioProblem::CombinationIndexOutOfRange {
                    combination: 0,
                    group: "A".to_string(),
                    size: 2,
                    index: 5,
                },
                ScenarioProblem::CombinationLabelNotFound {
                    combination: 1,
                    group: "A".to_string(),
                    label: "damp".to_string(),
                },
                ScenarioProblem::CombinationNeedsGroupLabels {
                    combination: 2,
                    group: "B".to_string(),
                    label: "second".to_string(),
                },
            ]
        );
    }

    /// Every problem is returned at once, in the documented order: duplicate group names, then
    /// each group's own as the groups are defined, then the combinations'.
    #[test]
    fn test_validate_scenarios_reports_every_problem_in_order() {
        assert_eq!(
            problems_of(
                r#"{
                    "groups": [{ "name": "B", "size": 0 }, { "name": "A", "size": 2 }, { "name": "A", "size": 1 }],
                    "combinations": [[0, 9, 0]]
                }"#
            ),
            vec![
                ScenarioProblem::DuplicateGroupName {
                    name: "A".to_string(),
                    count: 2,
                },
                ScenarioProblem::EmptyGroup { group: "B".to_string() },
                ScenarioProblem::CombinationIndexOutOfRange {
                    combination: 0,
                    group: "B".to_string(),
                    size: 0,
                    index: 0,
                },
                ScenarioProblem::CombinationIndexOutOfRange {
                    combination: 0,
                    group: "A".to_string(),
                    size: 2,
                    index: 9,
                },
            ]
        );
    }

    /// A model whose network names a scenario group is valid when the group is defined, and has a
    /// dangling reference when it is not, or when the model has no `scenarios` at all.
    #[test]
    fn test_validate_model_checks_scenario_group_references() {
        let model_referring_to_climate = |groups: Option<&str>| {
            let mut schema: ModelSchema = serde_json::from_str(&model_str()).unwrap();
            schema.scenarios = groups.map(scenarios);
            schema.network.parameters.as_mut().unwrap().push(
                serde_json::from_str(
                    r#"{
                        "meta": { "name": "inflow" },
                        "type": "ConstantScenario",
                        "scenario_group": "climate",
                        "values": { "type": "Literal", "values": [1.0, 2.0] }
                    }"#,
                )
                .unwrap(),
            );
            schema
        };

        model_referring_to_climate(Some(r#"{ "groups": [{ "name": "climate", "size": 2 }] }"#))
            .validate()
            .expect("A reference to a defined group is valid");

        for groups in [Some(r#"{ "groups": [{ "name": "weather", "size": 2 }] }"#), None] {
            let error = model_referring_to_climate(groups).validate().unwrap_err();

            assert_eq!(
                error.scenarios,
                vec![ScenarioProblem::UnknownGroupReference {
                    network: None,
                    owner: "parameter `inflow`".to_string(),
                    group: "climate".to_string(),
                }]
            );
            assert_eq!(
                error.report().to_string(),
                "The model has 1 problem(s):\n\
                 - The parameter `inflow` refers to the scenario group `climate`, which the model's scenarios do not define."
            );
        }
    }

    /// A multi-network model checks the one scenario domain its networks share, and names the
    /// network holding a dangling reference. A network given by path is not read.
    #[test]
    fn test_validate_multi_network_model_scenario_problems() {
        let schema: MultiNetworkModelSchema = serde_json::from_str(
            r#"{
                "metadata": { "title": "Two networks" },
                "time": { "start": "2015-01-01", "end": "2015-12-31", "timestep": { "type": "Days", "days": 1 } },
                "scenarios": { "groups": [{ "name": "climate", "size": 0 }] },
                "networks": [
                    {
                        "name": "north",
                        "network": {
                            "nodes": [{ "meta": { "name": "supply" }, "type": "Input" }],
                            "edges": [],
                            "parameters": [{
                                "meta": { "name": "inflow" },
                                "type": "ConstantScenario",
                                "scenario_group": "weather",
                                "values": { "type": "Literal", "values": [1.0] }
                            }]
                        },
                        "transfers": []
                    },
                    { "name": "south", "network": "south.json", "transfers": [] }
                ]
            }"#,
        )
        .unwrap();

        let error = schema.validate().unwrap_err();

        assert_eq!(
            error.scenarios,
            vec![
                ScenarioProblem::EmptyGroup {
                    group: "climate".to_string()
                },
                ScenarioProblem::UnknownGroupReference {
                    network: Some("north".to_string()),
                    owner: "parameter `inflow`".to_string(),
                    group: "weather".to_string(),
                },
            ]
        );

        assert_eq!(
            error.report().to_string(),
            "The model has 2 problem(s):\n\
             - The scenario group `climate` has a size of zero, but a group must have at least one scenario.\n\
             - The parameter `inflow` in the network `north` refers to the scenario group `weather`, which the model's scenarios do not define."
        );
    }
}

#[cfg(test)]
#[cfg(feature = "core")]
mod core_tests {
    use super::{ModelSchema, MultiNetworkModelSchema};
    use crate::agg_funcs::AggFunc;
    use crate::metric::{Metric, ParameterReference};
    use crate::parameters::{AggregatedParameter, ConstantParameter, Parameter, ParameterMeta, ParameterPhase};
    use ndarray::{Array1, Array2, Axis};
    use pywr_core::metric::UnresolvedMetricF64;
    use pywr_core::recorders::AssertionF64RecorderBuilder;
    use pywr_core::{solvers::ClpSolverSettings, test_utils::run_all_solvers};
    use std::fs::read_to_string;
    use std::path::PathBuf;

    fn model_str() -> String {
        read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/simple1.json")).unwrap()
    }

    #[test]
    fn test_simple1_run() {
        let data = model_str();
        let schema: ModelSchema = serde_json::from_str(&data).unwrap();
        let mut model_builder = schema.create_model_builder(None, None).unwrap();

        let network_builder = model_builder.network_builder();

        let expected_values: Array1<f64> = [10.0; 365].to_vec().into();
        let expected_values: Array2<f64> = expected_values.insert_axis(Axis(1));

        let rec = AssertionF64RecorderBuilder::new(
            "assert-demand1",
            UnresolvedMetricF64::NodeInFlow("demand1".into()),
            expected_values,
        );
        network_builder.recorder(Box::new(rec));

        let model = model_builder.build().unwrap();
        let network = model.network();
        assert_eq!(network.nodes().len(), 3);
        assert_eq!(network.edges().len(), 2);

        // Test all solvers
        run_all_solvers(&model, &[], &[], &[]);
    }

    /// Test that a cycle in parameter dependencies does not load.
    #[test]
    fn test_cycle_error() {
        let data = model_str();
        let mut schema: ModelSchema = serde_json::from_str(&data).unwrap();

        // Add additional parameters for the test
        if let Some(parameters) = &mut schema.network.parameters {
            parameters.extend(vec![
                Parameter::Aggregated(AggregatedParameter {
                    meta: ParameterMeta {
                        name: "agg1".to_string(),
                        comment: None,
                        tags: Default::default(),
                    },
                    agg_func: AggFunc::Sum,
                    phase: ParameterPhase::Before,
                    metrics: vec![
                        Metric::Parameter(ParameterReference {
                            name: "p1".to_string(),
                            key: None,
                            return_value: None,
                        }),
                        Metric::Parameter(ParameterReference {
                            name: "agg2".to_string(),
                            key: None,
                            return_value: None,
                        }),
                    ],
                }),
                Parameter::Constant(ConstantParameter {
                    meta: ParameterMeta {
                        name: "p1".to_string(),
                        comment: None,
                        tags: Default::default(),
                    },
                    value: 10.0.into(),
                    variable: None,
                }),
                Parameter::Aggregated(AggregatedParameter {
                    meta: ParameterMeta {
                        name: "agg2".to_string(),
                        comment: None,
                        tags: Default::default(),
                    },
                    agg_func: AggFunc::Sum,
                    phase: ParameterPhase::Before,
                    metrics: vec![
                        Metric::Parameter(ParameterReference {
                            name: "p1".to_string(),
                            key: None,
                            return_value: None,
                        }),
                        Metric::Parameter(ParameterReference {
                            name: "agg1".to_string(),
                            key: None,
                            return_value: None,
                        }),
                    ],
                }),
            ]);
        }

        // TODO this could assert a specific type of error
        let builder = schema.create_model_builder(None, None).unwrap();
        assert!(builder.build().is_err());
    }

    /// Test that a model loads if the aggregated parameter is defined before its dependencies.
    #[test]
    fn test_ordering() {
        let data = model_str();
        let mut schema: ModelSchema = serde_json::from_str(&data).unwrap();

        if let Some(parameters) = &mut schema.network.parameters {
            parameters.extend(vec![
                Parameter::Aggregated(AggregatedParameter {
                    meta: ParameterMeta {
                        name: "agg1".to_string(),
                        comment: None,
                        tags: Default::default(),
                    },
                    agg_func: AggFunc::Sum,
                    phase: ParameterPhase::Before,
                    metrics: vec![
                        Metric::Parameter(ParameterReference {
                            name: "p1".to_string(),
                            key: None,
                            return_value: None,
                        }),
                        Metric::Parameter(ParameterReference {
                            name: "p2".to_string(),
                            key: None,
                            return_value: None,
                        }),
                    ],
                }),
                Parameter::Constant(ConstantParameter {
                    meta: ParameterMeta {
                        name: "p1".to_string(),
                        comment: None,
                        tags: Default::default(),
                    },
                    value: 10.0.into(),
                    variable: None,
                }),
                Parameter::Constant(ConstantParameter {
                    meta: ParameterMeta {
                        name: "p2".to_string(),
                        comment: None,
                        tags: Default::default(),
                    },
                    value: 10.0.into(),
                    variable: None,
                }),
            ]);
        }
        // TODO this could assert a specific type of error
        let _ = schema.create_model_builder(None, None).unwrap();
    }

    /// Test the multi1 model
    #[test]
    fn test_multi1_model() {
        let mut model_fn = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        model_fn.push("tests/multi1/model.json");

        let schema = MultiNetworkModelSchema::from_path(model_fn.as_path()).unwrap();
        let mut builder = schema.create_model_builder(model_fn.parent(), None).unwrap();

        // Add some recorders for the expected outputs
        let network_1 = builder
            .entry_builder("network1")
            .expect("network 1 not found")
            .network_builder();

        let expected_values: Array1<f64> = [10.0; 365].to_vec().into();
        let expected_values: Array2<f64> = expected_values.insert_axis(Axis(1));

        let rec = AssertionF64RecorderBuilder::new(
            "assert-demand1",
            UnresolvedMetricF64::NodeInFlow("demand1".into()),
            expected_values,
        );
        network_1.recorder(Box::new(rec));

        // Inflow to demand2 should be 10.0 via the transfer from network1 (demand1)
        let network_2 = builder
            .entry_builder("network2")
            .expect("network 2 not found")
            .network_builder();

        let expected_values: Array1<f64> = [10.0; 365].to_vec().into();
        let expected_values: Array2<f64> = expected_values.insert_axis(Axis(1));

        let rec = AssertionF64RecorderBuilder::new(
            "assert-demand2",
            UnresolvedMetricF64::NodeInFlow("demand2".into()),
            expected_values,
        );
        network_2.recorder(Box::new(rec));

        let model = builder.build().unwrap();

        model.run(&ClpSolverSettings::default()).unwrap();
    }

    /// Test the multi2 model
    #[test]
    fn test_multi2_model() {
        let mut model_fn = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        model_fn.push("tests/multi2/model.json");

        let schema = MultiNetworkModelSchema::from_path(model_fn.as_path()).unwrap();
        let mut builder = schema.create_model_builder(model_fn.parent(), None).unwrap();

        // Add some recorders for the expected outputs
        // inflow1 should be set to a max of 20.0 from the "demand" parameter in network2
        let network_1 = builder
            .entry_builder("network1")
            .expect("network 1 not found")
            .network_builder();

        let expected_values: Array1<f64> = [10.0; 365].to_vec().into();
        let expected_values: Array2<f64> = expected_values.insert_axis(Axis(1));

        let rec = AssertionF64RecorderBuilder::new(
            "assert-demand1",
            UnresolvedMetricF64::NodeInFlow("demand1".into()),
            expected_values,
        );
        network_1.recorder(Box::new(rec));

        // Inflow to demand2 should be 10.0 via the transfer from network1 (demand1)
        let network_2 = builder
            .entry_builder("network2")
            .expect("network 2 not found")
            .network_builder();

        let expected_values: Array1<f64> = [10.0; 365].to_vec().into();
        let expected_values: Array2<f64> = expected_values.insert_axis(Axis(1));

        let rec = AssertionF64RecorderBuilder::new(
            "assert-demand2",
            UnresolvedMetricF64::NodeInFlow("demand2".into()),
            expected_values,
        );
        network_2.recorder(Box::new(rec));

        let model = builder.build().unwrap();

        model.run(&ClpSolverSettings::default()).unwrap();
    }
}
