use crate::metric::Metric;
use crate::network::Network;
use crate::parameters::{ActivationFunction, Parameter, ParameterMeta, VariableParameter};
use crate::scenario::ScenarioIndex;
use std::any::Any;

use crate::state::{ParameterState, State};
use crate::timestep::Timestep;
use crate::PywrError;

pub struct OffsetParameter {
    meta: ParameterMeta,
    metric: Metric,
    offset: f64,
    variable: Option<ActivationFunction>,
}

impl OffsetParameter {
    pub fn new(name: &str, metric: Metric, offset: f64, variable: Option<ActivationFunction>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            offset,
            variable,
        }
    }
}

impl Parameter for OffsetParameter {
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
        model: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, PywrError> {
        // Current value
        let x = self.metric.get_value(model, state)?;
        Ok(x + self.offset)
    }
    fn as_f64_variable(&self) -> Option<&dyn VariableParameter<f64>> {
        Some(self)
    }

    fn as_f64_variable_mut(&mut self) -> Option<&mut dyn VariableParameter<f64>> {
        Some(self)
    }
}

impl VariableParameter<f64> for OffsetParameter {
    fn is_active(&self) -> bool {
        self.variable.is_some()
    }

    fn size(&self) -> usize {
        1
    }

    fn set_variables(&mut self, values: &[f64]) -> Result<(), PywrError> {
        if values.len() == 1 {
            let variable = self.variable.ok_or(PywrError::ParameterVariableNotActive)?;
            self.offset = variable.apply(values[0]);
            Ok(())
        } else {
            Err(PywrError::ParameterVariableValuesIncorrectLength)
        }
    }

    fn get_variables(&self) -> Vec<f64> {
        vec![self.offset]
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
