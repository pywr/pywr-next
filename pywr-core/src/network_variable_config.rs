use crate::NetworkError;
use crate::network::{Network, NetworkState};
use crate::parameters::{ParameterIndex, ParameterName, VariableConfig};
use thiserror::Error;

pub struct NetworkVariableConfig {
    /// Configuration for f64 parameters.
    variable_configs_f64: Vec<(ParameterIndex<f64>, Box<dyn VariableConfig>)>,
    /// Configuration for u64 parameters.
    variable_configs_u64: Vec<(ParameterIndex<u64>, Box<dyn VariableConfig>)>,
}

impl NetworkVariableConfig {
    /// Iterate over the variable configurations.
    pub fn iter<'n>(
        &'n self,
        network: &'n Network,
    ) -> impl Iterator<Item = (&'n ParameterName, &'n dyn VariableConfig)> + use<'n> {
        self.variable_configs_f64
            .iter()
            .map(move |(parameter_index, config)| {
                (network.get_parameter(*parameter_index).unwrap().name(), config.as_ref())
            })
            .chain(self.variable_configs_u64.iter().map(move |(parameter_index, config)| {
                (
                    network.get_index_parameter(*parameter_index).unwrap().name(),
                    config.as_ref(),
                )
            }))
    }
    /// Apply the values to the network state.
    pub fn apply(
        &self,
        network: &Network,
        values_f64: &[f64],
        values_u64: &[u64],
        state: &mut NetworkState,
    ) -> Result<(), NetworkError> {
        let mut offset_f64: usize = 0;
        let mut offset_u64: usize = 0;

        for (parameter_index, var_config) in &self.variable_configs_f64 {
            let parameter = network.get_parameter(*parameter_index).expect("Parameter not found");
            let variable = parameter.as_variable().expect("Parameter cannot be variable");

            let (size_f64, size_u64) = variable.size(var_config.as_ref());

            let range_f64 = offset_f64..offset_f64 + size_f64;
            let range_u64 = offset_u64..offset_u64 + size_u64;

            for parameter_states in state.iter_parameter_states_mut() {
                let internal_state = parameter_states.get_mut_f64_state(*parameter_index).ok_or(
                    NetworkError::ParameterStateNotFound {
                        name: parameter.name().clone(),
                    },
                )?;

                variable
                    .set_variables(
                        &values_f64[range_f64.clone()],
                        &values_u64[range_u64.clone()],
                        var_config.as_ref(),
                        internal_state,
                    )
                    .map_err(|source| NetworkError::VariableParameterError {
                        name: parameter.name().clone(),
                        source,
                    })?;
            }

            offset_f64 += size_f64;
            offset_u64 += size_u64;
        }

        for (parameter_index, var_config) in &self.variable_configs_u64 {
            let parameter = network
                .get_index_parameter(*parameter_index)
                .expect("Parameter not found");
            let variable = parameter.as_variable().expect("Parameter cannot be variable");

            let (size_f64, size_u64) = variable.size(var_config.as_ref());
            let range_f64 = offset_f64..offset_f64 + size_f64;
            let range_u64 = offset_u64..offset_u64 + size_u64;

            // TODO handle the case where the range is out of bounds without a panic
            network.set_u64_parameter_variable_values(
                *parameter_index,
                &values_f64[range_f64],
                &values_u64[range_u64],
                var_config.as_ref(),
                state,
            )?;

            offset_f64 += size_f64;
            offset_u64 += size_u64;
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum NetworkVariableConfigBuilderError {
    #[error("Parameter not found: {0}")]
    ParameterNotFound(ParameterName),
}

pub struct NetworkVariableConfigBuilder<'n> {
    network: &'n Network,
    variable_configs_f64: Vec<(ParameterIndex<f64>, Box<dyn VariableConfig>)>,
    variable_configs_u64: Vec<(ParameterIndex<u64>, Box<dyn VariableConfig>)>,
}

impl<'n> NetworkVariableConfigBuilder<'n> {
    pub fn new(network: &'n Network) -> Self {
        Self {
            network,
            variable_configs_f64: Vec::new(),
            variable_configs_u64: Vec::new(),
        }
    }

    pub fn add_variable_config(
        mut self,
        parameter_name: &ParameterName,
        config: Box<dyn VariableConfig>,
    ) -> Result<Self, NetworkVariableConfigBuilderError> {
        if let Some(parameter_index) = self.network.get_parameter_index_by_name(parameter_name) {
            self.variable_configs_f64.push((parameter_index, config));
        } else if let Some(parameter_index) = self.network.get_index_parameter_index_by_name(parameter_name) {
            self.variable_configs_u64.push((parameter_index, config));
        } else {
            return Err(NetworkVariableConfigBuilderError::ParameterNotFound(
                parameter_name.clone(),
            ));
        }

        Ok(self)
    }

    pub fn build(self) -> NetworkVariableConfig {
        NetworkVariableConfig {
            variable_configs_f64: self.variable_configs_f64,
            variable_configs_u64: self.variable_configs_u64,
        }
    }
}
