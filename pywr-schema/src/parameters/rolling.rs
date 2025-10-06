use crate::agg_funcs::{AggFunc, IndexAggFunc};
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::{IndexMetric, Metric, NodeAttrReference};
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::parameters::ParameterMeta;
use crate::v1::IntoV2;
use crate::{ComponentConversionError, ConversionData, ConversionError, TryFromV1};
#[cfg(feature = "core")]
use pywr_core::parameters::{ParameterIndex, ParameterName};
use pywr_schema_macros::{PywrVisitAll, skip_serializing_none};
use pywr_v1_schema::parameters::RollingMeanFlowNodeParameter as RollingMeanFlowNodeParameterV1;
use schemars::JsonSchema;

/// A parameter that computes a rolling value based on a specified metric over a defined window size.
///
/// The rolling function can be configured to use different aggregation functions
/// (see [`AggFunc`] for more details). If the `min_values` is not specified, it defaults to the
/// `window_size`, meaning that the rolling value will only be computed once enough values are
/// available. Prior to the first `min_values` being reached, the parameter will return the
/// `initial_value`.
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct RollingParameter {
    pub meta: ParameterMeta,
    pub metric: Metric,
    pub window_size: u64,
    pub initial_value: f64,
    pub min_values: Option<u64>,
    pub agg_func: AggFunc,
}

#[cfg(feature = "core")]
impl RollingParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let metric = self.metric.load(network, args, None)?;
        let p = pywr_core::parameters::RollingParameter::new(
            ParameterName::new(&self.meta.name, parent),
            metric,
            self.window_size as usize,
            self.initial_value,
            self.min_values.unwrap_or(self.window_size) as usize,
            self.agg_func.load(args.data_path)?,
        );
        Ok(network.add_parameter(Box::new(p))?)
    }
}

/// A parameter that computes a rolling value based on a specified index metric over a defined window size.
///
/// The rolling function can be configured to use different aggregation functions
/// (see [`IndexAggFunc`] for more details). If the `min_values` is not specified, it defaults
/// to the `window_size`, meaning that the rolling value will only be computed once enough values
/// are available. Prior to the first `min_values` being reached, the parameter will return the
/// `initial_value`.
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct RollingIndexParameter {
    pub meta: ParameterMeta,
    pub metric: IndexMetric,
    pub window_size: u64,
    pub initial_value: u64,
    pub min_values: Option<u64>,
    pub agg_func: IndexAggFunc,
}

#[cfg(feature = "core")]
impl RollingIndexParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<u64>, SchemaError> {
        let metric = self.metric.load(network, args, None)?;
        let p = pywr_core::parameters::RollingParameter::new(
            ParameterName::new(&self.meta.name, parent),
            metric,
            self.window_size as usize,
            self.initial_value,
            self.min_values.unwrap_or(self.window_size) as usize,
            self.agg_func.load(args.data_path)?,
        );
        Ok(network.add_index_parameter(Box::new(p))?)
    }
}

impl TryFromV1<RollingMeanFlowNodeParameterV1> for RollingParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: RollingMeanFlowNodeParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let window_size = match (v1.timesteps, v1.days) {
            (Some(timesteps), None) => timesteps as u64,
            (None, Some(_)) => {
                return Err(ComponentConversionError::Parameter {
                        attr: "days".to_string(),
                        name: meta.name.clone(),
                        error: ConversionError::UnsupportedFeature {
                            feature: "`RollingMeanFlowNodeParameter` days window size must be specified as a number of time-steps. Use `window_size` in the updated schema, or convert a v1.x parameter with the `timesteps` attribute.".to_string(),
                        },
                    });
            }
            (Some(_), Some(_)) => {
                return Err(ComponentConversionError::Parameter {
                    attr: "timesteps".to_string(),
                    name: meta.name.clone(),
                    error: ConversionError::UnsupportedFeature {
                        feature: "`RollingMeanFlowNodeParameter` cannot have both `timesteps` and `days` specified. Use `window_size` in the updated schema, or correct the v1.x definition.".to_string(),
                    },
                });
            }
            (None, None) => {
                return Err(ComponentConversionError::Parameter {
                    attr: "timesteps".to_string(),
                    name: meta.name.clone(),
                    error: ConversionError::UnsupportedFeature {
                        feature: "`RollingMeanFlowNodeParameter` must have `timesteps` specified. Use `window_size` in the updated schema.".to_string(),
                    },
                });
            }
        };

        // Convert the node reference to a metric
        let node_ref = NodeAttrReference {
            name: v1.node,
            attribute: None,
        };
        let metric = Metric::Node(node_ref);

        // pywr 1 does not support interpolation
        let p = Self {
            meta,
            metric,
            window_size,
            initial_value: v1.initial_flow.unwrap_or_default(),
            min_values: None,
            agg_func: AggFunc::Mean,
        };
        Ok(p)
    }
}
