use crate::agg_funcs::{AggFuncError, AggFuncF64};
use crate::models::ModelDomain;
use crate::network::Network;
use crate::recorders::aggregator::{AggregatorValue, Event, PeriodValue};
use crate::recorders::metric_set::MetricSetOutputInfo;
use crate::recorders::{
    MetricSetIndex, MetricSetState, Recorder, RecorderAggregationError, RecorderDataFrameError, RecorderFinalResult,
    RecorderFinaliseError, RecorderInternalState, RecorderMeta, RecorderSaveError, RecorderSetupError,
    downcast_internal_state, downcast_internal_state_mut,
};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use chrono::NaiveDateTime;
use polars::df;
use polars::frame::DataFrame;
use std::collections::HashMap;
use std::ops::Deref;
use thiserror::Error;
use tracing::warn;

#[derive(Error, Debug)]
pub enum AggregationError {
    #[error("Aggregation function not defined.")]
    AggregationFunctionNotDefined,
    #[error("Aggregation function failed.")]
    AggregationFunctionFailed,
    #[error("Aggregation function error: {0}")]
    AggFuncError(#[from] AggFuncError),
    #[error("Invalid aggregation order: {0}")]
    InvalidOrder(String),
}

#[derive(Clone)]
pub struct Aggregation {
    scenario: Option<AggFuncF64>,
    time: Option<AggFuncF64>,
    metric: Option<AggFuncF64>,
}

impl Aggregation {
    pub fn new(scenario: Option<AggFuncF64>, time: Option<AggFuncF64>, metric: Option<AggFuncF64>) -> Self {
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
                .calc_iter_f64(&values.value)?
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
                .calc_iter_f64(values)?
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
                .calc_iter_f64(values)?
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
#[derive(Clone)]
struct PeriodicInternalState {
    data: Vec<Vec<PeriodValue<Vec<f64>>>>,
}

/// Final results for the memory recorder.
///
/// This is a 3D array, where the first dimension is the scenario, the second dimension is the time,
/// and the third dimension is the metric.
pub struct MemoryRecorderResult {
    meta: RecorderMeta,
    scenario_indices: Vec<ScenarioIndex>,
    metric_names: Vec<String>,
    metric_attrs: Vec<String>,
    data: InternalState,
    aggregation: Aggregation,
    order: AggregationOrder,
}

impl MemoryRecorderResult {
    /// Aggregate over the saved data to a single value using the provided aggregation functions.
    ///
    /// This method will first aggregation over the metrics, then over time, and finally over the scenarios.
    fn aggregate_metric_time_scenario(&self) -> Result<f64, AggregationError> {
        match &self.data {
            InternalState::Events(_) => Err(AggregationError::InvalidOrder(
                "Cannot aggregate over events by metric first. Events must be aggregated by time first.".to_string(),
            )),
            InternalState::Periodic(state) => {
                let scenario_data: Vec<f64> = state
                    .data
                    .iter()
                    .map(|time_data| {
                        // Aggregate each metric at each time step;
                        // this results in a time series iterator of aggregated values
                        let ts: Vec<PeriodValue<f64>> = time_data
                            .iter()
                            .map(|metric_data| self.aggregation.apply_metric_func_period_value(metric_data))
                            .collect::<Result<_, _>>()?;

                        self.aggregation.apply_time_func(&ts)
                    })
                    .collect::<Result<_, _>>()?;

                self.aggregation.apply_scenario_func(&scenario_data)
            }
        }
    }

    /// Aggregate over the saved data to a single value using the provided aggregation functions.
    ///
    /// This method will first aggregation over time, then over the metrics, and finally over the scenarios.
    fn aggregate_time_metric_scenario(&self) -> Result<f64, AggregationError> {
        match &self.data {
            InternalState::Events(state) => state.aggregate_time_metric_scenario(&self.aggregation),
            InternalState::Periodic(state) => {
                let scenario_data: Vec<f64> = state
                    .data
                    .iter()
                    .map(|time_data| {
                        // We expect the same number of metrics in all the entries
                        let num_metrics = time_data.first().expect("No metrics found in time data").len();

                        // Aggregate each metric over time first. This requires transposing the saved data.
                        let metric_ts: Vec<f64> = (0..num_metrics)
                            // TODO remove the collect allocation; requires `AggregationFunction.calc` to accept an iterator
                            .map(|metric_idx| time_data.iter().map(|t| t.index(metric_idx)).collect())
                            .map(|ts: Vec<PeriodValue<f64>>| self.aggregation.apply_time_func(&ts))
                            .collect::<Result<_, _>>()?;

                        // Now aggregate over the metrics
                        self.aggregation.apply_metric_func_f64(&metric_ts)
                    })
                    .collect::<Result<_, _>>()?;

                self.aggregation.apply_scenario_func(&scenario_data)
            }
        }
    }
}

impl RecorderFinalResult for MemoryRecorderResult {
    /// Aggregate the saved data to a single value using the provided aggregation functions.
    ///
    /// This method will first aggregation over the metrics, then over time, and finally over the scenarios.
    fn aggregated_value(&self) -> Result<f64, RecorderAggregationError> {
        let agg_value = match self.order {
            AggregationOrder::MetricTimeScenario => self.aggregate_metric_time_scenario(),
            AggregationOrder::TimeMetricScenario => self.aggregate_time_metric_scenario(),
        };

        agg_value.map_err(|source| RecorderAggregationError::AggregationError {
            name: self.meta.name.clone(),
            source,
        })
    }

    fn to_dataframe(&self) -> Result<DataFrame, RecorderDataFrameError> {
        match &self.data {
            InternalState::Events(state) => {
                let mut time_start = Vec::new();
                let mut time_end = Vec::new();
                let mut simulation_id = Vec::new();
                let mut label = Vec::new();
                let mut metric_set = Vec::new();
                let mut names = Vec::new();
                let mut attribute = Vec::new();

                self.scenario_indices
                    .iter()
                    .zip(state.events.iter())
                    .for_each(|(scenario_index, scenario_events)| {
                        scenario_events.iter().for_each(|ev| {
                            let name = &self.metric_names[ev.metric_index];
                            let attr = &self.metric_attrs[ev.metric_index];

                            time_start.push(ev.start);
                            time_end.push(ev.end);
                            simulation_id.push(scenario_index.simulation_id() as u32);
                            label.push(scenario_index.label());
                            metric_set.push(self.meta.name.clone());
                            names.push(name.clone());
                            attribute.push(attr.clone());
                        })
                    });

                df!(
                    "time_start" => time_start,
                    "time_end" => time_end,
                    "simulation_id" => simulation_id,
                    "label" => label,
                    "metric_set" => metric_set,
                    "name" => names,
                    "attribute" => attribute,
                )
                .map_err(|source| RecorderDataFrameError::PolarsError {
                    name: self.meta.name.clone(),
                    source,
                })
            }
            InternalState::Periodic(state) => {
                let mut time_start = Vec::new();
                let mut time_end = Vec::new();
                let mut simulation_id = Vec::new();
                let mut label = Vec::new();
                let mut metric_set = Vec::new();
                let mut names = Vec::new();
                let mut attribute = Vec::new();
                let mut value = Vec::new();

                self.scenario_indices
                    .iter()
                    .zip(state.data.iter())
                    .for_each(|(scenario_index, scenario_data)| {
                        scenario_data.iter().for_each(|pv| {
                            pv.value
                                .iter()
                                .zip(self.metric_names.iter())
                                .zip(self.metric_attrs.iter())
                                .for_each(|((v, name), attr)| {
                                    time_start.push(pv.start);
                                    time_end.push(pv.end());
                                    simulation_id.push(scenario_index.simulation_id() as u32);
                                    label.push(scenario_index.label());
                                    metric_set.push(self.meta.name.clone());
                                    names.push(name.clone());
                                    attribute.push(attr.clone());
                                    value.push(*v);
                                })
                        })
                    });

                df!(
                    "time_start" => time_start,
                    "time_end" => time_end,
                    "simulation_id" => simulation_id,
                    "label" => label,
                    "metric_set" => metric_set,
                    "name" => names,
                    "attribute" => attribute,
                    "value" => value,
                )
                .map_err(|source| RecorderDataFrameError::PolarsError {
                    name: self.meta.name.clone(),
                    source,
                })
            }
        }
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
#[derive(Clone)]
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
#[derive(Clone)]
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

    fn setup(
        &self,
        domain: &ModelDomain,
        network: &Network,
    ) -> Result<Option<Box<dyn RecorderInternalState>>, RecorderSetupError> {
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
        internal_state: &mut Option<Box<dyn RecorderInternalState>>,
    ) -> Result<(), RecorderSaveError> {
        let internal_state = downcast_internal_state_mut::<InternalState>(internal_state);

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
        network: &Network,
        scenario_indices: &[ScenarioIndex],
        metric_set_states: &[Vec<MetricSetState>],
        internal_state: Option<Box<dyn RecorderInternalState>>,
    ) -> Result<Option<Box<dyn RecorderFinalResult>>, RecorderFinaliseError> {
        let mut internal_state = downcast_internal_state::<InternalState>(internal_state);

        let metric_set =
            network
                .get_metric_set(self.metric_set_idx)
                .ok_or(RecorderFinaliseError::MetricSetIndexNotFound {
                    index: self.metric_set_idx,
                })?;

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

        let result = MemoryRecorderResult {
            meta: self.meta.clone(),
            scenario_indices: scenario_indices.to_vec(),
            metric_names: metric_set.iter_metrics().map(|m| m.name().to_string()).collect(),
            metric_attrs: metric_set.iter_metrics().map(|m| m.attribute().to_string()).collect(),
            data: internal_state.deref().clone(),
            aggregation: self.aggregation.clone(),
            order: self.order,
        };

        Ok(Some(Box::new(result)))
    }
}

#[cfg(test)]
mod tests {
    use super::{Aggregation, InternalState, MemoryRecorderResult};
    use crate::agg_funcs::AggFuncF64;
    use crate::models::ModelDomain;
    use crate::recorders::RecorderMeta;
    use crate::recorders::aggregator::{AggregatorValue, Event, PeriodValue};
    use crate::scenario::{ScenarioDomainBuilder, ScenarioGroupBuilder};
    use crate::test_utils::default_timestepper;
    use chrono::NaiveDate;
    use float_cmp::assert_approx_eq;
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;
    use rand_distr::Normal;

    #[test]
    fn test_aggregation_orders() {
        let mut scenario_builder = ScenarioDomainBuilder::default();
        let scenario_group = ScenarioGroupBuilder::new("test-scenario", 2).build().unwrap();
        scenario_builder = scenario_builder.with_group(scenario_group).unwrap();

        let domain = ModelDomain::try_from(default_timestepper(), scenario_builder).unwrap();

        let num_metrics = 3;
        let mut state = InternalState::new_periodic(domain.scenarios().len(), None);

        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let dist: Normal<f64> = Normal::new(0.0, 1.0).unwrap();

        // The expected values from this test
        let mut count_non_zero_max = 0.0;
        let mut count_non_zero_by_metric = vec![0.0; num_metrics];

        domain.time().timesteps().iter().for_each(|timestep| {
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
            Some(AggFuncF64::Sum),
            Some(AggFuncF64::CountFunc { func: |v: f64| v > 0.0 }),
            Some(AggFuncF64::Sum),
        );

        let result = MemoryRecorderResult {
            meta: RecorderMeta::new("test"),
            scenario_indices: domain.scenarios().indices().to_vec(),
            metric_names: vec!["m1".to_string(), "m2".to_string(), "m3".to_string()],
            metric_attrs: vec!["a1".to_string(), "a2".to_string(), "a3".to_string()],
            data: state.clone(),
            aggregation: agg,
            order: super::AggregationOrder::MetricTimeScenario,
        };

        let agg_value = result.aggregate_metric_time_scenario().expect("Aggregation failed");
        assert_approx_eq!(f64, agg_value, count_non_zero_max);

        let agg_value = result.aggregate_time_metric_scenario().expect("Aggregation failed");
        assert_approx_eq!(f64, agg_value, count_non_zero_by_metric.iter().sum());
    }

    #[test]
    fn test_memory_event_aggregation() {
        let mut scenario_builder = ScenarioDomainBuilder::default();
        let scenario_group = ScenarioGroupBuilder::new("test-scenario", 2).build().unwrap();
        scenario_builder = scenario_builder.with_group(scenario_group).unwrap();

        let domain = ModelDomain::try_from(default_timestepper(), scenario_builder).unwrap();

        let num_metrics = 3;
        let mut state = InternalState::new_event(domain.scenarios().len());

        for scenario_index in domain.scenarios().indices() {
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
        let agg = Aggregation::new(Some(AggFuncF64::Sum), Some(AggFuncF64::Sum), Some(AggFuncF64::Sum));

        let result = MemoryRecorderResult {
            meta: RecorderMeta::new("test"),
            scenario_indices: domain.scenarios().indices().to_vec(),
            metric_names: vec!["m1".to_string(), "m2".to_string(), "m3".to_string()],
            metric_attrs: vec!["a1".to_string(), "a2".to_string(), "a3".to_string()],
            data: state.clone(),
            aggregation: agg,
            order: super::AggregationOrder::MetricTimeScenario,
        };

        let expected_total_duration = domain.scenarios().len() as f64 * num_metrics as f64 * (1.0 + 2.0 + 3.0 + 4.0);
        let agg_value = result.aggregate_time_metric_scenario().expect("Aggregation failed");
        assert_approx_eq!(f64, agg_value, expected_total_duration);
    }
}
