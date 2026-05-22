/// Utilities for unit tests.
/// TODO move this to its own local crate ("test-utilities") as part of a workspace.
///
use crate::agg_funcs::AggFuncF64;
use crate::metric::{UnresolvedMetricF64, UnresolvedMetricU64};
use crate::models::{Model, ModelBuilder, ModelDomain, ModelDomainBuilder};
use crate::network::NetworkBuilder;
use crate::node::{NodeBuilder, NodeType, UnresolvedNode, UnresolvedStorageInitialVolume};
use crate::parameters::{
    AggregatedParameterBuilder, Array2ParameterBuilder, ConstantParameterBuilder, ParameterBuilder, ParameterName,
};
use crate::recorders::{AssertionF64RecorderBuilder, AssertionU64RecorderBuilder};
use crate::scenario::{ScenarioDomainBuilder, ScenarioGroupBuilder};
#[cfg(feature = "cbc")]
use crate::solvers::CbcSolver;
#[cfg(feature = "ipm-ocl")]
use crate::solvers::ClIpmF64Solver;
#[cfg(feature = "clp")]
use crate::solvers::ClpSolver;
#[cfg(feature = "highs")]
use crate::solvers::HighsSolver;
#[cfg(any(feature = "ipm-simd", feature = "ipm-ocl"))]
use crate::solvers::MultiStateSolver;
#[cfg(feature = "ipm-simd")]
use crate::solvers::SimdIpmF64Solver;
#[cfg(any(feature = "cbc", feature = "clp", feature = "highs", feature = "microlp"))]
use crate::solvers::Solver;
#[cfg(any(
    feature = "cbc",
    feature = "clp",
    feature = "highs",
    feature = "ipm-ocl",
    feature = "ipm-simd",
    feature = "microlp"
))]
use crate::solvers::SolverSettings;
use crate::timestep::{TimeDomainBuilder, TimestepDuration};
use chrono::{Days, NaiveDate};
use csv::{Reader, ReaderBuilder};
use float_cmp::{F64Margin, approx_eq};
use ndarray::{Array, Array2};
use rand::{Rng, RngExt};
use rand_distr::{Distribution, Normal};
use std::num::NonZeroU64;
use std::path::PathBuf;

pub fn default_time_domain_builder() -> TimeDomainBuilder {
    let start = NaiveDate::from_ymd_opt(2020, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let end = NaiveDate::from_ymd_opt(2020, 1, 15)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let duration = TimestepDuration::Days(NonZeroU64::new(1).unwrap());
    TimeDomainBuilder::new(start, end, duration)
}

pub fn default_domain_builder() -> ModelDomainBuilder {
    ModelDomainBuilder::new(default_time_domain_builder())
}

pub fn default_domain() -> ModelDomain {
    default_domain_builder().build().unwrap()
}

/// Add a simple test network with three nodes.
pub fn simple_network(builder: &mut NetworkBuilder, inflow_scenario: &str, num_inflow_scenarios: usize) {
    let mut input_node = NodeBuilder::input("input");
    input_node.max_flow(UnresolvedMetricF64::new_parameter_before("inflow"));

    builder.node(input_node);
    builder.node(NodeBuilder::link("link"));

    let mut output_node = NodeBuilder::output("output");
    output_node
        .max_flow(UnresolvedMetricF64::new_parameter_before("total-demand"))
        .cost(UnresolvedMetricF64::new_parameter_before("demand-cost"));
    builder.node(output_node);

    builder.connect("input", None, "link", None);
    builder.connect("link", None, "output", None);

    let inflow = Array::from_shape_fn((366, num_inflow_scenarios), |(i, j)| 1.0 + i as f64 + j as f64);
    let inflow = Array2ParameterBuilder::new("inflow".into(), inflow, inflow_scenario);

    builder.parameters().f64(Box::new(inflow));

    let base_demand = 10.0;
    let demand_factor = ConstantParameterBuilder::new("demand-factor".into(), 1.2);
    builder.parameters().f64(Box::new(demand_factor));

    let mut total_demand = AggregatedParameterBuilder::new("total-demand".into(), AggFuncF64::Product);
    total_demand.metric(base_demand.into());
    total_demand.metric(UnresolvedMetricF64::new_parameter_before("demand-factor"));

    builder.parameters().f64(Box::new(total_demand));

    let demand_cost = ConstantParameterBuilder::new("demand-cost".into(), -10.0);
    builder.parameters().f64(Box::new(demand_cost));
}

/// Create a simple test model builder
pub fn simple_model(num_scenarios: usize, time_builder: Option<TimeDomainBuilder>) -> ModelBuilder {
    let scenario = "test-scenario";

    let mut network_builder = NetworkBuilder::default();
    simple_network(&mut network_builder, scenario, num_scenarios);

    let mut scenario_builder = ScenarioDomainBuilder::default();
    let scenario_group = ScenarioGroupBuilder::new(scenario, num_scenarios).build().unwrap();
    scenario_builder = scenario_builder.with_group(scenario_group).unwrap();

    let mut domain_builder = ModelDomainBuilder::new(time_builder.unwrap_or_else(default_time_domain_builder));
    domain_builder.scenario(scenario_builder);

    ModelBuilder::new(domain_builder, network_builder)
}

/// A test model with a single storage node.
pub fn simple_storage_network() -> NetworkBuilder {
    let mut builder = NetworkBuilder::default();

    let mut storage_node = NodeBuilder::storage("reservoir");
    storage_node.initial_volume(UnresolvedStorageInitialVolume::Absolute(100.0));
    storage_node.max_volume(100.0.into());
    builder.node(storage_node);

    let mut output_node = NodeBuilder::output("output");
    output_node
        .max_flow(UnresolvedMetricF64::new_parameter_before("demand"))
        .cost(UnresolvedMetricF64::new_parameter_before("demand-cost"));
    builder.node(output_node);

    builder.connect("reservoir", None, "output", None);

    // Apply demand to the model
    // TODO convenience function for adding a constant constraint.
    let demand = ConstantParameterBuilder::new("demand".into(), 10.0);
    builder.parameters().f64(Box::new(demand));

    let demand_cost = ConstantParameterBuilder::new("demand-cost".into(), -10.0);
    builder.parameters().f64(Box::new(demand_cost));

    builder
}

pub fn simple_storage_model() -> ModelBuilder {
    let network = simple_storage_network();
    ModelBuilder::new(default_domain_builder(), network)
}

/// Add the given parameter to the given model along with an assertion recorder that asserts
/// whether the parameter returns the expected values when the model is run.
///
/// This function will run a number of time-steps equal to the number of rows in the expected
/// values array.
///
/// See [`AssertionF64Recorder`] for more information.
pub fn run_and_assert_parameter(
    mut model_builder: ModelBuilder,
    parameter: Box<dyn ParameterBuilder<f64>>,
    expected_values: Array2<f64>,
    ulps: Option<i64>,
    epsilon: Option<f64>,
) {
    let p_name = parameter.name().clone();
    model_builder.network_builder().parameters().f64(parameter);

    let start = NaiveDate::from_ymd_opt(2020, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let _end = start
        .checked_add_days(Days::new(expected_values.nrows() as u64 - 1))
        .unwrap();

    let mut rec = AssertionF64RecorderBuilder::new(
        "assert",
        UnresolvedMetricF64::new_parameter_before(p_name),
        expected_values,
    );

    if let Some(ulps) = ulps {
        rec.ulps(ulps);
    }
    if let Some(eps) = epsilon {
        rec.epsilon(eps);
    }

    model_builder.network_builder().recorder(Box::new(rec));

    let model = model_builder.build().unwrap();
    run_all_solvers(&model, &[], &[], &[])
}

/// Add the given parameter to the given model along with an assertion recorder that asserts
/// whether the parameter returns the expected values when the model is run.
///
/// This function will run a number of time-steps equal to the number of rows in the expected
/// values array.
///
/// See [`AssertionU64Recorder`] for more information.
pub fn run_and_assert_parameter_u64(
    mut model_builder: ModelBuilder,
    parameter: Box<dyn ParameterBuilder<u64>>,
    expected_values: Array2<u64>,
) {
    let p_name = parameter.name().clone();
    model_builder.network_builder().parameters().u64(parameter);

    let start = NaiveDate::from_ymd_opt(2020, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let _end = start
        .checked_add_days(Days::new(expected_values.nrows() as u64 - 1))
        .unwrap();

    let rec = AssertionU64RecorderBuilder::new(
        "assert",
        UnresolvedMetricU64::new_index_parameter_before(p_name),
        expected_values,
    );
    model_builder.network_builder().recorder(Box::new(rec));

    let model = model_builder.build().unwrap();
    run_all_solvers(&model, &[], &[], &[])
}

/// A trait with a verify method for checking model outputs.
///
/// The verify method should compare model outputs with expected results, raising
/// an error if they do not match.
pub trait VerifyExpected {
    fn verify(&self);
}

/// A struct representing an CSV output row in long format
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ExpectedRowLong {
    time_start: String,
    time_end: String,
    simulation_id: String,
    label: String,
    metric_set: String,
    name: String,
    attribute: String,
    value: f64,
}

impl PartialEq for ExpectedRowLong {
    fn eq(&self, other: &Self) -> bool {
        self.time_start == other.time_start
            && self.time_end == other.time_end
            && self.simulation_id == other.simulation_id
            && self.label == other.label
            && self.metric_set == other.metric_set
            && self.name == other.name
            && self.attribute == other.attribute
            && approx_eq!(f64, self.value, other.value, F64Margin { ulps: 2, epsilon: 1e-8 })
    }
}

/// A struct to hold the expected outputs in long format for a test.
pub struct ExpectedOutputsLong {
    output_path: PathBuf,
    expected_str: String,
}

impl ExpectedOutputsLong {
    pub fn new(output_path: PathBuf, expected_str: String) -> Self {
        Self {
            output_path,
            expected_str,
        }
    }
}

impl VerifyExpected for ExpectedOutputsLong {
    fn verify(&self) {
        assert!(
            self.output_path.exists(),
            "Output file does not exist: {:?}",
            self.output_path
        );
        let actual_str = std::fs::read_to_string(&self.output_path).unwrap();

        let mut expected_rdr = Reader::from_reader(self.expected_str.as_bytes());
        let mut actual_rdr = Reader::from_reader(actual_str.as_bytes());

        let expected_line_count = expected_rdr.records().count();
        let actual_line_count = actual_rdr.records().count();

        assert_eq!(
            expected_line_count, actual_line_count,
            "Row count mismatch (expected rows: {}, actual rows: {})",
            expected_line_count, actual_line_count
        );

        // Reset the readers to the beginning for actual comparison
        let mut expected_rdr = Reader::from_reader(self.expected_str.as_bytes());
        let mut actual_rdr = Reader::from_reader(actual_str.as_bytes());

        for (row_idx, (result, actual_result)) in expected_rdr
            .deserialize::<ExpectedRowLong>()
            .zip(actual_rdr.deserialize::<ExpectedRowLong>())
            .enumerate()
        {
            let record: ExpectedRowLong = result.unwrap();
            let actual_record: ExpectedRowLong = actual_result.unwrap();
            assert_eq!(record, actual_record, "Row {} differs", row_idx);
        }
    }
}
/// A struct to hold the expected outputs in wide format for a test.
pub struct ExpectedOutputsWide {
    output_path: PathBuf,
    expected_str: String,
}

impl ExpectedOutputsWide {
    pub fn new(output_path: PathBuf, expected_str: String) -> Self {
        Self {
            output_path,
            expected_str,
        }
    }
}

impl VerifyExpected for ExpectedOutputsWide {
    fn verify(&self) {
        assert!(
            self.output_path.exists(),
            "Output file does not exist: {:?}",
            self.output_path
        );
        let actual_str = std::fs::read_to_string(&self.output_path).unwrap();

        let mut expected_rdr = ReaderBuilder::new()
            .has_headers(false)
            .delimiter(b',')
            .from_reader(self.expected_str.as_bytes());
        let mut actual_rdr = ReaderBuilder::new()
            .has_headers(false)
            .delimiter(b',')
            .from_reader(actual_str.as_bytes());

        // first 4 lines are headers so compare line strings
        for i in 0..4 {
            let expected_line = expected_rdr.records().next().unwrap().unwrap();
            let actual_line = actual_rdr.records().next().unwrap().unwrap();
            assert_eq!(expected_line, actual_line, "Header line {} differs", i);
        }

        for (row_idx, (expected_result, actual_result)) in expected_rdr.records().zip(actual_rdr.records()).enumerate()
        {
            let expected_row = expected_result.unwrap();
            let actual_row = actual_result.unwrap();
            let mut expected_iter = expected_row.iter();
            let mut actual_iter = actual_row.iter();

            let expected_index = expected_iter.next().unwrap();
            let actual_index = actual_iter.next().unwrap();

            let expected_values: Vec<f64> = expected_iter
                .map(|s| s.trim().parse::<f64>().expect("Failed to parse expected value"))
                .collect();
            let actual_values: Vec<f64> = actual_iter
                .map(|s| s.trim().parse::<f64>().expect("Failed to parse actual value"))
                .collect();

            // Compare index values
            assert_eq!(
                expected_index.trim(),
                actual_index.trim(),
                "Row {}: index values differ",
                row_idx
            );

            // Compare the rest of the values
            for (col_idx, (expected, actual)) in expected_values.iter().zip(actual_values.iter()).enumerate() {
                if !approx_eq!(f64, *expected, *actual, F64Margin { ulps: 2, epsilon: 1e-8 }) {
                    panic!(
                        "Row {} with index {}: value at column {} differs (expected: {}, actual: {})",
                        row_idx, expected_index, col_idx, expected, actual
                    );
                }
            }
        }
    }
}

/// Run a model using each of the in-built solvers.
///
/// The model will only be run if the solver has the required solver features (and
/// is also enabled as a Cargo feature).
#[cfg(any(
    feature = "cbc",
    feature = "clp",
    feature = "highs",
    feature = "ipm-ocl",
    feature = "ipm-simd",
    feature = "microlp"
))]
pub fn run_all_solvers(
    model: &Model,
    solvers_without_features: &[&str],
    solvers_to_skip: &[&str],
    expected_outputs: &[Box<dyn VerifyExpected>],
) {
    #[cfg(feature = "clp")]
    {
        if !solvers_to_skip.contains(&"clp") {
            check_features_and_run::<ClpSolver>(model, !solvers_without_features.contains(&"clp"), expected_outputs);
        }
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

    #[cfg(feature = "microlp")]
    {
        if !solvers_to_skip.contains(&"microlp") {
            check_features_and_run::<crate::solvers::MicroLpSolver>(
                model,
                !solvers_without_features.contains(&"microlp"),
                expected_outputs,
            );
        }
    }

    #[cfg(feature = "ipm-simd")]
    {
        if !solvers_to_skip.contains(&"ipm-simd") {
            check_features_and_run_multi::<SimdIpmF64Solver>(
                model,
                !solvers_without_features.contains(&"ipm-simd"),
                expected_outputs,
            );
        }
    }

    #[cfg(feature = "ipm-ocl")]
    {
        if !solvers_to_skip.contains(&"ipm-ocl") {
            check_features_and_run_multi::<ClIpmF64Solver>(
                model,
                !solvers_without_features.contains(&"ipm-ocl"),
                expected_outputs,
            );
        }
    }
}

#[cfg(not(any(
    feature = "cbc",
    feature = "clp",
    feature = "highs",
    feature = "ipm-ocl",
    feature = "ipm-simd",
    feature = "microlp"
)))]
pub fn run_all_solvers(
    _model: &Model,
    _solvers_without_features: &[&str],
    _solvers_to_skip: &[&str],
    _expected_outputs: &[Box<dyn VerifyExpected>],
) {
    panic!("No solvers are enabled. Please enable at least one solver feature.");
}

/// Check features and
#[cfg(any(feature = "cbc", feature = "clp", feature = "highs", feature = "microlp"))]
fn check_features_and_run<S>(model: &Model, expect_features: bool, expected_outputs: &[Box<dyn VerifyExpected>])
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
fn check_features_and_run_multi<S>(model: &Model, expect_features: bool, _expected_outputs: &[Box<dyn VerifyExpected>])
where
    S: MultiStateSolver,
    <S as MultiStateSolver>::Settings: SolverSettings + Default,
{
    let has_features = model.check_multi_scenario_solver_features::<S>();
    if expect_features {
        assert!(
            has_features,
            "Solver `{}` (with features: {:#?}) was expected to have the required features: {:?}",
            S::name(),
            S::features(),
            model.required_features()
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
fn make_simple_system<R: Rng + ?Sized>(
    builder: &mut NetworkBuilder,
    suffix: &str,
    num_timesteps: usize,
    num_inflow_scenarios: usize,
    inflow_scenario: &str,
    rng: &mut R,
) {
    let inflow_parameter_name = ParameterName::new("inflow", Some(suffix));
    let input_cost = rng.random_range(-20.0..-5.00);
    let mut input_node = NodeBuilder::input("input");
    input_node
        .sub_name(suffix)
        .max_flow(UnresolvedMetricF64::new_parameter_before(inflow_parameter_name.clone()))
        .cost(input_cost.into());

    builder.node(input_node);

    let mut link_node = NodeBuilder::link("link");
    link_node.sub_name(suffix);
    builder.node(link_node);

    let outflow_distr = Normal::new(8.0, 3.0).unwrap();
    let mut outflow: f64 = outflow_distr.sample(rng);
    outflow = outflow.max(0.0);

    let mut output_node = NodeBuilder::output("output");
    output_node
        .sub_name(suffix)
        .max_flow(outflow.into())
        .cost((-500.0).into());
    builder.node(output_node);

    builder.connect("input", Some(suffix), "link", Some(suffix));
    builder.connect("link", Some(suffix), "output", Some(suffix));

    let inflow_distr: Normal<f64> = Normal::new(9.0, 1.0).unwrap();

    let mut inflow = Array2::zeros((num_timesteps, num_inflow_scenarios));

    for x in inflow.iter_mut() {
        *x = inflow_distr.sample(rng).max(0.0);
    }

    let inflow = Array2ParameterBuilder::new(inflow_parameter_name, inflow, inflow_scenario);

    builder.parameters().f64(Box::new(inflow));
}

/// Make a simple connections between random systems
///
///
fn make_simple_connections<R: Rng>(
    network_builder: &mut NetworkBuilder,
    num_systems: usize,
    density: usize,
    rng: &mut R,
) {
    let num_connections = (num_systems.pow(2) * density / 100 / 2).max(1);

    let mut connections_added: usize = 0;

    while connections_added < num_connections {
        let i = rng.random_range(0..num_systems);
        let j = rng.random_range(0..num_systems);

        if i == j {
            continue;
        }

        let sub_name = format!("{i:04}->{j:04}");
        let name = UnresolvedNode::new("transfer", Some(&sub_name));

        let transfer_already_exists = network_builder.node_builder(&name).is_some();

        if !transfer_already_exists {
            // Add a new transfers if it doesn't already exist
            let transfer_cost = rng.random_range(0.0..1.0);

            let mut node_builder = NodeBuilder::new("transfer", NodeType::Link);
            node_builder.sub_name(&sub_name).cost(transfer_cost.into());

            network_builder.node(node_builder);

            network_builder.connect("link", Some(&format!("sys-{i:04}")), "transfer", Some(&sub_name));
            network_builder.connect("transfer", Some(&sub_name), "link", Some(&format!("sys-{j:04}")));

            connections_added += 1;
        }
    }
}

pub fn make_random_model_builder<R: Rng>(
    num_systems: usize,
    density: usize,
    num_scenarios: usize,
    rng: &mut R,
) -> ModelBuilder {
    let start = NaiveDate::from_ymd_opt(2020, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let end = NaiveDate::from_ymd_opt(2020, 4, 9)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let duration = TimestepDuration::Days(NonZeroU64::new(1).unwrap());
    let time_builder = TimeDomainBuilder::new(start, end, duration);

    let mut scenario_builder = ScenarioDomainBuilder::default();
    let scenario_group = ScenarioGroupBuilder::new("test-scenario", num_scenarios)
        .build()
        .expect("Could not create scenario group");
    scenario_builder = scenario_builder
        .with_group(scenario_group)
        .expect("Could not add scenario group");

    let mut domain_builder = ModelDomainBuilder::new(time_builder);
    domain_builder.scenario(scenario_builder);

    // Quickly build the domain to determine its shape so we can setup the correct input data.
    let (num_timesteps, num_inflow_scenarios) = domain_builder.clone().build().unwrap().shape();

    let mut network_builder = NetworkBuilder::default();
    for i in 0..num_systems {
        let suffix = format!("sys-{i:04}");
        make_simple_system(
            &mut network_builder,
            &suffix,
            num_timesteps,
            num_inflow_scenarios,
            "test-scenario",
            rng,
        );
    }

    make_simple_connections(&mut network_builder, num_systems, density, rng);

    ModelBuilder::new(domain_builder, network_builder)
}

#[cfg(all(test, feature = "ipm-simd"))]
mod tests {
    use super::make_random_model_builder;
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
        let model = make_random_model_builder(n_sys, density, n_sc, &mut rng)
            .build()
            .expect("Failed to builder random model.");

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
