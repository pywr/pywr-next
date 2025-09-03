use crate::SchemaError;
use crate::nodes::{NodeAttribute, NodeMeta};
use pywr_schema_macros::PywrVisitAll;
use schemars::JsonSchema;

/// A placeholder node that can be used as a stand-in for a node that has not yet been defined.
///
/// This node does not have any functionality and is used to allow the schema to be valid
/// while the actual node is being defined elsewhere. For example, if this schema is to be
/// combined with another schema that defines the actual node.
///
/// Attempting to use this node in a model will result in an error.
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct PlaceholderNode {
    pub meta: NodeMeta,
}

impl PlaceholderNode {
    pub fn input_connectors(&self) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        Ok(vec![(self.meta.name.as_str(), None)])
    }

    pub fn output_connectors(&self) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        Ok(vec![(self.meta.name.as_str(), None)])
    }

    pub fn default_attribute(&self) -> NodeAttribute {
        NodeAttribute::Outflow
    }
}

#[cfg(feature = "core")]
impl PlaceholderNode {
    pub fn add_to_model(&self) -> Result<(), SchemaError> {
        Err(SchemaError::PlaceholderNodeNotAllowed {
            name: self.meta.name.clone(),
        })
    }

    pub fn node_indices_for_flow_constraints(&self) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        Err(SchemaError::PlaceholderNodeNotAllowed {
            name: self.meta.name.clone(),
        })
    }

    pub fn node_indices_for_storage_constraints(&self) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        Err(SchemaError::PlaceholderNodeNotAllowed {
            name: self.meta.name.clone(),
        })
    }

    pub fn set_constraints(&self) -> Result<(), SchemaError> {
        Err(SchemaError::PlaceholderNodeNotAllowed {
            name: self.meta.name.clone(),
        })
    }

    pub fn create_metric(&self) -> Result<pywr_core::metric::MetricF64, SchemaError> {
        Err(SchemaError::PlaceholderNodeNotAllowed {
            name: self.meta.name.clone(),
        })
    }
}

#[cfg(all(test, feature = "core"))]
mod test {
    use super::*;

    #[test]
    fn test_try_add_placeholder_node() {
        let placeholder = PlaceholderNode {
            meta: NodeMeta {
                name: "placeholder".to_string(),
                comment: None,
                position: None,
            },
        };

        // Attempt to add the placeholder node to a model
        let result = placeholder.add_to_model();
        assert!(result.is_err());
        assert!(matches!(result, Err(SchemaError::PlaceholderNodeNotAllowed { .. })));
    }
}
