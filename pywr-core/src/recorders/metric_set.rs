use crate::metric::{MetricF64, MetricF64Error};
use crate::network::Network;
use crate::recorders::aggregator::{Aggregator, AggregatorState, PeriodValue};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use thiserror::Error;

/// A container for a [`MetricF64`] that retains additional information from the schema.
///
/// This is used to store the name and attribute of the metric so that it can be output in
/// a context that is relevant to the originating schema, and therefore more meaningful to the user.
#[derive(Clone, Debug, PartialEq)]
pub struct OutputMetric {
    name: String,
    attribute: String,
    // The originating type of the metric (e.g. node, parameter, etc.)
    ty: String,
    // The originating subtype of the metric (e.g. node type, parameter type, etc.)
    sub_type: Option<String>,
    metric: MetricF64,
}

impl OutputMetric {
    pub fn new(name: &str, attribute: &str, ty: &str, sub_type: Option<&str>, metric: MetricF64) -> Self {
        Self {
            name: name.to_string(),
            attribute: attribute.to_string(),
            ty: ty.to_string(),
            sub_type: sub_type.map(|s| s.to_string()),
            metric,
        }
    }

    pub fn get_value(&self, model: &Network, state: &State) -> Result<f64, MetricF64Error> {
        self.metric.get_value(model, state)
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn attribute(&self) -> &str {
        &self.attribute
    }

    pub fn ty(&self) -> &str {
        &self.ty
    }

    pub fn sub_type(&self) -> Option<&str> {
        self.sub_type.as_deref()
    }
}

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

#[derive(Debug, Error)]
pub enum MetricSetSaveError {
    #[error("Metric error: {0}")]
    MetricF64Error(#[from] MetricF64Error),
}

/// A set of metrics with an optional aggregator
#[derive(Clone, Debug)]
pub struct MetricSet {
    name: String,
    aggregator: Option<Aggregator>,
    metrics: Vec<OutputMetric>,
}

impl MetricSet {
    pub fn new(name: &str, aggregator: Option<Aggregator>, metrics: Vec<OutputMetric>) -> Self {
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
    pub fn iter_metrics(&self) -> impl Iterator<Item = &OutputMetric> + '_ {
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
    ) -> Result<(), MetricSetSaveError> {
        // Combine all the values for metric across all of the scenarios
        let values: Vec<PeriodValue<f64>> = self
            .metrics
            .iter()
            .map(|metric| {
                let value = metric.get_value(model, state)?;
                Ok::<PeriodValue<f64>, MetricF64Error>(PeriodValue::new(timestep.date, timestep.duration, value))
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
