use crate::agg_funcs::{AggFunc, IndexAggFunc};
use crate::error::ComponentConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::{IndexMetric, Metric};
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::parameters::{ConversionData, ParameterMeta};
use crate::v1::{IntoV2, TryFromV1, try_convert_parameter_attr};
#[cfg(feature = "core")]
use pywr_core::parameters::{ParameterIndex, ParameterName};
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::parameters::{
    AggregatedIndexParameter as AggregatedIndexParameterV1, AggregatedParameter as AggregatedParameterV1,
};
use schemars::JsonSchema;
use std::collections::HashMap;

/// Schema for a parameter that aggregates metrics using a user specified function.
///
/// Each time-step the aggregation is updated using the current values of the referenced metrics.
/// The available aggregation functions are defined by the [`AggFunc`] enum.
///
/// This parameter definition is applied to a network using [`AggregatedParameter`].
///
/// See also [`AggregatedIndexParameter`] for aggregation of integer values.
///
/// # JSON Examples
///
/// The example below shows the definition of an [`AggregatedParameter`] that sums the values
/// from a variety of sources:
///  - a literal constant: 3.1415,
///  - a constant value from the table "demands" with reference "my-node",
///  - the current value of the parameter "my-other-parameter",
///  - the current volume of the node "my-reservoir", and
///  - the current value of the inline monthly profile, named "my-monthly-profile".
///
/// ```json
#[doc = include_str!("doc_examples/aggregated_1.json")]
/// ```

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct AggregatedParameter {
    pub meta: ParameterMeta,
    pub agg_func: AggFunc,
    pub metrics: Vec<Metric>,
}

#[cfg(feature = "core")]
impl AggregatedParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let metrics = self
            .metrics
            .iter()
            .map(|v| v.load(network, args, None))
            .collect::<Result<Vec<_>, _>>()?;

        let p = pywr_core::parameters::AggregatedParameter::new(
            ParameterName::new(&self.meta.name, parent),
            &metrics,
            self.agg_func.load(args.data_path)?,
        );

        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1<AggregatedParameterV1> for AggregatedParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: AggregatedParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let metrics = v1
            .parameters
            .into_iter()
            .map(|p| try_convert_parameter_attr(&meta.name, "parameters", p, parent_node, conversion_data))
            .collect::<Result<Vec<_>, _>>()?;

        let p = Self {
            meta,
            agg_func: v1.agg_func.into(),
            metrics,
        };
        Ok(p)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct AggregatedIndexParameter {
    pub meta: ParameterMeta,
    pub agg_func: IndexAggFunc,
    pub metrics: Vec<IndexMetric>,
}

impl AggregatedIndexParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }

    // pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
    //     let mut attributes = HashMap::new();
    //
    //     let parameters = &self.parameters;
    //     attributes.insert("parameters", parameters.into());
    //
    //     attributes
    // }
}

#[cfg(feature = "core")]
impl AggregatedIndexParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<u64>, SchemaError> {
        let metrics = self
            .metrics
            .iter()
            .map(|v| v.load(network, args, None))
            .collect::<Result<Vec<_>, _>>()?;

        let p = pywr_core::parameters::AggregatedIndexParameter::new(
            ParameterName::new(&self.meta.name, parent),
            &metrics,
            self.agg_func.load(args.data_path)?,
        );

        Ok(network.add_index_parameter(Box::new(p))?)
    }
}

impl TryFromV1<AggregatedIndexParameterV1> for AggregatedIndexParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: AggregatedIndexParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let metrics = v1
            .parameters
            .into_iter()
            .map(|p| try_convert_parameter_attr(&meta.name, "parameters", p, parent_node, conversion_data))
            .collect::<Result<Vec<_>, _>>()?;

        let p = Self {
            meta,
            agg_func: v1.agg_func.into(),
            metrics,
        };
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use crate::parameters::aggregated::AggregatedParameter;
    use crate::visit::VisitMetrics;

    #[test]
    fn test_aggregated() {
        let data = r#"
            {
                "meta": {
                    "name": "my-agg-param",
                    "comment": "Take the minimum of two parameters"
                },
                "agg_func": {
                  "type": "Min"
                },
                "metrics": [
                  {
                    "type": "Parameter",
                    "name": "First parameter"
                  },
                  {
                    "type": "Parameter",
                    "name":"Second parameter"
                  }
                ]
            }
            "#;

        let param: AggregatedParameter = serde_json::from_str(data).unwrap();

        let mut count_metrics = 0;
        param.visit_metrics(&mut |_metric| {
            count_metrics += 1;
        });

        assert_eq!(count_metrics, 2);
    }
}
