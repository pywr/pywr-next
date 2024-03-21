use crate::data_tables::LoadedTableCollection;
use crate::error::{ConversionError, SchemaError};
use crate::model::PywrMultiNetworkTransfer;
use crate::parameters::{
    DynamicFloatValue, DynamicFloatValueType, DynamicIndexValue, IntoV2Parameter, ParameterMeta, TryFromV1Parameter,
    TryIntoV2Parameter,
};
use crate::timeseries::LoadedTimeseriesCollection;
use pywr_core::models::ModelDomain;
use pywr_core::parameters::ParameterIndex;
use pywr_v1_schema::parameters::{
    AggFunc as AggFuncV1, AggregatedIndexParameter as AggregatedIndexParameterV1,
    AggregatedParameter as AggregatedParameterV1, IndexAggFunc as IndexAggFuncV1,
};
use std::collections::HashMap;
use std::path::Path;

// TODO complete these
#[derive(serde::Deserialize, serde::Serialize, Debug, Copy, Clone)]
#[serde(rename_all = "lowercase")]
pub enum AggFunc {
    Sum,
    Product,
    Max,
    Min,
}

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

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct AggregatedParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub agg_func: AggFunc,
    pub metrics: Vec<DynamicFloatValue>,
}

impl AggregatedParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }

    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        let mut attributes = HashMap::new();

        let metrics = &self.metrics;
        attributes.insert("parameters", metrics.into());

        attributes
    }

    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &crate::model::PywrNetwork,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
        timeseries: &LoadedTimeseriesCollection,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let metrics = self
            .metrics
            .iter()
            .map(|v| {
                v.load(
                    network,
                    schema,
                    domain,
                    tables,
                    data_path,
                    inter_network_transfers,
                    timeseries,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let p = pywr_core::parameters::AggregatedParameter::new(&self.meta.name, &metrics, self.agg_func.into());

        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1Parameter<AggregatedParameterV1> for AggregatedParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: AggregatedParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);

        let parameters = v1
            .parameters
            .into_iter()
            .map(|p| p.try_into_v2_parameter(Some(&meta.name), unnamed_count))
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
#[derive(serde::Deserialize, serde::Serialize, Debug, Copy, Clone)]
#[serde(rename_all = "lowercase")]
pub enum IndexAggFunc {
    Sum,
    Product,
    Max,
    Min,
    Any,
    All,
}

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

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct AggregatedIndexParameter {
    #[serde(flatten)]
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

    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &crate::model::PywrNetwork,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
        timeseries: &LoadedTimeseriesCollection,
    ) -> Result<ParameterIndex<usize>, SchemaError> {
        let parameters = self
            .parameters
            .iter()
            .map(|v| {
                v.load(
                    network,
                    schema,
                    domain,
                    tables,
                    data_path,
                    inter_network_transfers,
                    timeseries,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let p = pywr_core::parameters::AggregatedIndexParameter::new(&self.meta.name, parameters, self.agg_func.into());

        Ok(network.add_index_parameter(Box::new(p))?)
    }
}

impl TryFromV1Parameter<AggregatedIndexParameterV1> for AggregatedIndexParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: AggregatedIndexParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);

        let parameters = v1
            .parameters
            .into_iter()
            .map(|p| p.try_into_v2_parameter(Some(&meta.name), unnamed_count))
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
    use crate::parameters::{DynamicFloatValue, DynamicFloatValueType, MetricFloatValue, Parameter};

    #[test]
    fn test_aggregated() {
        let data = r#"
            {
                "name": "my-agg-param",
                "type": "aggregated",
                "agg_func": "min",
                "comment": "Take the minimum of two parameters",
                "metrics": [
                  {
                    "type": "InlineParameter",
                    "definition": {
                        "name": "First parameter",
                        "type": "ControlCurvePiecewiseInterpolated",
                        "storage_node": {
                          "name": "Reservoir",
                          "attribute": "ProportionalVolume"
                        },
                        "control_curves": [
                            {"type": "Parameter", "name": "reservoir_cc"},
                            0.2
                        ],
                        "comment": "A witty comment",
                        "values": [
                            [-0.1, -1.0],
                            [-100, -200],
                            [-300, -400]
                        ],
                        "minimum": 0.05
                    }
                  },
                  {
                    "type": "InlineParameter",
                    "definition": {
                        "name": "Second parameter",
                        "type": "ControlCurvePiecewiseInterpolated",
                        "storage_node": {
                          "name": "Reservoir",
                          "attribute": "ProportionalVolume"
                        },
                        "control_curves": [
                            {"type": "Parameter", "name": "reservoir_cc"},
                            0.2
                        ],
                        "comment": "A witty comment",
                        "values": [
                            [-0.1, -1.0],
                            [-100, -200],
                            [-300, -400]
                        ],
                        "minimum": 0.05
                    }
                  }
                ]
            }
            "#;

        let param: AggregatedParameter = serde_json::from_str(data).unwrap();

        assert_eq!(param.node_references().len(), 0);
        assert_eq!(param.parameters().len(), 1);
        match param.parameters().remove("parameters").unwrap() {
            DynamicFloatValueType::List(children) => {
                assert_eq!(children.len(), 2);
                for p in children {
                    match p {
                        DynamicFloatValue::Dynamic(p) => match p {
                            MetricFloatValue::InlineParameter { definition } => match definition.as_ref() {
                                Parameter::ControlCurvePiecewiseInterpolated(p) => {
                                    assert_eq!(p.storage_node.name, "Reservoir")
                                }
                                _ => panic!("Incorrect core parameter deserialized."),
                            },
                            _ => panic!("Non-core parameter was deserialized."),
                        },
                        _ => panic!("Wrong variant for child parameter."),
                    }
                }
            }
            _ => panic!("Wrong variant for parameters."),
        };
    }
}
