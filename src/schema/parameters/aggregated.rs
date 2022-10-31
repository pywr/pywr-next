use crate::schema::parameters::{DynamicFloatValue, DynamicFloatValueType, ParameterMeta};
use std::collections::HashMap;

// TODO complete these
#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum AggFunc {
    Sum,
    Product,
    Max,
    Min,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct AggregatedParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub agg_func: AggFunc,
    pub parameters: Vec<DynamicFloatValue>,
}

impl AggregatedParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }

    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        let mut attributes = HashMap::new();

        let parameters = &self.parameters;
        attributes.insert("parameters", parameters.into());

        attributes
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct AggregatedIndexParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub agg_func: AggFunc,
    // TODO this should be `DynamicIntValues`
    pub parameters: Vec<DynamicFloatValue>,
}

impl AggregatedIndexParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }

    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        let mut attributes = HashMap::new();

        let parameters = &self.parameters;
        attributes.insert("parameters", parameters.into());

        attributes
    }
}

#[cfg(test)]
mod tests {
    use crate::schema::parameters::aggregated::AggregatedParameter;
    use crate::schema::parameters::{DynamicFloatValue, DynamicFloatValueType, Parameter, ParameterFloatValue};

    #[test]
    fn test_aggregated() {
        let data = r#"
            {
                "type": "aggregated",
                "agg_func": "min",
                "comment": "Take the minimum of two parameters",
                "parameters": [
                        {
                            "type": "ControlCurvePiecewiseInterpolated",
                            "storage_node": "Reservoir",
                            "control_curves": [
                                "reservoir_cc",
                                {"type": "constant", "value":  0.2}
                            ],
                            "comment": "A witty comment",
                            "values": [
                                [-0.1, -1.0],
                                [-100, -200],
                                [-300, -400]
                            ],
                            "minimum": 0.05
                        },
                        {
                            "type": "ControlCurvePiecewiseInterpolatedParameter",
                            "storage_node": "Reservoir",
                            "control_curves": [
                                "reservoir_cc",
                                {"type": "constant", "value":  0.2}
                            ],
                            "comment": "A witty comment",
                            "values": [
                                [-0.1, -1.0],
                                [-100, -200],
                                [-300, -400]
                            ],
                            "minimum": 0.05
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
                            ParameterFloatValue::Inline(p) => match p.as_ref() {
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
