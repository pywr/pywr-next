use crate::metric::Metric;
use crate::models::{Model, ModelDomain};
/// Utilities for unit tests.
/// TODO move this to its own local crate ("test-utilities") as part of a workspace.
use crate::network::Network;
use crate::node::{Constraint, ConstraintValue, StorageInitialVolume};
use crate::parameters::{AggFunc, AggregatedParameter, Array2Parameter, ConstantParameter, Parameter};
use crate::recorders::AssertionRecorder;
use crate::scenario::ScenarioGroupCollection;
#[cfg(feature = "ipm-ocl")]
use crate::solvers::ClIpmF64Solver;
use crate::solvers::ClpSolver;
#[cfg(feature = "highs")]
use crate::solvers::HighsSolver;
#[cfg(feature = "ipm-simd")]
use crate::solvers::SimdIpmF64Solver;
use crate::timestep::{TimeDomain, TimestepDuration, Timestepper};
use crate::PywrError;
use chrono::{Days, NaiveDate};
use float_cmp::{approx_eq, F64Margin};
use ndarray::{Array, Array2};
use rand::Rng;
use rand_distr::{Distribution, Normal};

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

/// Create a simple test network with three nodes.
pub fn simple_network(network: &mut Network, inflow_scenario_index: usize, num_inflow_scenarios: usize) {
    let input_node = network.add_input_node("input", None).unwrap();
    let link_node = network.add_link_node("link", None).unwrap();
    let output_node = network.add_output_node("output", None).unwrap();

    network.connect_nodes(input_node, link_node).unwrap();
    network.connect_nodes(link_node, output_node).unwrap();

    let inflow = Array::from_shape_fn((366, num_inflow_scenarios), |(i, j)| 1.0 + i as f64 + j as f64);
    let inflow = Array2Parameter::new("inflow", inflow, inflow_scenario_index, None);

    let inflow = network.add_parameter(Box::new(inflow)).unwrap();

    let input_node = network.get_mut_node_by_name("input", None).unwrap();
    input_node
        .set_constraint(
            ConstraintValue::Metric(Metric::ParameterValue(inflow)),
            Constraint::MaxFlow,
        )
        .unwrap();

    let base_demand = 10.0;

    let demand_factor = ConstantParameter::new("demand-factor", 1.2);
    let demand_factor = network.add_parameter(Box::new(demand_factor)).unwrap();

    let total_demand = AggregatedParameter::new(
        "total-demand",
        &[Metric::Constant(base_demand), Metric::ParameterValue(demand_factor)],
        AggFunc::Product,
    );
    let total_demand = network.add_parameter(Box::new(total_demand)).unwrap();

    let demand_cost = ConstantParameter::new("demand-cost", -10.0);
    let demand_cost = network.add_parameter(Box::new(demand_cost)).unwrap();

    let output_node = network.get_mut_node_by_name("output", None).unwrap();
    output_node
        .set_constraint(
            ConstraintValue::Metric(Metric::ParameterValue(total_demand)),
            Constraint::MaxFlow,
        )
        .unwrap();
    output_node.set_cost(ConstraintValue::Metric(Metric::ParameterValue(demand_cost)));
}
/// Create a simple test model with three nodes.
pub fn simple_model(num_scenarios: usize) -> Model {
    let mut scenario_collection = ScenarioGroupCollection::default();
    scenario_collection.add_group("test-scenario", num_scenarios);

    let domain = ModelDomain::from(default_timestepper(), scenario_collection).unwrap();
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
            ConstraintValue::Scalar(0.0),
            ConstraintValue::Scalar(100.0),
        )
        .unwrap();
    let output_node = network.add_output_node("output", None).unwrap();

    network.connect_nodes(storage_node, output_node).unwrap();

    // Apply demand to the model
    // TODO convenience function for adding a constant constraint.
    let demand = ConstantParameter::new("demand", 10.0);
    let demand = network.add_parameter(Box::new(demand)).unwrap();

    let demand_cost = ConstantParameter::new("demand-cost", -10.0);
    let demand_cost = network.add_parameter(Box::new(demand_cost)).unwrap();

    let output_node = network.get_mut_node_by_name("output", None).unwrap();
    output_node
        .set_constraint(
            ConstraintValue::Metric(Metric::ParameterValue(demand)),
            Constraint::MaxFlow,
        )
        .unwrap();
    output_node.set_cost(ConstraintValue::Metric(Metric::ParameterValue(demand_cost)));

    Model::new(default_time_domain().into(), network)
}

/// Add the given parameter to the given model along with an assertion recorder that asserts
/// whether the parameter returns the expected values when the model is run.
///
/// This function will run a number of time-steps equal to the number of rows in the expected
/// values array.
///
/// See [`AssertionRecorder`] for more information.
pub fn run_and_assert_parameter(
    model: &mut Model,
    parameter: Box<dyn Parameter>,
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

    let rec = AssertionRecorder::new("assert", Metric::ParameterValue(p_idx), expected_values, ulps, epsilon);

    model.network_mut().add_recorder(Box::new(rec)).unwrap();
    run_all_solvers(model)
}

/// Run a model using each of the in-built solvers.
///
/// The model will only be run if the solver has the required solver features (and
/// is also enabled as a Cargo feature).
pub fn run_all_solvers(model: &Model) {
    model
        .run::<ClpSolver>(&Default::default())
        .expect("Failed to solve with CLP");

    #[cfg(feature = "highs")]
    {
        if model.check_solver_features::<HighsSolver>() {
            model
                .run::<HighsSolver>(&Default::default())
                .expect("Failed to solve with Highs");
        }
    }

    #[cfg(feature = "ipm-simd")]
    {
        if model.check_multi_scenario_solver_features::<SimdIpmF64Solver<4>>() {
            model
                .run_multi_scenario::<SimdIpmF64Solver<4>>(&Default::default())
                .expect("Failed to solve with SIMD IPM");
        }
    }

    #[cfg(feature = "ipm-ocl")]
    {
        if model.check_multi_scenario_solver_features::<ClIpmF64Solver>() {
            model
                .run_multi_scenario::<ClIpmF64Solver>(&Default::default())
                .expect("Failed to solve with OpenCl IPM");
        }
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
) -> Result<(), PywrError> {
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
    let inflow = Array2Parameter::new(&format!("inflow-{suffix}"), inflow, inflow_scenario_group_index, None);
    let idx = network.add_parameter(Box::new(inflow))?;

    network.set_node_max_flow(
        "input",
        Some(suffix),
        ConstraintValue::Metric(Metric::ParameterValue(idx)),
    )?;

    let input_cost = rng.gen_range(-20.0..-5.00);
    network.set_node_cost("input", Some(suffix), ConstraintValue::Scalar(input_cost))?;

    let outflow_distr = Normal::new(8.0, 3.0).unwrap();
    let mut outflow: f64 = outflow_distr.sample(rng);
    outflow = outflow.max(0.0);

    network.set_node_max_flow("output", Some(suffix), ConstraintValue::Scalar(outflow))?;

    network.set_node_cost("output", Some(suffix), ConstraintValue::Scalar(-500.0))?;

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
) -> Result<(), PywrError> {
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
            model.set_node_cost("transfer", Some(&name), ConstraintValue::Scalar(transfer_cost))?;

            let from_suffix = format!("sys-{i:04}");
            let from_idx = model.get_node_index_by_name("link", Some(&from_suffix))?;
            let to_suffix = format!("sys-{j:04}");
            let to_idx = model.get_node_index_by_name("link", Some(&to_suffix))?;

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
) -> Result<Model, PywrError> {
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

    let mut scenario_collection = ScenarioGroupCollection::default();
    scenario_collection.add_group("test-scenario", num_scenarios);

    let domain = ModelDomain::from(timestepper, scenario_collection).unwrap();

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
            .run_multi_scenario::<SimdIpmF64Solver<4>>(&settings)
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
