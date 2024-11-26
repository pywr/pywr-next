use crate::error::ConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::parameters::{ConversionData, DynamicIndexValue, ParameterMeta};
use crate::v1::{IntoV2, TryFromV1, TryIntoV2};
#[cfg(feature = "core")]
use pywr_core::parameters::ParameterIndex;
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::parameters::{
    AggFunc as AggFuncV1, AggregatedIndexParameter as AggregatedIndexParameterV1,
    AggregatedParameter as AggregatedParameterV1, IndexAggFunc as IndexAggFuncV1,
};
use schemars::JsonSchema;
use std::collections::HashMap;

// TODO complete these
#[derive(serde::Deserialize, serde::Serialize, Debug, Copy, Clone, strum_macros::Display, JsonSchema, PywrVisitAll)]
#[serde(rename_all = "lowercase")]
pub enum AggFunc {
    Sum,
    Product,
    Max,
    Min,
}

#[cfg(feature = "core")]
impl From<AggFunc> for pywr_core::parameters::AggFunc {
    fn from(value: AggFunc) -> Self {
        match value {
            AggFunc::Sum => pywr_core::parameters::AggFunc::Sum,
            AggFunc::Product => pywr_core::parameters::AggFunc::Product,
            AggFunc::Max => pywr_core::parameters::AggFunc::Max,
            AggFunc::Min => pywr_core::parameters::AggFunc::Min,
        }
    }
}

impl From<AggFuncV1> for AggFunc {
    fn from(v1: AggFuncV1) -> Self {
        match v1 {
            AggFuncV1::Sum => Self::Sum,
            AggFuncV1::Product => Self::Product,
            AggFuncV1::Max => Self::Max,
            AggFuncV1::Min => Self::Min,
        }
    }
}

/// Schema for a parameter that aggregates metrics using a user specified function.
///
/// Each time-step the aggregation is updated using the current values of the referenced metrics.
/// The available aggregation functions are defined by the [`AggFunc`] enum.
///
/// This parameter definition is applied to a network using [`crate::parameters::AggregatedParameter`].
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
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let metrics = self
            .metrics
            .iter()
            .map(|v| v.load(network, args))
            .collect::<Result<Vec<_>, _>>()?;

        let p = pywr_core::parameters::AggregatedParameter::new(
            self.meta.name.as_str().into(),
            &metrics,
            self.agg_func.into(),
        );

        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1<AggregatedParameterV1> for AggregatedParameter {
    type Error = ConversionError;

    fn try_from_v1(
        v1: AggregatedParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let parameters = v1
            .parameters
            .into_iter()
            .map(|p| p.try_into_v2(parent_node, conversion_data))
            .collect::<Result<Vec<_>, _>>()?;

        let p = Self {
            meta,
            agg_func: v1.agg_func.into(),
            metrics: parameters,
        };
        Ok(p)
    }
}

// TODO complete these
#[derive(serde::Deserialize, serde::Serialize, Debug, Copy, Clone, strum_macros::Display, JsonSchema, PywrVisitAll)]
#[serde(rename_all = "lowercase")]
pub enum IndexAggFunc {
    Sum,
    Product,
    Max,
    Min,
    Any,
    All,
}

#[cfg(feature = "core")]
impl From<IndexAggFunc> for pywr_core::parameters::AggIndexFunc {
    fn from(value: IndexAggFunc) -> Self {
        match value {
            IndexAggFunc::Sum => pywr_core::parameters::AggIndexFunc::Sum,
            IndexAggFunc::Product => pywr_core::parameters::AggIndexFunc::Product,
            IndexAggFunc::Max => pywr_core::parameters::AggIndexFunc::Max,
            IndexAggFunc::Min => pywr_core::parameters::AggIndexFunc::Min,
            IndexAggFunc::Any => pywr_core::parameters::AggIndexFunc::Any,
            IndexAggFunc::All => pywr_core::parameters::AggIndexFunc::All,
        }
    }
}

impl From<IndexAggFuncV1> for IndexAggFunc {
    fn from(v1: IndexAggFuncV1) -> Self {
        match v1 {
            IndexAggFuncV1::Sum => Self::Sum,
            IndexAggFuncV1::Product => Self::Product,
            IndexAggFuncV1::Max => Self::Max,
            IndexAggFuncV1::Min => Self::Min,
            IndexAggFuncV1::Any => Self::Any,
            IndexAggFuncV1::All => Self::All,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct AggregatedIndexParameter {
    pub meta: ParameterMeta,
    pub agg_func: IndexAggFunc,
    // TODO this should be `DynamicIntValues`
    pub parameters: Vec<DynamicIndexValue>,
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
    ) -> Result<ParameterIndex<usize>, SchemaError> {
        let parameters = self
            .parameters
            .iter()
            .map(|v| v.load(network, args))
            .collect::<Result<Vec<_>, _>>()?;

        let p = pywr_core::parameters::AggregatedIndexParameter::new(
            self.meta.name.as_str().into(),
            parameters,
            self.agg_func.into(),
        );

        Ok(network.add_index_parameter(Box::new(p))?)
    }
}

impl TryFromV1<AggregatedIndexParameterV1> for AggregatedIndexParameter {
    type Error = ConversionError;

    fn try_from_v1(
        v1: AggregatedIndexParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let parameters = v1
            .parameters
            .into_iter()
            .map(|p| p.try_into_v2(parent_node, conversion_data))
            .collect::<Result<Vec<_>, _>>()?;

        let p = Self {
            meta,
            agg_func: v1.agg_func.into(),
            parameters,
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
                "agg_func": "min",
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
