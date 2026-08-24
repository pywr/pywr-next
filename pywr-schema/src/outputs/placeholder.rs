#[cfg(feature = "core")]
use crate::SchemaError;
use pywr_schema_macros::{PywrVisitPaths, skip_serializing_none};
use schemars::JsonSchema;

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitPaths)]
pub struct PlaceholderOutput {
    pub name: String,
}

#[cfg(feature = "core")]
impl PlaceholderOutput {
    pub fn add_to_network(&self) -> Result<(), SchemaError> {
        Err(SchemaError::PlaceholderOutputNotAllowed {
            name: self.name.clone(),
        })
    }
}
