use crate::schema::parameters::{DynamicFloatValue, DynamicFloatValueType, ParameterMeta};
use std::collections::HashMap;

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct IndexedArrayParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    #[serde(alias = "params")]
    pub parameters: Vec<DynamicFloatValue>,
    pub index_parameter: DynamicFloatValue,
}

impl IndexedArrayParameter {
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
