#[cfg(feature = "core")]
use crate::SchemaError;
use crate::variable_config::NamedVariableConfig;
#[cfg(feature = "core")]
use pywr_core::parameters::ParameterName;
use pywr_schema_macros::skip_serializing_none;
use schemars::JsonSchema;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Whether an objective should be minimised or maximised.
#[derive(serde::Deserialize, serde::Serialize, Debug, Copy, Clone, PartialEq, Eq, JsonSchema)]
pub enum ObjectiveSense {
    Minimise,
    Maximise,
}

#[cfg(feature = "core")]
impl From<ObjectiveSense> for pywr_core::problem::ObjectiveSense {
    fn from(sense: ObjectiveSense) -> Self {
        match sense {
            ObjectiveSense::Minimise => Self::Minimise,
            ObjectiveSense::Maximise => Self::Maximise,
        }
    }
}

/// An objective, which is the aggregated value of an output.
///
/// The output must support aggregation to a single value (e.g. a `Memory` output).
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectiveConfig {
    /// The name of the output.
    pub output: String,
    /// The name of the network the output belongs to. This is required for a multi-network
    /// model, and must not be given for a single-network model.
    pub network: Option<String>,
    pub sense: ObjectiveSense,
}

/// A constraint, which requires the aggregated value of an output to be within bounds.
///
/// The output must support aggregation to a single value (e.g. a `Memory` output). At least one
/// of the bounds must be given.
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConstraintConfig {
    /// The name of the output.
    pub output: String,
    /// The name of the network the output belongs to. This is required for a multi-network
    /// model, and must not be given for a single-network model.
    pub network: Option<String>,
    pub lower_bound: Option<f64>,
    pub upper_bound: Option<f64>,
}

#[derive(Error, Debug)]
pub enum ProblemConfigReadError {
    #[error("IO error on path `{path}`: {error}")]
    IO { path: PathBuf, error: std::io::Error },
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// The definition of an optimisation problem for a model.
///
/// This defines the parameters that can be varied by external algorithms (e.g. optimisation
/// algorithms), and the objectives and constraints calculated from the model's outputs. It is
/// kept separate from the model definition so that a model can be used with different problems.
///
/// The variable values are split between the parameters in the order they are listed in
/// `variables`. A parameter may only be listed once.
///
/// For a multi-network model, each variable, objective and constraint must give the `network`
/// it belongs to. For a single-network model, `network` must not be given.
///
/// # JSON Examples
///
/// ```json
#[doc = include_str!("doc_examples/problem1.json")]
/// ```
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProblemConfig {
    pub variables: Vec<NamedVariableConfig>,
    pub objectives: Vec<ObjectiveConfig>,
    #[serde(default)]
    pub constraints: Vec<ConstraintConfig>,
}

impl ProblemConfig {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, ProblemConfigReadError> {
        let data = std::fs::read_to_string(&path).map_err(|error| ProblemConfigReadError::IO {
            path: path.as_ref().to_path_buf(),
            error,
        })?;
        Ok(serde_json::from_str(data.as_str())?)
    }

    /// The `network` of each variable, objective and constraint, with a description of the item.
    #[cfg(feature = "core")]
    fn networks(&self) -> impl Iterator<Item = (Option<&str>, String)> {
        let variables = self
            .variables
            .iter()
            .map(|v| (v.network.as_deref(), format!("variable `{}`", v.name)));
        let objectives = self
            .objectives
            .iter()
            .map(|o| (o.network.as_deref(), format!("objective `{}`", o.output)));
        let constraints = self
            .constraints
            .iter()
            .map(|c| (c.network.as_deref(), format!("constraint `{}`", c.output)));

        variables.chain(objectives).chain(constraints)
    }
}

#[cfg(feature = "core")]
impl ProblemConfig {
    /// Convert the variables to a "core" network variable configuration for the given network.
    pub fn build_variable_config(
        &self,
        network: &pywr_core::network::Network,
    ) -> Result<pywr_core::network_variable_config::NetworkVariableConfig, SchemaError> {
        let mut builder = pywr_core::network_variable_config::NetworkVariableConfigBuilder::new(network);

        for variable in &self.variables {
            let core_config = variable.config.load();
            let name: ParameterName = variable.name.as_str().into();

            builder = builder.add_variable_config(&name, core_config)?;
        }

        Ok(builder.build())
    }

    /// Build a "core" problem for the given built model.
    ///
    /// An error is returned if any variable, objective or constraint gives a `network`.
    pub fn build_model_problem(
        &self,
        model: pywr_core::models::Model,
    ) -> Result<pywr_core::problem::ModelProblem, SchemaError> {
        if let Some((_, item)) = self.networks().find(|(network, _)| network.is_some()) {
            return Err(SchemaError::ProblemNetworkNotAllowed(item));
        }

        let variables = self.build_variable_config(model.network())?;

        let mut builder = pywr_core::problem::ModelProblemBuilder::new(model, variables);

        for objective in &self.objectives {
            builder.objective(pywr_core::problem::Objective::new(
                &objective.output,
                objective.sense.into(),
            ));
        }

        for constraint in &self.constraints {
            builder.constraint(pywr_core::problem::Constraint::new(
                &constraint.output,
                constraint.lower_bound,
                constraint.upper_bound,
            ));
        }

        Ok(builder.build()?)
    }

    /// Convert the variables to a "core" variable configuration for the given multi-network model.
    ///
    /// An error is returned if any variable does not give its `network`.
    pub fn build_multi_network_variable_config(
        &self,
        model: &pywr_core::models::MultiNetworkModel,
    ) -> Result<pywr_core::network_variable_config::MultiNetworkVariableConfig, SchemaError> {
        let mut builder = pywr_core::network_variable_config::MultiNetworkVariableConfigBuilder::new(model);

        for variable in &self.variables {
            let network = required_network(variable.network.as_deref(), || format!("variable `{}`", variable.name))?;
            let name: ParameterName = variable.name.as_str().into();

            builder = builder.add_variable_config(network, &name, variable.config.load())?;
        }

        Ok(builder.build())
    }

    /// Build a "core" problem for the given built multi-network model.
    ///
    /// An error is returned if any variable, objective or constraint does not give its `network`.
    pub fn build_multi_network_model_problem(
        &self,
        model: pywr_core::models::MultiNetworkModel,
    ) -> Result<pywr_core::problem::MultiNetworkModelProblem, SchemaError> {
        if let Some((_, item)) = self.networks().find(|(network, _)| network.is_none()) {
            return Err(SchemaError::ProblemNetworkRequired(item));
        }

        let variables = self.build_multi_network_variable_config(&model)?;

        let mut builder = pywr_core::problem::MultiNetworkModelProblemBuilder::new(model, variables);

        for objective in &self.objectives {
            let network = required_network(objective.network.as_deref(), || {
                format!("objective `{}`", objective.output)
            })?;
            builder.objective(pywr_core::problem::NetworkObjective::new(
                network,
                pywr_core::problem::Objective::new(&objective.output, objective.sense.into()),
            ));
        }

        for constraint in &self.constraints {
            let network = required_network(constraint.network.as_deref(), || {
                format!("constraint `{}`", constraint.output)
            })?;
            builder.constraint(pywr_core::problem::NetworkConstraint::new(
                network,
                pywr_core::problem::Constraint::new(&constraint.output, constraint.lower_bound, constraint.upper_bound),
            ));
        }

        Ok(builder.build()?)
    }
}

#[cfg(feature = "core")]
fn required_network(network: Option<&str>, item: impl FnOnce() -> String) -> Result<&str, SchemaError> {
    network.ok_or_else(|| SchemaError::ProblemNetworkRequired(item()))
}

#[cfg(test)]
mod tests {
    use super::ProblemConfig;
    use std::fs::{self, read_to_string};
    use std::path::PathBuf;

    #[test]
    fn test_problem_config_doc_examples() {
        let mut doc_examples = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        doc_examples.push("src/doc_examples");

        for entry in fs::read_dir(doc_examples).unwrap() {
            let p = entry.unwrap().path();
            if p.is_file() && p.file_name().unwrap().to_str().unwrap().starts_with("problem") {
                let data = read_to_string(&p).unwrap_or_else(|e| panic!("Failed to read file: {p:?}: {e}",));

                let value: ProblemConfig =
                    serde_json::from_str(&data).unwrap_or_else(|e| panic!("Failed to deserialize {p:?}: {e}",));

                // The serialized config must deserialize to the same config
                let round_trip: ProblemConfig = serde_json::from_str(&serde_json::to_string(&value).unwrap())
                    .unwrap_or_else(|e| panic!("Failed to round-trip {p:?}: {e}",));
                assert_eq!(round_trip.variables.len(), value.variables.len());
                assert_eq!(round_trip.objectives.len(), value.objectives.len());
                assert_eq!(round_trip.constraints.len(), value.constraints.len());
            }
        }
    }
}
