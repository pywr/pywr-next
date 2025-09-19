use crate::SchemaError;
use crate::parameters::{ActivationFunction, MonthlyProfileVariableConfig, RbfProfileVariableConfig};
#[cfg(feature = "core")]
use pywr_core::parameters::ParameterName;
use schemars::JsonSchema;
use std::path::{Path, PathBuf};
use strum_macros::{Display, EnumDiscriminants, EnumString, IntoStaticStr, VariantNames};
use thiserror::Error;

/// Configuration of a variable parameter.
///
/// The variant of this enum determines must match the type of the parameter it is
/// applied to.
#[derive(serde::Deserialize, serde::Serialize, Debug, EnumDiscriminants, Clone, JsonSchema, Display)]
#[serde(tag = "type")]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, VariantNames))]
// This creates a separate enum called `ParameterType` that is available in this module.
#[strum_discriminants(name(ParameterType))]
pub enum VariableConfig {
    ActivationFunction(ActivationFunction),
    RbfProfile(RbfProfileVariableConfig),
    MonthlyProfile(MonthlyProfileVariableConfig),
}

#[cfg(feature = "core")]
impl VariableConfig {
    fn load(&self) -> Box<dyn pywr_core::parameters::VariableConfig> {
        match self {
            VariableConfig::ActivationFunction(activation_function) => {
                Box::<pywr_core::parameters::ActivationFunction>::new((*activation_function).into())
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

#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema)]
pub struct NamedVariableConfig {
    pub name: String,
    pub config: VariableConfig,
}

#[derive(Error, Debug)]
pub enum VariableConfigsReadError {
    #[error("IO error on path `{path}`: {error}")]
    IO { path: PathBuf, error: std::io::Error },
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, JsonSchema)]
pub struct VariableConfigs {
    pub configs: Vec<NamedVariableConfig>,
}

impl VariableConfigs {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, VariableConfigsReadError> {
        let data = std::fs::read_to_string(&path).map_err(|error| VariableConfigsReadError::IO {
            path: path.as_ref().to_path_buf(),
            error,
        })?;
        Ok(serde_json::from_str(data.as_str())?)
    }
}

#[cfg(feature = "core")]
impl VariableConfigs {
    /// Convert the variable configuration to a "core" network variable configuration.
    ///
    /// This is required in order to apply the built configuration to the built core network.
    pub fn build_config(
        &self,
        network: &pywr_core::network::Network,
    ) -> Result<pywr_core::network_variable_config::NetworkVariableConfig, SchemaError> {
        let mut builder = pywr_core::network_variable_config::NetworkVariableConfigBuilder::new(network);

        for config in &self.configs {
            let core_config = config.config.load();
            let name: ParameterName = config.name.as_str().into();

            builder = builder.add_variable_config(&name, core_config)?;
        }

        Ok(builder.build())
    }
}
