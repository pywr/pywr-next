use crate::network::{Network, NetworkState};
use crate::parameters::{
    Parameter, ParameterIndex, ParameterName, ParameterState, ParameterStates, VariableConfig, VariableParameter,
    VariableParameterError, VariableParameterValues,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkVariableConfigError {
    #[error(
        "Expected {expected_f64} f64 and {expected_u64} u64 variable values, but received {received_f64} f64 and {received_u64} u64 values."
    )]
    IncorrectNumberOfValues {
        expected_f64: usize,
        expected_u64: usize,
        received_f64: usize,
        received_u64: usize,
    },
    #[error("F64 parameter with index `{0}` not found.")]
    ParameterF64IndexNotFound(ParameterIndex<f64>),
    #[error("U64 parameter with index `{0}` not found.")]
    ParameterU64IndexNotFound(ParameterIndex<u64>),
    #[error("Parameter `{name}` cannot be a variable.")]
    ParameterTypeNotVariable { name: ParameterName },
    #[error("Internal state for parameter `{name}` not found.")]
    ParameterStateNotFound { name: ParameterName },
    #[error("Variable bounds are not defined for parameter `{name}`.")]
    BoundsNotDefined { name: ParameterName },
    #[error("Error setting variable values for parameter `{name}`: {source}")]
    VariableParameterError {
        name: ParameterName,
        #[source]
        source: VariableParameterError,
    },
}

/// The index of a variable parameter, which may be either a f64 or u64 parameter.
#[derive(Debug, Copy, Clone)]
enum VariableParameterIndex {
    F64(ParameterIndex<f64>),
    U64(ParameterIndex<u64>),
}

impl VariableParameterIndex {
    fn parameter<'n>(&self, network: &'n Network) -> Result<&'n dyn Parameter, NetworkVariableConfigError> {
        match self {
            Self::F64(idx) => network
                .get_parameter(*idx)
                .ok_or(NetworkVariableConfigError::ParameterF64IndexNotFound(*idx)),
            Self::U64(idx) => network
                .get_index_parameter(*idx)
                .ok_or(NetworkVariableConfigError::ParameterU64IndexNotFound(*idx)),
        }
    }

    fn variable<'n>(&self, network: &'n Network) -> Result<&'n dyn VariableParameter, NetworkVariableConfigError> {
        let parameter = self.parameter(network)?;

        parameter
            .as_variable()
            .ok_or_else(|| NetworkVariableConfigError::ParameterTypeNotVariable {
                name: parameter.name().clone(),
            })
    }

    fn internal_state<'s>(&self, parameter_states: &'s ParameterStates) -> Option<&'s Option<Box<dyn ParameterState>>> {
        match self {
            Self::F64(idx) => parameter_states.get_f64_state(*idx),
            Self::U64(idx) => parameter_states.get_u64_state(*idx),
        }
    }

    fn internal_state_mut<'s>(
        &self,
        parameter_states: &'s mut ParameterStates,
    ) -> Option<&'s mut Option<Box<dyn ParameterState>>> {
        match self {
            Self::F64(idx) => parameter_states.get_mut_f64_state(*idx),
            Self::U64(idx) => parameter_states.get_mut_u64_state(*idx),
        }
    }
}

struct VariableEntry {
    name: ParameterName,
    index: VariableParameterIndex,
    config: Box<dyn VariableConfig>,
}

/// The variable configuration for a network.
///
/// This defines which parameters are variable, and how a flat vector of f64 and u64 values is
/// split between them. Values are assigned to the parameters in the order they were added to
/// the [`NetworkVariableConfigBuilder`].
pub struct NetworkVariableConfig {
    entries: Vec<VariableEntry>,
}

impl NetworkVariableConfig {
    /// Iterate over the parameter names and their variable configurations, in order.
    pub fn iter(&self) -> impl Iterator<Item = (&ParameterName, &dyn VariableConfig)> {
        self.entries.iter().map(|entry| (&entry.name, entry.config.as_ref()))
    }

    /// The total number of f64 and u64 variable values required by this configuration.
    pub fn size(&self, network: &Network) -> Result<(usize, usize), NetworkVariableConfigError> {
        let mut size_f64: usize = 0;
        let mut size_u64: usize = 0;

        for entry in &self.entries {
            let (s_f64, s_u64) = entry.index.variable(network)?.size(entry.config.as_ref());
            size_f64 += s_f64;
            size_u64 += s_u64;
        }

        Ok((size_f64, size_u64))
    }

    /// The lower bounds of the variable values, in the same order as the values.
    ///
    /// These are bounds on the values given to [`Self::apply_to_parameter_states`].
    pub fn lower_bounds(&self, network: &Network) -> Result<VariableParameterValues, NetworkVariableConfigError> {
        self.bounds(network, |variable, config| variable.get_lower_bounds(config))
    }

    /// The upper bounds of the variable values, in the same order as the values.
    ///
    /// These are bounds on the values given to [`Self::apply_to_parameter_states`].
    pub fn upper_bounds(&self, network: &Network) -> Result<VariableParameterValues, NetworkVariableConfigError> {
        self.bounds(network, |variable, config| variable.get_upper_bounds(config))
    }

    fn bounds(
        &self,
        network: &Network,
        get_bounds: impl Fn(&dyn VariableParameter, &dyn VariableConfig) -> Option<VariableParameterValues>,
    ) -> Result<VariableParameterValues, NetworkVariableConfigError> {
        let mut bounds = VariableParameterValues::default();

        for entry in &self.entries {
            let variable = entry.index.variable(network)?;
            let parameter_bounds = get_bounds(variable, entry.config.as_ref()).ok_or_else(|| {
                NetworkVariableConfigError::BoundsNotDefined {
                    name: entry.name.clone(),
                }
            })?;

            bounds.f64.extend(parameter_bounds.f64);
            bounds.u64.extend(parameter_bounds.u64);
        }

        Ok(bounds)
    }

    /// The current variable values of each scenario, in the same order as the values.
    ///
    /// These are the values used by the parameters in the simulation. They are after any
    /// activation function or clamping has been applied, so they may differ from the values given
    /// to [`Self::apply_to_parameter_states`] and are not necessarily within [`Self::lower_bounds`]
    /// and [`Self::upper_bounds`]. A scenario's values are `None` if any of the variable
    /// parameters have not had their values set.
    pub fn values(
        &self,
        network: &Network,
        state: &NetworkState,
    ) -> Result<Vec<Option<VariableParameterValues>>, NetworkVariableConfigError> {
        state
            .iter_parameter_states()
            .map(|parameter_states| self.values_for_parameter_states(network, parameter_states))
            .collect()
    }

    fn values_for_parameter_states(
        &self,
        network: &Network,
        parameter_states: &ParameterStates,
    ) -> Result<Option<VariableParameterValues>, NetworkVariableConfigError> {
        let mut values = VariableParameterValues::default();

        for entry in &self.entries {
            let variable = entry.index.variable(network)?;
            let internal_state = entry.index.internal_state(parameter_states).ok_or_else(|| {
                NetworkVariableConfigError::ParameterStateNotFound {
                    name: entry.name.clone(),
                }
            })?;

            match variable.get_variables(internal_state) {
                Some(parameter_values) => {
                    values.f64.extend(parameter_values.f64);
                    values.u64.extend(parameter_values.u64);
                }
                None => return Ok(None),
            }
        }

        Ok(Some(values))
    }

    /// Apply the values to the parameter states of a single scenario.
    ///
    /// The values are split between the variable parameters in the order they were added. This must
    /// be called before the constant parameters are computed (see [`Network::setup_network_with_variables`])
    /// otherwise changes to constant parameters will not be reflected in the simulation.
    pub fn apply_to_parameter_states(
        &self,
        network: &Network,
        values_f64: &[f64],
        values_u64: &[u64],
        parameter_states: &mut ParameterStates,
    ) -> Result<(), NetworkVariableConfigError> {
        let (expected_f64, expected_u64) = self.size(network)?;
        if values_f64.len() != expected_f64 || values_u64.len() != expected_u64 {
            return Err(NetworkVariableConfigError::IncorrectNumberOfValues {
                expected_f64,
                expected_u64,
                received_f64: values_f64.len(),
                received_u64: values_u64.len(),
            });
        }

        let mut offset_f64: usize = 0;
        let mut offset_u64: usize = 0;

        for entry in &self.entries {
            let variable = entry.index.variable(network)?;
            let internal_state = entry.index.internal_state_mut(parameter_states).ok_or_else(|| {
                NetworkVariableConfigError::ParameterStateNotFound {
                    name: variable.name().clone(),
                }
            })?;

            // The slices are in bounds because the total size was checked above
            let (size_f64, size_u64) = variable.size(entry.config.as_ref());

            variable
                .set_variables(
                    &values_f64[offset_f64..offset_f64 + size_f64],
                    &values_u64[offset_u64..offset_u64 + size_u64],
                    entry.config.as_ref(),
                    internal_state,
                )
                .map_err(|source| NetworkVariableConfigError::VariableParameterError {
                    name: variable.name().clone(),
                    source,
                })?;

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
    #[error("Parameter `{0}` cannot be a variable.")]
    ParameterNotVariable(ParameterName),
    #[error("Parameter `{0}` has already been added as a variable.")]
    DuplicateParameter(ParameterName),
    #[error("Invalid variable config for parameter `{name}`: {source}")]
    InvalidConfig {
        name: ParameterName,
        #[source]
        source: VariableParameterError,
    },
}

pub struct NetworkVariableConfigBuilder<'n> {
    network: &'n Network,
    entries: Vec<VariableEntry>,
}

impl<'n> NetworkVariableConfigBuilder<'n> {
    pub fn new(network: &'n Network) -> Self {
        Self {
            network,
            entries: Vec::new(),
        }
    }

    /// Add a variable parameter and its configuration.
    ///
    /// An error is returned if the parameter does not exist, cannot be a variable, has already been
    /// added, or if the configuration is not valid for the parameter.
    pub fn add_variable_config(
        mut self,
        parameter_name: &ParameterName,
        config: Box<dyn VariableConfig>,
    ) -> Result<Self, NetworkVariableConfigBuilderError> {
        if self.entries.iter().any(|entry| &entry.name == parameter_name) {
            return Err(NetworkVariableConfigBuilderError::DuplicateParameter(
                parameter_name.clone(),
            ));
        }

        let index = if let Some(idx) = self.network.get_parameter_index_by_name(parameter_name) {
            VariableParameterIndex::F64(idx)
        } else if let Some(idx) = self.network.get_index_parameter_index_by_name(parameter_name) {
            VariableParameterIndex::U64(idx)
        } else {
            return Err(NetworkVariableConfigBuilderError::ParameterNotFound(
                parameter_name.clone(),
            ));
        };

        let variable = index
            .variable(self.network)
            .map_err(|_| NetworkVariableConfigBuilderError::ParameterNotVariable(parameter_name.clone()))?;

        variable.validate_config(config.as_ref()).map_err(|source| {
            NetworkVariableConfigBuilderError::InvalidConfig {
                name: parameter_name.clone(),
                source,
            }
        })?;

        self.entries.push(VariableEntry {
            name: parameter_name.clone(),
            index,
            config,
        });

        Ok(self)
    }

    pub fn build(self) -> NetworkVariableConfig {
        NetworkVariableConfig { entries: self.entries }
    }
}
