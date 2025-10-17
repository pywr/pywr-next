use crate::ConversionError;
use schemars::JsonSchema;
use strum_macros::{Display, EnumIter};

/// All possible slots that could be attached to a node.
///
///
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Display, JsonSchema, PartialEq, EnumIter)]
#[serde(tag = "type")]
pub enum NodeSlot {
    Storage,
    River,
    Spill,
    Compensation,
    Abstraction,
    Split { position: usize },
    User { name: String },
}

impl NodeSlot {
    pub fn try_from_v1_str(s: &str) -> Result<Self, ConversionError> {
        match s {
            "abstraction" => Ok(NodeSlot::Abstraction),
            "river" => Ok(NodeSlot::River),
            "storage" => Ok(NodeSlot::Storage),
            "spill" => Ok(NodeSlot::Spill),
            "compensation" => Ok(NodeSlot::Compensation),
            _ => Err(ConversionError::InvalidSlot { slot: s.to_string() }),
        }
    }
}
