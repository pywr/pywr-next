use crate::network::Network;
use crate::parameters::{
    downcast_internal_state_mut, downcast_internal_state_ref, ActivationFunction, Parameter, ParameterMeta,
    VariableParameter,
};
use crate::scenario::ScenarioIndex;
use crate::state::{ParameterState, State};
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;

pub struct ConstantParameter {
    meta: ParameterMeta,
    value: f64,
    variable: Option<ActivationFunction>,
}

// We store this internal value as an Option<f64> so that it can be updated by the variable API
type InternalValue = Option<f64>;

impl ConstantParameter {
    pub fn new(name: &str, value: f64, variable: Option<ActivationFunction>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            value,
            variable,
        }
    }

    /// Return the current value.
    ///
    /// If the internal state is None, the value is returned directly. Otherwise, the internal value must
    /// have come from the variable API and is passed through the activation function.
    fn value(&self, internal_state: &Option<Box<dyn ParameterState>>) -> f64 {
        match downcast_internal_state_ref::<InternalValue>(internal_state) {
            Some(value) => match self.variable {
                Some(variable) => variable.apply(*value),
                None => unreachable!("Internal state should not be set if variable is not active"),
            },
            None => self.value,
        }
    }
}

impl Parameter for ConstantParameter {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

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

    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _model: &Network,
        _state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, PywrError> {
        Ok(self.value(internal_state))
    }

    fn as_f64_variable(&self) -> Option<&dyn VariableParameter<f64>> {
        Some(self)
    }

    fn as_f64_variable_mut(&mut self) -> Option<&mut dyn VariableParameter<f64>> {
        Some(self)
    }
}

impl VariableParameter<f64> for ConstantParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn is_active(&self) -> bool {
        self.variable.is_some()
    }

    fn size(&self) -> usize {
        1
    }

    fn set_variables(
        &self,
        values: &[f64],
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), PywrError> {
        if values.len() == 1 {
            let value = downcast_internal_state_mut::<InternalValue>(internal_state);
            *value = Some(values[0]);
            Ok(())
        } else {
            Err(PywrError::ParameterVariableValuesIncorrectLength)
        }
    }

    fn get_variables(&self, internal_state: &Option<Box<dyn ParameterState>>) -> Option<Vec<f64>> {
        match downcast_internal_state_ref::<InternalValue>(internal_state) {
            Some(value) => Some(vec![*value]),
            None => None,
        }
    }

    fn get_lower_bounds(&self) -> Result<Vec<f64>, PywrError> {
        match self.variable {
            Some(variable) => Ok(vec![variable.lower_bound()]),
            None => Err(PywrError::ParameterVariableNotActive),
        }
    }

    fn get_upper_bounds(&self) -> Result<Vec<f64>, PywrError> {
        match self.variable {
            Some(variable) => Ok(vec![variable.upper_bound()]),
            None => Err(PywrError::ParameterVariableNotActive),
        }
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

        let p = ConstantParameter::new("test", 1.0, Some(ActivationFunction::Unit { min: 0.0, max: 2.0 }));
        let mut state = p
            .setup(
                &domain.time().timesteps(),
                domain.scenarios().indices().first().unwrap(),
            )
            .unwrap();

        // No value set initially
        assert_eq!(p.get_variables(&state), None);

        // Update the value via the variable API
        p.set_variables(&[2.0], &mut state).unwrap();

        // Check the parameter returns the new value
        assert_approx_eq!(f64, p.value(&state), 2.0);

        assert_approx_eq!(&[f64], &p.get_variables(&state).unwrap(), &[2.0]);
    }
}
