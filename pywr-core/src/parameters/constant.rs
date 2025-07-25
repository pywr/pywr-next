use crate::parameters::errors::{ConstCalculationError, ParameterSetupError};
use crate::parameters::{
    ActivationFunction, ConstParameter, Parameter, ParameterMeta, ParameterName, ParameterState, VariableConfig,
    VariableParameter, VariableParameterError, downcast_internal_state_mut, downcast_internal_state_ref,
    downcast_variable_config_ref,
};
use crate::scenario::ScenarioIndex;
use crate::state::ConstParameterValues;
use crate::timestep::Timestep;

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
    ) -> Result<Option<Box<dyn ParameterState>>, ParameterSetupError> {
        let value: Option<f64> = None;
        Ok(Some(Box::new(value)))
    }
    fn as_f64_variable(&self) -> Option<&dyn VariableParameter<f64>> {
        Some(self)
    }

    fn as_f64_variable_mut(&mut self) -> Option<&mut dyn VariableParameter<f64>> {
        Some(self)
    }
}

impl ConstParameter<f64> for ConstantParameter {
    fn compute(
        &self,
        _scenario_index: &ScenarioIndex,
        _values: &ConstParameterValues,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, ConstCalculationError> {
        Ok(self.value(internal_state))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl VariableParameter<f64> for ConstantParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn size(&self, _variable_config: &dyn VariableConfig) -> usize {
        1
    }

    fn set_variables(
        &self,
        values: &[f64],
        variable_config: &dyn VariableConfig,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), VariableParameterError> {
        let activation_function = downcast_variable_config_ref::<ActivationFunction>(variable_config);

        if values.len() == 1 {
            let value = downcast_internal_state_mut::<InternalValue>(internal_state);
            *value = Some(activation_function.apply(values[0]));
            Ok(())
        } else {
            Err(VariableParameterError::IncorrectNumberOfValues {
                expected: 1,
                received: values.len(),
            })
        }
    }

    fn get_variables(&self, internal_state: &Option<Box<dyn ParameterState>>) -> Option<Vec<f64>> {
        downcast_internal_state_ref::<InternalValue>(internal_state)
            .as_ref()
            .map(|value| vec![*value])
    }

    fn get_lower_bounds(&self, variable_config: &dyn VariableConfig) -> Option<Vec<f64>> {
        let activation_function = downcast_variable_config_ref::<ActivationFunction>(variable_config);
        Some(vec![activation_function.lower_bound()])
    }

    fn get_upper_bounds(&self, variable_config: &dyn VariableConfig) -> Option<Vec<f64>> {
        let activation_function = downcast_variable_config_ref::<ActivationFunction>(variable_config);
        Some(vec![activation_function.upper_bound()])
    }
}

#[cfg(test)]
mod tests {
    use crate::parameters::{ActivationFunction, ConstantParameter, Parameter, VariableParameter};
    use crate::test_utils::default_domain;
    use float_cmp::assert_approx_eq;
    use std::f64::consts::PI;

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
        p.set_variables(&[2.0], &var, &mut state).unwrap();

        // Check the parameter returns the new value
        assert_approx_eq!(f64, p.value(&state), 2.0);

        assert_approx_eq!(&[f64], &p.get_variables(&state).unwrap(), &[2.0]);
    }

    #[test]
    /// Test `ConstantParameter` returns the correct value.
    fn test_constant_parameter() {
        let domain = default_domain();

        let p = ConstantParameter::new("my-parameter".into(), PI);
        let state = p
            .setup(domain.time().timesteps(), domain.scenarios().indices().first().unwrap())
            .unwrap();

        assert_approx_eq!(f64, p.value(&state), PI);
    }
}
