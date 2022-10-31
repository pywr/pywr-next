use crate::schema::parameters::{DynamicFloatValueType, ParameterMeta};
use std::collections::HashMap;

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct Polynomial1DParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub storage_node: String,
    pub coefficients: Vec<f64>,
    pub use_proportional_volume: Option<bool>,
}

impl Polynomial1DParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        vec![("storage_node", self.storage_node.as_str())].into_iter().collect()
    }
    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        HashMap::new()
    }
}
