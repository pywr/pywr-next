/// Some simple benchmarks of random Pywr models.
///
/// The test models here are made up of a number of simple systems. Each system is three
/// node (input->link->output) model. A number of transfers between different systems'
/// link nodes are also generated. This makes for an overall model with some joint connectivity.
///
/// Benchmarks test the performance the solvers with different sized models (numbers of
/// systems and density of transfers between them), numbers of scenarios (which vary the
/// input flows) and number of CPU threads.
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use pywr_core::models::ModelTimings;
#[cfg(feature = "ipm-ocl")]
use pywr_core::solvers::{ClIpmF64Solver, ClIpmSolverSettings, ClIpmSolverSettingsBuilder};
use pywr_core::solvers::{ClpSolver, ClpSolverSettings, ClpSolverSettingsBuilder};
#[cfg(feature = "highs")]
use pywr_core::solvers::{HighsSolver, HighsSolverSettings};
#[cfg(feature = "ipm-simd")]
use pywr_core::solvers::{SimdIpmF64Solver, SimdIpmSolverSettings, SimdIpmSolverSettingsBuilder};
use pywr_core::test_utils::make_random_model;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
#[cfg(feature = "ipm-ocl")]
use std::num::NonZeroUsize;

fn random_benchmark(
    c: &mut Criterion,
    group_name: &str,
    num_systems: &[usize],
    densities: &[usize],
    num_scenarios: &[usize],
    solver_setups: &[SolverSetup], // TODO This should be an enum (see one also in main.rs; should incorporated into the crate).
    sample_size: Option<usize>,
) {
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
                let num_timesteps = model.domain().time().timesteps().len();

                // This is the number of time-steps
                group.throughput(Throughput::Elements((num_timesteps * n_sc) as u64));

                for setup in solver_setups {
                    match &setup.setting {
                        SolverSetting::Clp(settings) => {
                            let parameter_string = format!("clp * {n_sys} * {density} * {n_sc} * {}", &setup.name);

                            group.bench_with_input(
                                BenchmarkId::new("random-model", parameter_string),
                                &(n_sys, density, n_sc),
                                |b, _n| {
                                    // Do the setup here outside of the time-step loop
                                    let mut state =
                                        model.setup::<ClpSolver>(settings).expect("Failed to setup the model.");
                                    let mut timings = ModelTimings::new_with_component_timings(model.network());

                                    b.iter(|| model.run_with_state(&mut state, settings, &mut timings))
                                },
                            );
                        }
                        #[cfg(feature = "highs")]
                        SolverSetting::Highs(settings) => {
                            let parameter_string = format!("highs * {n_sys} * {density} * {n_sc} * {}", &setup.name);

                            group.bench_with_input(
                                BenchmarkId::new("random-model", parameter_string),
                                &(n_sys, density, n_sc),
                                |b, _n| {
                                    let mut state = model
                                        .setup::<HighsSolver>(settings)
                                        .expect("Failed to setup the model.");
                                    let mut timings = ModelTimings::new_with_component_timings(model.network());

                                    b.iter(|| model.run_with_state(&mut state, settings, &mut timings))
                                },
                            );
                        }
                        #[cfg(feature = "ipm-simd")]
                        SolverSetting::IpmSimdF64x4(settings) => {
                            let parameter_string =
                                format!("ipm-simd-f64x4 * {n_sys} * {density} * {n_sc} * {}", &setup.name);

                            group.bench_with_input(
                                BenchmarkId::new("random-model", parameter_string),
                                &(n_sys, density, n_sc),
                                |b, _n| {
                                    // Do the setup here outside of the time-step loop
                                    let mut state = model
                                        .setup_multi_scenario::<SimdIpmF64Solver>(settings)
                                        .expect("Failed to setup the model.");
                                    let mut timings = ModelTimings::new_with_component_timings(model.network());

                                    b.iter(|| model.run_multi_scenario_with_state(&mut state, settings, &mut timings))
                                },
                            );
                        }
                        #[cfg(feature = "ipm-ocl")]
                        SolverSetting::IpmOcl(settings) => {
                            let parameter_string =
                                format!("ipm-ocl-f64 * {n_sys} * {density} * {n_sc} * {}", &setup.name);

                            group.bench_with_input(
                                BenchmarkId::new("random-model", parameter_string),
                                &(n_sys, density, n_sc),
                                |b, _n| {
                                    // Do the setup here outside of the time-step loop
                                    let mut state = model
                                        .setup_multi_scenario::<ClIpmF64Solver>(settings)
                                        .expect("Failed to setup the model.");

                                    let mut timings = ModelTimings::new_with_component_timings(model.network());

                                    b.iter(|| model.run_multi_scenario_with_state(&mut state, settings, &mut timings))
                                },
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
    #[cfg(feature = "highs")]
    Highs(HighsSolverSettings),
    #[cfg(feature = "ipm-simd")]
    IpmSimdF64x4(SimdIpmSolverSettings),
    #[cfg(feature = "ipm-ocl")]
    IpmOcl(ClIpmSolverSettings),
}

struct SolverSetup {
    setting: SolverSetting,
    name: String,
}

fn default_solver_setups() -> Vec<SolverSetup> {
    vec![
        #[cfg(feature = "highs")]
        SolverSetup {
            setting: SolverSetting::Highs(HighsSolverSettings::default()),
            name: "default".to_string(),
        },
        SolverSetup {
            setting: SolverSetting::Clp(ClpSolverSettings::default()),
            name: "default".to_string(),
        },
        #[cfg(feature = "ipm-simd")]
        SolverSetup {
            setting: SolverSetting::IpmSimdF64x4(SimdIpmSolverSettings::default()),
            name: "default".to_string(),
        },
        #[cfg(feature = "ipm-ocl")]
        SolverSetup {
            setting: SolverSetting::IpmOcl(ClIpmSolverSettings::default()),
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

/// Single thread small scenario benchmarks
fn bench_scenarios(c: &mut Criterion) {
    let scenarios: Vec<usize> = vec![1, 2, 4, 6, 8, 10, 12, 24, 48, 64];
    let solver_setups = default_solver_setups();

    random_benchmark(
        c,
        "random-models-scenarios",
        &[20],
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
            name: format!("threads-{n_threads}",),
        });
        #[cfg(feature = "ipm-simd")]
        solver_setups.push(SolverSetup {
            setting: SolverSetting::IpmSimdF64x4(
                SimdIpmSolverSettingsBuilder::default()
                    .parallel()
                    .threads(n_threads)
                    .build(),
            ),
            name: format!("threads-{n_threads}"),
        });

        #[cfg(feature = "ipm-ocl")]
        solver_setups.push(SolverSetup {
            setting: SolverSetting::IpmOcl(
                ClIpmSolverSettingsBuilder::default()
                    .parallel()
                    .threads(n_threads)
                    .build(),
            ),
            name: format!("threads-{n_threads}"),
        });
    }

    random_benchmark(
        c,
        "random-models-threads",
        &[20],
        &[5],
        &[256, 32768],
        &solver_setups,
        Some(10),
    )
}

#[cfg(any(feature = "ipm-simd", feature = "ipm-ocl"))]
fn bench_ipm_convergence(c: &mut Criterion) {
    #[cfg(any(feature = "ipm-simd", feature = "ipm-ocl"))]
    const N_THREADS: usize = 0;

    let mut solver_setups = Vec::new();

    for optimality in [1e-3, 1e-4, 1e-5, 1e-6, 1e-7, 1e-8] {
        #[cfg(feature = "ipm-simd")]
        solver_setups.push(SolverSetup {
            setting: SolverSetting::IpmSimdF64x4(
                SimdIpmSolverSettingsBuilder::default()
                    .optimality(optimality)
                    .parallel()
                    .threads(N_THREADS)
                    .build(),
            ),
            name: format!("opt-tol-{optimality:e}"),
        });
        #[cfg(feature = "ipm-ocl")]
        solver_setups.push(SolverSetup {
            setting: SolverSetting::IpmOcl(
                ClIpmSolverSettingsBuilder::default()
                    .optimality(optimality)
                    .parallel()
                    .threads(N_THREADS)
                    .build(),
            ),
            name: format!("opt-tol-{optimality:e}"),
        });
    }

    random_benchmark(
        c,
        "random-models-ipm-convergence",
        &[20],
        &[5],
        &[256, 32768],
        &solver_setups,
        Some(10),
    )
}

#[cfg(feature = "ipm-ocl")]
fn bench_ocl_chunks(c: &mut Criterion) {
    #[cfg(feature = "ipm-ocl")]
    const N_THREADS: usize = 0;

    let mut solver_setups = Vec::new();

    let num_chunks = vec![1, 2, 4, 8, 16];

    for num_chunks in num_chunks {
        solver_setups.push(SolverSetup {
            setting: SolverSetting::IpmOcl(
                ClIpmSolverSettingsBuilder::default()
                    .parallel()
                    .threads(N_THREADS)
                    .num_chunks(NonZeroUsize::new(num_chunks).unwrap())
                    .build(),
            ),
            name: format!("num-chunks-{num_chunks}"),
        });
    }

    random_benchmark(
        c,
        "random-models-ocl-chunks",
        &[20],
        &[5],
        &[32768],
        &solver_setups,
        Some(10),
    )
}

#[cfg(not(feature = "ipm-ocl"))]
fn bench_ocl_chunks(_c: &mut Criterion) {}

#[cfg(not(any(feature = "ipm-simd", feature = "ipm-ocl")))]
fn bench_ipm_convergence(_c: &mut Criterion) {}

/// Benchmark a large number of scenarios using various solvers
fn bench_hyper_scenarios(c: &mut Criterion) {
    // Go from largest to smallest
    let scenarios: Vec<usize> = (10..21).map(|p| 2_usize.pow(p)).rev().collect();

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
        #[cfg(feature = "ipm-simd")]
        SolverSetup {
            setting: SolverSetting::IpmSimdF64x4(
                SimdIpmSolverSettingsBuilder::default()
                    .parallel()
                    .threads(N_THREADS)
                    .build(),
            ),
            name: "default".to_string(),
        },
        #[cfg(feature = "ipm-ocl")]
        SolverSetup {
            setting: SolverSetting::IpmOcl(
                ClIpmSolverSettingsBuilder::default()
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
        &[20],
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
