use criterion::measurement::Measurement;
/// Some simple benchmarks of random Pywr models.
///
/// The test models here are made up of a number of simple systems. Each system is three
/// node (input->link->output) model. A number of transfers between different systems'
/// link nodes are also generated. This makes for an overall model with some joint connectivity.
///
/// Benchmarks test the performance the solvers with different sized models (numbers of
/// systems and density of transfers between them), numbers of scenarios (which vary the
/// input flows) and number of CPU threads.
use criterion::{BatchSize, BenchmarkGroup, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use pywr_core::models::{Model, ModelTimings};
#[cfg(feature = "highs")]
use pywr_core::solvers::HighsSolverSettings;
#[cfg(any(feature = "ipm-simd", feature = "ipm-ocl"))]
use pywr_core::solvers::{BuiltInMultiStateSolverConfig, MultiStateSolverConfig};
use pywr_core::solvers::{BuiltInSolverConfig, ClpSolverSettings, ClpSolverSettingsBuilder, SolverConfig};
#[cfg(feature = "cbc")]
use pywr_core::solvers::{CbcSolverSettings, CbcSolverSettingsBuilder};
#[cfg(feature = "ipm-ocl")]
use pywr_core::solvers::{ClIpmSolverSettings, ClIpmSolverSettingsBuilder};
#[cfg(feature = "ipm-simd")]
use pywr_core::solvers::{SimdIpmSolverSettings, SimdIpmSolverSettingsBuilder};
use pywr_core::test_utils::make_random_model_builder;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
#[cfg(feature = "ipm-ocl")]
use std::num::NonZeroUsize;

struct ModelExperiment<'m> {
    name: String,
    parameter: String,
    model: &'m Model,
}

impl<'m> ModelExperiment<'m> {
    fn new(model: &'m Model, name: &str, parameter: &str) -> Self {
        Self {
            name: name.to_string(),
            parameter: parameter.to_string(),
            model,
        }
    }

    /// Undertake a "run only" benchmark, where the model has already been setup and we just want to time the run.
    fn benchmark_run_only<C, M>(&self, group: &mut BenchmarkGroup<M>, solver_config: &C)
    where
        C: SolverConfig,
        M: Measurement,
    {
        group.bench_function(
            BenchmarkId::new(format!("{}-run-only", self.name), &self.parameter),
            |b| {
                b.iter_batched(
                    || {
                        // Do the setup here outside of the time-step loop
                        let state = self.model.setup(solver_config).expect("Failed to setup the model.");
                        let timings = ModelTimings::new_with_component_timings(self.model);
                        (state, solver_config, timings)
                    },
                    |(mut state, solver_config, mut timings)| {
                        self.model
                            .run_with_state(&mut state, solver_config, &mut timings)
                            .expect("Failed to run the model.")
                    },
                    BatchSize::LargeInput,
                )
            },
        );
    }

    /// Undertake a "run only" benchmark, where the model has already been setup and we just want to time the run.
    #[cfg(any(feature = "ipm-simd", feature = "ipm-ocl"))]
    fn benchmark_run_only_multi<C, M>(&self, group: &mut BenchmarkGroup<M>, solver_config: &C)
    where
        C: MultiStateSolverConfig,
        M: Measurement,
    {
        group.bench_function(
            BenchmarkId::new(format!("{}-run-only", self.name), &self.parameter),
            |b| {
                b.iter_batched(
                    || {
                        // Do the setup here outside of the time-step loop
                        let state = self
                            .model
                            .setup_multi_scenario(solver_config)
                            .expect("Failed to setup the model.");
                        let timings = ModelTimings::new_with_component_timings(self.model);
                        (state, solver_config, timings)
                    },
                    |(mut state, solver_config, mut timings)| {
                        self.model
                            .run_multi_scenario_with_state(&mut state, solver_config, &mut timings)
                            .expect("Failed to run the model.")
                    },
                    BatchSize::LargeInput,
                )
            },
        );
    }
}

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
                let model = make_random_model_builder(n_sys, density, n_sc, &mut rng)
                    .build()
                    .expect("Failed to build random model!");
                let num_timesteps = model.domain().time().timesteps().len();

                // This is the number of time-steps
                group.throughput(Throughput::Elements((num_timesteps * n_sc) as u64));

                for solver_setup in solver_setups {
                    let parameter_string = format!(
                        "{} * {n_sys} * {density} * {n_sc}{}",
                        solver_setup.config.name(),
                        if solver_setup.label.is_empty() {
                            String::new()
                        } else {
                            format!(" * {}", solver_setup.label)
                        }
                    );

                    let experiment = ModelExperiment::new(&model, "random-model", &parameter_string);
                    match &solver_setup.config {
                        BenchmarkSolverConfig::PerScenario(config) => {
                            experiment.benchmark_run_only(&mut group, config);
                        }
                        #[cfg(any(feature = "ipm-simd", feature = "ipm-ocl"))]
                        BenchmarkSolverConfig::MultiScenario(config) => {
                            experiment.benchmark_run_only_multi(&mut group, config);
                        }
                    }
                }
            }
        }
    }

    group.finish();
}

enum BenchmarkSolverConfig {
    PerScenario(BuiltInSolverConfig),
    #[cfg(any(feature = "ipm-simd", feature = "ipm-ocl"))]
    MultiScenario(BuiltInMultiStateSolverConfig),
}

impl BenchmarkSolverConfig {
    fn name(&self) -> &str {
        match self {
            BenchmarkSolverConfig::PerScenario(config) => config.name(),
            #[cfg(any(feature = "ipm-simd", feature = "ipm-ocl"))]
            BenchmarkSolverConfig::MultiScenario(config) => config.name(),
        }
    }
}

impl From<BuiltInSolverConfig> for BenchmarkSolverConfig {
    fn from(config: BuiltInSolverConfig) -> Self {
        BenchmarkSolverConfig::PerScenario(config)
    }
}

#[cfg(any(feature = "ipm-simd", feature = "ipm-ocl"))]
impl From<BuiltInMultiStateSolverConfig> for BenchmarkSolverConfig {
    fn from(config: BuiltInMultiStateSolverConfig) -> Self {
        BenchmarkSolverConfig::MultiScenario(config)
    }
}

struct SolverSetup {
    config: BenchmarkSolverConfig,
    label: String,
}

fn default_solver_setups() -> Vec<SolverSetup> {
    vec![
        #[cfg(feature = "highs")]
        SolverSetup {
            config: BuiltInSolverConfig::Highs(HighsSolverSettings::default()).into(),
            label: "".to_string(),
        },
        SolverSetup {
            config: BuiltInSolverConfig::Clp(ClpSolverSettings::default()).into(),
            label: "".to_string(),
        },
        #[cfg(feature = "cbc")]
        SolverSetup {
            config: BuiltInSolverConfig::Cbc(CbcSolverSettings::default()).into(),
            label: "".to_string(),
        },
        #[cfg(feature = "ipm-simd")]
        SolverSetup {
            config: BuiltInMultiStateSolverConfig::SimdIpm(SimdIpmSolverSettings::default()).into(),
            label: "".to_string(),
        },
        #[cfg(feature = "ipm-ocl")]
        SolverSetup {
            config: BuiltInMultiStateSolverConfig::ClIpmF64(ClIpmSolverSettings::default()).into(),
            label: "".to_string(),
        },
    ]
}

fn bench_system_size(c: &mut Criterion) {
    let solver_setups = default_solver_setups();

    random_benchmark(c, "random-models-size", &[10, 20, 50], &[5], &[1], &solver_setups, None)
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
            config: BuiltInSolverConfig::Clp(
                ClpSolverSettingsBuilder::default()
                    .parallel()
                    .threads(n_threads)
                    .build(),
            )
            .into(),
            label: format!("threads-{n_threads}",),
        });

        #[cfg(feature = "cbc")]
        solver_setups.push(SolverSetup {
            config: BuiltInSolverConfig::Cbc(
                CbcSolverSettingsBuilder::default()
                    .parallel()
                    .threads(n_threads)
                    .build(),
            )
            .into(),
            label: format!("threads-{n_threads}",),
        });

        #[cfg(feature = "ipm-simd")]
        solver_setups.push(SolverSetup {
            config: BuiltInMultiStateSolverConfig::SimdIpm(
                SimdIpmSolverSettingsBuilder::default()
                    .parallel()
                    .threads(n_threads)
                    .build(),
            )
            .into(),
            label: format!("threads-{n_threads}"),
        });

        #[cfg(feature = "ipm-ocl")]
        solver_setups.push(SolverSetup {
            config: BuiltInMultiStateSolverConfig::ClIpmF64(
                ClIpmSolverSettingsBuilder::default()
                    .parallel()
                    .threads(n_threads)
                    .build(),
            )
            .into(),
            label: format!("threads-{n_threads}"),
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
            config: BuiltInMultiStateSolverConfig::SimdIpm(
                SimdIpmSolverSettingsBuilder::default()
                    .optimality(optimality)
                    .parallel()
                    .threads(N_THREADS)
                    .build(),
            )
            .into(),
            label: format!("opt-tol-{optimality:e}"),
        });
        #[cfg(feature = "ipm-ocl")]
        solver_setups.push(SolverSetup {
            config: BuiltInMultiStateSolverConfig::ClIpmF64(
                ClIpmSolverSettingsBuilder::default()
                    .optimality(optimality)
                    .parallel()
                    .threads(N_THREADS)
                    .build(),
            )
            .into(),
            label: format!("opt-tol-{optimality:e}"),
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
            config: BuiltInMultiStateSolverConfig::ClIpmF64(
                ClIpmSolverSettingsBuilder::default()
                    .parallel()
                    .threads(N_THREADS)
                    .num_chunks(NonZeroUsize::new(num_chunks).unwrap())
                    .build(),
            )
            .into(),
            label: format!("num-chunks-{num_chunks}"),
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
            config: BuiltInSolverConfig::Clp(
                ClpSolverSettingsBuilder::default()
                    .parallel()
                    .threads(N_THREADS)
                    .build(),
            )
            .into(),
            label: "default".to_string(),
        },
        #[cfg(feature = "cbc")]
        SolverSetup {
            config: BuiltInSolverConfig::Cbc(
                CbcSolverSettingsBuilder::default()
                    .parallel()
                    .threads(N_THREADS)
                    .build(),
            )
            .into(),
            label: "default".to_string(),
        },
        #[cfg(feature = "ipm-simd")]
        SolverSetup {
            config: BuiltInMultiStateSolverConfig::SimdIpm(
                SimdIpmSolverSettingsBuilder::default()
                    .parallel()
                    .threads(N_THREADS)
                    .build(),
            )
            .into(),
            label: "default".to_string(),
        },
        #[cfg(feature = "ipm-ocl")]
        SolverSetup {
            config: BuiltInMultiStateSolverConfig::ClIpmF64(
                ClIpmSolverSettingsBuilder::default()
                    .parallel()
                    .threads(N_THREADS)
                    .build(),
            )
            .into(),
            label: "default".to_string(),
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
