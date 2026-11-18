//! Definition and evaluation of optimisation problems.
//!
//! A [`ModelProblem`] combines a [`Model`], the variable parameters of its network (see
//! [`NetworkVariableConfig`]), and the objectives and constraints to calculate from the results
//! of a model run. It is designed to be used by external optimisation algorithms (e.g.
//! multi-objective evolutionary algorithms), which repeatedly evaluate the problem with
//! different variable values.
//!
//! A [`MultiNetworkModelProblem`] does the same for a [`MultiNetworkModel`], whose variable
//! parameters, objectives and constraints can belong to any of its networks.
use crate::models::{
    Model, ModelResult, ModelRunError, MultiNetworkModel, MultiNetworkModelResult, MultiNetworkModelRunError,
};
use crate::network_variable_config::{MultiNetworkVariableConfig, NetworkVariableConfig, NetworkVariableConfigError};
use crate::parameters::VariableParameterValues;
use crate::recorders::RecorderAggregationError;
use crate::solvers::SolverConfig;
use thiserror::Error;

/// Whether an objective should be minimised or maximised.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ObjectiveSense {
    Minimise,
    Maximise,
}

/// An objective of a problem, which is the aggregated value of a recorder.
#[derive(Debug, Clone)]
pub struct Objective {
    recorder: String,
    sense: ObjectiveSense,
}

impl Objective {
    pub fn new(recorder: &str, sense: ObjectiveSense) -> Self {
        Self {
            recorder: recorder.to_string(),
            sense,
        }
    }

    pub fn recorder(&self) -> &str {
        &self.recorder
    }

    pub fn sense(&self) -> ObjectiveSense {
        self.sense
    }
}

/// A constraint of a problem, which requires the aggregated value of a recorder to be within
/// the given bounds.
#[derive(Debug, Clone)]
pub struct Constraint {
    recorder: String,
    lower_bound: Option<f64>,
    upper_bound: Option<f64>,
}

impl Constraint {
    pub fn new(recorder: &str, lower_bound: Option<f64>, upper_bound: Option<f64>) -> Self {
        Self {
            recorder: recorder.to_string(),
            lower_bound,
            upper_bound,
        }
    }

    pub fn recorder(&self) -> &str {
        &self.recorder
    }

    pub fn lower_bound(&self) -> Option<f64> {
        self.lower_bound
    }

    pub fn upper_bound(&self) -> Option<f64> {
        self.upper_bound
    }

    /// The amount by which `value` is outside the bounds of the constraint.
    ///
    /// This is zero if the constraint is satisfied and positive otherwise. A `NaN` value is
    /// treated as an infinite violation.
    pub fn violation(&self, value: f64) -> f64 {
        if value.is_nan() {
            return f64::INFINITY;
        }

        let below = self.lower_bound.map_or(0.0, |lb| lb - value);
        let above = self.upper_bound.map_or(0.0, |ub| value - ub);

        below.max(above).max(0.0)
    }

    fn has_valid_bounds(&self) -> bool {
        match (self.lower_bound, self.upper_bound) {
            (None, None) => false,
            (Some(lb), None) => !lb.is_nan(),
            (None, Some(ub)) => !ub.is_nan(),
            (Some(lb), Some(ub)) => lb <= ub,
        }
    }
}

/// The result of evaluating a problem.
///
/// The values are in the same order as the problem's objectives and constraints. Objective values
/// are not adjusted for their [`ObjectiveSense`].
#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    pub objectives: Vec<f64>,
    pub constraints: Vec<f64>,
}

#[derive(Debug, Error)]
pub enum ModelProblemBuildError {
    #[error("A problem requires at least one objective.")]
    NoObjectives,
    #[error("Recorder `{0}` not found in the network.")]
    RecorderNotFound(String),
    #[error(
        "Constraint on recorder `{0}` is invalid. It requires at least one bound, and the lower bound must not be greater than the upper bound."
    )]
    InvalidConstraintBounds(String),
    #[error("The variable config is not valid for the network.")]
    VariableConfigError(#[from] NetworkVariableConfigError),
}

#[derive(Debug, Error)]
pub enum ModelProblemEvaluateError {
    #[error("Error running the model.")]
    ModelRunError(#[from] ModelRunError),
    #[error("Result for recorder `{0}` not found.")]
    RecorderResultNotFound(String),
    #[error("Error aggregating the result of recorder `{name}`.")]
    AggregationError {
        name: String,
        #[source]
        source: RecorderAggregationError,
    },
}

/// Builder for a [`ModelProblem`].
pub struct ModelProblemBuilder {
    model: Model,
    variables: NetworkVariableConfig,
    objectives: Vec<Objective>,
    constraints: Vec<Constraint>,
}

impl ModelProblemBuilder {
    /// Create a new builder for a problem of `model`, with the given variable config for the
    /// model's network.
    pub fn new(model: Model, variables: NetworkVariableConfig) -> Self {
        Self {
            model,
            variables,
            objectives: Vec::new(),
            constraints: Vec::new(),
        }
    }

    /// Add an objective. Objectives are evaluated in the order they are added.
    pub fn objective(&mut self, objective: Objective) -> &mut Self {
        self.objectives.push(objective);
        self
    }

    /// Add a constraint. Constraints are evaluated in the order they are added.
    pub fn constraint(&mut self, constraint: Constraint) -> &mut Self {
        self.constraints.push(constraint);
        self
    }

    /// Build the problem.
    ///
    /// An error is returned if there are no objectives, an objective or constraint refers to a
    /// recorder that is not in the model's network, a constraint's bounds are invalid, or the
    /// variable config does not match the model's network.
    pub fn build(self) -> Result<ModelProblem, ModelProblemBuildError> {
        if self.objectives.is_empty() {
            return Err(ModelProblemBuildError::NoObjectives);
        }

        let recorders = self
            .objectives
            .iter()
            .map(|o| o.recorder())
            .chain(self.constraints.iter().map(|c| c.recorder()));
        for recorder in recorders {
            if self.model.network().get_recorder_by_name(recorder).is_none() {
                return Err(ModelProblemBuildError::RecorderNotFound(recorder.to_string()));
            }
        }

        if let Some(constraint) = self.constraints.iter().find(|c| !c.has_valid_bounds()) {
            return Err(ModelProblemBuildError::InvalidConstraintBounds(
                constraint.recorder().to_string(),
            ));
        }

        // Check the variable config refers to variable parameters in this network
        self.variables.size(self.model.network())?;

        Ok(ModelProblem {
            model: self.model,
            variables: self.variables,
            objectives: self.objectives,
            constraints: self.constraints,
        })
    }
}

/// An optimisation problem defined by a [`Model`], its variable parameters, and the objectives
/// and constraints calculated from the results of a model run.
///
/// Use [`ModelProblemBuilder`] to create a problem.
pub struct ModelProblem {
    model: Model,
    variables: NetworkVariableConfig,
    objectives: Vec<Objective>,
    constraints: Vec<Constraint>,
}

impl ModelProblem {
    pub fn model(&self) -> &Model {
        &self.model
    }

    pub fn variables(&self) -> &NetworkVariableConfig {
        &self.variables
    }

    pub fn objectives(&self) -> &[Objective] {
        &self.objectives
    }

    pub fn constraints(&self) -> &[Constraint] {
        &self.constraints
    }

    /// The number of f64 and u64 variable values required to evaluate the problem.
    pub fn size(&self) -> Result<(usize, usize), NetworkVariableConfigError> {
        self.variables.size(self.model.network())
    }

    /// The lower bounds of the variable values.
    pub fn lower_bounds(&self) -> Result<VariableParameterValues, NetworkVariableConfigError> {
        self.variables.lower_bounds(self.model.network())
    }

    /// The upper bounds of the variable values.
    pub fn upper_bounds(&self) -> Result<VariableParameterValues, NetworkVariableConfigError> {
        self.variables.upper_bounds(self.model.network())
    }

    /// Evaluate the problem by running the model with the given variable values, and returning
    /// the values of the objectives and constraints.
    ///
    /// Each evaluation sets up and runs the model from scratch, so evaluations are independent
    /// and can be run in parallel.
    pub fn evaluate<C>(
        &self,
        solver_config: &C,
        values_f64: &[f64],
        values_u64: &[u64],
    ) -> Result<Evaluation, ModelProblemEvaluateError>
    where
        C: SolverConfig,
    {
        let result = self
            .model
            .run_with_variables(solver_config, &self.variables, values_f64, values_u64)?;

        let objectives = self
            .objectives
            .iter()
            .map(|o| aggregated_value(&result, o.recorder()))
            .collect::<Result<_, _>>()?;

        let constraints = self
            .constraints
            .iter()
            .map(|c| aggregated_value(&result, c.recorder()))
            .collect::<Result<_, _>>()?;

        Ok(Evaluation {
            objectives,
            constraints,
        })
    }
}

fn aggregated_value(result: &ModelResult, recorder: &str) -> Result<f64, ModelProblemEvaluateError> {
    result
        .network_result
        .get(recorder)
        .ok_or_else(|| ModelProblemEvaluateError::RecorderResultNotFound(recorder.to_string()))?
        .aggregated_value()
        .map_err(|source| ModelProblemEvaluateError::AggregationError {
            name: recorder.to_string(),
            source,
        })
}

/// An objective of a [`MultiNetworkModelProblem`], from a recorder of one of the model's networks.
#[derive(Debug, Clone)]
pub struct NetworkObjective {
    network: String,
    objective: Objective,
}

impl NetworkObjective {
    pub fn new(network: &str, objective: Objective) -> Self {
        Self {
            network: network.to_string(),
            objective,
        }
    }

    /// The name of the network whose recorder the objective uses.
    pub fn network(&self) -> &str {
        &self.network
    }

    pub fn objective(&self) -> &Objective {
        &self.objective
    }
}

/// A constraint of a [`MultiNetworkModelProblem`], from a recorder of one of the model's networks.
#[derive(Debug, Clone)]
pub struct NetworkConstraint {
    network: String,
    constraint: Constraint,
}

impl NetworkConstraint {
    pub fn new(network: &str, constraint: Constraint) -> Self {
        Self {
            network: network.to_string(),
            constraint,
        }
    }

    /// The name of the network whose recorder the constraint uses.
    pub fn network(&self) -> &str {
        &self.network
    }

    pub fn constraint(&self) -> &Constraint {
        &self.constraint
    }
}

#[derive(Debug, Error)]
pub enum MultiNetworkModelProblemBuildError {
    #[error("A problem requires at least one objective.")]
    NoObjectives,
    #[error("Network `{0}` not found in the model.")]
    NetworkNotFound(String),
    #[error("Recorder `{recorder}` not found in network `{network}`.")]
    RecorderNotFound { network: String, recorder: String },
    #[error(
        "Constraint on recorder `{recorder}` of network `{network}` is invalid. It requires at least one bound, and the lower bound must not be greater than the upper bound."
    )]
    InvalidConstraintBounds { network: String, recorder: String },
    #[error("The variable config is not valid for the model.")]
    VariableConfigError(#[from] NetworkVariableConfigError),
}

#[derive(Debug, Error)]
pub enum MultiNetworkModelProblemEvaluateError {
    #[error("Error running the model.")]
    ModelRunError(#[from] MultiNetworkModelRunError),
    #[error("Results for network `{0}` not found.")]
    NetworkResultNotFound(String),
    #[error("Result for recorder `{recorder}` of network `{network}` not found.")]
    RecorderResultNotFound { network: String, recorder: String },
    #[error("Error aggregating the result of recorder `{recorder}` of network `{network}`.")]
    AggregationError {
        network: String,
        recorder: String,
        #[source]
        source: RecorderAggregationError,
    },
}

/// Builder for a [`MultiNetworkModelProblem`].
pub struct MultiNetworkModelProblemBuilder {
    model: MultiNetworkModel,
    variables: MultiNetworkVariableConfig,
    objectives: Vec<NetworkObjective>,
    constraints: Vec<NetworkConstraint>,
}

impl MultiNetworkModelProblemBuilder {
    /// Create a new builder for a problem of `model`, with the given variable config for the
    /// model's networks.
    pub fn new(model: MultiNetworkModel, variables: MultiNetworkVariableConfig) -> Self {
        Self {
            model,
            variables,
            objectives: Vec::new(),
            constraints: Vec::new(),
        }
    }

    /// Add an objective. Objectives are evaluated in the order they are added.
    pub fn objective(&mut self, objective: NetworkObjective) -> &mut Self {
        self.objectives.push(objective);
        self
    }

    /// Add a constraint. Constraints are evaluated in the order they are added.
    pub fn constraint(&mut self, constraint: NetworkConstraint) -> &mut Self {
        self.constraints.push(constraint);
        self
    }

    /// Build the problem.
    ///
    /// An error is returned if there are no objectives, an objective or constraint refers to a
    /// network or recorder that is not in the model, a constraint's bounds are invalid, or the
    /// variable config does not match the model's networks.
    pub fn build(self) -> Result<MultiNetworkModelProblem, MultiNetworkModelProblemBuildError> {
        if self.objectives.is_empty() {
            return Err(MultiNetworkModelProblemBuildError::NoObjectives);
        }

        let recorders = self
            .objectives
            .iter()
            .map(|o| (o.network(), o.objective().recorder()))
            .chain(
                self.constraints
                    .iter()
                    .map(|c| (c.network(), c.constraint().recorder())),
            );
        for (network_name, recorder) in recorders {
            let network = self
                .model
                .get_network_index_by_name(network_name)
                .and_then(|idx| self.model.network(idx))
                .ok_or_else(|| MultiNetworkModelProblemBuildError::NetworkNotFound(network_name.to_string()))?;

            if network.get_recorder_by_name(recorder).is_none() {
                return Err(MultiNetworkModelProblemBuildError::RecorderNotFound {
                    network: network_name.to_string(),
                    recorder: recorder.to_string(),
                });
            }
        }

        if let Some(constraint) = self.constraints.iter().find(|c| !c.constraint().has_valid_bounds()) {
            return Err(MultiNetworkModelProblemBuildError::InvalidConstraintBounds {
                network: constraint.network().to_string(),
                recorder: constraint.constraint().recorder().to_string(),
            });
        }

        // Check the variable config refers to variable parameters in this model
        self.variables.size(&self.model)?;

        Ok(MultiNetworkModelProblem {
            model: self.model,
            variables: self.variables,
            objectives: self.objectives,
            constraints: self.constraints,
        })
    }
}

/// An optimisation problem defined by a [`MultiNetworkModel`], the variable parameters of its
/// networks, and the objectives and constraints calculated from the results of a model run.
///
/// Each objective and constraint uses a recorder of one of the model's networks (see
/// [`NetworkObjective`] and [`NetworkConstraint`]). Use [`MultiNetworkModelProblemBuilder`] to
/// create a problem.
pub struct MultiNetworkModelProblem {
    model: MultiNetworkModel,
    variables: MultiNetworkVariableConfig,
    objectives: Vec<NetworkObjective>,
    constraints: Vec<NetworkConstraint>,
}

impl MultiNetworkModelProblem {
    pub fn model(&self) -> &MultiNetworkModel {
        &self.model
    }

    pub fn variables(&self) -> &MultiNetworkVariableConfig {
        &self.variables
    }

    pub fn objectives(&self) -> &[NetworkObjective] {
        &self.objectives
    }

    pub fn constraints(&self) -> &[NetworkConstraint] {
        &self.constraints
    }

    /// The number of f64 and u64 variable values required to evaluate the problem.
    pub fn size(&self) -> Result<(usize, usize), NetworkVariableConfigError> {
        self.variables.size(&self.model)
    }

    /// The lower bounds of the variable values.
    pub fn lower_bounds(&self) -> Result<VariableParameterValues, NetworkVariableConfigError> {
        self.variables.lower_bounds(&self.model)
    }

    /// The upper bounds of the variable values.
    pub fn upper_bounds(&self) -> Result<VariableParameterValues, NetworkVariableConfigError> {
        self.variables.upper_bounds(&self.model)
    }

    /// Evaluate the problem by running the model with the given variable values, and returning
    /// the values of the objectives and constraints.
    ///
    /// Each evaluation sets up and runs the model from scratch, so evaluations are independent
    /// and can be run in parallel.
    pub fn evaluate<C>(
        &self,
        solver_config: &C,
        values_f64: &[f64],
        values_u64: &[u64],
    ) -> Result<Evaluation, MultiNetworkModelProblemEvaluateError>
    where
        C: SolverConfig,
    {
        let result = self
            .model
            .run_with_variables(solver_config, &self.variables, values_f64, values_u64)?;

        let objectives = self
            .objectives
            .iter()
            .map(|o| multi_network_aggregated_value(&result, o.network(), o.objective().recorder()))
            .collect::<Result<_, _>>()?;

        let constraints = self
            .constraints
            .iter()
            .map(|c| multi_network_aggregated_value(&result, c.network(), c.constraint().recorder()))
            .collect::<Result<_, _>>()?;

        Ok(Evaluation {
            objectives,
            constraints,
        })
    }
}

fn multi_network_aggregated_value(
    result: &MultiNetworkModelResult,
    network: &str,
    recorder: &str,
) -> Result<f64, MultiNetworkModelProblemEvaluateError> {
    result
        .network_results
        .get(network)
        .ok_or_else(|| MultiNetworkModelProblemEvaluateError::NetworkResultNotFound(network.to_string()))?
        .get(recorder)
        .ok_or_else(|| MultiNetworkModelProblemEvaluateError::RecorderResultNotFound {
            network: network.to_string(),
            recorder: recorder.to_string(),
        })?
        .aggregated_value()
        .map_err(|source| MultiNetworkModelProblemEvaluateError::AggregationError {
            network: network.to_string(),
            recorder: recorder.to_string(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::{
        Constraint, ModelProblem, ModelProblemBuildError, ModelProblemBuilder, MultiNetworkModelProblem,
        MultiNetworkModelProblemBuildError, MultiNetworkModelProblemBuilder, NetworkConstraint, NetworkObjective,
        Objective, ObjectiveSense,
    };
    use crate::agg_funcs::AggFuncF64;
    use crate::metric::UnresolvedMetricF64;
    use crate::models::{
        ModelDomainBuilder, MultiNetworkEntryBuilder, MultiNetworkModelBuilder, MultiNetworkModelSetupError,
    };
    use crate::network::NetworkBuilder;
    use crate::network_variable_config::{
        MultiNetworkVariableConfigBuilder, MultiNetworkVariableConfigBuilderError, NetworkVariableConfigBuilder,
        NetworkVariableConfigError,
    };
    use crate::parameters::ActivationFunction;
    use crate::recorders::{
        Aggregation, AggregationOrder, MemoryRecorderBuilder, MetricSetBuilder, UnresolvedOutputMetric,
    };
    use crate::scenario::{ScenarioDomainBuilder, ScenarioGroupBuilder};
    use crate::solvers::ClpSolverSettings;
    use crate::test_utils::{default_time_domain_builder, simple_model, simple_network};
    use float_cmp::assert_approx_eq;

    /// Add a memory recorder of the mean total demand, "mean-demand", to a simple test network.
    ///
    /// The total demand is 10.0 multiplied by the constant parameter "demand-factor".
    fn add_mean_demand_recorder(network_builder: &mut NetworkBuilder) {
        let mut metric_set = MetricSetBuilder::new("demand");
        metric_set.metric(UnresolvedOutputMetric::new(
            "total-demand",
            "value",
            "parameter",
            None,
            UnresolvedMetricF64::new_parameter_before("total-demand"),
        ));
        network_builder.metric_set(metric_set);

        let recorder = MemoryRecorderBuilder::new(
            "mean-demand",
            "demand",
            Aggregation::new(Some(AggFuncF64::Mean), Some(AggFuncF64::Mean), None),
            AggregationOrder::MetricTimeScenario,
        );
        network_builder.recorder(Box::new(recorder));
    }

    /// A problem builder for a simple model with a memory recorder of the mean total demand, and
    /// a variable config that makes the "demand-factor" constant parameter variable.
    ///
    /// The total demand is 10.0 multiplied by "demand-factor".
    fn test_builder() -> ModelProblemBuilder {
        let mut model_builder = simple_model(2, None);
        add_mean_demand_recorder(model_builder.network_builder());

        let model = model_builder.build().unwrap();

        let variables = NetworkVariableConfigBuilder::new(model.network())
            .add_variable_config(
                &"demand-factor".into(),
                Box::new(ActivationFunction::Unit { min: 0.0, max: 10.0 }),
            )
            .unwrap()
            .build();

        ModelProblemBuilder::new(model, variables)
    }

    #[test]
    fn test_evaluate() {
        let mut builder = test_builder();
        builder
            .objective(Objective::new("mean-demand", ObjectiveSense::Minimise))
            .constraint(Constraint::new("mean-demand", None, Some(6.0)));
        let problem = builder.build().unwrap();

        assert_eq!(problem.size().unwrap(), (1, 0));
        assert_eq!(problem.lower_bounds().unwrap().f64, vec![0.0]);
        assert_eq!(problem.upper_bounds().unwrap().f64, vec![10.0]);

        let solver_config = ClpSolverSettings::default();

        // A demand factor of 0.5 gives a demand of 5.0, which satisfies the constraint
        let evaluation = problem.evaluate(&solver_config, &[0.5], &[]).unwrap();
        assert_approx_eq!(&[f64], &evaluation.objectives, &[5.0]);
        assert_approx_eq!(&[f64], &evaluation.constraints, &[5.0]);
        assert_approx_eq!(f64, problem.constraints()[0].violation(evaluation.constraints[0]), 0.0);

        // A demand factor of 1.0 gives a demand of 10.0, which violates the constraint
        let evaluation = problem.evaluate(&solver_config, &[1.0], &[]).unwrap();
        assert_approx_eq!(&[f64], &evaluation.objectives, &[10.0]);
        assert_approx_eq!(f64, problem.constraints()[0].violation(evaluation.constraints[0]), 4.0);
    }

    #[test]
    fn test_model_problem_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ModelProblem>();
    }

    #[test]
    /// Test that a problem can be evaluated from several threads at once, with each evaluation
    /// returning the result of its own variable values.
    fn test_evaluate_in_parallel() {
        let mut builder = test_builder();
        builder.objective(Objective::new("mean-demand", ObjectiveSense::Minimise));
        let problem = builder.build().unwrap();

        let factors = [0.1, 0.5, 1.0, 2.0];

        let objectives: Vec<f64> = std::thread::scope(|scope| {
            let handles: Vec<_> = factors
                .iter()
                .map(|factor| {
                    let problem = &problem;
                    scope.spawn(move || {
                        let solver_config = ClpSolverSettings::default();
                        problem.evaluate(&solver_config, &[*factor], &[]).unwrap().objectives[0]
                    })
                })
                .collect();

            handles.into_iter().map(|handle| handle.join().unwrap()).collect()
        });

        for (factor, objective) in factors.iter().zip(objectives) {
            assert_approx_eq!(f64, objective, 10.0 * factor);
        }
    }

    #[test]
    fn test_build_errors() {
        let result = test_builder().build();
        assert!(matches!(result, Err(ModelProblemBuildError::NoObjectives)));

        let mut builder = test_builder();
        builder.objective(Objective::new("missing", ObjectiveSense::Minimise));
        assert!(matches!(
            builder.build(),
            Err(ModelProblemBuildError::RecorderNotFound(_))
        ));

        let mut builder = test_builder();
        builder
            .objective(Objective::new("mean-demand", ObjectiveSense::Minimise))
            .constraint(Constraint::new("mean-demand", None, None));
        assert!(matches!(
            builder.build(),
            Err(ModelProblemBuildError::InvalidConstraintBounds(_))
        ));

        let mut builder = test_builder();
        builder
            .objective(Objective::new("mean-demand", ObjectiveSense::Minimise))
            .constraint(Constraint::new("mean-demand", Some(2.0), Some(1.0)));
        assert!(matches!(
            builder.build(),
            Err(ModelProblemBuildError::InvalidConstraintBounds(_))
        ));
    }

    #[test]
    fn test_constraint_violation() {
        let constraint = Constraint::new("c", Some(1.0), Some(2.0));
        assert_approx_eq!(f64, constraint.violation(1.5), 0.0);
        assert_approx_eq!(f64, constraint.violation(0.5), 0.5);
        assert_approx_eq!(f64, constraint.violation(3.0), 1.0);
        assert_eq!(constraint.violation(f64::NAN), f64::INFINITY);

        let constraint = Constraint::new("c", None, Some(2.0));
        assert_approx_eq!(f64, constraint.violation(-100.0), 0.0);
        assert_approx_eq!(f64, constraint.violation(2.5), 0.5);
    }

    /// A problem builder for a multi-network model of two independent simple networks, "network1"
    /// and "network2", each with a memory recorder of its mean total demand.
    ///
    /// The "demand-factor" of "network2" is added as a variable before that of "network1", with
    /// different bounds, so that the order of the values differs from the order of the networks.
    fn test_multi_network_builder() -> MultiNetworkModelProblemBuilder {
        let mut scenario_builder = ScenarioDomainBuilder::default();
        scenario_builder.with_group(ScenarioGroupBuilder::new("test-scenario", 2));
        let mut domain_builder = ModelDomainBuilder::new(default_time_domain_builder());
        domain_builder.scenario(scenario_builder);

        let mut model_builder = MultiNetworkModelBuilder::new(domain_builder.build().unwrap());
        for name in ["network1", "network2"] {
            let mut network_builder = NetworkBuilder::default();
            simple_network(&mut network_builder, "test-scenario", 2);
            add_mean_demand_recorder(&mut network_builder);
            model_builder.network(MultiNetworkEntryBuilder::new(name, network_builder));
        }
        let model = model_builder.build().unwrap();

        let variables = MultiNetworkVariableConfigBuilder::new(&model)
            .add_variable_config(
                "network2",
                &"demand-factor".into(),
                Box::new(ActivationFunction::Unit { min: 0.0, max: 20.0 }),
            )
            .unwrap()
            .add_variable_config(
                "network1",
                &"demand-factor".into(),
                Box::new(ActivationFunction::Unit { min: 0.0, max: 10.0 }),
            )
            .unwrap()
            .build();

        MultiNetworkModelProblemBuilder::new(model, variables)
    }

    #[test]
    fn test_multi_network_evaluate() {
        let mut builder = test_multi_network_builder();
        builder
            .objective(NetworkObjective::new(
                "network1",
                Objective::new("mean-demand", ObjectiveSense::Minimise),
            ))
            .objective(NetworkObjective::new(
                "network2",
                Objective::new("mean-demand", ObjectiveSense::Maximise),
            ))
            .constraint(NetworkConstraint::new(
                "network2",
                Constraint::new("mean-demand", Some(4.0), None),
            ));
        let problem = builder.build().unwrap();

        // The values and bounds are in the order the variables were added (network2 first)
        assert_eq!(problem.size().unwrap(), (2, 0));
        assert_eq!(problem.lower_bounds().unwrap().f64, vec![0.0, 0.0]);
        assert_eq!(problem.upper_bounds().unwrap().f64, vec![20.0, 10.0]);

        let solver_config = ClpSolverSettings::default();

        // The demand factor of network2 is 0.5 and of network1 is 0.2
        let evaluation = problem.evaluate(&solver_config, &[0.5, 0.2], &[]).unwrap();
        assert_approx_eq!(&[f64], &evaluation.objectives, &[2.0, 5.0]);
        assert_approx_eq!(&[f64], &evaluation.constraints, &[5.0]);
        assert_approx_eq!(
            f64,
            problem.constraints()[0]
                .constraint()
                .violation(evaluation.constraints[0]),
            0.0
        );

        // A demand of 3.0 in network2 violates its constraint
        let evaluation = problem.evaluate(&solver_config, &[0.3, 0.2], &[]).unwrap();
        assert_approx_eq!(&[f64], &evaluation.objectives, &[2.0, 3.0]);
        assert_approx_eq!(
            f64,
            problem.constraints()[0]
                .constraint()
                .violation(evaluation.constraints[0]),
            1.0
        );
    }

    #[test]
    fn test_multi_network_variable_values() {
        let mut builder = test_multi_network_builder();
        builder.objective(NetworkObjective::new(
            "network1",
            Objective::new("mean-demand", ObjectiveSense::Minimise),
        ));
        let problem = builder.build().unwrap();

        let state = problem
            .model()
            .setup_with_variables(&ClpSolverSettings::default(), problem.variables(), &[0.5, 0.2], &[])
            .unwrap();

        let values = problem
            .variables()
            .values(problem.model(), state.network_states())
            .unwrap();

        // The values of each scenario are in the order the variables were added
        assert_eq!(values.len(), 2);
        for scenario_values in values {
            assert_approx_eq!(&[f64], &scenario_values.unwrap().f64, &[0.5, 0.2]);
        }
    }

    #[test]
    fn test_multi_network_variable_config_with_different_model() {
        // The variable config is built for a model with two networks
        let problem_builder = test_multi_network_builder();

        // A model with only one network
        let mut domain_builder = ModelDomainBuilder::new(default_time_domain_builder());
        let mut scenario_builder = ScenarioDomainBuilder::default();
        scenario_builder.with_group(ScenarioGroupBuilder::new("test-scenario", 1));
        domain_builder.scenario(scenario_builder);
        let mut model_builder = MultiNetworkModelBuilder::new(domain_builder.build().unwrap());
        let mut network_builder = NetworkBuilder::default();
        simple_network(&mut network_builder, "test-scenario", 1);
        model_builder.network(MultiNetworkEntryBuilder::new("network1", network_builder));
        let model = model_builder.build().unwrap();

        let result = problem_builder.variables.size(&model);
        assert!(matches!(
            result,
            Err(NetworkVariableConfigError::IncorrectNumberOfNetworks {
                expected: 2,
                received: 1
            })
        ));

        let result = model.setup_with_variables(
            &ClpSolverSettings::default(),
            &problem_builder.variables,
            &[0.5, 0.2],
            &[],
        );
        assert!(matches!(
            result,
            Err(MultiNetworkModelSetupError::VariableConfigError(_))
        ));

        // The states of only one network
        let state = problem_builder
            .model
            .setup_with_variables(
                &ClpSolverSettings::default(),
                &problem_builder.variables,
                &[0.5, 0.2],
                &[],
            )
            .unwrap();
        let result = problem_builder
            .variables
            .values(&problem_builder.model, &state.network_states()[..1]);
        assert!(matches!(
            result,
            Err(NetworkVariableConfigError::IncorrectNumberOfNetworkStates {
                expected: 2,
                received: 1
            })
        ));
    }

    #[test]
    fn test_multi_network_model_problem_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MultiNetworkModelProblem>();
    }

    #[test]
    fn test_multi_network_build_errors() {
        let mut builder = test_multi_network_builder();
        builder.objective(NetworkObjective::new(
            "missing",
            Objective::new("mean-demand", ObjectiveSense::Minimise),
        ));
        assert!(matches!(
            builder.build(),
            Err(MultiNetworkModelProblemBuildError::NetworkNotFound(_))
        ));

        let mut builder = test_multi_network_builder();
        builder.objective(NetworkObjective::new(
            "network1",
            Objective::new("missing", ObjectiveSense::Minimise),
        ));
        assert!(matches!(
            builder.build(),
            Err(MultiNetworkModelProblemBuildError::RecorderNotFound { .. })
        ));

        let mut builder = test_multi_network_builder();
        builder
            .objective(NetworkObjective::new(
                "network1",
                Objective::new("mean-demand", ObjectiveSense::Minimise),
            ))
            .constraint(NetworkConstraint::new(
                "network2",
                Constraint::new("mean-demand", None, None),
            ));
        assert!(matches!(
            builder.build(),
            Err(MultiNetworkModelProblemBuildError::InvalidConstraintBounds { .. })
        ));

        let builder = test_multi_network_builder();
        let result = MultiNetworkVariableConfigBuilder::new(&builder.model).add_variable_config(
            "missing",
            &"demand-factor".into(),
            Box::new(ActivationFunction::Unit { min: 0.0, max: 10.0 }),
        );
        assert!(matches!(
            result,
            Err(MultiNetworkVariableConfigBuilderError::NetworkNotFound(_))
        ));
    }
}
