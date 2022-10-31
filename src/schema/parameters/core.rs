use crate::schema::parameters::{ConstantFloatValue, DynamicFloatValueType, ParameterFloatValue, ParameterMeta};
use std::collections::HashMap;

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct ConstantParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub value: ConstantFloatValue,
}

impl ConstantParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }

    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        HashMap::new()
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct MaxParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub parameter: Box<ParameterFloatValue>,
    pub threshold: Option<f64>,
}

impl MaxParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }
    // pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
    //     let mut attributes = HashMap::new();
    //     attributes.insert("parameter", self.parameter.as_ref().into());
    //     attributes
    // }
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct NegativeParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub parameter: Box<ParameterFloatValue>,
}

impl NegativeParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }
    // pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
    //     let mut attributes = HashMap::new();
    //     attributes.insert("parameter", self.parameter.as_ref().into());
    //     attributes
    // }
}
