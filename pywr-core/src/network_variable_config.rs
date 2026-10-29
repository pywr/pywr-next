use crate::models::MultiNetworkModel;
use crate::network::{Network, NetworkState};
use crate::parameters::{
    Parameter, ParameterIndex, ParameterName, ParameterState, ParameterStates, VariableConfig, VariableParameter,
    VariableParameterError, VariableParameterValues,
};
use std::ops::Range;
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
    #[error("The variable config is for a model with {expected} networks, but the model has {received}.")]
    IncorrectNumberOfNetworks { expected: usize, received: usize },
    #[error("Expected the states of {expected} networks, but received {received}.")]
    IncorrectNumberOfNetworkStates { expected: usize, received: usize },
    #[error("Network with index `{0}` not found in the model.")]
    NetworkIndexNotFound(usize),
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

    /// The number of f64 and u64 variable values required by each variable parameter, in order.
    pub fn variable_sizes(&self, network: &Network) -> Result<Vec<(usize, usize)>, NetworkVariableConfigError> {
        self.entries
            .iter()
            .map(|entry| Ok(entry.index.variable(network)?.size(entry.config.as_ref())))
            .collect()
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

/// The location of a variable parameter's values within its network's values.
#[derive(Debug, Clone)]
struct VariableLocation {
    network_idx: usize,
    f64: Range<usize>,
    u64: Range<usize>,
}

/// The variable configuration for a [`MultiNetworkModel`].
///
/// Each variable parameter belongs to one of the model's networks. A flat vector of f64 and u64
/// values is split between the parameters in the order they were added to the
/// [`MultiNetworkVariableConfigBuilder`], regardless of which network they belong to.
pub struct MultiNetworkVariableConfig {
    /// The variable config of each network, by network index.
    networks: Vec<NetworkVariableConfig>,
    /// The network index of each variable parameter, in the order its values are given.
    order: Vec<usize>,
}

impl MultiNetworkVariableConfig {
    /// The variable config of the network with index `network_idx`.
    pub fn network(&self, network_idx: usize) -> Option<&NetworkVariableConfig> {
        self.networks.get(network_idx)
    }

    /// The total number of f64 and u64 variable values required by this configuration.
    pub fn size(&self, model: &MultiNetworkModel) -> Result<(usize, usize), NetworkVariableConfigError> {
        let locations = self.locations(model)?;
        Ok(total_size(&locations))
    }

    /// The lower bounds of the variable values, in the same order as the values.
    pub fn lower_bounds(
        &self,
        model: &MultiNetworkModel,
    ) -> Result<VariableParameterValues, NetworkVariableConfigError> {
        self.check_model(model)?;

        let bounds = self
            .networks
            .iter()
            .enumerate()
            .map(|(idx, config)| config.lower_bounds(network(model, idx)?))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(gather(&self.locations(model)?, &bounds))
    }

    /// The upper bounds of the variable values, in the same order as the values.
    pub fn upper_bounds(
        &self,
        model: &MultiNetworkModel,
    ) -> Result<VariableParameterValues, NetworkVariableConfigError> {
        self.check_model(model)?;

        let bounds = self
            .networks
            .iter()
            .enumerate()
            .map(|(idx, config)| config.upper_bounds(network(model, idx)?))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(gather(&self.locations(model)?, &bounds))
    }

    /// The current variable values of each scenario, in the same order as the values.
    ///
    /// `states` are the states of each network, in network order. See
    /// [`NetworkVariableConfig::values`] for details of the values returned.
    pub fn values(
        &self,
        model: &MultiNetworkModel,
        states: &[NetworkState],
    ) -> Result<Vec<Option<VariableParameterValues>>, NetworkVariableConfigError> {
        let locations = self.locations(model)?;

        if states.len() != self.networks.len() {
            return Err(NetworkVariableConfigError::IncorrectNumberOfNetworkStates {
                expected: self.networks.len(),
                received: states.len(),
            });
        }

        // The values of each network, for each scenario
        let network_values = self
            .networks
            .iter()
            .zip(states)
            .enumerate()
            .map(|(idx, (config, state))| config.values(network(model, idx)?, state))
            .collect::<Result<Vec<_>, _>>()?;

        let num_scenarios = network_values.first().map_or(0, |values| values.len());

        let values = (0..num_scenarios)
            .map(|scenario| {
                network_values
                    .iter()
                    .map(|values| values.get(scenario).cloned().flatten())
                    .collect::<Option<Vec<_>>>()
                    .map(|values| gather(&locations, &values))
            })
            .collect();

        Ok(values)
    }

    /// Split the values in to the values of each network, in network order.
    ///
    /// An error is returned if the number of values does not match the size of the configuration.
    pub fn split(
        &self,
        model: &MultiNetworkModel,
        values_f64: &[f64],
        values_u64: &[u64],
    ) -> Result<Vec<VariableParameterValues>, NetworkVariableConfigError> {
        let locations = self.locations(model)?;

        let (expected_f64, expected_u64) = total_size(&locations);
        if values_f64.len() != expected_f64 || values_u64.len() != expected_u64 {
            return Err(NetworkVariableConfigError::IncorrectNumberOfValues {
                expected_f64,
                expected_u64,
                received_f64: values_f64.len(),
                received_u64: values_u64.len(),
            });
        }

        let mut split = vec![VariableParameterValues::default(); self.networks.len()];
        let mut offset_f64: usize = 0;
        let mut offset_u64: usize = 0;

        // The locations are in the order of the values, and within each network they are in
        // the order of that network's values.
        for location in &locations {
            let size_f64 = location.f64.len();
            let size_u64 = location.u64.len();

            let network_values = &mut split[location.network_idx];
            network_values
                .f64
                .extend_from_slice(&values_f64[offset_f64..offset_f64 + size_f64]);
            network_values
                .u64
                .extend_from_slice(&values_u64[offset_u64..offset_u64 + size_u64]);

            offset_f64 += size_f64;
            offset_u64 += size_u64;
        }

        Ok(split)
    }

    /// The location of each variable parameter's values within its network's values, in the order
    /// the values are given.
    fn locations(&self, model: &MultiNetworkModel) -> Result<Vec<VariableLocation>, NetworkVariableConfigError> {
        self.check_model(model)?;

        let sizes = self
            .networks
            .iter()
            .enumerate()
            .map(|(idx, config)| config.variable_sizes(network(model, idx)?))
            .collect::<Result<Vec<_>, _>>()?;

        let mut next_variable = vec![0; self.networks.len()];
        let mut offsets = vec![(0, 0); self.networks.len()];

        let locations = self
            .order
            .iter()
            .map(|&network_idx| {
                let (size_f64, size_u64) = sizes[network_idx][next_variable[network_idx]];
                let (offset_f64, offset_u64) = offsets[network_idx];

                next_variable[network_idx] += 1;
                offsets[network_idx] = (offset_f64 + size_f64, offset_u64 + size_u64);

                VariableLocation {
                    network_idx,
                    f64: offset_f64..offset_f64 + size_f64,
                    u64: offset_u64..offset_u64 + size_u64,
                }
            })
            .collect();

        Ok(locations)
    }

    /// Check that `model` has the same number of networks as the model the config was built for.
    fn check_model(&self, model: &MultiNetworkModel) -> Result<(), NetworkVariableConfigError> {
        if model.num_networks() == self.networks.len() {
            Ok(())
        } else {
            Err(NetworkVariableConfigError::IncorrectNumberOfNetworks {
                expected: self.networks.len(),
                received: model.num_networks(),
            })
        }
    }
}

fn network(model: &MultiNetworkModel, idx: usize) -> Result<&Network, NetworkVariableConfigError> {
    model
        .network(idx)
        .ok_or(NetworkVariableConfigError::NetworkIndexNotFound(idx))
}

fn total_size(locations: &[VariableLocation]) -> (usize, usize) {
    locations.iter().fold((0, 0), |(size_f64, size_u64), location| {
        (size_f64 + location.f64.len(), size_u64 + location.u64.len())
    })
}

/// Gather the values of each network in to a single set of values, in the order of `locations`.
fn gather(locations: &[VariableLocation], network_values: &[VariableParameterValues]) -> VariableParameterValues {
    let mut values = VariableParameterValues::default();

    for location in locations {
        let network_values = &network_values[location.network_idx];
        values.f64.extend_from_slice(&network_values.f64[location.f64.clone()]);
        values.u64.extend_from_slice(&network_values.u64[location.u64.clone()]);
    }

    values
}

#[derive(Debug, Error)]
pub enum MultiNetworkVariableConfigBuilderError {
    #[error("Network `{0}` not found.")]
    NetworkNotFound(String),
    #[error("Error adding a variable to network `{network}`.")]
    NetworkError {
        network: String,
        #[source]
        source: NetworkVariableConfigBuilderError,
    },
}

pub struct MultiNetworkVariableConfigBuilder<'m> {
    model: &'m MultiNetworkModel,
    /// The builder of each network. A builder is only `None` while a variable is being added.
    networks: Vec<Option<NetworkVariableConfigBuilder<'m>>>,
    order: Vec<usize>,
}

impl<'m> MultiNetworkVariableConfigBuilder<'m> {
    pub fn new(model: &'m MultiNetworkModel) -> Self {
        let networks = (0..model.num_networks())
            .filter_map(|idx| model.network(idx))
            .map(|network| Some(NetworkVariableConfigBuilder::new(network)))
            .collect();

        Self {
            model,
            networks,
            order: Vec::new(),
        }
    }

    /// Add a variable parameter of the network `network_name` and its configuration.
    ///
    /// An error is returned if the network does not exist, or if the parameter can not be added
    /// to the network's variable config (see [`NetworkVariableConfigBuilder::add_variable_config`]).
    pub fn add_variable_config(
        mut self,
        network_name: &str,
        parameter_name: &ParameterName,
        config: Box<dyn VariableConfig>,
    ) -> Result<Self, MultiNetworkVariableConfigBuilderError> {
        let network_not_found = || MultiNetworkVariableConfigBuilderError::NetworkNotFound(network_name.to_string());

        let network_idx = self
            .model
            .get_network_index_by_name(network_name)
            .ok_or_else(network_not_found)?;

        // Adding a variable consumes the network's builder, so take it and then put it back
        let builder = self
            .networks
            .get_mut(network_idx)
            .and_then(Option::take)
            .ok_or_else(network_not_found)?;

        let builder = builder.add_variable_config(parameter_name, config).map_err(|source| {
            MultiNetworkVariableConfigBuilderError::NetworkError {
                network: network_name.to_string(),
                source,
            }
        })?;

        self.networks[network_idx] = Some(builder);
        self.order.push(network_idx);

        Ok(self)
    }

    pub fn build(self) -> MultiNetworkVariableConfig {
        MultiNetworkVariableConfig {
            networks: self
                .networks
                .into_iter()
                .flatten()
                .map(|builder| builder.build())
                .collect(),
            order: self.order,
        }
    }
}
