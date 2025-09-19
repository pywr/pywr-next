#[cfg(feature = "core")]
use crate::SchemaError;
use crate::parameters::ParameterMeta;
#[cfg(feature = "core")]
use pywr_core::parameters::ParameterIndex;
use pywr_schema_macros::PywrVisitAll;
use schemars::JsonSchema;

/// A placeholder parameter that can be used as a stand-in for a parameter that has not yet been defined.
///
/// This parameter does not have any functionality and is used to allow the schema to be valid
/// while the actual parameter is being defined elsewhere. For example, if this schema is to be
/// combined with another schema that defines the actual parameter.
///
/// Attempting to use this parameter in a model will result in an error.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct PlaceholderParameter {
    pub meta: ParameterMeta,
}

#[cfg(feature = "core")]
impl PlaceholderParameter {
    pub fn add_to_model(&self) -> Result<ParameterIndex<f64>, SchemaError> {
        Err(SchemaError::PlaceholderParameterNotAllowed {
            name: self.meta.name.clone(),
        })
    }
}

#[cfg(all(test, feature = "core"))]
mod test {
    use super::*;

    #[test]
    fn test_try_add_placeholder_parameter() {
        let placeholder = PlaceholderParameter {
            meta: ParameterMeta {
                name: "placeholder".to_string(),
                comment: None,
            },
        };

        // Attempt to add the placeholder parameter to a model
        let result = placeholder.add_to_model();
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(SchemaError::PlaceholderParameterNotAllowed { .. })
        ));
    }
}
