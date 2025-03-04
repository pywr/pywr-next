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
}

#[derive(Clone, Debug)]
pub struct ScenarioGroup {
    /// Name of the scenario group
    name: String,
    /// Number of scenarios in the group
    size: usize,
    /// Optional slice of scenarios to run
    slice: Option<(usize, usize)>,
    /// Optional labels for the group
    labels: Option<Vec<String>>,
}

impl Default for ScenarioGroup {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            size: 1,
            slice: None,
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
}

/// A builder for creating a [`ScenarioDomain`].
#[derive(Clone, Debug, Default)]
pub struct ScenarioDomainBuilder {
    groups: Vec<ScenarioGroup>,
    combinations: Option<Vec<Vec<usize>>>,
}

impl ScenarioDomainBuilder {
    /// Add a [`ScenarioGroup`] to the collection
    pub fn with_group(
        mut self,
        name: &str,
        size: usize,
        slice: Option<(usize, usize)>,
        labels: Option<Vec<String>>,
    ) -> Result<Self, ScenarioError> {
        for group in self.groups.iter() {
            if group.name == name {
                return Err(ScenarioError::DuplicateGroupName(name.to_string()));
            }
        }

        self.groups.push(ScenarioGroup {
            name: name.to_string(),
            size,
            slice,
            labels,
        });

        Ok(self)
    }

    pub fn with_combinations(mut self, combinations: Vec<Vec<usize>>) -> Self {
        self.combinations = Some(combinations);
        self
    }

    /// Build a map of simulation indices to schema indices for each group
    fn build_scenario_map_from_slices(&self) -> Vec<Option<Vec<usize>>> {
        let mut scenario_map: Vec<Option<Vec<usize>>> = vec![None; self.groups.len()];

        for (group_index, group) in self.groups.iter().enumerate() {
            if let Some((start, end)) = group.slice {
                let mut indices: Vec<usize> = Vec::with_capacity(end - start);
                for i in start..end {
                    indices.push(i);
                }
                scenario_map[group_index] = Some(indices);
            }
        }

        scenario_map
    }

    /// Build a map of simulation indices to schema indices for each group from a list of combinations
    fn build_scenario_map_from_combinations(&self) -> Vec<Option<Vec<usize>>> {
        let mut scenario_map: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); self.groups.len()];

        if let Some(combinations) = &self.combinations {
            for combination in combinations.iter() {
                for (group_index, _group) in self.groups.iter().enumerate() {
                    scenario_map[group_index].insert(combination[group_index]);
                }
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

        scenario_map
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

            if self.groups.iter().any(|grp| grp.slice.is_some()) && self.combinations.is_some() {
                return Err(ScenarioError::CombinationsAndSlices);
            }

            // Handle the case where there are specific combinations of scenarios
            let scenario_map = if let Some(combinations) = &self.combinations {
                let scenario_map_from_combinations = self.build_scenario_map_from_combinations();

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
                        scenario_index_builder.with_schema(self.schema_id(combination), combination.clone());

                    scenario_indices.push(scenario_index_builder.build());
                }
                scenario_map_from_combinations
            } else {
                // Case with either all scenarios or a subset of scenarios via slices
                let scenario_map_from_slices = self.build_scenario_map_from_slices();

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
                        if let Some((start, end)) = group.slice {
                            if schema_indices[group_index] < start || schema_indices[group_index] >= end {
                                // Skip this scenario
                                include_scenario = false;
                                break;
                            } else {
                                simulation_indices.push(schema_indices[group_index] - start);
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

    /// Concatenated labels for the scenario
    ///
    /// This is useful for generating a unique label for each scenario index. The labels are
    /// concatenated with a `-` separator.
    pub fn label(&self) -> String {
        self.labels.join("-")
    }
}

/// The scenario domain for a model.
#[derive(Debug)]
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
}

#[cfg(test)]
mod tests {
    use super::{ScenarioDomain, ScenarioDomainBuilder, ScenarioError};

    #[test]
    /// Test duplicate scenario group names
    fn test_duplicate_scenario_group_names() {
        let result = ScenarioDomainBuilder::default()
            .with_group("A", 1, None, None)
            .unwrap()
            .with_group("A", 1, None, None);

        assert!(result.is_err());
        assert!(matches!(result.err().unwrap(), ScenarioError::DuplicateGroupName(_)));
    }

    #[test]
    /// Test slices and combinations is invalid
    fn test_slices_and_combinations() {
        let result = ScenarioDomainBuilder::default()
            .with_group("A", 1, Some((0, 1)), None)
            .unwrap()
            .with_group("B", 1, None, None)
            .unwrap()
            .with_group("C", 1, None, None)
            .unwrap()
            .with_combinations(vec![vec![0, 0, 0]])
            .build();

        assert!(result.is_err());
        assert!(matches!(result.err().unwrap(), ScenarioError::CombinationsAndSlices));
    }

    #[test]
    /// Test [`ScenarioDomain`] iteration
    fn test_scenario_iteration() {
        let builder = ScenarioDomainBuilder::default()
            .with_group("A", 10, None, None)
            .unwrap()
            .with_group("B", 2, None, None)
            .unwrap()
            .with_group("C", 5, None, None)
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
        let builder = ScenarioDomainBuilder::default()
            .with_group("A", 10, Some((2, 8)), None)
            .unwrap();

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
        let builder = ScenarioDomainBuilder::default()
            .with_group("A", 10, None, None)
            .unwrap()
            .with_group("B", 2, None, None)
            .unwrap()
            .with_group("C", 5, None, None)
            .unwrap()
            .with_combinations(vec![vec![0, 0, 0], vec![0, 1, 0], vec![0, 1, 1], vec![2, 1, 3]]);

        let domain: ScenarioDomain = builder.build().unwrap();
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
