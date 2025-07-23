use crate::metric::MetricF64;
use crate::models::{Model, ModelDomain};
/// Utilities for unit tests.
/// TODO move this to its own local crate ("test-utilities") as part of a workspace.
use crate::network::{Network, NetworkError};
use crate::node::StorageInitialVolume;
use crate::parameters::{AggFunc, AggregatedParameter, Array2Parameter, ConstantParameter, GeneralParameter};
use crate::recorders::{AssertionF64Recorder, AssertionU64Recorder};
use crate::scenario::{ScenarioDomain, ScenarioDomainBuilder, ScenarioGroupBuilder};
#[cfg(feature = "cbc")]
use crate::solvers::CbcSolver;
#[cfg(feature = "ipm-ocl")]
use crate::solvers::ClIpmF64Solver;
#[cfg(feature = "highs")]
use crate::solvers::HighsSolver;
#[cfg(any(feature = "ipm-simd", feature = "ipm-ocl"))]
use crate::solvers::MultiStateSolver;
#[cfg(feature = "ipm-simd")]
use crate::solvers::SimdIpmF64Solver;
use crate::solvers::{ClpSolver, Solver, SolverSettings};
use crate::timestep::{TimeDomain, TimestepDuration, Timestepper};
use chrono::{Days, NaiveDate};
use float_cmp::{F64Margin, approx_eq};
use ndarray::{Array, Array2};
use rand::Rng;
use rand_distr::{Distribution, Normal};
use std::path::PathBuf;

pub fn default_timestepper() -> Timestepper {
    let start = NaiveDate::from_ymd_opt(2020, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let end = NaiveDate::from_ymd_opt(2020, 1, 15)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let duration = TimestepDuration::Days(1);
    Timestepper::new(start, end, duration)
}

pub fn default_time_domain() -> TimeDomain {
    default_timestepper().try_into().unwrap()
}

pub fn default_domain() -> ModelDomain {
    default_time_domain().into()
}

pub fn default_model() -> Model {
    let domain = default_domain();
    let network = Network::default();
    Model::new(domain, network)
}

/// Create a test scenario domain with a single scenario group containing the specified number of scenarios.
pub fn test_scenario_domain(num_scenarios: usize) -> ScenarioDomain {
    let mut scenario_builder = ScenarioDomainBuilder::default();
    let scenario_group = ScenarioGroupBuilder::new("test-scenario", num_scenarios)
        .build()
        .unwrap();
    scenario_builder = scenario_builder.with_group(scenario_group).unwrap();

    scenario_builder.build().expect("Failed to build Scenario domain.")
}

/// Create a simple test network with three nodes.
pub fn simple_network(network: &mut Network, inflow_scenario_index: usize, num_inflow_scenarios: usize) {
    let input_node = network.add_input_node("input", None).unwrap();
    let link_node = network.add_link_node("link", None).unwrap();
    let output_node = network.add_output_node("output", None).unwrap();

    network.connect_nodes(input_node, link_node).unwrap();
    network.connect_nodes(link_node, output_node).unwrap();

    let inflow = Array::from_shape_fn((366, num_inflow_scenarios), |(i, j)| 1.0 + i as f64 + j as f64);
    let inflow = Array2Parameter::new("inflow".into(), inflow, inflow_scenario_index, None);

    let inflow = network.add_simple_parameter(Box::new(inflow)).unwrap();

    let input_node = network.get_mut_node_by_name("input", None).unwrap();
    input_node.set_max_flow_constraint(Some(inflow.into())).unwrap();

    let base_demand = 10.0;

    let demand_factor = ConstantParameter::new("demand-factor".into(), 1.2);
    let demand_factor = network.add_const_parameter(Box::new(demand_factor)).unwrap();

    let total_demand: AggregatedParameter<MetricF64> = AggregatedParameter::new(
        "total-demand".into(),
        &[base_demand.into(), demand_factor.into()],
        AggFunc::Product,
    );
    let total_demand = network.add_parameter(Box::new(total_demand)).unwrap();

    let demand_cost = ConstantParameter::new("demand-cost".into(), -10.0);
    let demand_cost = network.add_const_parameter(Box::new(demand_cost)).unwrap();

    let output_node = network.get_mut_node_by_name("output", None).unwrap();
    output_node.set_max_flow_constraint(Some(total_demand.into())).unwrap();
    output_node.set_cost(Some(demand_cost.into()));
}
/// Create a simple test model with three nodes.
pub fn simple_model(num_scenarios: usize, timestepper: Option<Timestepper>) -> Model {
    let mut scenario_builder = ScenarioDomainBuilder::default();
    let scenario_group = ScenarioGroupBuilder::new("test-scenario", num_scenarios)
        .build()
        .unwrap();
    scenario_builder = scenario_builder.with_group(scenario_group).unwrap();

    let domain = ModelDomain::from(timestepper.unwrap_or_else(default_timestepper), scenario_builder).unwrap();
    let mut network = Network::default();

    let idx = domain
        .scenarios()
        .group_index("test-scenario")
        .expect("Could not find scenario group");

    simple_network(&mut network, idx, num_scenarios);

    Model::new(domain, network)
}

/// A test model with a single storage node.
pub fn simple_storage_model() -> Model {
    let mut network = Network::default();
    let storage_node = network
        .add_storage_node(
            "reservoir",
            None,
            StorageInitialVolume::Absolute(100.0),
            None,
            Some(100.0.into()),
        )
        .unwrap();
    let output_node = network.add_output_node("output", None).unwrap();

    network.connect_nodes(storage_node, output_node).unwrap();

    // Apply demand to the model
    // TODO convenience function for adding a constant constraint.
    let demand = ConstantParameter::new("demand".into(), 10.0);
    let demand = network.add_const_parameter(Box::new(demand)).unwrap();

    let demand_cost = ConstantParameter::new("demand-cost".into(), -10.0);
    let demand_cost = network.add_const_parameter(Box::new(demand_cost)).unwrap();

    let output_node = network.get_mut_node_by_name("output", None).unwrap();
    output_node.set_max_flow_constraint(Some(demand.into())).unwrap();
    output_node.set_cost(Some(demand_cost.into()));

    Model::new(default_time_domain().into(), network)
}

/// Add the given parameter to the given model along with an assertion recorder that asserts
/// whether the parameter returns the expected values when the model is run.
///
/// This function will run a number of time-steps equal to the number of rows in the expected
/// values array.
///
/// See [`AssertionF64Recorder`] for more information.
pub fn run_and_assert_parameter(
    model: &mut Model,
    parameter: Box<dyn GeneralParameter<f64>>,
    expected_values: Array2<f64>,
    ulps: Option<i64>,
    epsilon: Option<f64>,
) {
    let p_idx = model.network_mut().add_parameter(parameter).unwrap();

    let start = NaiveDate::from_ymd_opt(2020, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let _end = start
        .checked_add_days(Days::new(expected_values.nrows() as u64 - 1))
        .unwrap();

    let rec = AssertionF64Recorder::new("assert", p_idx.into(), expected_values, ulps, epsilon);

    model.network_mut().add_recorder(Box::new(rec)).unwrap();
    run_all_solvers(model, &[], &[], &[])
}

/// Add the given parameter to the given model along with an assertion recorder that asserts
/// whether the parameter returns the expected values when the model is run.
///
/// This function will run a number of time-steps equal to the number of rows in the expected
/// values array.
///
/// See [`AssertionU64Recorder`] for more information.
pub fn run_and_assert_parameter_u64(
    model: &mut Model,
    parameter: Box<dyn GeneralParameter<u64>>,
    expected_values: Array2<u64>,
) {
    let p_idx = model.network_mut().add_index_parameter(parameter).unwrap();

    let start = NaiveDate::from_ymd_opt(2020, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let _end = start
        .checked_add_days(Days::new(expected_values.nrows() as u64 - 1))
        .unwrap();

    let rec = AssertionU64Recorder::new("assert", p_idx.into(), expected_values);

    model.network_mut().add_recorder(Box::new(rec)).unwrap();
    run_all_solvers(model, &[], &[], &[])
}

/// A struct to hold the expected outputs for a test.
pub struct ExpectedOutputs {
    output_path: PathBuf,
    expected_str: String,
}

impl ExpectedOutputs {
    pub fn new(output_path: PathBuf, expected_str: String) -> Self {
        Self {
            output_path,
            expected_str,
        }
    }

    fn verify(&self) {
        assert!(
            self.output_path.exists(),
            "Output file does not exist: {:?}",
            self.output_path
        );
        let actual_str = std::fs::read_to_string(&self.output_path).unwrap();
        assert_eq!(actual_str, self.expected_str, "Output file contents do not match");
    }
}

/// Run a model using each of the in-built solvers.
///
/// The model will only be run if the solver has the required solver features (and
/// is also enabled as a Cargo feature).
pub fn run_all_solvers(
    model: &Model,
    solvers_without_features: &[&str],
    solvers_to_skip: &[&str],
    expected_outputs: &[ExpectedOutputs],
) {
    if !solvers_to_skip.contains(&"clp") {
        check_features_and_run::<ClpSolver>(model, !solvers_without_features.contains(&"clp"), expected_outputs);
    }

    #[cfg(feature = "cbc")]
    {
        if !solvers_to_skip.contains(&"cbc") {
            check_features_and_run::<CbcSolver>(model, !solvers_without_features.contains(&"cbc"), expected_outputs);
        }
    }

    #[cfg(feature = "highs")]
    {
        if !solvers_to_skip.contains(&"highs") {
            check_features_and_run::<HighsSolver>(
                model,
                !solvers_without_features.contains(&"highs"),
                expected_outputs,
            );
        }
    }

    #[cfg(feature = "ipm-simd")]
    {
        if !solvers_to_skip.contains(&"ipm-simd") {
            check_features_and_run_multi::<SimdIpmF64Solver>(model, !solvers_without_features.contains(&"ipm-simd"));
        }
    }

    #[cfg(feature = "ipm-ocl")]
    {
        if !solvers_to_skip.contains(&"ipm-ocl") {
            check_features_and_run_multi::<ClIpmF64Solver>(model, !solvers_without_features.contains(&"ipm-ocl"));
        }
    }
}

/// Check features and
fn check_features_and_run<S>(model: &Model, expect_features: bool, expected_outputs: &[ExpectedOutputs])
where
    S: Solver,
    <S as Solver>::Settings: SolverSettings + Default,
{
    let has_features = model.check_solver_features::<S>();
    if expect_features {
        assert!(
            has_features,
            "Solver `{}` was expected to have the required features",
            S::name()
        );
        model
            .run::<S>(&Default::default())
            .unwrap_or_else(|e| panic!("Failed to solve with {}: {}", S::name(), e));

        // Verify any expected outputs
        for expected_output in expected_outputs {
            expected_output.verify();
        }
    } else {
        assert!(
            !has_features,
            "Solver `{}` was not expected to have the required features",
            S::name()
        );
    }
}

/// Check features and run with a multi-scenario solver
#[cfg(any(feature = "ipm-simd", feature = "ipm-ocl"))]
fn check_features_and_run_multi<S>(model: &Model, expect_features: bool)
where
    S: MultiStateSolver,
    <S as MultiStateSolver>::Settings: SolverSettings + Default,
{
    let has_features = model.check_multi_scenario_solver_features::<S>();
    if expect_features {
        assert!(
            has_features,
            "Solver `{}` was expected to have the required features",
            S::name()
        );
        model
            .run_multi_scenario::<S>(&Default::default())
            .unwrap_or_else(|_| panic!("Failed to solve with: {}", S::name()));
    } else {
        assert!(
            !has_features,
            "Solver `{}` was not expected to have the required features",
            S::name()
        );
    }
}

/// Make a simple system with random inputs.
fn make_simple_system<R: Rng>(
    network: &mut Network,
    suffix: &str,
    num_timesteps: usize,
    num_inflow_scenarios: usize,
    inflow_scenario_group_index: usize,
    rng: &mut R,
) -> Result<(), NetworkError> {
    let input_idx = network.add_input_node("input", Some(suffix))?;
    let link_idx = network.add_link_node("link", Some(suffix))?;
    let output_idx = network.add_output_node("output", Some(suffix))?;

    network.connect_nodes(input_idx, link_idx)?;
    network.connect_nodes(link_idx, output_idx)?;

    let inflow_distr: Normal<f64> = Normal::new(9.0, 1.0).unwrap();

    let mut inflow = ndarray::Array2::zeros((num_timesteps, num_inflow_scenarios));

    for x in inflow.iter_mut() {
        *x = inflow_distr.sample(rng).max(0.0);
    }
    let inflow = Array2Parameter::new(
        format!("inflow-{suffix}").as_str().into(),
        inflow,
        inflow_scenario_group_index,
        None,
    );
    let idx = network.add_simple_parameter(Box::new(inflow))?;

    network.set_node_max_flow("input", Some(suffix), Some(idx.into()))?;

    let input_cost = rng.gen_range(-20.0..-5.00);
    network.set_node_cost("input", Some(suffix), Some(input_cost.into()))?;

    let outflow_distr = Normal::new(8.0, 3.0).unwrap();
    let mut outflow: f64 = outflow_distr.sample(rng);
    outflow = outflow.max(0.0);

    network.set_node_max_flow("output", Some(suffix), Some(outflow.into()))?;

    network.set_node_cost("output", Some(suffix), Some((-500.0).into()))?;

    Ok(())
}

/// Make a simple connections between random systems
///
///
fn make_simple_connections<R: Rng>(
    model: &mut Network,
    num_systems: usize,
    density: usize,
    rng: &mut R,
) -> Result<(), NetworkError> {
    let num_connections = (num_systems.pow(2) * density / 100 / 2).max(1);

    let mut connections_added: usize = 0;

    while connections_added < num_connections {
        let i = rng.gen_range(0..num_systems);
        let j = rng.gen_range(0..num_systems);

        if i == j {
            continue;
        }

        let name = format!("{i:04}->{j:04}");

        if let Ok(idx) = model.add_link_node("transfer", Some(&name)) {
            let transfer_cost = rng.gen_range(0.0..1.0);
            model.set_node_cost("transfer", Some(&name), Some(transfer_cost.into()))?;

            let from_suffix = format!("sys-{i:04}");
            let from_idx = model
                .get_node_index_by_name("link", Some(&from_suffix))
                .expect("missing link node");
            let to_suffix = format!("sys-{j:04}");
            let to_idx = model
                .get_node_index_by_name("link", Some(&to_suffix))
                .expect("missing link node");

            model.connect_nodes(from_idx, idx)?;
            model.connect_nodes(idx, to_idx)?;

            connections_added += 1;
        }
    }

    Ok(())
}

pub fn make_random_model<R: Rng>(
    num_systems: usize,
    density: usize,
    num_scenarios: usize,
    rng: &mut R,
) -> Result<Model, NetworkError> {
    let start = NaiveDate::from_ymd_opt(2020, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let end = NaiveDate::from_ymd_opt(2020, 4, 9)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let duration = TimestepDuration::Days(1);
    let timestepper = Timestepper::new(start, end, duration);

    let mut scenario_builder = ScenarioDomainBuilder::default();
    let scenario_group = ScenarioGroupBuilder::new("test-scenario", num_scenarios)
        .build()
        .expect("Could not create scenario group");
    scenario_builder = scenario_builder
        .with_group(scenario_group)
        .expect("Could not add scenario group");

    let domain = ModelDomain::from(timestepper, scenario_builder).expect("Could not create model domain");

    let inflow_scenario_group_index = domain
        .scenarios()
        .group_index("test-scenario")
        .expect("Could not find scenario group.");

    let (num_timesteps, num_inflow_scenarios) = domain.shape();

    let mut network = Network::default();
    for i in 0..num_systems {
        let suffix = format!("sys-{i:04}");
        make_simple_system(
            &mut network,
            &suffix,
            num_timesteps,
            num_inflow_scenarios,
            inflow_scenario_group_index,
            rng,
        )?;
    }

    make_simple_connections(&mut network, num_systems, density, rng)?;

    let model = Model::new(domain, network);

    Ok(model)
}

#[cfg(all(test, feature = "ipm-simd"))]
mod tests {
    use super::make_random_model;
    use crate::solvers::{SimdIpmF64Solver, SimdIpmSolverSettings};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    #[test]
    fn test_random_model() {
        let n_sys = 50;
        let density = 5;
        let n_sc = 12;

        // Make a consistent random number generator
        // ChaCha8 should be consistent across builds and platforms
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let model = make_random_model(n_sys, density, n_sc, &mut rng).unwrap();

        let settings = SimdIpmSolverSettings::default();
        model
            .run_multi_scenario::<SimdIpmF64Solver>(&settings)
            .expect("Failed to run model!");
    }
}

/// Compare two arrays of f64
pub fn assert_approx_array_eq(calculated_values: &[f64], expected_values: &[f64]) {
    let margins = F64Margin {
        epsilon: 2.0,
        ulps: (f64::EPSILON * 2.0) as i64,
    };
    for (i, (calculated, expected)) in calculated_values.iter().zip(expected_values).enumerate() {
        if !approx_eq!(f64, *calculated, *expected, margins) {
            panic!(
                r#"assertion failed on item #{i:?}
                    actual: `{calculated:?}`,
                    expected: `{expected:?}`"#,
            )
        }
    }
}
