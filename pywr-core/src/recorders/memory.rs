use crate::models::ModelDomain;
use crate::network::Network;
use crate::recorders::aggregator::PeriodValue;
use crate::recorders::{
    AggregationFunction, MetricSetIndex, MetricSetState, Recorder, RecorderAggregationError, RecorderFinaliseError,
    RecorderMeta, RecorderSaveError, RecorderSetupError,
};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use std::any::Any;
use std::ops::Deref;
use thiserror::Error;
use tracing::warn;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum AggregationError {
    #[error("Aggregation function not defined.")]
    AggregationFunctionNotDefined,
    #[error("Aggregation function failed.")]
    AggregationFunctionFailed,
}

pub struct Aggregation {
    scenario: Option<AggregationFunction>,
    time: Option<AggregationFunction>,
    metric: Option<AggregationFunction>,
}

impl Aggregation {
    pub fn new(
        scenario: Option<AggregationFunction>,
        time: Option<AggregationFunction>,
        metric: Option<AggregationFunction>,
    ) -> Self {
        Self { scenario, time, metric }
    }

    /// Apply the metric aggregation function to the provided data.
    ///
    /// If there is only one value in the data, the aggregation function is not required. If one
    /// is provided, a warning is logged.
    fn apply_metric_func_period_value(
        &self,
        values: &PeriodValue<Vec<f64>>,
    ) -> Result<PeriodValue<f64>, AggregationError> {
        let agg_value = if values.len() == 1 {
            if self.metric.is_some() {
                warn!("Aggregation function defined for metric, but not used.")
            }
            *values.value.first().expect("No values found in time series")
        } else {
            self.metric
                .as_ref()
                .ok_or(AggregationError::AggregationFunctionNotDefined)?
                .calc_f64(&values.value)
                .ok_or(AggregationError::AggregationFunctionFailed)?
        };

        Ok(PeriodValue::new(values.start, values.duration, agg_value))
    }

    /// Apply the metric aggregation function to the provided data.
    ///
    /// If there is only one value in the data, the aggregation function is not required. If one
    /// is provided, a warning is logged.
    fn apply_metric_func_f64(&self, values: &[f64]) -> Result<f64, AggregationError> {
        let agg_value = if values.len() == 1 {
            if self.metric.is_some() {
                warn!("Aggregation function defined for metric, but not used.")
            }
            *values.first().expect("No values found in time series")
        } else {
            self.metric
                .as_ref()
                .ok_or(AggregationError::AggregationFunctionNotDefined)?
                .calc_f64(values)
                .ok_or(AggregationError::AggregationFunctionFailed)?
        };

        Ok(agg_value)
    }

    /// Apply the scenario aggregation function to the provided data.
    ///
    /// If there is only one value in the data, the aggregation function is not required. If one
    /// is provided, a warning is logged.
    fn apply_scenario_func(&self, values: &[f64]) -> Result<f64, AggregationError> {
        let agg_value = if values.len() == 1 {
            if self.scenario.is_some() {
                warn!("Aggregation function defined for scenario, but not used.")
            }
            *values.first().expect("No values found in time series")
        } else {
            self.scenario
                .as_ref()
                .ok_or(AggregationError::AggregationFunctionNotDefined)?
                .calc_f64(values)
                .ok_or(AggregationError::AggregationFunctionFailed)?
        };

        Ok(agg_value)
    }

    /// Apply the time aggregation function to the provided data.
    ///
    /// If there is only one value in the data, the aggregation function is not required. If one
    /// is provided, a warning is logged.
    fn apply_time_func(&self, values: &[PeriodValue<f64>]) -> Result<f64, AggregationError> {
        let agg_value = if values.len() == 1 {
            if self.time.is_some() {
                warn!("Aggregation function defined for time, but not used.")
            }
            values.first().expect("No values found in time series").value
        } else {
            self.time
                .as_ref()
                .ok_or(AggregationError::AggregationFunctionNotDefined)?
                .calc_period_values(values)
                .ok_or(AggregationError::AggregationFunctionFailed)?
        };

        Ok(agg_value)
    }
}

/// Internal state for the memory recorder.
///
/// This is a 3D array, where the first dimension is the scenario, the second dimension is the time,
/// and the third dimension is the metric.
struct InternalState {
    data: Vec<Vec<PeriodValue<Vec<f64>>>>,
}

impl InternalState {
    fn new(num_scenarios: usize) -> Self {
        let mut data: Vec<Vec<PeriodValue<Vec<f64>>>> = Vec::with_capacity(num_scenarios);

        for _ in 0..num_scenarios {
            // We can't use `Vec::with_capacity` here because we don't know the number of
            // periods that will be recorded.
            data.push(Vec::new())
        }

        Self { data }
    }

    /// Aggregate over the saved data to a single value using the provided aggregation functions.
    ///
    /// This method will first aggregation over the metrics, then over time, and finally over the scenarios.
    fn aggregate_metric_time_scenario(&self, aggregation: &Aggregation) -> Result<f64, AggregationError> {
        let scenario_data: Vec<f64> = self
            .data
            .iter()
            .map(|time_data| {
                // Aggregate each metric at each time step;
                // this results in a time series iterator of aggregated values
                let ts: Vec<PeriodValue<f64>> = time_data
                    .iter()
                    .map(|metric_data| aggregation.apply_metric_func_period_value(metric_data))
                    .collect::<Result<_, _>>()?;

                aggregation.apply_time_func(&ts)
            })
            .collect::<Result<_, _>>()?;

        aggregation.apply_scenario_func(&scenario_data)
    }

    /// Aggregate over the saved data to a single value using the provided aggregation functions.
    ///
    /// This method will first aggregation over time, then over the metrics, and finally over the scenarios.
    fn aggregate_time_metric_scenario(&self, aggregation: &Aggregation) -> Result<f64, AggregationError> {
        let scenario_data: Vec<f64> = self
            .data
            .iter()
            .map(|time_data| {
                // We expect the same number of metrics in all the entries
                let num_metrics = time_data.first().expect("No metrics found in time data").len();

                // Aggregate each metric over time first. This requires transposing the saved data.
                let metric_ts: Vec<f64> = (0..num_metrics)
                    // TODO remove the collect allocation; requires `AggregationFunction.calc` to accept an iterator
                    .map(|metric_idx| time_data.iter().map(|t| t.index(metric_idx)).collect())
                    .map(|ts: Vec<PeriodValue<f64>>| aggregation.apply_time_func(&ts))
                    .collect::<Result<_, _>>()?;

                // Now aggregate over the metrics
                aggregation.apply_metric_func_f64(&metric_ts)
            })
            .collect::<Result<_, _>>()?;

        aggregation.apply_scenario_func(&scenario_data)
    }
}

#[derive(Default, Copy, Clone)]
pub enum AggregationOrder {
    #[default]
    MetricTimeScenario,
    TimeMetricScenario,
}

/// A recorder that saves the metric values to memory.
///
/// This recorder saves data into memory and can be used to provide aggregated data for external
/// analysis. The data is saved in a 3D array, where the first dimension is the scenario, the second
/// dimension is the time, and the third dimension is the metric.
///
/// Users should be aware that this recorder can consume a large amount of memory if the number of
/// scenarios, time steps, and metrics is large.
pub struct MemoryRecorder {
    meta: RecorderMeta,
    metric_set_idx: MetricSetIndex,
    aggregation: Aggregation,
    order: AggregationOrder,
}

impl MemoryRecorder {
    pub fn new(name: &str, metric_set_idx: MetricSetIndex, aggregation: Aggregation, order: AggregationOrder) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            metric_set_idx,
            aggregation,
            order,
        }
    }
}

impl Recorder for MemoryRecorder {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }

    fn setup(&self, domain: &ModelDomain, _network: &Network) -> Result<Option<Box<(dyn Any)>>, RecorderSetupError> {
        let data = InternalState::new(domain.scenarios().len());

        Ok(Some(Box::new(data)))
    }

    fn save(
        &self,
        _timestep: &Timestep,
        _scenario_indices: &[ScenarioIndex],
        _model: &Network,
        _state: &[State],
        metric_set_states: &[Vec<MetricSetState>],
        internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<(), RecorderSaveError> {
        let internal_state = match internal_state {
            Some(internal) => match internal.downcast_mut::<InternalState>() {
                Some(pa) => pa,
                None => panic!("Internal state did not downcast to the correct type! :("),
            },
            None => panic!("No internal state defined when one was expected! :("),
        };

        // Iterate through all of the scenario's state
        for (ms_scenario_states, scenario_data) in metric_set_states.iter().zip(internal_state.data.iter_mut()) {
            let metric_set_state = ms_scenario_states.get(*self.metric_set_idx.deref()).ok_or_else(|| {
                RecorderSaveError::MetricSetIndexNotFound {
                    index: self.metric_set_idx,
                }
            })?;

            if let Some(current_values) = metric_set_state.current_values() {
                scenario_data.push(current_values.into());
            }
        }

        Ok(())
    }

    fn finalise(
        &self,
        _network: &Network,
        _scenario_indices: &[ScenarioIndex],
        metric_set_states: &[Vec<MetricSetState>],
        internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<(), RecorderFinaliseError> {
        let internal_state = match internal_state {
            Some(internal) => match internal.downcast_mut::<InternalState>() {
                Some(pa) => pa,
                None => panic!("Internal state did not downcast to the correct type! :("),
            },
            None => panic!("No internal state defined when one was expected! :("),
        };

        // Iterate through all of the scenario's state
        for (ms_scenario_states, scenario_data) in metric_set_states.iter().zip(internal_state.data.iter_mut()) {
            let metric_set_state = ms_scenario_states.get(*self.metric_set_idx.deref()).ok_or_else(|| {
                RecorderFinaliseError::MetricSetIndexNotFound {
                    index: self.metric_set_idx,
                }
            })?;

            if let Some(current_values) = metric_set_state.current_values() {
                scenario_data.push(current_values.into());
            }
        }

        Ok(())
    }

    /// Aggregate the saved data to a single value using the provided aggregation functions.
    ///
    /// This method will first aggregation over the metrics, then over time, and finally over the scenarios.
    fn aggregated_value(&self, internal_state: &Option<Box<dyn Any>>) -> Result<f64, RecorderAggregationError> {
        let internal_state = match internal_state {
            Some(internal) => match internal.downcast_ref::<InternalState>() {
                Some(pa) => pa,
                None => panic!("Internal state did not downcast to the correct type! :("),
            },
            None => panic!("No internal state defined when one was expected! :("),
        };

        let agg_value = match self.order {
            AggregationOrder::MetricTimeScenario => internal_state.aggregate_metric_time_scenario(&self.aggregation),
            AggregationOrder::TimeMetricScenario => internal_state.aggregate_time_metric_scenario(&self.aggregation),
        };

        agg_value.map_err(|source| RecorderAggregationError::AggregationError {
            name: self.meta.name.clone(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Aggregation, InternalState};
    use crate::recorders::AggregationFunction;
    use crate::recorders::aggregator::PeriodValue;
    use crate::test_utils::default_timestepper;
    use crate::timestep::TimeDomain;
    use float_cmp::assert_approx_eq;
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;
    use rand_distr::Normal;

    #[test]
    fn test_aggregation_orders() {
        let num_scenarios = 2;
        let num_metrics = 3;
        let mut state = InternalState::new(num_scenarios);

        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let dist: Normal<f64> = Normal::new(0.0, 1.0).unwrap();

        let time_domain: TimeDomain = default_timestepper().try_into().unwrap();
        // The expected values from this test
        let mut count_non_zero_max = 0.0;
        let mut count_non_zero_by_metric = vec![0.0; num_metrics];

        time_domain.timesteps().iter().for_each(|timestep| {
            state.data.iter_mut().for_each(|scenario_data| {
                let metric_data = (&mut rng).sample_iter(&dist).take(num_metrics).collect::<Vec<f64>>();

                // Compute the expected values
                if metric_data.iter().sum::<f64>() > 0.0 {
                    count_non_zero_max += 1.0;
                }
                // ... and by metric
                metric_data.iter().enumerate().for_each(|(i, v)| {
                    if *v > 0.0 {
                        count_non_zero_by_metric[i] += 1.0;
                    }
                });

                let metric_data = PeriodValue::new(timestep.date, timestep.duration, metric_data);

                scenario_data.push(metric_data);
            });
        });

        let agg = Aggregation::new(
            Some(AggregationFunction::Sum),
            Some(AggregationFunction::CountFunc { func: |v: f64| v > 0.0 }),
            Some(AggregationFunction::Sum),
        );
        let agg_value = state.aggregate_metric_time_scenario(&agg).expect("Aggregation failed");
        assert_approx_eq!(f64, agg_value, count_non_zero_max);

        let agg_value = state.aggregate_time_metric_scenario(&agg).expect("Aggregation failed");
        assert_approx_eq!(f64, agg_value, count_non_zero_by_metric.iter().sum());
    }
}
