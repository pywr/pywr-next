#[cfg(feature = "pyo3")]
use pyo3::{pyclass, pymethods};
use std::collections::BTreeSet;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ScenarioError {
    #[error("Scenario group name `{0}` already exists")]
    DuplicateGroupName(String),
    #[error("Scenario group name `{0}` not found")]
    GroupNameNotFound(String),
    #[error("Cannot use both combinations and slices")]
    CombinationsAndSlices,
    #[error("No labels provided for for scenario group `{group}`.")]
    NoLabels { group: String },
    #[error("Scenario group `{group}` label not found: `{label}`")]
    LabelNotFound { group: String, label: String },
    #[error("Incorrect number of labels for scenario group `{group}` subset; found {found}, expected {expected}")]
    IncorrectNumberOfLabels {
        group: String,
        found: usize,
        expected: usize,
    },
    #[error("Invalid slice ({start}, {end}) for scenario group `{group}` with size {size}")]
    InvalidSlice {
        group: String,
        size: usize,
        start: usize,
        end: usize,
    },
}

#[derive(Clone, Debug)]
pub enum ScenarioGroupSubset {
    /// Slice of scenarios to run
    Slice { start: usize, end: usize },
    /// Specific scenarios to run
    Indices(Vec<usize>),
}

#[derive(Clone, Debug)]
pub struct ScenarioGroup {
    /// Name of the scenario group
    name: String,
    /// Number of scenarios in the group
    size: usize,
    /// Optional subset of scenarios to run
    subset: Option<ScenarioGroupSubset>,
    /// Optional labels for the group
    labels: Option<Vec<String>>,
}

impl Default for ScenarioGroup {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            size: 1,
            subset: None,
            labels: None,
        }
    }
}

impl ScenarioGroup {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn size(&self) -> usize {
        self.size
    }

    fn label_position(&self, label: &str) -> Result<usize, ScenarioError> {
        match &self.labels {
            Some(labels) => labels
                .iter()
                .position(|l| l == label)
                .ok_or_else(|| ScenarioError::LabelNotFound {
                    group: self.name.clone(),
                    label: label.to_string(),
                }),
            None => Err(ScenarioError::NoLabels {
                group: self.name.clone(),
            }),
        }
    }
}

pub enum ScenarioGroupSubsetBuilder {
    Slice { start: usize, end: usize },
    Indices(Vec<usize>),
    Labels(Vec<String>),
}

/// Builder for [`ScenarioGroup`] instances.
pub struct ScenarioGroupBuilder {
    name: String,
    size: usize,
    subset: Option<ScenarioGroupSubsetBuilder>,
    labels: Option<Vec<String>>,
}

impl ScenarioGroupBuilder {
    pub fn new(name: &str, size: usize) -> Self {
        Self {
            name: name.to_string(),
            size,
            subset: None,
            labels: None,
        }
    }

    /// Set the subset of scenarios to run as a slice
    pub fn with_subset_slice(mut self, start: usize, end: usize) -> Self {
        self.subset = Some(ScenarioGroupSubsetBuilder::Slice { start, end });
        self
    }

    /// Set the subset of scenarios to run as a list of indices
    pub fn with_subset_indices(mut self, indices: Vec<usize>) -> Self {
        self.subset = Some(ScenarioGroupSubsetBuilder::Indices(indices));
        self
    }

    /// Set the subset of scenarios to run as a list of labels. The complete list of labels must
    /// be defined using the [`with_labels`] method.
    pub fn with_subset_labels<T: AsRef<str>>(mut self, labels: &[T]) -> Self {
        self.subset = Some(ScenarioGroupSubsetBuilder::Labels(
            labels.iter().map(|l| l.as_ref().to_string()).collect(),
        ));
        self
    }

    /// Set the labels for the group
    pub fn with_labels<T: AsRef<str>>(mut self, labels: &[T]) -> Self {
        self.labels = Some(labels.iter().map(|l| l.as_ref().to_string()).collect());
        self
    }

    pub fn build(self) -> Result<ScenarioGroup, ScenarioError> {
        let subset = match self.subset {
            Some(ScenarioGroupSubsetBuilder::Slice { start, end }) => {
                if start >= end || end > self.size {
                    return Err(ScenarioError::InvalidSlice {
                        group: self.name.clone(),
                        size: self.size,
                        start,
                        end,
                    });
                }

                Some(ScenarioGroupSubset::Slice { start, end })
            }
            Some(ScenarioGroupSubsetBuilder::Indices(indices)) => Some(ScenarioGroupSubset::Indices(indices)),
            Some(ScenarioGroupSubsetBuilder::Labels(subset_labels)) => {
                if let Some(labels) = &self.labels {
                    let indices: Vec<usize> = subset_labels
                        .iter()
                        .map(|l| {
                            labels
                                .iter()
                                .position(|label| label == l)
                                .ok_or_else(|| ScenarioError::LabelNotFound {
                                    label: l.to_string(),
                                    group: self.name.clone(),
                                })
                        })
                        .collect::<Result<Vec<usize>, ScenarioError>>()?;

                    Some(ScenarioGroupSubset::Indices(indices))
                } else {
                    return Err(ScenarioError::NoLabels {
                        group: self.name.clone(),
                    });
                }
            }
            None => None,
        };

        if let Some(labels) = &self.labels {
            if labels.len() != self.size {
                return Err(ScenarioError::IncorrectNumberOfLabels {
                    group: self.name,
                    found: labels.len(),
                    expected: self.size,
                });
            }
        }

        Ok(ScenarioGroup {
            name: self.name,
            size: self.size,
            subset,
            labels: self.labels,
        })
    }
}

#[derive(Clone, Debug)]
pub enum ScenarioLabelOrIndex {
    Label(String),
    Index(usize),
}

impl From<String> for ScenarioLabelOrIndex {
    fn from(label: String) -> Self {
        Self::Label(label)
    }
}

impl From<&str> for ScenarioLabelOrIndex {
    fn from(label: &str) -> Self {
        Self::Label(label.to_string())
    }
}

impl From<usize> for ScenarioLabelOrIndex {
    fn from(index: usize) -> Self {
        Self::Index(index)
    }
}

/// A builder for creating a [`ScenarioDomain`].
#[derive(Clone, Debug, Default)]
pub struct ScenarioDomainBuilder {
    groups: Vec<ScenarioGroup>,
    combinations: Option<Vec<Vec<ScenarioLabelOrIndex>>>,
}

impl ScenarioDomainBuilder {
    /// Add a [`ScenarioGroup`] to the collection
    pub fn with_group(mut self, group: ScenarioGroup) -> Result<Self, ScenarioError> {
        for g in self.groups.iter() {
            if g.name == group.name {
                return Err(ScenarioError::DuplicateGroupName(group.name.to_string()));
            }
        }

        self.groups.push(group);

        Ok(self)
    }

    pub fn with_combinations<T: Into<ScenarioLabelOrIndex>>(mut self, combinations: Vec<Vec<T>>) -> Self {
        self.combinations = Some(
            combinations
                .into_iter()
                .map(|inner| inner.into_iter().map(|c| c.into()).collect())
                .collect(),
        );
        self
    }

    /// Build a map of simulation indices to schema indices for each group
    fn build_scenario_map_from_subsets(&self) -> Vec<Option<Vec<usize>>> {
        let mut scenario_map: Vec<Option<Vec<usize>>> = vec![None; self.groups.len()];

        for (group_index, group) in self.groups.iter().enumerate() {
            if let Some(subset) = &group.subset {
                match subset {
                    ScenarioGroupSubset::Slice { start, end } => {
                        let mut indices: Vec<usize> = Vec::with_capacity(end - start);
                        for i in *start..*end {
                            indices.push(i);
                        }
                        scenario_map[group_index] = Some(indices);
                    }
                    ScenarioGroupSubset::Indices(indices) => {
                        scenario_map[group_index] = Some(indices.clone());
                    }
                }
            }
        }

        scenario_map
    }

    /// Build a map of simulation indices to schema indices for each group from a list of combinations
    fn build_scenario_map_from_combinations(
        &self,
        combinations: &[Vec<usize>],
    ) -> Result<Vec<Option<Vec<usize>>>, ScenarioError> {
        let mut scenario_map: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); self.groups.len()];

        for combination in combinations.iter() {
            for (group_index, _group) in self.groups.iter().enumerate() {
                scenario_map[group_index].insert(combination[group_index]);
            }
        }

        let scenario_map: Vec<Option<Vec<usize>>> = scenario_map
            .iter()
            .map(|set| {
                if set.is_empty() {
                    None
                } else {
                    Some(set.iter().cloned().collect())
                }
            })
            .collect();

        Ok(scenario_map)
    }

    fn schema_id(&self, combination: &[usize]) -> usize {
        let mut id = 0;
        let mut multiplier = 1;
        for (group_index, group) in self.groups.iter().enumerate() {
            id += combination[group_index] * multiplier;
            multiplier *= group.size;
        }
        id
    }

    pub fn build(self) -> Result<ScenarioDomain, ScenarioError> {
        let (indices, groups, scenario_map) = if self.groups.is_empty() {
            // Default to a single scenario if no groups are defined.
            (
                vec![ScenarioIndex::default()],
                vec![ScenarioGroup::default()],
                vec![None],
            )
        } else {
            let num: usize = self.groups.iter().map(|grp| grp.size).product();
            let mut scenario_indices: Vec<ScenarioIndex> = Vec::with_capacity(num);

            if self.groups.iter().any(|grp| grp.subset.is_some()) && self.combinations.is_some() {
                return Err(ScenarioError::CombinationsAndSlices);
            }

            // Handle the case where there are specific combinations of scenarios
            let scenario_map = if let Some(combinations) = &self.combinations {
                // First turn all the maybe labels into indices
                let combinations = combinations
                    .iter()
                    .map(|combination| {
                        combination
                            .iter()
                            .zip(&self.groups)
                            .map(|(c, group)| match c {
                                ScenarioLabelOrIndex::Label(label) => group.label_position(label),
                                ScenarioLabelOrIndex::Index(index) => Ok(*index),
                            })
                            .collect::<Result<Vec<usize>, ScenarioError>>()
                    })
                    .collect::<Result<Vec<Vec<usize>>, ScenarioError>>()?;

                let scenario_map_from_combinations = self.build_scenario_map_from_combinations(&combinations)?;

                for combination in combinations {
                    let simulation_indices = scenario_map_from_combinations
                        .iter()
                        .zip(combination.iter())
                        .map(|(s, c)| match s {
                            Some(set) => set.iter().position(|i| i == c).unwrap(),
                            None => *c,
                        })
                        .collect();

                    let labels: Vec<_> = self
                        .groups
                        .iter()
                        .zip(combination.iter())
                        .map(|(group, idx)| match group.labels.as_ref() {
                            Some(labels) => labels[*idx].clone(),
                            None => idx.to_string(),
                        })
                        .collect();

                    let mut scenario_index_builder =
                        ScenarioIndexBuilder::new(scenario_indices.len(), simulation_indices, labels);

                    scenario_index_builder =
                        scenario_index_builder.with_schema(self.schema_id(&combination), combination.clone());

                    scenario_indices.push(scenario_index_builder.build());
                }
                scenario_map_from_combinations
            } else {
                // Case with either all scenarios or a subset of scenarios via slices
                let scenario_map_from_slices = self.build_scenario_map_from_subsets();

                let is_sliced = scenario_map_from_slices.iter().any(|s| s.is_some());

                if is_sliced && self.combinations.is_some() {
                    return Err(ScenarioError::CombinationsAndSlices);
                }

                for scenario_id in 0..num {
                    let mut remaining = scenario_id;
                    // These are the indices as defined in the schema (i.e. all combinations)
                    let mut schema_indices: Vec<usize> = Vec::with_capacity(self.groups.len());
                    let mut schema_labels: Vec<String> = Vec::with_capacity(self.groups.len());

                    for grp in self.groups.iter().rev() {
                        let idx = remaining % grp.size;
                        remaining /= grp.size;
                        schema_indices.push(idx);

                        let label = match grp.labels.as_ref() {
                            Some(labels) => labels[idx].clone(),
                            None => idx.to_string(),
                        };
                        schema_labels.push(label);
                    }
                    schema_indices.reverse();
                    schema_labels.reverse();

                    // Test whether the indices are within any defined slices for each group
                    let mut include_scenario = true;

                    // These are the indices as defined for the simulation; adjusted for slices
                    let mut simulation_indices: Vec<usize> = Vec::with_capacity(self.groups.len());

                    for (group_index, group) in self.groups.iter().enumerate() {
                        if let Some(subset) = &group.subset {
                            match subset {
                                ScenarioGroupSubset::Slice { start, end } => {
                                    if schema_indices[group_index] < *start || schema_indices[group_index] >= *end {
                                        // Skip this scenario
                                        include_scenario = false;
                                        break;
                                    } else {
                                        simulation_indices.push(schema_indices[group_index] - start);
                                    }
                                }
                                ScenarioGroupSubset::Indices(indices) => {
                                    if !indices.contains(&schema_indices[group_index]) {
                                        // Skip this scenario
                                        include_scenario = false;
                                        break;
                                    } else {
                                        simulation_indices.push(
                                            indices.iter().position(|i| i == &schema_indices[group_index]).unwrap(),
                                        );
                                    }
                                }
                            }
                        } else {
                            simulation_indices.push(schema_indices[group_index]);
                        }
                    }

                    if include_scenario {
                        let simulation_id = scenario_indices.len();
                        let mut scenario_index_builder =
                            ScenarioIndexBuilder::new(simulation_id, simulation_indices, schema_labels);

                        if is_sliced {
                            scenario_index_builder = scenario_index_builder.with_schema(scenario_id, schema_indices);
                        }

                        scenario_indices.push(scenario_index_builder.build());
                    }
                }
                scenario_map_from_slices
            };

            (scenario_indices, self.groups, scenario_map)
        };

        Ok(ScenarioDomain {
            indices,
            groups,
            scenario_map,
        })
    }
}

/// A scenario index and its indices for each group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioIndices {
    /// The global index of the scenario.
    pub index: usize,
    /// The index of the scenario in each group.
    pub indices: Vec<usize>,
}

impl ScenarioIndices {
    pub fn new(index: usize, indices: Vec<usize>) -> Self {
        Self { index, indices }
    }
}

impl Default for ScenarioIndices {
    fn default() -> Self {
        Self {
            index: 0,
            indices: vec![0],
        }
    }
}

pub struct ScenarioIndexBuilder {
    core: ScenarioIndices,
    schema: Option<ScenarioIndices>,
    labels: Vec<String>,
}

impl ScenarioIndexBuilder {
    pub fn new<IL, L>(index: usize, indices: Vec<usize>, labels: IL) -> Self
    where
        IL: IntoIterator<Item = L>,
        L: Into<String>,
    {
        Self {
            core: ScenarioIndices::new(index, indices),
            schema: None,
            labels: labels.into_iter().map(Into::into).collect(),
        }
    }

    pub fn with_schema(mut self, index: usize, indices: Vec<usize>) -> Self {
        self.schema = Some(ScenarioIndices::new(index, indices));
        self
    }

    pub fn build(self) -> ScenarioIndex {
        ScenarioIndex {
            core: self.core,
            schema: self.schema,
            labels: self.labels,
        }
    }
}

#[cfg_attr(feature = "pyo3", pyclass)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioIndex {
    /// The indices of the scenarios run in the model.
    core: ScenarioIndices,
    /// The indices as defined in the original schema. When running a sub-set of the
    /// schema's scenarios this will contain the original indices. Otherwise, it will
    /// be `None`.
    schema: Option<ScenarioIndices>,
    /// Labels to use for the scenario; one for each group
    labels: Vec<String>,
}

impl Default for ScenarioIndex {
    fn default() -> Self {
        Self {
            core: ScenarioIndices::default(),
            schema: None,
            labels: vec!["0".to_string()],
        }
    }
}

#[cfg(feature = "pyo3")]
#[pymethods]
impl ScenarioIndex {
    /// The global index of the scenario for this simulation. This may be different
    /// from the global index of the scenario in the schema.
    #[getter]
    pub fn get_simulation_id(&self) -> usize {
        self.core.index
    }

    /// The indices for each scenario group for this simulation.
    #[getter]
    pub fn get_simulation_indices(&self) -> &[usize] {
        &self.core.indices
    }
}

impl ScenarioIndex {
    /// The global index of the scenario for this simulation. This may be different
    /// from the global index of the scenario in the schema.
    pub fn simulation_id(&self) -> usize {
        self.core.index
    }

    pub fn simulation_indices(&self) -> &[usize] {
        &self.core.indices
    }

    pub fn simulation_index_for_group(&self, group_index: usize) -> usize {
        self.core.indices[group_index]
    }

    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    pub fn schema_index_for_group(&self, group_index: usize) -> usize {
        self.schema.as_ref().map(|s| s.indices[group_index]).unwrap_or_else(|| self.core.indices[group_index])
    }

    /// Concatenated labels for the scenario
    ///
    /// This is useful for generating a unique label for each scenario index. The labels are
    /// concatenated with a `-` separator.
    pub fn label(&self) -> String {
        self.labels.join("-")
    }
}

/// The scenario domain for a model.
#[derive(Debug, Clone)]
pub struct ScenarioDomain {
    indices: Vec<ScenarioIndex>,
    groups: Vec<ScenarioGroup>,
    scenario_map: Vec<Option<Vec<usize>>>,
}

impl ScenarioDomain {
    /// The total number of scenario combinations in the domain.
    pub fn len(&self) -> usize {
        self.indices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn indices(&self) -> &[ScenarioIndex] {
        &self.indices
    }

    /// Return the index of a scenario group by name
    pub fn group_index(&self, name: &str) -> Result<usize, ScenarioError> {
        self.groups
            .iter()
            .position(|g| g.name == name)
            .ok_or_else(|| ScenarioError::GroupNameNotFound(name.to_string()))
    }

    pub fn groups(&self) -> &[ScenarioGroup] {
        &self.groups
    }

    /// Return a map of the simulation indices to the schema indices for a group
    pub fn group_scenario_subset(&self, name: &str) -> Result<Option<&[usize]>, ScenarioError> {
        let group_index = self.group_index(name)?;
        Ok(self.scenario_map[group_index].as_deref())
    }

    pub fn group_size(&self, name: &str) -> Result<usize, ScenarioError> {
        let group_index = self.group_index(name)?;
        Ok(self.groups[group_index].size())
    }
}

#[cfg(test)]
mod tests {
    use super::{ScenarioDomain, ScenarioDomainBuilder, ScenarioError, ScenarioGroupBuilder};

    #[test]
    fn test_group_builder() {
        let group = ScenarioGroupBuilder::new("A", 3)
            .with_subset_slice(0, 2)
            .with_labels(&["1", "2", "3"])
            .build()
            .unwrap();

        assert_eq!(group.name(), "A");
        assert_eq!(group.size(), 3);
    }

    #[test]
    fn test_group_builder_wrong_num_labels() {
        let group = ScenarioGroupBuilder::new("A", 3)
            .with_labels(&["1", "2", "3", "4"])
            .build();

        assert!(group.is_err());
        assert!(matches!(
            group.err().unwrap(),
            ScenarioError::IncorrectNumberOfLabels {
                group: _,
                found: 4,
                expected: 3
            }
        ));
    }

    #[test]
    fn test_group_builder_subset_labels_no_labels() {
        let group = ScenarioGroupBuilder::new("A", 3)
            .with_subset_labels(&["1", "2", "3"])
            .build();

        assert!(group.is_err());
        assert!(matches!(group.err().unwrap(), ScenarioError::NoLabels { group: _ }));
    }

    #[test]
    fn test_group_builder_invalid_slice() {
        let group = ScenarioGroupBuilder::new("A", 3).with_subset_slice(0, 4).build();

        assert!(group.is_err());
        assert!(matches!(
            group.err().unwrap(),
            ScenarioError::InvalidSlice {
                group: _,
                size: 3,
                start: 0,
                end: 4
            }
        ));
    }

    #[test]
    /// Test duplicate scenario group names
    fn test_duplicate_scenario_group_names() {
        let group_a = ScenarioGroupBuilder::new("A", 1).build().unwrap();
        let group_b = ScenarioGroupBuilder::new("A", 1).build().unwrap();

        let result = ScenarioDomainBuilder::default()
            .with_group(group_a)
            .unwrap()
            .with_group(group_b);

        assert!(result.is_err());
        assert!(matches!(result.err().unwrap(), ScenarioError::DuplicateGroupName(_)));
    }

    #[test]
    /// Test slices and combinations is invalid
    fn test_slices_and_combinations() {
        let group_a = ScenarioGroupBuilder::new("A", 1)
            .with_subset_slice(0, 1)
            .build()
            .unwrap();
        let group_b = ScenarioGroupBuilder::new("B", 1).build().unwrap();
        let group_c = ScenarioGroupBuilder::new("C", 1).build().unwrap();

        let result = ScenarioDomainBuilder::default()
            .with_group(group_a)
            .unwrap()
            .with_group(group_b)
            .unwrap()
            .with_group(group_c)
            .unwrap()
            .with_combinations(vec![vec![0, 0, 0]])
            .build();

        assert!(result.is_err());
        assert!(matches!(result.err().unwrap(), ScenarioError::CombinationsAndSlices));
    }

    #[test]
    /// Test [`ScenarioDomain`] iteration
    fn test_scenario_iteration() {
        let group_a = ScenarioGroupBuilder::new("A", 10).build().unwrap();
        let group_b = ScenarioGroupBuilder::new("B", 2).build().unwrap();
        let group_c = ScenarioGroupBuilder::new("C", 5).build().unwrap();

        let builder = ScenarioDomainBuilder::default()
            .with_group(group_a)
            .unwrap()
            .with_group(group_b)
            .unwrap()
            .with_group(group_c)
            .unwrap();

        let domain: ScenarioDomain = builder.build().unwrap();
        let mut iter = domain.indices().iter();

        // Test generation of scenario indices
        let si = iter.next().unwrap();
        assert_eq!(si.simulation_id(), 0);
        assert_eq!(si.simulation_indices(), &[0, 0, 0]);

        let si = iter.next().unwrap();
        assert_eq!(si.simulation_id(), 1);
        assert_eq!(si.simulation_indices(), &[0, 0, 1]);

        let si = iter.next().unwrap();
        assert_eq!(si.simulation_id(), 2);
        assert_eq!(si.simulation_indices(), &[0, 0, 2]);

        let si = iter.next().unwrap();
        assert_eq!(si.simulation_id(), 3);
        assert_eq!(si.simulation_indices(), &[0, 0, 3]);

        let si = iter.next().unwrap();
        assert_eq!(si.simulation_id(), 4);
        assert_eq!(si.simulation_indices(), &[0, 0, 4]);

        let si = iter.next().unwrap();
        assert_eq!(si.simulation_id(), 5);
        assert_eq!(si.simulation_indices(), &[0, 1, 0]);

        let si = iter.next().unwrap();
        assert_eq!(si.simulation_id(), 6);
        assert_eq!(si.simulation_indices(), &[0, 1, 1]);

        let si = iter.next().unwrap();
        assert_eq!(si.simulation_id(), 7);
        assert_eq!(si.simulation_indices(), &[0, 1, 2]);

        let si = iter.next().unwrap();
        assert_eq!(si.simulation_id(), 8);
        assert_eq!(si.simulation_indices(), &[0, 1, 3]);

        let si = iter.next().unwrap();
        assert_eq!(si.simulation_id(), 9);
        assert_eq!(si.simulation_indices(), &[0, 1, 4]);

        let si = iter.next().unwrap();
        assert_eq!(si.simulation_id(), 10);
        assert_eq!(si.simulation_indices(), &[1, 0, 0]);

        // Test final index
        let si = iter.last().unwrap();
        assert_eq!(si.simulation_id(), 99);
        assert_eq!(si.simulation_indices(), &[9, 1, 4]);
    }

    #[test]
    /// Test [`ScenarioDomain`] iteration with slices
    fn test_scenario_iteration_with_slices() {
        let group = ScenarioGroupBuilder::new("A", 10)
            .with_subset_slice(2, 8)
            .build()
            .unwrap();
        let builder = ScenarioDomainBuilder::default().with_group(group).unwrap();

        let domain: ScenarioDomain = builder.build().unwrap();
        let mut iter = domain.indices().iter();

        // Test generation of scenario indices
        let si = iter.next().unwrap();
        assert_eq!(si.simulation_id(), 0);
        assert_eq!(si.simulation_indices(), &[0]);
        assert_eq!(si.labels(), &["2"]);

        let si = iter.next().unwrap();
        assert_eq!(si.simulation_id(), 1);
        assert_eq!(si.simulation_indices(), &[1]);
        assert_eq!(si.labels(), &["3"]);

        // Test final index
        let si = iter.last().unwrap();
        assert_eq!(si.simulation_id(), 5);
        assert_eq!(si.simulation_indices(), &[5]);
        assert_eq!(si.labels(), &["7"]);
    }

    #[test]
    /// Test [`ScenarioDomain`] iteration with combinations
    fn test_scenario_iteration_with_combinations() {
        let group_a = ScenarioGroupBuilder::new("A", 10).build().unwrap();
        let group_b = ScenarioGroupBuilder::new("B", 2).build().unwrap();
        let group_c = ScenarioGroupBuilder::new("C", 5).build().unwrap();

        let domain = ScenarioDomainBuilder::default()
            .with_group(group_a)
            .unwrap()
            .with_group(group_b)
            .unwrap()
            .with_group(group_c)
            .unwrap()
            .with_combinations(vec![vec![0, 0, 0], vec![0, 1, 0], vec![0, 1, 1], vec![2, 1, 3]])
            .build()
            .unwrap();

        let mut iter = domain.indices().iter();

        // Test generation of scenario indices
        let si = iter.next().unwrap();
        assert_eq!(si.simulation_id(), 0);
        assert_eq!(si.simulation_indices(), &[0, 0, 0]);

        let si = iter.next().unwrap();
        assert_eq!(si.simulation_id(), 1);
        assert_eq!(si.simulation_indices(), &[0, 1, 0]);

        let si = iter.next().unwrap();
        assert_eq!(si.simulation_id(), 2);
        assert_eq!(si.simulation_indices(), &[0, 1, 1]);

        // Test final index
        let si = iter.last().unwrap();
        assert_eq!(si.simulation_id(), 3);
        assert_eq!(si.simulation_indices(), &[1, 1, 2]);
    }
}
