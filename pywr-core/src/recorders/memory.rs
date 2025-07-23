use crate::models::ModelDomain;
use crate::network::Network;
use crate::recorders::aggregator::{AggregatorValue, Event, PeriodValue};
use crate::recorders::metric_set::MetricSetOutputInfo;
use crate::recorders::{
    AggregationFunction, MetricSetIndex, MetricSetState, Recorder, RecorderAggregationError, RecorderFinaliseError,
    RecorderMeta, RecorderSaveError, RecorderSetupError,
};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use chrono::NaiveDateTime;
use std::any::Any;
use std::collections::HashMap;
use std::ops::Deref;
use thiserror::Error;
use tracing::warn;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum AggregationError {
    #[error("Aggregation function not defined.")]
    AggregationFunctionNotDefined,
    #[error("Aggregation function failed.")]
    AggregationFunctionFailed,
    #[error("Invalid aggregation order: {0}")]
    InvalidOrder(String),
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

    /// Apply the time aggregation function to the provided events.
    fn apply_time_func_events(&self, events: &[Event]) -> Result<f64, AggregationError> {
        let agg_value = if events.len() == 1 {
            if self.time.is_some() {
                warn!("Aggregation function defined for time, but not used.")
            }
            events
                .first()
                .expect("No events found in time series")
                .duration()
                .map(|d| d.fractional_days())
                .ok_or(AggregationError::AggregationFunctionFailed)?
        } else {
            self.time
                .as_ref()
                .ok_or(AggregationError::AggregationFunctionNotDefined)?
                .calc_events(events)
                .ok_or(AggregationError::AggregationFunctionFailed)?
        };

        Ok(agg_value)
    }
}

/// Periodic internal state for the memory recorder.
///
/// This is a 3D array, where the first dimension is the scenario, the second dimension is the time,
/// and the third dimension is the metric. It is used for storing periodic output data which
/// produces a value for every scenario at the same time.
struct PeriodicInternalState {
    data: Vec<Vec<PeriodValue<Vec<f64>>>>,
}

impl PeriodicInternalState {
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

#[derive(Copy, Clone)]
struct MemoryEvent {
    start: NaiveDateTime,
    end: Option<NaiveDateTime>,
    metric_index: usize,
}

impl MemoryEvent {
    fn from_event(event: Event, metric_index: usize) -> MemoryEvent {
        MemoryEvent {
            start: event.start,
            end: event.end,
            metric_index,
        }
    }
}

impl From<MemoryEvent> for Event {
    fn from(me: MemoryEvent) -> Self {
        Event {
            start: me.start,
            end: me.end,
        }
    }
}

/// Event internal state for the memory recorder.
///
/// This is a nested vector of events where the outer vec is the length of the scenarios,
/// and the inner vector are the events for that scenario.
struct EventInternalState {
    events: Vec<Vec<MemoryEvent>>,
}

impl EventInternalState {
    /// Aggregate over the saved data to a single value using the provided aggregation functions.
    ///
    /// This method will first aggregation over time, then over the metrics, and finally over the scenarios.
    fn aggregate_time_metric_scenario(&self, aggregation: &Aggregation) -> Result<f64, AggregationError> {
        let scenario_data: Vec<f64> = self
            .events
            .iter()
            .map(|events| {
                // Accumulate the events for each metric
                let mut events_by_metric: HashMap<usize, Vec<Event>> = HashMap::new();

                for event in events {
                    events_by_metric
                        .entry(event.metric_index)
                        .or_default()
                        .push((*event).into());
                }

                // Aggregate each metric over time first.
                // NB, these are not necessarily in order of the metric index.
                // Some metrics may not have any events.
                let metric_ts: Vec<f64> = events_by_metric
                    .values()
                    .map(|metric_events| aggregation.apply_time_func_events(metric_events))
                    .collect::<Result<_, _>>()?;

                // Now aggregate over the metrics
                aggregation.apply_metric_func_f64(&metric_ts)
            })
            .collect::<Result<_, _>>()?;

        aggregation.apply_scenario_func(&scenario_data)
    }
}

/// Internal state for the memory recorder.
///
/// The variant used depends on the type of data produced by the aggregator.
enum InternalState {
    Periodic(PeriodicInternalState),
    Events(EventInternalState),
}

impl InternalState {
    fn new_periodic(num_scenarios: usize, num_periods: Option<usize>) -> Self {
        let mut data: Vec<Vec<PeriodValue<Vec<f64>>>> = Vec::with_capacity(num_scenarios);

        for _ in 0..num_scenarios {
            data.push(Vec::with_capacity(num_periods.unwrap_or_default()))
        }

        Self::Periodic(PeriodicInternalState { data })
    }

    fn new_event(num_scenarios: usize) -> Self {
        let mut events: Vec<_> = Vec::with_capacity(num_scenarios);

        for _ in 0..num_scenarios {
            events.push(Vec::new());
        }

        Self::Events(EventInternalState { events })
    }

    /// Aggregate over the saved data to a single value using the provided aggregation functions.
    ///
    /// This method will first aggregation over the metrics, then over time, and finally over the scenarios.
    fn aggregate_metric_time_scenario(&self, aggregation: &Aggregation) -> Result<f64, AggregationError> {
        match self {
            Self::Periodic(state) => state.aggregate_metric_time_scenario(aggregation),
            Self::Events(_) => Err(AggregationError::InvalidOrder(
                "Cannot aggregate over events by metric first. Events must be aggregated by time first.".to_string(),
            )),
        }
    }

    /// Aggregate over the saved data to a single value using the provided aggregation functions.
    ///
    /// This method will first aggregation over time, then over the metrics, and finally over the scenarios.
    fn aggregate_time_metric_scenario(&self, aggregation: &Aggregation) -> Result<f64, AggregationError> {
        match self {
            Self::Periodic(state) => state.aggregate_time_metric_scenario(aggregation),
            Self::Events(state) => state.aggregate_time_metric_scenario(aggregation),
        }
    }

    fn append_value(&mut self, scenario_index: &ScenarioIndex, values: &[Option<AggregatorValue>]) {
        match self {
            Self::Periodic(state) => {
                let scenario_data = state
                    .data
                    .get_mut(scenario_index.simulation_id())
                    .expect("No scenario data found");

                // Find the first non-None value and use that as the start time
                let (start, duration) = values
                    .iter()
                    .find_map(|maybe_v| {
                        maybe_v.as_ref().and_then(|v| match v {
                            AggregatorValue::Periodic(p) => Some((p.start, p.duration)),
                            AggregatorValue::Event(_) => None,
                        })
                    })
                    .unwrap_or_else(|| panic!("Could not determine time-step information."));

                let period_values = values
                    .iter()
                    .map(|maybe_v| match maybe_v {
                        Some(v) => match v {
                            AggregatorValue::Periodic(v) => v.value,
                            AggregatorValue::Event(_) => panic!("Cannot append event values to periodic data."),
                        },
                        None => panic!("No value found for metric."),
                    })
                    .collect::<Vec<_>>();

                scenario_data.push(PeriodValue::new(start, duration, period_values));
            }
            Self::Events(state) => {
                let scenario_data = state
                    .events
                    .get_mut(scenario_index.simulation_id())
                    .expect("No scenario data found");

                for (metric_idx, value) in values.iter().enumerate() {
                    match value {
                        Some(AggregatorValue::Event(e)) => scenario_data.push(MemoryEvent::from_event(*e, metric_idx)),
                        Some(AggregatorValue::Periodic(_)) => panic!("Cannot append periodic values to event data."),
                        None => panic!("No value found for metric."),
                    }
                }
            }
        }
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

    fn setup(&self, domain: &ModelDomain, network: &Network) -> Result<Option<Box<(dyn Any)>>, RecorderSetupError> {
        let metric_set =
            network
                .get_metric_set(self.metric_set_idx)
                .ok_or_else(|| RecorderSetupError::MetricSetIndexNotFound {
                    index: self.metric_set_idx,
                })?;

        let state = match metric_set.output_info(domain.time()) {
            MetricSetOutputInfo::Periodic { num_periods } => {
                InternalState::new_periodic(domain.scenarios().len(), Some(num_periods))
            }
            MetricSetOutputInfo::Event => InternalState::new_event(domain.scenarios().len()),
        };

        Ok(Some(Box::new(state)))
    }

    fn save(
        &self,
        _timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
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

        // Iterate through all the scenario's state
        for (scenario_index, ms_scenario_states) in scenario_indices.iter().zip(metric_set_states.iter()) {
            let metric_set_state = ms_scenario_states.get(*self.metric_set_idx.deref()).ok_or_else(|| {
                RecorderSaveError::MetricSetIndexNotFound {
                    index: self.metric_set_idx,
                }
            })?;

            if metric_set_state.has_some_values() {
                internal_state.append_value(scenario_index, metric_set_state.current_values());
            }
        }

        Ok(())
    }

    fn finalise(
        &self,
        _network: &Network,
        scenario_indices: &[ScenarioIndex],
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

        // Iterate through all the scenario's state
        for (scenario_index, ms_scenario_states) in scenario_indices.iter().zip(metric_set_states.iter()) {
            let metric_set_state = ms_scenario_states.get(*self.metric_set_idx.deref()).ok_or_else(|| {
                RecorderFinaliseError::MetricSetIndexNotFound {
                    index: self.metric_set_idx,
                }
            })?;

            if metric_set_state.has_some_values() {
                internal_state.append_value(scenario_index, metric_set_state.current_values());
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
    use crate::recorders::aggregator::{AggregatorValue, Event, PeriodValue};
    use crate::test_utils::{default_timestepper, test_scenario_domain};
    use crate::timestep::TimeDomain;
    use chrono::NaiveDate;
    use float_cmp::assert_approx_eq;
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;
    use rand_distr::Normal;

    #[test]
    fn test_aggregation_orders() {
        let num_scenarios = 2;
        let num_metrics = 3;
        let mut state = InternalState::new_periodic(num_scenarios, None);

        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let dist: Normal<f64> = Normal::new(0.0, 1.0).unwrap();

        let time_domain: TimeDomain = default_timestepper().try_into().unwrap();
        // The expected values from this test
        let mut count_non_zero_max = 0.0;
        let mut count_non_zero_by_metric = vec![0.0; num_metrics];

        time_domain.timesteps().iter().for_each(|timestep| {
            if let InternalState::Periodic(state) = &mut state {
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
            }
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

    #[test]
    fn test_memory_event_aggregation() {
        let num_scenarios = 2;
        let num_metrics = 3;

        let mut state = InternalState::new_event(num_scenarios);

        let scenario_domain = test_scenario_domain(num_scenarios);

        for scenario_index in scenario_domain.indices() {
            for event_index in 0..4 {
                // Create an event with a known start and end time
                let start = NaiveDate::from_ymd_opt(2016, event_index + 1, 8).unwrap();
                let end = start + chrono::Duration::days(event_index as i64 + 1);

                let events: Vec<_> = (0..num_metrics)
                    .map(|_| {
                        let e = Event {
                            start: start.into(),
                            end: Some(end.into()),
                        };
                        Some(AggregatorValue::Event(e))
                    })
                    .collect();

                state.append_value(scenario_index, &events);
            }
        }

        // This should be the total duration of all the events
        let agg = Aggregation::new(
            Some(AggregationFunction::Sum),
            Some(AggregationFunction::Sum),
            Some(AggregationFunction::Sum),
        );

        let expected_total_duration = num_scenarios as f64 * num_metrics as f64 * (1.0 + 2.0 + 3.0 + 4.0);
        let agg_value = state.aggregate_time_metric_scenario(&agg).expect("Aggregation failed");
        assert_approx_eq!(f64, agg_value, expected_total_duration);
    }
}
