use crate::error::SchemaError;
use crate::model::PywrNetwork;
use crate::nodes::NodeAttribute;
use serde::{Deserialize, Serialize};

/// Output metrics that can be recorded from a model run.
#[derive(Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum Metric {
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

impl Metric {
    pub fn try_clone_into_metric(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &PywrNetwork,
    ) -> Result<pywr_core::metric::MetricF64, SchemaError> {
        match self {
            Self::Default { node } => {
                // Get the node from the schema; not the model itself
                let node = schema
                    .get_node_by_name(node)
                    .ok_or_else(|| SchemaError::NodeNotFound(node.to_string()))?;
                // Create and return the node's default metric
                node.create_metric(network, None)
            }
            Self::Deficit { node } => {
                // Get the node from the schema; not the model itself
                let node = schema
                    .get_node_by_name(node)
                    .ok_or_else(|| SchemaError::NodeNotFound(node.to_string()))?;
                // Create and return the metric
                node.create_metric(network, Some(NodeAttribute::Deficit))
            }
            Self::Parameter { name } => {
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
