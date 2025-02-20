use thiserror::Error;

#[derive(Error, Debug)]
pub enum ScenarioError {
    #[error("Scenario group name `{0}` already exists")]
    DuplicateGroupName(String),
    #[error("Scenario group name `{0}` not found")]
    GroupNameNotFound(String),
}

#[derive(Clone, Debug)]
pub struct ScenarioGroup {
    name: String,
    size: usize,
    // TODO labels
    // labels: Option<Vec<String>>
}

impl ScenarioGroup {
    fn new(name: &str, size: usize) -> Self {
        Self {
            name: name.to_string(),
            size,
        }
    }

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
}

impl ScenarioDomainBuilder {
    /// Add a [`ScenarioGroup`] to the collection
    pub fn add_group(mut self, name: &str, size: usize) -> Result<Self, ScenarioError> {
        for group in self.groups.iter() {
            if group.name == name {
                return Err(ScenarioError::DuplicateGroupName(name.to_string()));
            }
        }

        self.groups.push(ScenarioGroup::new(name, size));

        Ok(self)
    }

    pub fn build(self) -> ScenarioDomain {
        let (indices, groups) = if self.groups.is_empty() {
            // Default to a single scenario if no groups are defined.
            (
                vec![ScenarioIndex::new_same_as_schema(0, vec![0])],
                vec![ScenarioGroup::new("default", 1)],
            )
        } else {
            let num: usize = self.groups.iter().map(|grp| grp.size).product();
            let mut scenario_indices: Vec<ScenarioIndex> = Vec::with_capacity(num);

            for scenario_id in 0..num {
                let mut remaining = scenario_id;
                let mut indices: Vec<usize> = Vec::with_capacity(self.groups.len());
                for grp in self.groups.iter().rev() {
                    let idx = remaining % grp.size;
                    remaining /= grp.size;
                    indices.push(idx);
                }
                indices.reverse();
                scenario_indices.push(ScenarioIndex::new_same_as_schema(scenario_id, indices));
            }
            (scenario_indices, self.groups)
        };

        ScenarioDomain { indices, groups }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioIndex {
    /// The indices of the scenarios run in the model.
    core: ScenarioIndices,
    /// The indices as defined in the original schema. When running a sub-set of the
    /// schema's scenarios this will contain the original indices. Otherwise, it will
    /// be `None`.
    schema: Option<ScenarioIndices>,
}

impl ScenarioIndex {
    /// Create a new scenario index with identical core and schema indices.
    pub fn new_same_as_schema(index: usize, indices: Vec<usize>) -> Self {
        Self {
            core: ScenarioIndices::new(index, indices.clone()),
            schema: None,
        }
    }

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

    pub fn core(&self) -> &ScenarioIndices {
        &self.core
    }

    pub fn schema(&self) -> Option<&ScenarioIndices> {
        self.schema.as_ref()
    }
}

/// The scenario domain for a model.
#[derive(Debug)]
pub struct ScenarioDomain {
    indices: Vec<ScenarioIndex>,
    groups: Vec<ScenarioGroup>,
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
}

#[cfg(test)]
mod tests {
    use super::{ScenarioDomain, ScenarioDomainBuilder, ScenarioError, ScenarioIndex};

    #[test]
    /// Test duplicate scenario group names
    fn test_duplicate_scenario_group_names() {
        let result = ScenarioDomainBuilder::default()
            .add_group("A", 1)
            .unwrap()
            .add_group("A", 1);

        assert!(result.is_err());
        assert!(matches!(result.err().unwrap(), ScenarioError::DuplicateGroupName(_)));
    }

    #[test]
    /// Test [`ScenarioDomain`] iteration
    fn test_scenario_iteration() {
        let builder = ScenarioDomainBuilder::default()
            .add_group("Scenarion A", 10)
            .unwrap()
            .add_group("Scenarion B", 2)
            .unwrap()
            .add_group("Scenarion C", 5)
            .unwrap();

        let domain: ScenarioDomain = builder.build();
        let mut iter = domain.indices().iter();

        // Test generation of scenario indices
        assert_eq!(iter.next(), Some(&ScenarioIndex::new_same_as_schema(0, vec![0, 0, 0])));
        assert_eq!(iter.next(), Some(&ScenarioIndex::new_same_as_schema(1, vec![0, 0, 1])));
        assert_eq!(iter.next(), Some(&ScenarioIndex::new_same_as_schema(2, vec![0, 0, 2])));
        assert_eq!(iter.next(), Some(&ScenarioIndex::new_same_as_schema(3, vec![0, 0, 3])));
        assert_eq!(iter.next(), Some(&ScenarioIndex::new_same_as_schema(4, vec![0, 0, 4])));
        assert_eq!(iter.next(), Some(&ScenarioIndex::new_same_as_schema(5, vec![0, 1, 0])));
        assert_eq!(iter.next(), Some(&ScenarioIndex::new_same_as_schema(6, vec![0, 1, 1])));
        assert_eq!(iter.next(), Some(&ScenarioIndex::new_same_as_schema(7, vec![0, 1, 2])));
        assert_eq!(iter.next(), Some(&ScenarioIndex::new_same_as_schema(8, vec![0, 1, 3])));
        assert_eq!(iter.next(), Some(&ScenarioIndex::new_same_as_schema(9, vec![0, 1, 4])));
        assert_eq!(iter.next(), Some(&ScenarioIndex::new_same_as_schema(10, vec![1, 0, 0])));

        // Test final index
        assert_eq!(iter.last(), Some(&ScenarioIndex::new_same_as_schema(99, vec![9, 1, 4])));
    }
}
