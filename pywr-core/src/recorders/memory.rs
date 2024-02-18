use crate::models::ModelDomain;
use crate::network::Network;
use crate::recorders::aggregator::PeriodValue;
use crate::recorders::{AggregationFunction, MetricSetIndex, MetricSetState, Recorder, RecorderMeta};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;
use std::ops::Deref;

pub struct Aggregation {
    time: Option<AggregationFunction>,
    metric: Option<AggregationFunction>,
    scenario: Option<AggregationFunction>,
}

impl Aggregation {
    pub fn new(
        time: Option<AggregationFunction>,
        metric: Option<AggregationFunction>,
        scenario: Option<AggregationFunction>,
    ) -> Self {
        Self { time, metric, scenario }
    }
}

pub struct MemoryRecorder {
    meta: RecorderMeta,
    metric_set_idx: MetricSetIndex,
    aggregation: Aggregation,
}

impl MemoryRecorder {
    pub fn new(name: &str, metric_set_idx: MetricSetIndex, aggregation: Aggregation) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            metric_set_idx,
            aggregation,
        }
    }
}

impl Recorder for MemoryRecorder {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }

    fn setup(&self, domain: &ModelDomain, _network: &Network) -> Result<Option<Box<(dyn Any)>>, PywrError> {
        // This data is organised
        let mut data: Vec<Vec<Vec<PeriodValue>>> = Vec::with_capacity(domain.scenarios().len());

        for _ in 0..domain.scenarios().len() {
            data.push(Vec::new())
        }

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
    ) -> Result<(), PywrError> {
        let data = match internal_state {
            Some(internal) => match internal.downcast_mut::<Vec<Vec<Vec<PeriodValue>>>>() {
                Some(pa) => pa,
                None => panic!("Internal state did not downcast to the correct type! :("),
            },
            None => panic!("No internal state defined when one was expected! :("),
        };

        // Iterate through all of the scenario's state
        for (ms_scenario_states, scenario_data) in metric_set_states.iter().zip(data.iter_mut()) {
            let metric_set_state = ms_scenario_states
                .get(*self.metric_set_idx.deref())
                .ok_or_else(|| PywrError::MetricSetIndexNotFound(self.metric_set_idx))?;

            if let Some(current_values) = metric_set_state.current_values() {
                scenario_data.push(current_values.to_vec());
            }
        }

        Ok(())
    }

    fn finalise(
        &self,
        metric_set_states: &[Vec<MetricSetState>],
        internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<(), PywrError> {
        let data = match internal_state {
            Some(internal) => match internal.downcast_mut::<Vec<Vec<Vec<PeriodValue>>>>() {
                Some(pa) => pa,
                None => panic!("Internal state did not downcast to the correct type! :("),
            },
            None => panic!("No internal state defined when one was expected! :("),
        };

        // Iterate through all of the scenario's state
        for (ms_scenario_states, scenario_data) in metric_set_states.iter().zip(data.iter_mut()) {
            let metric_set_state = ms_scenario_states
                .get(*self.metric_set_idx.deref())
                .ok_or_else(|| PywrError::MetricSetIndexNotFound(self.metric_set_idx))?;

            if let Some(current_values) = metric_set_state.current_values() {
                scenario_data.push(current_values.to_vec());
            }
        }

        Ok(())
    }

    fn aggregated_value(&self, internal_state: &Option<Box<dyn Any>>) -> Result<f64, PywrError> {
        let data = match internal_state {
            Some(internal) => match internal.downcast_ref::<Vec<Vec<Vec<PeriodValue>>>>() {
                Some(pa) => pa,
                None => panic!("Internal state did not downcast to the correct type! :("),
            },
            None => panic!("No internal state defined when one was expected! :("),
        };

        let scenario_data: Vec<f64> = data
            .iter()
            .map(|time_data| {
                // We expect the same number of metrics in all the entries
                let num_metrics = time_data.first().expect("No metrics found in time data").len();

                // Aggregate each metric over time first. This requires transposing the saved data.
                let metric_ts: Vec<f64> = (0..num_metrics)
                    // TODO remove the collect allocation; requires `AggregationFunction.calc` to accept an iterator
                    .map(|metric_idx| time_data.iter().map(|t| t[metric_idx]).collect())
                    .map(|ts: Vec<PeriodValue>| {
                        if ts.len() == 1 {
                            // TODO what if the aggregation function is defined, but not used? Warning?
                            return ts.first().expect("No values found in time series").value;
                        } else {
                            // TODO makes these error types
                            self.aggregation
                                .time
                                .as_ref()
                                .expect("Cannot aggregate over time without a time aggregation function.")
                                .calc_period_values(&ts)
                                .expect("Failed to calculate time aggregation.")
                        }
                    })
                    .collect();

                // Now aggregate over the metrics
                if metric_ts.len() == 1 {
                    // TODO what if the aggregation function is defined, but not used? Warning?
                    *metric_ts.first().expect("No values found in time series")
                } else {
                    self.aggregation
                        .metric
                        .as_ref()
                        .expect("Cannot aggregate over metrics without a metric aggregation function.")
                        .calc_f64(&metric_ts)
                        .expect("Failed to calculate metric aggregation.")
                }
            })
            .collect();

        let agg_value = if scenario_data.len() == 1 {
            // TODO what if the aggregation function is defined, but not used? Warning?
            *scenario_data.first().expect("No values found in time series")
        } else {
            self.aggregation
                .scenario
                .as_ref()
                .expect("Cannot aggregate over scenarios without a scenario aggregation function.")
                .calc_f64(&scenario_data)
                .expect("Failed to calculate scenario aggregation.")
        };

        Ok(agg_value)
    }
}
