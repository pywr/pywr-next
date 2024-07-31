#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use pywr_schema_macros::PywrVisitPaths;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;

/// Aggregation function to apply over metric values.
#[derive(serde::Deserialize, serde::Serialize, Debug, Copy, Clone, JsonSchema, PywrVisitPaths)]
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

#[derive(serde::Deserialize, serde::Serialize, Debug, Copy, Clone, JsonSchema)]
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
#[derive(Deserialize, Serialize, Clone, JsonSchema)]
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

#[derive(Deserialize, Serialize, Clone, JsonSchema, Default)]
struct MetricSetFilters {
    #[serde(default)]
    all_nodes: bool,
    #[serde(default)]
    all_parameters: bool,
}

#[cfg(feature = "core")]
impl MetricSetFilters {
    fn create_metrics(&self, args: &LoadArgs) -> Option<Vec<Metric>> {
        use crate::metric::{NodeReference, ParameterReference};

        if !self.all_nodes && !self.all_parameters {
            return None;
        }

        let mut metrics = vec![];

        if self.all_nodes {
            for node in args.schema.nodes.iter() {
                metrics.push(Metric::Node(NodeReference::new(node.name().to_string(), None)));
            }
        }

        if self.all_parameters {
            if let Some(parameters) = args.schema.parameters.as_ref() {
                for parameter in parameters.iter() {
                    metrics.push(Metric::Parameter(ParameterReference::new(
                        parameter.name().to_string(),
                        None,
                    )));
                }
            }
        }

        Some(metrics)
    }
}

/// A set of metrics that can be output from a model run.
///
/// A metric set can optionally have an aggregator, which will apply an aggregation function
/// over metrics set. If the aggregator has a defined frequency then the aggregation will result
/// in multiple values (i.e. per each period implied by the frequency).
#[derive(Deserialize, Serialize, Clone, JsonSchema)]
pub struct MetricSet {
    pub name: String,
    pub metrics: Vec<Metric>,
    pub aggregator: Option<MetricAggregator>,
    #[serde(default)]
    filters: MetricSetFilters,
}

impl MetricSet {
    #[cfg(feature = "core")]
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network, args: &LoadArgs) -> Result<(), SchemaError> {
        use pywr_core::recorders::OutputMetric;

        let mut metrics: Vec<OutputMetric> = self
            .metrics
            .iter()
            //.chain(additional_metric.iter())
            .map(|m| m.load_as_output(network, args))
            .collect::<Result<_, _>>()?;

        if let Some(additional_metrics) = self.filters.create_metrics(args) {
            for m in additional_metrics.iter() {
                match m {
                    Metric::Node(n) => {
                        if !self.metrics.iter().any(|m| match m {
                            Metric::Node(n2) => n2.name == n.name,
                            _ => false,
                        }) {
                            metrics.push(m.load_as_output(network, args)?);
                        }
                    }
                    Metric::Parameter(p) => {
                        if !self.metrics.iter().any(|m| match m {
                            Metric::Parameter(p2) => p2.name == p.name,
                            _ => false,
                        }) {
                            metrics.push(m.load_as_output(network, args)?);
                        }
                    }
                    _ => {}
                }
            }
        }

        let aggregator = self.aggregator.clone().map(|a| a.into());

        let metric_set = pywr_core::recorders::MetricSet::new(&self.name, aggregator, metrics);
        let _ = network.add_metric_set(metric_set)?;

        Ok(())
    }
}
