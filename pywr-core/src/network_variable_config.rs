use crate::network::{Network, NetworkState};
use crate::parameters::{ParameterIndex, ParameterName, VariableConfig};
use crate::PywrError;

pub struct NetworkVariableConfig<'n> {
    /// The network
    network: &'n Network,
    /// Configuration for f64 parameters.
    variable_configs_f64: Vec<(ParameterIndex<f64>, Box<dyn VariableConfig>)>,
    /// Configuration for u64 parameters.
    variable_configs_u64: Vec<(ParameterIndex<u64>, Box<dyn VariableConfig>)>,
}

impl NetworkVariableConfig<'_> {
    /// Iterate over the variable configurations.
    pub fn iter(&self) -> impl Iterator<Item = (&ParameterName, &dyn VariableConfig)> {
        self.variable_configs_f64
            .iter()
            .map(move |(parameter_index, config)| {
                (
                    self.network.get_parameter(*parameter_index).unwrap().name(),
                    config.as_ref(),
                )
            })
            .chain(self.variable_configs_u64.iter().map(move |(parameter_index, config)| {
                (
                    self.network.get_index_parameter(*parameter_index).unwrap().name(),
                    config.as_ref(),
                )
            }))
    }
    /// Apply the values to the network state.
    pub fn apply(&self, values_f64: &[f64], values_u64: &[u64], state: &mut NetworkState) -> Result<(), PywrError> {
        let mut offset_f64: usize = 0;
        let mut offset_u64: usize = 0;

        for (parameter_index, var_config) in &self.variable_configs_f64 {
            let size_f64 = var_config.size_f64();
            let size_u64 = var_config.size_u64();
            let range_f64 = offset_f64..offset_f64 + size_f64;
            let range_u64 = offset_u64..offset_u64 + size_u64;

            // TODO handle the case where the range is out of bounds without a panic
            self.network.set_f64_parameter_variable_values(
                *parameter_index,
                &values_f64[range_f64],
                &values_u64[range_u64],
                var_config.as_ref(),
                state,
            )?;

            offset_f64 += size_f64;
            offset_u64 += size_u64;
        }

        for (parameter_index, var_config) in &self.variable_configs_u64 {
            let size_f64 = var_config.size_f64();
            let size_u64 = var_config.size_u64();
            let range_f64 = offset_f64..offset_f64 + size_f64;
            let range_u64 = offset_u64..offset_u64 + size_u64;

            // TODO handle the case where the range is out of bounds without a panic
            self.network.set_u64_parameter_variable_values(
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
    ) -> Result<Self, PywrError> {
        if let Ok(parameter_index) = self.network.get_parameter_index_by_name(parameter_name) {
            self.variable_configs_f64.push((parameter_index, config));
        } else if let Ok(parameter_index) = self.network.get_index_parameter_index_by_name(parameter_name) {
            self.variable_configs_u64.push((parameter_index, config));
        } else {
            return Err(PywrError::ParameterNotFound(parameter_name.clone()));
        }

        Ok(self)
    }

    pub fn build(self) -> NetworkVariableConfig<'n> {
        NetworkVariableConfig {
            network: self.network,
            variable_configs_f64: self.variable_configs_f64,
            variable_configs_u64: self.variable_configs_u64,
        }
    }
}
