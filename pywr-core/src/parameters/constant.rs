use crate::parameters::{
    downcast_internal_state_mut, downcast_internal_state_ref, downcast_variable_config_ref, ActivationFunction,
    ConstParameter, Parameter, ParameterMeta, ParameterName, ParameterState, VariableConfig, VariableParameter,
};
use crate::scenario::ScenarioIndex;
use crate::state::ConstParameterValues;
use crate::timestep::Timestep;
use crate::PywrError;

pub struct ConstantParameter {
    meta: ParameterMeta,
    value: f64,
}

// We store this internal value as an Option<f64> so that it can be updated by the variable API
type InternalValue = Option<f64>;

impl ConstantParameter {
    pub fn new(name: ParameterName, value: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            value,
        }
    }

    /// Return the current value.
    ///
    /// If the internal state is None, the value is returned directly. Otherwise, the internal value must
    /// have come from the variable API and is passed through the activation function.
    fn value(&self, internal_state: &Option<Box<dyn ParameterState>>) -> f64 {
        match downcast_internal_state_ref::<InternalValue>(internal_state) {
            Some(value) => *value,
            None => self.value,
        }
    }
}

impl Parameter for ConstantParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, PywrError> {
        let value: Option<f64> = None;
        Ok(Some(Box::new(value)))
    }
    fn as_variable(&self) -> Option<&dyn VariableParameter> {
        Some(self)
    }
}

impl ConstParameter<f64> for ConstantParameter {
    fn compute(
        &self,
        _scenario_index: &ScenarioIndex,
        _values: &ConstParameterValues,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, PywrError> {
        Ok(self.value(internal_state))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl VariableParameter for ConstantParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn size(&self, _variable_config: &dyn VariableConfig) -> (usize, usize) {
        (1, 0)
    }

    fn set_variables(
        &self,
        values_f64: &[f64],
        values_u64: &[u64],
        variable_config: &dyn VariableConfig,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), PywrError> {
        let activation_function = downcast_variable_config_ref::<ActivationFunction>(variable_config);

        if !values_u64.is_empty() {
            return Err(PywrError::ParameterVariableValuesIncorrectLength);
        }

        if values_f64.len() == 1 {
            let value = downcast_internal_state_mut::<InternalValue>(internal_state);
            *value = Some(activation_function.apply(values_f64[0]));
            Ok(())
        } else {
            Err(PywrError::ParameterVariableValuesIncorrectLength)
        }
    }

    fn get_variables(&self, internal_state: &Option<Box<dyn ParameterState>>) -> Option<(Vec<f64>, Vec<u64>)> {
        downcast_internal_state_ref::<InternalValue>(internal_state)
            .as_ref()
            .map(|value| (vec![*value], vec![]))
    }

    fn get_lower_bounds(&self, variable_config: &dyn VariableConfig) -> Result<(Vec<f64>, Vec<u64>), PywrError> {
        let activation_function = downcast_variable_config_ref::<ActivationFunction>(variable_config);
        Ok((vec![activation_function.lower_bound()], vec![]))
    }

    fn get_upper_bounds(&self, variable_config: &dyn VariableConfig) -> Result<(Vec<f64>, Vec<u64>), PywrError> {
        let activation_function = downcast_variable_config_ref::<ActivationFunction>(variable_config);
        Ok((vec![activation_function.upper_bound()], vec![]))
    }
}

#[cfg(test)]
mod tests {
    use crate::parameters::{ActivationFunction, ConstantParameter, Parameter, VariableParameter};
    use crate::test_utils::default_domain;
    use float_cmp::assert_approx_eq;

    #[test]
    fn test_variable_api() {
        let domain = default_domain();

        let var = ActivationFunction::Unit { min: 0.0, max: 2.0 };
        let p = ConstantParameter::new("test".into(), 1.0);
        let mut state = p
            .setup(domain.time().timesteps(), domain.scenarios().indices().first().unwrap())
            .unwrap();

        // No value set initially
        assert_eq!(p.get_variables(&state), None);

        // Update the value via the variable API
        p.set_variables(&[2.0], &[], &var, &mut state).unwrap();

        // Check the parameter returns the new value
        assert_approx_eq!(f64, p.value(&state), 2.0);

        let (v_f64, v_u64) = p.get_variables(&state).unwrap();
        assert_approx_eq!(&[f64], &v_f64, &[2.0]);
        assert!(v_u64.is_empty());
    }
}
