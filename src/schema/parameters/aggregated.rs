use crate::schema::data_tables::LoadedTableCollection;
use crate::schema::error::ConversionError;
use crate::schema::parameters::{
    DynamicFloatValue, DynamicFloatValueType, DynamicIndexValue, IntoV2Parameter, ParameterMeta, TryFromV1Parameter,
    TryIntoV2Parameter,
};
use crate::{IndexParameterIndex, ParameterIndex, PywrError};
use pywr_schema::parameters::{
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

impl From<AggFunc> for crate::parameters::AggFunc {
    fn from(value: AggFunc) -> Self {
        match value {
            AggFunc::Sum => crate::parameters::AggFunc::Sum,
            AggFunc::Product => crate::parameters::AggFunc::Product,
            AggFunc::Max => crate::parameters::AggFunc::Max,
            AggFunc::Min => crate::parameters::AggFunc::Min,
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

// TODO finish this documentation
/// A parameter that aggregates other parameters using a user specified function.
///
/// # JSON Examples
///
/// A simple example:
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
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<ParameterIndex, PywrError> {
        let metrics = self
            .metrics
            .iter()
            .map(|v| v.load(model, tables, data_path))
            .collect::<Result<Vec<_>, _>>()?;

        let p = crate::parameters::AggregatedParameter::new(&self.meta.name, &metrics, self.agg_func.into());

        model.add_parameter(Box::new(p))
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

impl From<IndexAggFunc> for crate::parameters::AggIndexFunc {
    fn from(value: IndexAggFunc) -> Self {
        match value {
            IndexAggFunc::Sum => crate::parameters::AggIndexFunc::Sum,
            IndexAggFunc::Product => crate::parameters::AggIndexFunc::Product,
            IndexAggFunc::Max => crate::parameters::AggIndexFunc::Max,
            IndexAggFunc::Min => crate::parameters::AggIndexFunc::Min,
            IndexAggFunc::Any => crate::parameters::AggIndexFunc::Any,
            IndexAggFunc::All => crate::parameters::AggIndexFunc::All,
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
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<IndexParameterIndex, PywrError> {
        let parameters = self
            .parameters
            .iter()
            .map(|v| v.load(model, tables, data_path))
            .collect::<Result<Vec<_>, _>>()?;

        let p = crate::parameters::AggregatedIndexParameter::new(&self.meta.name, parameters, self.agg_func.into());

        model.add_index_parameter(Box::new(p))
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
    use crate::schema::parameters::aggregated::AggregatedParameter;
    use crate::schema::parameters::{DynamicFloatValue, DynamicFloatValueType, MetricFloatValue, Parameter};

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
                        "storage_node": "Reservoir",
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
                        "storage_node": "Reservoir",
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
                                    assert_eq!(p.node_references().remove("storage_node"), Some("Reservoir"))
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
