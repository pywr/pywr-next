/// Some simple benchmarks of random Pywr models.
///
/// The test models here are made up of a number of simple systems. Each system is three
/// node (input->link->output) model. A number of transfers between different systems'
/// link nodes are also generated. This makes for an overall model with some joint connectivity.
///
/// Benchmarks test the performance the solvers with different sized models (numbers of
/// systems and density of transfers between them), numbers of scenarios (which vary the
/// input flows) and number of CPU threads.
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use pywr::solvers::{
    ClIpmF64Solver, ClIpmSolverSettings, ClIpmSolverSettingsBuilder, ClpSolver, ClpSolverSettings,
    ClpSolverSettingsBuilder, HighsSolver, HighsSolverSettings, HighsSolverSettingsBuilder, SimdIpmF64Solver,
    SimdIpmSolverSettings, SimdIpmSolverSettingsBuilder,
};
use pywr::test_utils::make_random_model;
use pywr::timestep::Timestepper;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::num::NonZeroUsize;
use time::macros::date;

fn random_benchmark(
    c: &mut Criterion,
    group_name: &str,
    num_systems: &[usize],
    densities: &[usize],
    num_scenarios: &[usize],
    solver_setups: &[SolverSetup], // TODO This should be an enum (see one also in main.rs; should incorporated into the crate).
    sample_size: Option<usize>,
) {
    // We'll do 100 days to make interpretation of the timings a little easier.
    // i.e. a run time of 1s would equal 1000 timesteps per second
    let timestepper = Timestepper::new(date!(2020 - 01 - 01), date!(2020 - 04 - 09), 1);

    let mut group = c.benchmark_group(group_name);
    // group.sampling_mode(SamplingMode::Flat);
    if let Some(n) = sample_size {
        group.sample_size(n);
    }
    // group.measurement_time(std::time::Duration::from_secs(60));

    for &n_sys in num_systems {
        for &density in densities {
            for &n_sc in num_scenarios {
                // Make a consistent random number generator
                // ChaCha8 should be consistent across builds and platforms
                let mut rng = ChaCha8Rng::seed_from_u64(0);
                let model = make_random_model(n_sys, density, n_sc, &mut rng).unwrap();

                // This is the number of time-steps
                group.throughput(Throughput::Elements(100 * n_sc as u64));

                for setup in solver_setups {
                    match &setup.setting {
                        SolverSetting::Clp(settings) => {
                            let parameter_string = format!("clp * {n_sys} * {density} * {n_sc} * {}", &setup.name);

                            group.bench_with_input(
                                BenchmarkId::new("random-model", parameter_string),
                                &(n_sys, density, n_sc),
                                |b, _n| b.iter(|| model.run::<ClpSolver>(&timestepper, &settings)),
                            );
                        }
                        SolverSetting::Highs(settings) => {
                            let parameter_string = format!("highs * {n_sys} * {density} * {n_sc} * {}", &setup.name);

                            group.bench_with_input(
                                BenchmarkId::new("random-model", parameter_string),
                                &(n_sys, density, n_sc),
                                |b, _n| b.iter(|| model.run::<HighsSolver>(&timestepper, &settings)),
                            );
                        }
                        SolverSetting::IpmSimd(settings) => {
                            let parameter_string =
                                format!("ipm-simd-f64 * {n_sys} * {density} * {n_sc} * {}", &setup.name);

                            group.bench_with_input(
                                BenchmarkId::new("random-model", parameter_string),
                                &(n_sys, density, n_sc),
                                |b, _n| {
                                    b.iter(|| model.run_multi_scenario::<SimdIpmF64Solver>(&timestepper, &settings))
                                },
                            );
                        }
                        SolverSetting::IpmOcl(settings) => {
                            let parameter_string =
                                format!("ipm-ocl-f64 * {n_sys} * {density} * {n_sc} * {}", &setup.name);

                            group.bench_with_input(
                                BenchmarkId::new("random-model", parameter_string),
                                &(n_sys, density, n_sc),
                                |b, _n| b.iter(|| model.run_multi_scenario::<ClIpmF64Solver>(&timestepper, &settings)),
                            );
                        }
                    }
                }
            }
        }
    }

    group.finish();
}

enum SolverSetting {
    Clp(ClpSolverSettings),
    Highs(HighsSolverSettings),
    IpmSimd(SimdIpmSolverSettings),
    IpmOcl(ClIpmSolverSettings),
}

struct SolverSetup {
    setting: SolverSetting,
    name: String,
}

fn default_solver_setups() -> Vec<SolverSetup> {
    vec![
        // SolverSetup {
        //     setting: SolverSetting::Highs(HighsSolverSettings::default()),
        //     name: "default".to_string(),
        // },
        SolverSetup {
            setting: SolverSetting::Clp(ClpSolverSettings::default()),
            name: "default".to_string(),
        },
        SolverSetup {
            setting: SolverSetting::IpmSimd(SimdIpmSolverSettings::default()),
            name: "default".to_string(),
        },
    ]
}

fn bench_system_size(c: &mut Criterion) {
    let solver_setups = default_solver_setups();

    random_benchmark(
        c,
        "random-models-size",
        &[5, 10, 20, 30, 40, 50],
        &[2, 5],
        &[1],
        &solver_setups,
        None,
    )
}

fn bench_scenarios(c: &mut Criterion) {
    let scenarios: Vec<usize> = vec![1, 2, 4, 6, 8, 10, 12, 24, 48, 64];
    let solver_setups = default_solver_setups();

    random_benchmark(
        c,
        "random-models-scenarios",
        &[20, 50],
        &[5],
        &scenarios,
        &solver_setups,
        Some(10),
    )
}

fn bench_threads(c: &mut Criterion) {
    let mut solver_setups = Vec::new();

    for n_threads in [1, 2, 4, 8, 16] {
        solver_setups.push(SolverSetup {
            setting: SolverSetting::Clp(
                ClpSolverSettingsBuilder::default()
                    .parallel()
                    .threads(n_threads)
                    .build(),
            ),
            name: format!("threads-{}", n_threads),
        });

        solver_setups.push(SolverSetup {
            setting: SolverSetting::IpmSimd(
                SimdIpmSolverSettingsBuilder::default()
                    .parallel()
                    .threads(n_threads)
                    .build(),
            ),
            name: format!("threads-{}", n_threads),
        });
    }

    random_benchmark(
        c,
        "random-models-threads",
        &[20, 50],
        &[5],
        &[256, 32768],
        &solver_setups,
        Some(10),
    )
}

fn bench_ipm_convergence(c: &mut Criterion) {
    const N_THREADS: usize = 0;

    let mut solver_setups = Vec::new();

    for optimality in [1e-3, 1e-4, 1e-5, 1e-6, 1e-7, 1e-8] {
        solver_setups.push(SolverSetup {
            setting: SolverSetting::IpmSimd(
                SimdIpmSolverSettingsBuilder::default()
                    .optimality(optimality)
                    .parallel()
                    .threads(N_THREADS)
                    .build(),
            ),
            name: format!("opt-tol-{:e}", optimality),
        });
    }

    random_benchmark(
        c,
        "random-models-ipm-convergence",
        &[20, 50],
        &[5],
        &[256, 32768],
        &solver_setups,
        Some(10),
    )
}

fn bench_ocl_chunks(c: &mut Criterion) {
    const N_THREADS: usize = 0;

    let mut solver_setups = Vec::new();

    for chunk_size in [64, 128, 256, 512, 1024, 2056, 4096] {
        solver_setups.push(SolverSetup {
            setting: SolverSetting::IpmOcl(
                ClIpmSolverSettingsBuilder::default()
                    .parallel()
                    .threads(N_THREADS)
                    .chunk_size(NonZeroUsize::new(chunk_size).unwrap())
                    .build(),
            ),
            name: format!("chunk-size-{}", chunk_size),
        });
    }

    random_benchmark(
        c,
        "random-models-olc-chunks",
        &[20],
        &[5],
        &[32768],
        &solver_setups,
        Some(10),
    )
}

fn bench_hyper_scenarios(c: &mut Criterion) {
    let scenarios: Vec<usize> = (0..17).into_iter().map(|p| 2_usize.pow(p)).collect();

    const N_THREADS: usize = 0;

    let solver_setups = vec![
        SolverSetup {
            setting: SolverSetting::Clp(
                ClpSolverSettingsBuilder::default()
                    .parallel()
                    .threads(N_THREADS)
                    .build(),
            ),
            name: "default".to_string(),
        },
        SolverSetup {
            setting: SolverSetting::IpmSimd(
                SimdIpmSolverSettingsBuilder::default()
                    .parallel()
                    .threads(N_THREADS)
                    .build(),
            ),
            name: "default".to_string(),
        },
    ];

    random_benchmark(
        c,
        "random-models-hyper-scenarios",
        &[20, 50],
        &[5],
        &scenarios,
        &solver_setups,
        Some(10),
    )
}

criterion_group!(
    benches,
    bench_system_size,
    bench_scenarios,
    bench_threads,
    bench_hyper_scenarios,
    bench_ipm_convergence,
    bench_ocl_chunks
);
criterion_main!(benches);
