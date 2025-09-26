use crate::parameters::errors::ConstCalculationError;
use crate::parameters::{ConstParameter, Parameter, ParameterMeta, ParameterName, ParameterState};
use crate::scenario::ScenarioIndex;
use crate::state::ConstParameterValues;

pub struct ConstantScenarioParameter {
    meta: ParameterMeta,
    values: Vec<f64>,
    scenario_group_index: usize,
}

impl ConstantScenarioParameter {
    pub fn new(name: ParameterName, values: Vec<f64>, scenario_group_index: usize) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            values,
            scenario_group_index,
        }
    }
}

impl Parameter for ConstantScenarioParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl ConstParameter<f64> for ConstantScenarioParameter {
    fn compute(
        &self,
        scenario_index: &ScenarioIndex,
        _values: &ConstParameterValues,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, ConstCalculationError> {
        let s_idx = scenario_index.schema_index_for_group(self.scenario_group_index);
        Ok(self.values[s_idx])
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::parameters::ConstParameter;
    use crate::parameters::constant_scenario::ConstantScenarioParameter;
    use crate::scenario::{self, ScenarioGroupBuilder};
    use crate::state::ConstParameterValues;
    use float_cmp::assert_approx_eq;

    #[test]
    /// Test `ConstantScenarioParameter` returns the correct values.
    fn test_constant_scenario_parameter() {
        let scenario_group = ScenarioGroupBuilder::new("group1", 3).build().unwrap();
        let scenario_domain = scenario::ScenarioDomainBuilder::default()
            .with_group(scenario_group)
            .unwrap()
            .build()
            .unwrap();

        let p = ConstantScenarioParameter::new("my-parameter".into(), vec![3.0, 2.0, 1.0], 0);

        let const_scenario_values = ConstParameterValues::default();

        for (si, expected) in scenario_domain.indices().iter().zip([3.0, 2.0, 1.0].iter()) {
            assert_approx_eq!(
                f64,
                p.compute(si, &const_scenario_values, &mut None).unwrap(),
                *expected
            );
        }
    }

    #[test]
    fn test_constant_scenario_parameter_with_subset_indices() {
        let scenario_group = ScenarioGroupBuilder::new("group1", 5).with_subset_indices(vec![0, 2, 4]).build().unwrap();
        let scenario_domain = scenario::ScenarioDomainBuilder::default()
            .with_group(scenario_group)
            .unwrap()
            .build()
            .unwrap();

        let p = ConstantScenarioParameter::new("my-parameter".into(), vec![5.0, 4.0, 3.0, 2.0, 1.0], 0);

        let const_scenario_values = ConstParameterValues::default();

        for (si, expected) in scenario_domain.indices().iter().zip([5.0, 3.0, 1.0].iter()) {
            assert_approx_eq!(
                f64,
                p.compute(si, &const_scenario_values, &mut None).unwrap(),
                *expected
            );
        }
    }

    #[test]
    fn test_constant_scenario_parameter_with_subset_slice() {
        let scenario_group = ScenarioGroupBuilder::new("group1", 5).with_subset_slice(1, 3).build().unwrap();
        let scenario_domain = scenario::ScenarioDomainBuilder::default()
            .with_group(scenario_group)
            .unwrap()
            .build()
            .unwrap();

        let p = ConstantScenarioParameter::new("my-parameter".into(), vec![5.0, 4.0, 3.0, 2.0, 1.0], 0);

        let const_scenario_values = ConstParameterValues::default();

        for (si, expected) in scenario_domain.indices().iter().zip([4.0, 3.0].iter()) {
            assert_approx_eq!(
                f64,
                p.compute(si, &const_scenario_values, &mut None).unwrap(),
                *expected
            );
        }
    }
}
