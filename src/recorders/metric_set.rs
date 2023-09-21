use crate::metric::Metric;
use crate::model::Model;
use crate::recorders::aggregator::{PeriodValue, PeriodicAggregator, PeriodicAggregatorState};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::slice::Iter;

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

pub struct MetricSetState {
    // Populated with any yielded values from the last processing.
    current_values: Option<Vec<PeriodValue>>,
    // If the metric set aggregates then this state tracks the aggregation of each metric
    aggregation_states: Option<Vec<PeriodicAggregatorState>>,
}

/// A set of metrics with an optional aggregator
#[derive(Clone, Debug)]
pub struct MetricSet {
    name: String,
    aggregator: Option<PeriodicAggregator>,
    metrics: Vec<Metric>,
}

impl MetricSet {
    pub fn new(name: &str, aggregator: Option<PeriodicAggregator>, metrics: Vec<Metric>) -> Self {
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
    pub fn iter_metrics(&self) -> Iter<'_, Metric> {
        self.metrics.iter()
    }

    fn save(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        model: &Model,
        state: &[State],
        internal_state: &mut MetricSetState,
    ) -> Result<(), PywrError> {
        // Combine all the values for metric across all of the scenarios
        let values: Vec<PeriodValue> = self
            .metrics
            .iter()
            .flat_map(|metric| {
                scenario_indices.iter().zip(state).map(|(_, s)| {
                    let value = metric.get_value(model, s)?;
                    Ok::<PeriodValue, PywrError>(PeriodValue::new(timestep.date, timestep.duration, value))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        if let Some(aggregator) = &self.aggregator {
            // Apply aggregation if required

            // TODO: make this a `PywrError`.
            let aggregation_states = internal_state
                .aggregation_states
                .as_mut()
                .expect("Aggregation state expected for metric set with aggregator!");

            let agg_values = values
                .into_iter()
                .zip(aggregation_states.iter_mut())
                .map(|(value, current_state)| aggregator.process_value(current_state, value))
                .collect::<Option<Vec<_>>>();

            internal_state.current_values = agg_values;
        } else {
            internal_state.current_values = Some(values);
        }

        Ok(())
    }
}
