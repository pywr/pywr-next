use crate::model::Model;
use crate::parameters::{ActivationFunction, Parameter, ParameterMeta, VariableParameter};
use crate::scenario::ScenarioIndex;
use crate::state::State;
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
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _model: &Model,
        _state: &State,
        _internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<f64, PywrError> {
        Ok(self.value)
    }

    fn as_variable(&self) -> Option<&dyn VariableParameter> {
        Some(self)
    }

    fn as_variable_mut(&mut self) -> Option<&mut dyn VariableParameter> {
        Some(self)
    }
}

impl VariableParameter for ConstantParameter {
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

    fn get_lower_bounds(&self) -> Vec<f64> {
        vec![self
            .variable
            .as_ref()
            .expect("Can't get lower bounds of an inactive variable!")
            .lower_bound()]
    }

    fn get_upper_bounds(&self) -> Vec<f64> {
        vec![self
            .variable
            .as_ref()
            .expect("Can't get upper bounds of an inactive variable!")
            .upper_bound()]
    }
}
