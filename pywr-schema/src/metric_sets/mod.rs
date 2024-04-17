#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;

/// Aggregation function to apply over metric values.
#[derive(serde::Deserialize, serde::Serialize, Debug, Copy, Clone)]
#[serde(tag = "type")]
pub enum MetricAggFunc {
    Sum,
    Max,
    Min,
    Mean,
    CountNonZero,
}

#[cfg(feature = "core")]
impl From<MetricAggFunc> for pywr_core::recorders::AggregationFunction {
    fn from(value: MetricAggFunc) -> Self {
        match value {
            MetricAggFunc::Sum => pywr_core::recorders::AggregationFunction::Sum,
            MetricAggFunc::Max => pywr_core::recorders::AggregationFunction::Max,
            MetricAggFunc::Min => pywr_core::recorders::AggregationFunction::Min,
            MetricAggFunc::Mean => pywr_core::recorders::AggregationFunction::Mean,
            MetricAggFunc::CountNonZero => pywr_core::recorders::AggregationFunction::CountNonZero,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Copy, Clone)]
#[serde(tag = "type")]
pub enum MetricAggFrequency {
    Monthly,
    Annual,
    Days { days: NonZeroUsize },
}

#[cfg(feature = "core")]
impl From<MetricAggFrequency> for pywr_core::recorders::AggregationFrequency {
    fn from(value: MetricAggFrequency) -> Self {
        match value {
            MetricAggFrequency::Monthly => pywr_core::recorders::AggregationFrequency::Monthly,
            MetricAggFrequency::Annual => pywr_core::recorders::AggregationFrequency::Annual,
            MetricAggFrequency::Days { days } => pywr_core::recorders::AggregationFrequency::Days(days),
        }
    }
}

/// A set of metrics that can be output from a model run.
///
/// A metric set can optionally have an aggregator, which will apply an aggregation function
/// over the metrics in the set. If an aggregation frequency is provided then the aggregation
/// will be performed over each period implied by that frequency. For example, if the frequency
/// is monthly then the aggregation will be performed over each month in the model run.
///
/// If the metric set has a child aggregator then the aggregation will be performed over the
/// aggregated values of the child aggregator.
#[derive(Deserialize, Serialize, Clone)]
pub struct MetricAggregator {
    /// Optional aggregation frequency.
    pub freq: Option<MetricAggFrequency>,
    /// Aggregation function to apply over metric values.
    pub func: MetricAggFunc,
    /// Optional child aggregator.
    pub child: Option<Box<MetricAggregator>>,
}

#[cfg(feature = "core")]
impl From<MetricAggregator> for pywr_core::recorders::Aggregator {
    fn from(value: MetricAggregator) -> Self {
        pywr_core::recorders::Aggregator::new(
            value.freq.map(|p| p.into()),
            value.func.into(),
            value.child.map(|a| (*a).into()),
        )
    }
}

/// A set of metrics that can be output from a model run.
///
/// A metric set can optionally have an aggregator, which will apply an aggregation function
/// over metrics set. If the aggregator has a defined frequency then the aggregation will result
/// in multiple values (i.e. per each period implied by the frequency).
#[derive(Deserialize, Serialize, Clone)]
pub struct MetricSet {
    pub name: String,
    pub metrics: Vec<Metric>,
    pub aggregator: Option<MetricAggregator>,
}

impl MetricSet {
    #[cfg(feature = "core")]
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network, args: &LoadArgs) -> Result<(), SchemaError> {
        // Convert the schema representation to internal metrics.
        let metrics: Vec<_> = self
            .metrics
            .iter()
            .map(|m| m.load_as_output(network, args))
            .collect::<Result<_, _>>()?;

        let aggregator = self.aggregator.clone().map(|a| a.into());

        let metric_set = pywr_core::recorders::MetricSet::new(&self.name, aggregator, metrics);
        let _ = network.add_metric_set(metric_set)?;

        Ok(())
    }
}
