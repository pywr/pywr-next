use crate::metric::MetricF64;
use crate::network::Network;
use crate::recorders::aggregator::{Aggregator, AggregatorState, PeriodValue};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::ops::Deref;

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct MetricSetIndex(usize);

impl MetricSetIndex {
    pub fn new(idx: usize) -> Self {
        Self(idx)
    }
}

impl Deref for MetricSetIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for MetricSetIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub struct MetricSetState {
    // Populated with any yielded values from the last processing.
    current_values: Option<Vec<PeriodValue<f64>>>,
    // If the metric set aggregates then this state tracks the aggregation of each metric
    aggregation_states: Option<Vec<AggregatorState>>,
}

impl MetricSetState {
    pub fn current_values(&self) -> Option<&[PeriodValue<f64>]> {
        self.current_values.as_deref()
    }
}

/// A set of metrics with an optional aggregator
#[derive(Clone, Debug)]
pub struct MetricSet {
    name: String,
    aggregator: Option<Aggregator>,
    metrics: Vec<MetricF64>,
}

impl MetricSet {
    pub fn new(name: &str, aggregator: Option<Aggregator>, metrics: Vec<MetricF64>) -> Self {
        Self {
            name: name.to_string(),
            aggregator,
            metrics,
        }
    }

    /// The name of the [`MetricSet`].
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn iter_metrics(&self) -> impl Iterator<Item = &MetricF64> + '_ {
        self.metrics.iter()
    }

    /// Setup a new [`MetricSetState`] for this [`MetricSet`].
    pub fn setup(&self) -> MetricSetState {
        MetricSetState {
            current_values: None,
            aggregation_states: self
                .aggregator
                .as_ref()
                .map(|a| self.metrics.iter().map(|_| a.setup()).collect()),
        }
    }

    pub fn save(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut MetricSetState,
    ) -> Result<(), PywrError> {
        // Combine all the values for metric across all of the scenarios
        let values: Vec<PeriodValue<f64>> = self
            .metrics
            .iter()
            .map(|metric| {
                let value = metric.get_value(model, state)?;
                Ok::<PeriodValue<f64>, PywrError>(PeriodValue::new(timestep.date, timestep.duration, value))
            })
            .collect::<Result<Vec<_>, _>>()?;

        if let Some(aggregator) = &self.aggregator {
            // Apply aggregation if required

            // TODO: make this a `PywrError`.
            let aggregation_states = internal_state
                .aggregation_states
                .as_mut()
                .expect("Aggregation state expected for metric set with aggregator!");

            // Collect any aggregated values. This will remain empty if the aggregator yields
            // no values. However, if there are values we will expect the same number of aggregated
            // values as the input values / metrics.
            let mut agg_values = Vec::with_capacity(values.len());
            // Use a for loop instead of using an iterator because we need to execute the
            // `append_value` method on all aggregators.
            for (value, current_state) in values.iter().zip(aggregation_states.iter_mut()) {
                if let Some(agg_value) = aggregator.append_value(current_state, *value) {
                    agg_values.push(agg_value);
                }
            }

            let agg_values = if agg_values.is_empty() {
                None
            } else if agg_values.len() == values.len() {
                Some(agg_values)
            } else {
                // This should never happen because the aggregator should either yield no values
                // or the same number of values as the input metrics.
                unreachable!("Some values were aggregated and some were not!");
            };

            internal_state.current_values = agg_values;
        } else {
            internal_state.current_values = Some(values);
        }

        Ok(())
    }

    pub fn finalise(&self, internal_state: &mut MetricSetState) {
        if let Some(aggregator) = &self.aggregator {
            let aggregation_states = internal_state
                .aggregation_states
                .as_mut()
                .expect("Aggregation state expected for metric set with aggregator!");

            let final_values = aggregation_states
                .iter_mut()
                .map(|current_state| aggregator.finalise(current_state))
                .collect::<Option<Vec<_>>>();

            internal_state.current_values = final_values;
        } else {
            internal_state.current_values = None;
        }
    }
}
