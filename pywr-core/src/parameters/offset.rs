use crate::metric::Metric;
use crate::network::Network;
use crate::parameters::{
    downcast_internal_state_mut, downcast_internal_state_ref, downcast_variable_config_ref, ActivationFunction,
    Parameter, ParameterMeta, VariableConfig, VariableParameter,
};
use crate::scenario::ScenarioIndex;
use std::any::Any;

use crate::state::{ParameterState, State};
use crate::timestep::Timestep;
use crate::PywrError;

pub struct OffsetParameter {
    meta: ParameterMeta,
    metric: Metric,
    offset: f64,
}

// We store this internal value as an Option<f64> so that it can be updated by the variable API
type InternalValue = Option<f64>;

impl OffsetParameter {
    pub fn new(name: &str, metric: Metric, offset: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            offset,
        }
    }

    /// Return the current value.
    ///
    /// If the internal state is None, the value is returned directly. Otherwise, the internal value must
    /// have come from the variable API and is passed through the activation function.
    fn offset(&self, internal_state: &Option<Box<dyn ParameterState>>) -> f64 {
        match downcast_internal_state_ref::<InternalValue>(internal_state) {
            Some(value) => *value,
            None => self.offset,
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
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, PywrError> {
        let offset = self.offset(internal_state);
        // Current value
        let x = self.metric.get_value(model, state)?;
        Ok(x + offset)
    }
    fn as_f64_variable(&self) -> Option<&dyn VariableParameter<f64>> {
        Some(self)
    }

    fn as_f64_variable_mut(&mut self) -> Option<&mut dyn VariableParameter<f64>> {
        Some(self)
    }
}

impl VariableParameter<f64> for OffsetParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn size(&self, _variable_config: &Box<dyn VariableConfig>) -> usize {
        1
    }

    fn set_variables(
        &self,
        values: &[f64],
        variable_config: &Box<dyn VariableConfig>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), PywrError> {
        let activation_function = downcast_variable_config_ref::<ActivationFunction>(variable_config);

        if values.len() == 1 {
            let value = downcast_internal_state_mut::<InternalValue>(internal_state);
            *value = Some(activation_function.apply(values[0]));
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

    fn get_lower_bounds(&self, variable_config: &Box<dyn VariableConfig>) -> Result<Vec<f64>, PywrError> {
        let activation_function = downcast_variable_config_ref::<ActivationFunction>(variable_config);
        Ok(vec![activation_function.lower_bound()])
    }

    fn get_upper_bounds(&self, variable_config: &Box<dyn VariableConfig>) -> Result<Vec<f64>, PywrError> {
        let activation_function = downcast_variable_config_ref::<ActivationFunction>(variable_config);
        Ok(vec![activation_function.upper_bound()])
    }
}
