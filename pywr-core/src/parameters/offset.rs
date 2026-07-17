use crate::metric::{MetricF64, UnresolvedMetricF64};
use crate::network::{Network, ResolutionMaps};
use crate::parameters::errors::GeneralCalculationError;
use crate::parameters::{
    ActivationFunction, BuiltParameter, GeneralParameter, MaybeBuiltParameter, Parameter, ParameterBuildError,
    ParameterBuilder, ParameterMeta, ParameterName, ParameterState, VariableConfig, VariableParameter,
    VariableParameterError, downcast_internal_state_mut, downcast_internal_state_ref, downcast_variable_config_ref,
};
use crate::resolve_metric_f64;
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;

#[derive(Debug)]
pub struct OffsetParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    offset: f64,
}

// We store this internal value as an Option<f64> so that it can be updated by the variable API
type InternalValue = Option<f64>;

impl OffsetParameter {
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
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn as_f64_variable(&self) -> Option<&dyn VariableParameter<f64>> {
        Some(self)
    }

    fn as_f64_variable_mut(&mut self) -> Option<&mut dyn VariableParameter<f64>> {
        Some(self)
    }
}
impl GeneralParameter<f64> for OffsetParameter {
    fn before(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<f64>, GeneralCalculationError> {
        let offset = self.offset(internal_state);
        // Current value
        let x = self.metric.get_value(model, state)?;
        Ok(Some(x + offset))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl VariableParameter<f64> for OffsetParameter {
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

#[derive(Debug)]
pub struct OffsetParameterBuilder {
    meta: ParameterMeta,
    metric: UnresolvedMetricF64,
    offset: f64,
}

impl OffsetParameterBuilder {
    pub fn new(name: ParameterName, metric: UnresolvedMetricF64, offset: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            offset,
        }
    }
}

impl ParameterBuilder<f64> for OffsetParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        let metric = resolve_metric_f64!(self, self.metric, resolution_maps, "metric");

        let p = OffsetParameter {
            meta: self.meta,
            metric,
            offset: self.offset,
        };

        Ok(MaybeBuiltParameter::Built(BuiltParameter::General(Box::new(p))))
    }
}
