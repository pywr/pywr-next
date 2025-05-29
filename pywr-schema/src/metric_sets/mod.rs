#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
#[cfg(feature = "core")]
use crate::parameters::{Parameter, PythonReturnType};
use crate::predicate::Predicate;
use pywr_schema_macros::PywrVisitPaths;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;

/// Aggregation function to apply over metric values.
#[derive(
    serde::Deserialize, serde::Serialize, Debug, Copy, Clone, JsonSchema, PywrVisitPaths, strum_macros::Display,
)]
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

#[derive(serde::Deserialize, serde::Serialize, Debug, Copy, Clone, JsonSchema, strum_macros::Display)]
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
#[serde(deny_unknown_fields)]
pub struct PeriodicMetricAggregator {
    /// Optional aggregation frequency.
    pub freq: Option<MetricAggFrequency>,
    /// Aggregation function to apply over metric values.
    pub func: MetricAggFunc,
}

#[cfg(feature = "core")]
impl From<PeriodicMetricAggregator> for pywr_core::recorders::PeriodicAggregator {
    fn from(value: PeriodicMetricAggregator) -> Self {
        pywr_core::recorders::PeriodicAggregator::new(value.freq.map(|p| p.into()), value.func.into())
    }
}

#[derive(Deserialize, Serialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EventMetricAggregator {
    pub predicate: Predicate,
    pub threshold: f64,
}

#[cfg(feature = "core")]
impl From<EventMetricAggregator> for pywr_core::recorders::EventAggregator {
    fn from(value: EventMetricAggregator) -> Self {
        pywr_core::recorders::EventAggregator::new(value.predicate.into(), value.threshold)
    }
}

#[derive(Deserialize, Serialize, Clone, JsonSchema)]
#[serde(tag = "type")]
pub enum MetricAggregator {
    Periodic(PeriodicMetricAggregator),
    Event(EventMetricAggregator),
}

#[cfg(feature = "core")]
impl From<MetricAggregator> for pywr_core::recorders::Aggregator {
    fn from(value: MetricAggregator) -> Self {
        match value {
            MetricAggregator::Periodic(p) => pywr_core::recorders::Aggregator::Periodic(p.into()),
            MetricAggregator::Event(e) => pywr_core::recorders::Aggregator::Event(e.into()),
        }
    }
}

#[derive(Deserialize, Serialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NestedMetricAggregator {
    pub parent: MetricAggregator,
    /// Optional child aggregator.
    pub child: Option<Box<NestedMetricAggregator>>,
}

#[cfg(feature = "core")]
impl From<NestedMetricAggregator> for pywr_core::recorders::NestedAggregator {
    fn from(value: NestedMetricAggregator) -> Self {
        pywr_core::recorders::NestedAggregator::new(value.parent.into(), value.child.map(|a| (*a).into()))
    }
}

/// Filters that allow multiple metrics to be added to a metric set.
///
/// The filters allow the default metrics for all nodes and/or parameters in a model
/// to be added to a metric set.
#[derive(Deserialize, Serialize, Clone, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct MetricSetFilters {
    #[serde(default)]
    pub all_nodes: bool,
    #[serde(default)]
    pub all_parameters: bool,
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
                    // Skip Python parameters that return multiple values as the type or keys of these values is not
                    // known at this point.
                    if let Parameter::Python(param) = parameter {
                        if matches!(param.return_type, PythonReturnType::Dict) {
                            continue;
                        }
                    }

                    metrics.push(Metric::Parameter(ParameterReference::new(parameter.name(), None)));
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
///
/// Metrics added by the filters will be appended to any metrics specified for the metric attribute,
/// if they are not a duplication.
#[derive(Deserialize, Serialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MetricSet {
    pub name: String,
    pub metrics: Option<Vec<Metric>>,
    pub aggregator: Option<NestedMetricAggregator>,
    #[serde(default)]
    pub filters: MetricSetFilters,
}

impl MetricSet {
    #[cfg(feature = "core")]
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network, args: &LoadArgs) -> Result<(), SchemaError> {
        use pywr_core::recorders::OutputMetric;

        let output_metrics = match self.metrics {
            Some(ref metrics) => {
                let mut output_metrics: Vec<OutputMetric> = metrics
                    .iter()
                    .map(|m| m.load_as_output(network, args, None))
                    .collect::<Result<_, _>>()?;

                if let Some(additional_metrics) = self.filters.create_metrics(args) {
                    for m in additional_metrics.iter() {
                        let output_metric = m.load_as_output(network, args, None)?;
                        if !output_metrics.contains(&output_metric) {
                            output_metrics.push(output_metric);
                        }
                    }
                }
                output_metrics
            }
            None => {
                if let Some(metrics) = self.filters.create_metrics(args) {
                    metrics
                        .iter()
                        .map(|m| m.load_as_output(network, args, None))
                        .collect::<Result<_, _>>()?
                } else {
                    return Err(SchemaError::EmptyMetricSet(self.name.clone()));
                }
            }
        };

        let aggregator = self.aggregator.clone().map(|a| a.into());

        let metric_set = pywr_core::recorders::MetricSet::new(&self.name, aggregator, output_metrics);
        let _ = network.add_metric_set(metric_set)?;

        Ok(())
    }
}
