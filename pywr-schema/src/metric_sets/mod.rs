use crate::agg_funcs::AggFunc;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::metric::{EdgeReference, VirtualNodeAttrReference};
#[cfg(feature = "core")]
use crate::network::LoadArgs;
#[cfg(feature = "core")]
use crate::parameters::{Parameter, PythonReturnType};
use pywr_schema_macros::skip_serializing_none;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
#[cfg(feature = "core")]
use std::path::Path;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};

#[derive(serde::Deserialize, serde::Serialize, Debug, Copy, Clone, JsonSchema, Display, EnumDiscriminants)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(MetricAggFrequencyType))]
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
pub struct MetricAggregator {
    /// Optional aggregation frequency.
    pub freq: Option<MetricAggFrequency>,
    /// Aggregation function to apply over metric values.
    pub func: AggFunc,
    /// Optional child aggregator.
    pub child: Option<Box<MetricAggregator>>,
}

#[cfg(feature = "core")]
impl MetricAggregator {
    fn load(&self, data_path: Option<&Path>) -> Result<pywr_core::recorders::Aggregator, SchemaError> {
        let child = self.child.as_ref().map(|a| a.load(data_path)).transpose()?;

        Ok(pywr_core::recorders::Aggregator::new(
            self.freq.map(|p| p.into()),
            self.func.load(data_path)?,
            child,
        ))
    }
}

/// Filters that allow multiple metrics to be added to a metric set.
///
/// The filters allow the default metrics for all nodes, virtual nodes, parameters and/or edges in
/// a model to be added to a metric set.
#[derive(Deserialize, Serialize, Clone, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct MetricSetFilters {
    #[serde(default)]
    pub all_nodes: bool,
    #[serde(default)]
    pub all_virtual_nodes: bool,
    #[serde(default)]
    pub all_parameters: bool,
    #[serde(default)]
    pub all_edges: bool,
}

#[cfg(feature = "core")]
impl MetricSetFilters {
    fn create_metrics(&self, args: &LoadArgs) -> Vec<Metric> {
        use crate::metric::{NodeAttrReference, ParameterReference};

        let mut metrics = vec![];

        if self.all_nodes {
            for node in args.schema.nodes.iter() {
                metrics.push(Metric::Node(NodeAttrReference::new(node.name().to_string(), None)));
            }
        }

        if self.all_virtual_nodes {
            if let Some(virtual_nodes) = args.schema.virtual_nodes.as_ref() {
                for node in virtual_nodes.iter() {
                    metrics.push(Metric::VirtualNode(VirtualNodeAttrReference::new(
                        node.name().to_string(),
                        None,
                    )));
                }
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

        if self.all_edges {
            for edge in args.schema.edges.iter() {
                metrics.push(Metric::Edge(EdgeReference { edge: edge.clone() }));
            }
        }

        metrics
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
#[skip_serializing_none]
#[derive(Deserialize, Serialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MetricSet {
    pub name: String,
    pub metrics: Option<Vec<Metric>>,
    pub aggregator: Option<MetricAggregator>,
    #[serde(default)]
    pub filters: MetricSetFilters,
}

impl MetricSet {
    #[cfg(feature = "core")]
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network, args: &LoadArgs) -> Result<(), SchemaError> {
        use pywr_core::recorders::OutputMetric;

        // Create metrics from filters and load them as output metrics
        let metrics_from_filters = self
            .filters
            .create_metrics(args)
            .iter()
            .map(|m| m.load_as_output(network, args, None))
            .collect::<Result<Vec<_>, _>>()?;

        let output_metrics = match &self.metrics {
            Some(metrics) => {
                let mut output_metrics: Vec<OutputMetric> = metrics
                    .iter()
                    .map(|m| m.load_as_output(network, args, None))
                    .collect::<Result<_, _>>()?;

                for output_metric in metrics_from_filters.into_iter() {
                    if !output_metrics.contains(&output_metric) {
                        output_metrics.push(output_metric);
                    }
                }

                output_metrics
            }
            None => metrics_from_filters,
        };

        if output_metrics.is_empty() {
            return Err(SchemaError::EmptyMetricSet(self.name.clone()));
        }

        let aggregator = self.aggregator.clone().map(|a| a.load(args.data_path)).transpose()?;

        let metric_set = pywr_core::recorders::MetricSet::new(&self.name, aggregator, output_metrics);
        let _ = network.add_metric_set(metric_set)?;

        Ok(())
    }
}
