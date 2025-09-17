use crate::agg_funcs::{AggFuncF64, AggFuncU64};
use crate::metric::{MetricF64, MetricF64Error, MetricU64, MetricU64Error, SimpleMetricF64, SimpleMetricU64};
use crate::network::Network;
use crate::parameters::errors::{ParameterCalculationError, ParameterSetupError, SimpleCalculationError};
use crate::parameters::{
    GeneralParameter, Parameter, ParameterMeta, ParameterName, ParameterState, SimpleParameter,
    downcast_internal_state_mut, downcast_internal_state_ref,
};
use crate::scenario::ScenarioIndex;
use crate::state::{SimpleParameterValues, State};
use crate::timestep::Timestep;
use std::collections::VecDeque;

/// A rolling parameter that computes an aggregated value over a specified window of metric
/// values.
///
/// This parameter is useful for scenarios where you want to smooth out fluctuations in a metric
/// by averaging over a defined number of previous values. The `window_size` determines how many
/// previous metric values are included in the calculation. If an `initial_value` is provided,
/// it will be used as the return value until `min_values` number of metric values have been
/// processed.
pub struct RollingParameter<M, T, AF> {
    meta: ParameterMeta,
    metric: M,
    window_size: usize,
    initial_value: T,
    min_values: usize,
    agg_func: AF,
}

impl<M, T, AF> RollingParameter<M, T, AF>
where
    M: Send + Sync,
    T: Send + Sync,
    AF: Send + Sync,
{
    /// Creates a new `RollingParameter`.
    ///
    /// # Arguments
    /// * `name` - The name of the parameter.
    /// * `metric` - The metric to aggregate over.
    /// * `window_size` - The size of the rolling window.
    /// * `initial_value` - The initial value to return before enough values are collected.
    /// * `min_values` - The minimum number of values required before aggregation starts.
    /// * `agg_func` - The aggregation function to use (e.g., sum, mean).
    pub fn new(
        name: ParameterName,
        metric: M,
        window_size: usize,
        initial_value: T,
        min_values: usize,
        agg_func: AF,
    ) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            window_size,
            initial_value,
            min_values,
            agg_func,
        }
    }
}

impl TryInto<RollingParameter<SimpleMetricF64, f64, AggFuncF64>> for &RollingParameter<MetricF64, f64, AggFuncF64> {
    type Error = MetricF64Error;

    fn try_into(self) -> Result<RollingParameter<SimpleMetricF64, f64, AggFuncF64>, Self::Error> {
        Ok(RollingParameter {
            meta: self.meta.clone(),
            metric: self.metric.clone().try_into()?,
            window_size: self.window_size,
            initial_value: self.initial_value,
            min_values: self.min_values,
            agg_func: self.agg_func.clone(),
        })
    }
}

impl TryInto<RollingParameter<SimpleMetricU64, u64, AggFuncU64>> for &RollingParameter<MetricU64, u64, AggFuncU64> {
    type Error = MetricU64Error;

    fn try_into(self) -> Result<RollingParameter<SimpleMetricU64, u64, AggFuncU64>, Self::Error> {
        Ok(RollingParameter {
            meta: self.meta.clone(),
            metric: self.metric.clone().try_into()?,
            window_size: self.window_size,
            initial_value: self.initial_value,
            min_values: self.min_values,
            agg_func: self.agg_func.clone(),
        })
    }
}

impl<M, AF> Parameter for RollingParameter<M, f64, AF>
where
    M: Send + Sync,
    AF: Send + Sync,
{
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, ParameterSetupError> {
        // Internal state is the memory
        let memory: VecDeque<f64> = VecDeque::with_capacity(self.window_size);
        Ok(Some(Box::new(memory)))
    }
}

impl<M, AF> Parameter for RollingParameter<M, u64, AF>
where
    M: Send + Sync,
    AF: Send + Sync,
{
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, ParameterSetupError> {
        // Internal state is the memory
        let memory: VecDeque<u64> = VecDeque::with_capacity(self.window_size);
        Ok(Some(Box::new(memory)))
    }
}

impl GeneralParameter<f64> for RollingParameter<MetricF64, f64, AggFuncF64> {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _model: &Network,
        _state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, ParameterCalculationError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_ref::<VecDeque<f64>>(internal_state);

        if memory.len() < self.min_values {
            // Not enough values collected yet, return the initial value
            Ok(self.initial_value)
        } else {
            Ok(self.agg_func.calc_iter_f64(memory)?)
        }
    }

    fn after(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), ParameterCalculationError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_mut::<VecDeque<f64>>(internal_state);

        // Get today's value from the metric
        let value = self.metric.get_value(model, state)?;
        memory.push_back(value);

        // If the memory exceeds the window size, remove the oldest value
        if memory.len() > self.window_size {
            memory.pop_front();
        }

        Ok(())
    }

    fn try_into_simple(&self) -> Option<Box<dyn SimpleParameter<f64>>>
    where
        Self: Sized,
    {
        self.try_into()
            .ok()
            .map(|p: RollingParameter<SimpleMetricF64, f64, AggFuncF64>| Box::new(p) as Box<dyn SimpleParameter<f64>>)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl SimpleParameter<f64> for RollingParameter<SimpleMetricF64, f64, AggFuncF64> {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _values: &SimpleParameterValues,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, SimpleCalculationError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_ref::<VecDeque<f64>>(internal_state);

        if memory.len() < self.min_values {
            // Not enough values collected yet, return the initial value
            Ok(self.initial_value)
        } else {
            Ok(self.agg_func.calc_iter_f64(memory)?)
        }
    }

    fn after(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        values: &SimpleParameterValues,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), SimpleCalculationError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_mut::<VecDeque<f64>>(internal_state);

        // Get today's value from the metric
        let value = self.metric.get_value(values)?;
        memory.push_back(value);

        // If the memory exceeds the window size, remove the oldest value
        if memory.len() > self.window_size {
            memory.pop_front();
        }

        Ok(())
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralParameter<u64> for RollingParameter<MetricU64, u64, AggFuncU64> {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _model: &Network,
        _state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, ParameterCalculationError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_ref::<VecDeque<u64>>(internal_state);

        if memory.len() < self.min_values {
            // Not enough values collected yet, return the initial value
            Ok(self.initial_value)
        } else {
            Ok(self.agg_func.calc_iter_u64(memory.iter())?)
        }
    }

    fn after(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), ParameterCalculationError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_mut::<VecDeque<u64>>(internal_state);

        // Get today's value from the metric
        let value = self.metric.get_value(model, state)?;
        memory.push_back(value);

        // If the memory exceeds the window size, remove the oldest value
        if memory.len() > self.window_size {
            memory.pop_front();
        }

        Ok(())
    }

    fn try_into_simple(&self) -> Option<Box<dyn SimpleParameter<u64>>>
    where
        Self: Sized,
    {
        self.try_into()
            .ok()
            .map(|p: RollingParameter<SimpleMetricU64, u64, AggFuncU64>| Box::new(p) as Box<dyn SimpleParameter<u64>>)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl SimpleParameter<u64> for RollingParameter<SimpleMetricU64, u64, AggFuncU64> {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _values: &SimpleParameterValues,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, SimpleCalculationError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_ref::<VecDeque<u64>>(internal_state);

        if memory.len() < self.min_values {
            // Not enough values collected yet, return the initial value
            Ok(self.initial_value)
        } else {
            Ok(self.agg_func.calc_iter_u64(memory.iter())?)
        }
    }

    fn after(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        values: &SimpleParameterValues,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), SimpleCalculationError> {
        // Downcast the internal state to the correct type
        let memory = downcast_internal_state_mut::<VecDeque<u64>>(internal_state);

        // Get today's value from the metric
        let value = self.metric.get_value(values)?;
        memory.push_back(value);

        // If the memory exceeds the window size, remove the oldest value
        if memory.len() > self.window_size {
            memory.pop_front();
        }

        Ok(())
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
    use super::*;
    use crate::parameters::Array1Parameter;
    use crate::test_utils::{run_and_assert_parameter, run_and_assert_parameter_u64, simple_model};
    use ndarray::{Array1, Array2, Axis};

    #[test]
    /// Test `RollingParameter` returns the correct f64 value.
    fn test_rolling_f64() {
        let mut model = simple_model(1, None);

        let metric = Array1Parameter::new("my-metric".into(), Array1::from(Array1::linspace(1.0, 21.0, 21)), None);
        let metric_idx: MetricF64 = model
            .network_mut()
            .add_simple_parameter(Box::new(metric))
            .unwrap()
            .into();

        let parameter = RollingParameter::new("my-parameter".into(), metric_idx, 3, 0.0, 3, AggFuncF64::Mean);

        // Before the first three values are collected, the parameter should return the initial value.
        let expected_values: Array1<f64> = [
            0.0, 0.0, 0.0, // initial values
            2.0, 3.0, 4.0, // first rolling values
            5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0, 19.0, 20.0, 21.0,
        ]
        .to_vec()
        .into();

        let expected_values: Array2<f64> = expected_values.insert_axis(Axis(1));

        run_and_assert_parameter(&mut model, Box::new(parameter), expected_values, None, Some(1e-12));
    }

    #[test]
    /// Test `RollingParameter` returns the correct u64 value.
    fn test_rolling_u64() {
        let mut model = simple_model(1, None);

        let values: Array1<u64> = Array1::from(Array1::linspace(1.0, 21.0, 21).map(|x| *x as u64));

        let metric = Array1Parameter::new("my-metric".into(), values, None);
        let metric_idx: MetricU64 = model
            .network_mut()
            .add_simple_index_parameter(Box::new(metric))
            .unwrap()
            .into();

        let parameter = RollingParameter::new("my-parameter".into(), metric_idx, 3, 0, 3, AggFuncU64::Max);

        // Before the first three values are collected, the parameter should return the initial value.
        let expected_values: Array1<u64> = [
            0, 0, 0, // initial values
            3, 4, 5, // first rolling values
            6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21,
        ]
        .to_vec()
        .into();

        let expected_values: Array2<u64> = expected_values.insert_axis(Axis(1));

        run_and_assert_parameter_u64(&mut model, Box::new(parameter), expected_values);
    }
}
