#[cfg(feature = "core")]
use crate::SchemaError;
use crate::nodes::{NodeAttribute, NodeMeta};
#[cfg(feature = "core")]
use pywr_core::{metric::UnresolvedMetricF64, node::UnresolvedNode};
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
    pub fn default_attribute(&self) -> NodeAttribute {
        NodeAttribute::Outflow
    }
}

#[cfg(feature = "core")]
impl PlaceholderNode {
    pub fn input_connectors(&self) -> Result<Vec<UnresolvedNode>, SchemaError> {
        Ok(vec![self.meta.name.as_str().into()])
    }

    pub fn output_connectors(&self) -> Result<Vec<UnresolvedNode>, SchemaError> {
        Ok(vec![self.meta.name.as_str().into()])
    }
    pub fn add_to_network(&self) -> Result<(), SchemaError> {
        Err(SchemaError::PlaceholderNodeNotAllowed {
            name: self.meta.name.clone(),
        })
    }

    pub fn nodes_for_flow_constraints(&self) -> Result<Vec<UnresolvedNode>, SchemaError> {
        Err(SchemaError::PlaceholderNodeNotAllowed {
            name: self.meta.name.clone(),
        })
    }

    pub fn nodes_for_storage_constraints(&self) -> Result<Vec<UnresolvedNode>, SchemaError> {
        Err(SchemaError::PlaceholderNodeNotAllowed {
            name: self.meta.name.clone(),
        })
    }

    pub fn create_metric(&self) -> Result<UnresolvedMetricF64, SchemaError> {
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
                tags: Default::default(),
            },
        };

        // Attempt to add the placeholder node to a model
        let result = placeholder.add_to_network();
        assert!(result.is_err());
        assert!(matches!(result, Err(SchemaError::PlaceholderNodeNotAllowed { .. })));
    }
}
