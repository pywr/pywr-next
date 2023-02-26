/// Some simple benchmarks of random Pywr models.
///
/// The test models here are made up of a number of simple systems. Each system is three
/// node (input->link->output) model. A number of transfers between different systems'
/// link nodes are also generated. This makes for an overall model with some joint connectivity.
///
/// Benchmarks test the performance the solvers with different sized models (numbers of
/// systems and density of transfers between them), numbers of scenarios (which vary the
/// input flows) and number of CPU threads.
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode, Throughput};
use pywr::metric::Metric;
use pywr::model::{Model, RunOptions};
use pywr::node::ConstraintValue;
use pywr::parameters::Array2Parameter;
use pywr::solvers::{ClpSolver, HighsSolver};
use pywr::timestep::Timestepper;
use pywr::PywrError;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rand_distr::{Distribution, Normal};
use time::macros::date;

/// Make a simple system with random inputs.
fn make_simple_system<R: Rng>(model: &mut Model, suffix: &str, rng: &mut R) -> Result<(), PywrError> {
    let input_idx = model.add_input_node("input", Some(suffix))?;
    let link_idx = model.add_link_node("link", Some(suffix))?;
    let output_idx = model.add_output_node("output", Some(suffix))?;

    model.connect_nodes(input_idx, link_idx)?;
    model.connect_nodes(link_idx, output_idx)?;

    let num_scenarios = model.get_scenario_group_size_by_name("test-scenario")?;
    let scenario_group_index = model.get_scenario_group_index_by_name("test-scenario")?;

    let inflow_distr: Normal<f64> = Normal::new(9.0, 1.0).unwrap();

    let mut inflow = ndarray::Array2::zeros((1000, num_scenarios));

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

    model.set_node_cost("input", Some(suffix), ConstraintValue::Scalar(-10.0))?;

    let outflow_distr = Normal::new(8.0, 3.0).unwrap();
    let mut outflow: f64 = outflow_distr.sample(rng);
    outflow = outflow.max(0.0);

    model.set_node_max_flow("output", Some(suffix), ConstraintValue::Scalar(outflow))?;

    model.set_node_cost("output", Some(suffix), ConstraintValue::Scalar(-500.0))?;

    Ok(())
}

/// Make a simple connection between two random systems
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

fn make_random_model<R: Rng>(
    num_systems: usize,
    density: usize,
    num_scenarios: usize,
    rng: &mut R,
) -> Result<Model, PywrError> {
    let mut model = Model::default();

    model.add_scenario_group("test-scenario", num_scenarios)?;

    for i in 0..num_systems {
        let suffix = format!("sys-{i:04}");
        make_simple_system(&mut model, &suffix, rng)?;
    }

    make_simple_connections(&mut model, num_systems, density, rng)?;

    Ok(model)
}

fn random_benchmark(
    c: &mut Criterion,
    group_name: &str,
    num_systems: &[usize],
    densities: &[usize],
    num_scenarios: &[usize],
    num_threads: &[usize],
) {
    // We'll do 1000 days to make interpretation of the timings a little easier.
    // i.e. a run time of 1s would equal 1000 timesteps per second
    let timestepper = Timestepper::new(date!(2020 - 01 - 01), date!(2022 - 09 - 26), 1);

    let mut group = c.benchmark_group(group_name);
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(20);
    group.measurement_time(std::time::Duration::from_secs(60));

    for &n_sys in num_systems {
        for &density in densities {
            for &n_sc in num_scenarios {
                for &n_threads in num_threads {
                    // Make a consistent random number generator
                    // ChaCha8 should be consistent across builds and platforms
                    let mut rng = ChaCha8Rng::seed_from_u64(0);
                    let model = make_random_model(n_sys, density, n_sc, &mut rng).unwrap();

                    let parameter_string = format!("{n_sys} * {density} * {n_sc} * {n_threads}");
                    // This is the number of time-steps
                    group.throughput(Throughput::Elements(1000 * n_sc as u64));

                    let options = if n_threads > 1 {
                        RunOptions::default().parallel().threads(n_threads)
                    } else {
                        RunOptions::default()
                    };

                    if n_threads == 1 {
                        // Only do Highs benchmark for single-threaded
                        group.bench_with_input(
                            BenchmarkId::new("random-model-highs", parameter_string.clone()),
                            &n_sys,
                            |b, _n| b.iter(|| model.run::<HighsSolver>(&timestepper, &options).unwrap()),
                        );
                    }

                    group.bench_with_input(
                        BenchmarkId::new("random-model-clp", parameter_string.clone()),
                        &(n_sys, density, n_sc),
                        |b, _n| b.iter(|| model.run::<ClpSolver>(&timestepper, &options).unwrap()),
                    );
                }
            }
        }
    }

    group.finish();
}

fn bench_system_size(c: &mut Criterion) {
    random_benchmark(
        c,
        "random-models-size",
        &[5, 10, 20, 30, 40, 50, 100],
        &[2, 5],
        &[1],
        &[1],
    )
}

fn bench_scenarios(c: &mut Criterion) {
    random_benchmark(
        c,
        "random-models-scenarios",
        &[20, 50],
        &[5],
        &[2, 10, 50, 100, 400, 1000],
        &[1],
    )
}

fn bench_threads(c: &mut Criterion) {
    random_benchmark(c, "random-models-threads", &[20, 50], &[5], &[100], &[2, 4, 8, 16])
}

criterion_group!(benches, bench_system_size, bench_scenarios, bench_threads);
criterion_main!(benches);
