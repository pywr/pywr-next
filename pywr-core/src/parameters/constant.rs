use crate::network::Network;
use crate::parameters::{downcast_internal_state, ActivationFunction, Parameter, ParameterMeta, VariableParameter};
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

impl ConstantParameter {
    pub fn new(name: &str, value: f64, variable: Option<ActivationFunction>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            value,
            variable,
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
        timesteps: &[Timestep],
        scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, PywrError> {
        Ok(Some(Box::new(self.value)))
    }

    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _model: &Network,
        _state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, PywrError> {
        let value = downcast_internal_state::<f64>(internal_state);

        Ok(*value)
    }

    fn as_f64_variable(&self) -> Option<&dyn VariableParameter<f64>> {
        Some(self)
    }

    fn as_f64_variable_mut(&mut self) -> Option<&mut dyn VariableParameter<f64>> {
        Some(self)
    }
}

impl VariableParameter<f64> for ConstantParameter {
    fn is_active(&self) -> bool {
        self.variable.is_some()
    }

    fn size(&self) -> usize {
        1
    }

    fn set_variables(&mut self, values: &[f64]) -> Result<(), PywrError> {
        if values.len() == 1 {
            let variable = self.variable.ok_or(PywrError::ParameterVariableNotActive)?;
            self.value = variable.apply(values[0]);
            Ok(())
        } else {
            Err(PywrError::ParameterVariableValuesIncorrectLength)
        }
    }

    fn get_variables(&self) -> Vec<f64> {
        vec![self.value]
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
