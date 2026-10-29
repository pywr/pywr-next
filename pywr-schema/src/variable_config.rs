use crate::parameters::{ActivationFunction, MonthlyProfileVariableConfig, RbfProfileVariableConfig};
use schemars::JsonSchema;
use strum_macros::{Display, EnumDiscriminants, EnumString, IntoStaticStr, VariantNames};

/// Configuration of a variable parameter.
///
/// The variant must be one supported by the type of the parameter it is applied to:
///
/// - [`VariableConfig::ActivationFunction`]: `Constant` and `Offset` parameters.
/// - [`VariableConfig::MonthlyProfile`]: `MonthlyProfile` parameters.
/// - [`VariableConfig::RbfProfile`]: `RbfProfile` parameters.
///
/// A mismatch is reported as an error when the config is built for a network.
#[derive(serde::Deserialize, serde::Serialize, Debug, EnumDiscriminants, Clone, JsonSchema, Display)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, VariantNames))]
// This creates a separate enum called `VariableConfigType` that is available in this module.
#[strum_discriminants(name(VariableConfigType))]
pub enum VariableConfig {
    /// The variable value is transformed by an activation function before it is used.
    ///
    /// The activation function is nested in its own field because it is also tagged by `type`.
    ActivationFunction {
        activation: ActivationFunction,
    },
    RbfProfile(RbfProfileVariableConfig),
    MonthlyProfile(MonthlyProfileVariableConfig),
}

#[cfg(feature = "core")]
impl VariableConfig {
    pub(crate) fn load(&self) -> Box<dyn pywr_core::parameters::VariableConfig> {
        match self {
            VariableConfig::ActivationFunction { activation } => {
                Box::<pywr_core::parameters::ActivationFunction>::new((*activation).into())
            }
            VariableConfig::RbfProfile(rbf_profile) => {
                Box::<pywr_core::parameters::RbfProfileVariableConfig>::new((*rbf_profile).into())
            }
            VariableConfig::MonthlyProfile(monthly_profile) => {
                Box::<pywr_core::parameters::MonthlyProfileVariableConfig>::new((*monthly_profile).into())
            }
        }
    }
}

/// The variable config for a named parameter.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NamedVariableConfig {
    /// The name of the parameter to make variable.
    pub name: String,
    pub config: VariableConfig,
}

#[cfg(test)]
mod tests {
    use super::VariableConfig;

    #[test]
    fn test_activation_function_config() {
        let data = r#"
            {
                "type": "ActivationFunction",
                "activation": {
                    "type": "Unit",
                    "min": 0.0,
                    "max": 2.0
                }
            }"#;

        let config: VariableConfig = serde_json::from_str(data).unwrap();
        assert!(matches!(config, VariableConfig::ActivationFunction { .. }));
    }
}
