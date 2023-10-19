use crate::error::SchemaError;
use crate::parameters::{DynamicFloatValueType, ParameterMeta};
use pywr_core::parameters::ParameterIndex;
use std::collections::HashMap;

/// A parameter that can receive values from another model.
///
/// See [`PywrMultiModel`] for more information.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct InterModelTransferParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
}

impl InterModelTransferParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }

    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        HashMap::new()
    }

    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<ParameterIndex, SchemaError> {
        let p = pywr_core::parameters::InterModelTransfer::new(&self.meta.name);
        Ok(network.add_parameter(Box::new(p))?)
    }
}
