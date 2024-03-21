use crate::error::SchemaError;
use crate::model::PywrNetwork;
use crate::nodes::NodeAttribute;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;

/// Output metrics that can be recorded from a model run.
#[derive(Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum OutputMetric {
    /// Output the default metric for a node.
    Default {
        node: String,
    },
    Deficit {
        node: String,
    },
    Parameter {
        name: String,
    },
}

impl OutputMetric {
    fn try_clone_into_metric(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &PywrNetwork,
    ) -> Result<pywr_core::metric::MetricF64, SchemaError> {
        match self {
            OutputMetric::Default { node } => {
                // Get the node from the schema; not the model itself
                let node = schema
                    .get_node_by_name(node)
                    .ok_or_else(|| SchemaError::NodeNotFound(node.to_string()))?;
                // Create and return the node's default metric
                node.create_metric(network, None)
            }
            OutputMetric::Deficit { node } => {
                // Get the node from the schema; not the model itself
                let node = schema
                    .get_node_by_name(node)
                    .ok_or_else(|| SchemaError::NodeNotFound(node.to_string()))?;
                // Create and return the metric
                node.create_metric(network, Some(NodeAttribute::Deficit))
            }
            OutputMetric::Parameter { name } => {
                if let Ok(idx) = network.get_parameter_index_by_name(name) {
                    Ok(pywr_core::metric::MetricF64::ParameterValue(idx))
                } else if let Ok(idx) = network.get_index_parameter_index_by_name(name) {
                    Ok(pywr_core::metric::MetricF64::IndexParameterValue(idx))
                } else {
                    Err(SchemaError::ParameterNotFound(name.to_string()))
                }
            }
        }
    }
}

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
    freq: Option<MetricAggFrequency>,
    /// Aggregation function to apply over metric values.
    func: MetricAggFunc,
    /// Optional child aggregator.
    child: Option<Box<MetricAggregator>>,
}

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
    name: String,
    metrics: Vec<OutputMetric>,
    aggregator: Option<MetricAggregator>,
}

impl MetricSet {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &PywrNetwork,
    ) -> Result<(), SchemaError> {
        // Convert the schema representation to internal metrics.
        let metrics: Vec<pywr_core::metric::MetricF64> = self
            .metrics
            .iter()
            .map(|m| m.try_clone_into_metric(network, schema))
            .collect::<Result<_, _>>()?;

        let aggregator = self.aggregator.clone().map(|a| a.into());

        let metric_set = pywr_core::recorders::MetricSet::new(&self.name, aggregator, metrics);
        let _ = network.add_metric_set(metric_set)?;

        Ok(())
    }
}
