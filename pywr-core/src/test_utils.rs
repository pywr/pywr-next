use crate::metric::Metric;
/// Utilities for unit tests.
/// TODO move this to its own local crate ("test-utilities") as part of a workspace.
use crate::model::Model;
use crate::node::{Constraint, ConstraintValue, StorageInitialVolume};
use crate::parameters::{AggFunc, AggregatedParameter, Array2Parameter, ConstantParameter, Parameter};
use crate::recorders::AssertionRecorder;
#[cfg(feature = "ipm-ocl")]
use crate::solvers::ClIpmF64Solver;
use crate::solvers::ClpSolver;
#[cfg(feature = "highs")]
use crate::solvers::HighsSolver;
#[cfg(feature = "ipm-simd")]
use crate::solvers::SimdIpmF64Solver;
use crate::timestep::Timestepper;
use crate::PywrError;
use ndarray::{Array, Array2};
use rand::Rng;
use rand_distr::{Distribution, Normal};
use time::ext::NumericalDuration;
use time::macros::date;

pub fn default_timestepper() -> Timestepper {
    Timestepper::new(date!(2020 - 01 - 01), date!(2020 - 01 - 15), 1)
}

/// Create a simple test model with three nodes.
pub fn simple_model(num_scenarios: usize) -> Model {
    let mut model = Model::default();
    model.add_scenario_group("test-scenario", num_scenarios).unwrap();
    let scenario_idx = model.get_scenario_group_index_by_name("test-scenario").unwrap();

    let input_node = model.add_input_node("input", None).unwrap();
    let link_node = model.add_link_node("link", None).unwrap();
    let output_node = model.add_output_node("output", None).unwrap();

    model.connect_nodes(input_node, link_node).unwrap();
    model.connect_nodes(link_node, output_node).unwrap();

    let inflow = Array::from_shape_fn((366, num_scenarios), |(i, j)| 1.0 + i as f64 + j as f64);
    let inflow = Array2Parameter::new("inflow", inflow, scenario_idx);

    let inflow = model.add_parameter(Box::new(inflow)).unwrap();

    let input_node = model.get_mut_node_by_name("input", None).unwrap();
    input_node
        .set_constraint(
            ConstraintValue::Metric(Metric::ParameterValue(inflow)),
            Constraint::MaxFlow,
        )
        .unwrap();

    let base_demand = 10.0;

    let demand_factor = ConstantParameter::new("demand-factor", 1.2, None);
    let demand_factor = model.add_parameter(Box::new(demand_factor)).unwrap();

    let total_demand = AggregatedParameter::new(
        "total-demand",
        &[Metric::Constant(base_demand), Metric::ParameterValue(demand_factor)],
        AggFunc::Product,
    );
    let total_demand = model.add_parameter(Box::new(total_demand)).unwrap();

    let demand_cost = ConstantParameter::new("demand-cost", -10.0, None);
    let demand_cost = model.add_parameter(Box::new(demand_cost)).unwrap();

    let output_node = model.get_mut_node_by_name("output", None).unwrap();
    output_node
        .set_constraint(
            ConstraintValue::Metric(Metric::ParameterValue(total_demand)),
            Constraint::MaxFlow,
        )
        .unwrap();
    output_node.set_cost(ConstraintValue::Metric(Metric::ParameterValue(demand_cost)));

    model
}

/// A test model with a single storage node.
pub fn simple_storage_model() -> Model {
    let mut model = Model::default();

    let storage_node = model
        .add_storage_node(
            "reservoir",
            None,
            StorageInitialVolume::Absolute(100.0),
            ConstraintValue::Scalar(0.0),
            ConstraintValue::Scalar(100.0),
        )
        .unwrap();
    let output_node = model.add_output_node("output", None).unwrap();

    model.connect_nodes(storage_node, output_node).unwrap();

    // Apply demand to the model
    // TODO convenience function for adding a constant constraint.
    let demand = ConstantParameter::new("demand", 10.0, None);
    let demand = model.add_parameter(Box::new(demand)).unwrap();

    let demand_cost = ConstantParameter::new("demand-cost", -10.0, None);
    let demand_cost = model.add_parameter(Box::new(demand_cost)).unwrap();

    let output_node = model.get_mut_node_by_name("output", None).unwrap();
    output_node
        .set_constraint(
            ConstraintValue::Metric(Metric::ParameterValue(demand)),
            Constraint::MaxFlow,
        )
        .unwrap();
    output_node.set_cost(ConstraintValue::Metric(Metric::ParameterValue(demand_cost)));

    let max_volume = 100.0;

    let storage_node = model.get_mut_node_by_name("reservoir", None).unwrap();
    storage_node
        .set_constraint(ConstraintValue::Scalar(max_volume), Constraint::MaxVolume)
        .unwrap();

    model
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
    let p_idx = model.add_parameter(parameter).unwrap();

    let start = date!(2020 - 01 - 01);
    let end = start.checked_add((expected_values.nrows() as i64 - 1).days()).unwrap();
    let timestepper = Timestepper::new(start, end, 1);

    let rec = AssertionRecorder::new("assert", Metric::ParameterValue(p_idx), expected_values, ulps, epsilon);

    model.add_recorder(Box::new(rec)).unwrap();
    run_all_solvers(model, &timestepper)
}

/// Run a model using each of the in-built solvers.
///
/// The model will only be run if the solver has the required solver features (and
/// is also enabled as a Cargo feature).
pub fn run_all_solvers(model: &Model, timestepper: &Timestepper) {
    model
        .run::<ClpSolver>(timestepper, &Default::default())
        .expect("Failed to solve with CLP");

    #[cfg(feature = "highs")]
    {
        if model.check_solver_features::<HighsSolver>() {
            model
                .run::<HighsSolver>(timestepper, &Default::default())
                .expect("Failed to solve with Highs");
        }
    }

    #[cfg(feature = "ipm-simd")]
    {
        if model.check_multi_scenario_solver_features::<SimdIpmF64Solver<4>>() {
            model
                .run_multi_scenario::<SimdIpmF64Solver<4>>(timestepper, &Default::default())
                .expect("Failed to solve with SIMD IPM");
        }
    }

    #[cfg(feature = "ipm-ocl")]
    {
        if model.check_multi_scenario_solver_features::<ClIpmF64Solver>() {
            model
                .run_multi_scenario::<ClIpmF64Solver>(timestepper, &Default::default())
                .expect("Failed to solve with OpenCl IPM");
        }
    }
}

/// Make a simple system with random inputs.
fn make_simple_system<R: Rng>(
    model: &mut Model,
    suffix: &str,
    num_timesteps: usize,
    rng: &mut R,
) -> Result<(), PywrError> {
    let input_idx = model.add_input_node("input", Some(suffix))?;
    let link_idx = model.add_link_node("link", Some(suffix))?;
    let output_idx = model.add_output_node("output", Some(suffix))?;

    model.connect_nodes(input_idx, link_idx)?;
    model.connect_nodes(link_idx, output_idx)?;

    let num_scenarios = model.get_scenario_group_size_by_name("test-scenario")?;
    let scenario_group_index = model.get_scenario_group_index_by_name("test-scenario")?;

    let inflow_distr: Normal<f64> = Normal::new(9.0, 1.0).unwrap();

    let mut inflow = ndarray::Array2::zeros((num_timesteps, num_scenarios));

    for x in inflow.iter_mut() {
        *x = inflow_distr.sample(rng).max(0.0);
    }
    let inflow = Array2Parameter::new(&format!("inflow-{suffix}"), inflow, scenario_group_index);
    let idx = model.add_parameter(Box::new(inflow))?;

    model.set_node_max_flow(
        "input",
        Some(suffix),
        ConstraintValue::Metric(Metric::ParameterValue(idx)),
    )?;

    let input_cost = rng.gen_range(-20.0..-5.00);
    model.set_node_cost("input", Some(suffix), ConstraintValue::Scalar(input_cost))?;

    let outflow_distr = Normal::new(8.0, 3.0).unwrap();
    let mut outflow: f64 = outflow_distr.sample(rng);
    outflow = outflow.max(0.0);

    model.set_node_max_flow("output", Some(suffix), ConstraintValue::Scalar(outflow))?;

    model.set_node_cost("output", Some(suffix), ConstraintValue::Scalar(-500.0))?;

    Ok(())
}

/// Make a simple connections between random systems
///
///
fn make_simple_connections<R: Rng>(
    model: &mut Model,
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
    num_timesteps: usize,
    num_scenarios: usize,
    rng: &mut R,
) -> Result<Model, PywrError> {
    let mut model = Model::default();

    model.add_scenario_group("test-scenario", num_scenarios)?;

    for i in 0..num_systems {
        let suffix = format!("sys-{i:04}");
        make_simple_system(&mut model, &suffix, num_timesteps, rng)?;
    }

    make_simple_connections(&mut model, num_systems, density, rng)?;

    Ok(model)
}

#[cfg(all(test, feature = "ipm-simd"))]
mod tests {
    use super::make_random_model;
    use crate::solvers::{SimdIpmF64Solver, SimdIpmSolverSettings};
    use crate::timestep::Timestepper;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use time::macros::date;

    #[test]
    fn test_random_model() {
        let n_sys = 50;
        let density = 5;
        let n_sc = 12;
        let timestepper = Timestepper::new(date!(2020 - 01 - 01), date!(2020 - 04 - 09), 1);

        // Make a consistent random number generator
        // ChaCha8 should be consistent across builds and platforms
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let model = make_random_model(n_sys, density, timestepper.timesteps().len(), n_sc, &mut rng).unwrap();

        let settings = SimdIpmSolverSettings::default();
        model
            .run_multi_scenario::<SimdIpmF64Solver<4>>(&timestepper, &settings)
            .expect("Failed to run model!");
    }
}
