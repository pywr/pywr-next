mod tracing;

use crate::tracing::setup_tracing;
use ::tracing::info;
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
#[cfg(feature = "cbc")]
use pywr_core::solvers::{CbcSolver, CbcSolverSettings, CbcSolverSettingsBuilder};
#[cfg(feature = "ipm-ocl")]
use pywr_core::solvers::{ClIpmF32Solver, ClIpmF64Solver, ClIpmSolverSettings, ClIpmSolverSettingsBuilder};
use pywr_core::solvers::{ClpSolver, ClpSolverSettings, ClpSolverSettingsBuilder};
#[cfg(feature = "highs")]
use pywr_core::solvers::{HighsSolver, HighsSolverSettings, HighsSolverSettingsBuilder};
#[cfg(feature = "microlp")]
use pywr_core::solvers::{MicroLpSolver, MicroLpSolverSettings, MicroLpSolverSettingsBuilder};
#[cfg(feature = "ipm-simd")]
use pywr_core::solvers::{SimdIpmF64Solver, SimdIpmSolverSettings, SimdIpmSolverSettingsBuilder};
use pywr_core::test_utils::make_random_model;
use pywr_schema::{ComponentConversionError, PywrModel, PywrMultiNetworkModel, PywrNetwork};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use schemars::schema_for;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

#[derive(Copy, Clone, ValueEnum)]
enum Solver {
    Clp,
    #[cfg(feature = "highs")]
    Highs,
    #[cfg(feature = "cbc")]
    Cbc,
    #[cfg(feature = "ipm-ocl")]
    CLIPMF32,
    #[cfg(feature = "ipm-ocl")]
    CLIPMF64,
    #[cfg(feature = "ipm-simd")]
    IpmSimd,
    #[cfg(feature = "microlp")]
    Microlp,
}

impl Display for Solver {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Solver::Clp => write!(f, "clp"),
            #[cfg(feature = "highs")]
            Solver::Highs => write!(f, "highs"),
            #[cfg(feature = "cbc")]
            Solver::Cbc => write!(f, "cbc"),
            #[cfg(feature = "ipm-ocl")]
            Solver::CLIPMF32 => write!(f, "clipmf32"),
            #[cfg(feature = "ipm-ocl")]
            Solver::CLIPMF64 => write!(f, "clipmf64"),
            #[cfg(feature = "ipm-simd")]
            Solver::IpmSimd => write!(f, "ipm-simd"),
            #[cfg(feature = "microlp")]
            Solver::Microlp => write!(f, "microlp"),
        }
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Turn debugging information on
    #[arg(long, default_value_t = false)]
    debug: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Convert {
        /// Path to Pywr v1.x JSON.
        input: PathBuf,
        /// Path to output Pywr v2 JSON.
        // TODO support printing to stdout?
        output: PathBuf,
        /// Stop if there is an error converting the model.
        #[arg(short, long, default_value_t = false)]
        stop_on_error: bool,
        /// Convert only the network schema.
        #[arg(short, long, default_value_t = false)]
        network_only: bool,
    },

    Run {
        /// Path to Pywr model JSON.
        model: PathBuf,
        /// Solver to use.
        #[arg(short, long, default_value_t=Solver::Clp)]
        solver: Solver,
        #[arg(short, long)]
        data_path: Option<PathBuf>,
        #[arg(short, long)]
        output_path: Option<PathBuf>,
        /// The number of threads to use in parallel simulation.
        #[arg(short, long, default_value_t = 1)]
        threads: usize,
        /// Ignore the feature requirements of a solver.
        #[arg(short, long, default_value_t = false)]
        ignore_feature_requirements: bool,
    },
    RunMulti {
        /// Path to Pywr model JSON.
        model: PathBuf,
        /// Solver to use.
        #[arg(short, long, default_value_t=Solver::Clp)]
        solver: Solver,
        #[arg(short, long)]
        data_path: Option<PathBuf>,
        #[arg(short, long)]
        output_path: Option<PathBuf>,
        /// The number of threads to use in parallel simulation.
        #[arg(short, long, default_value_t = 1)]
        threads: usize,
    },
    RunRandom {
        num_systems: usize,
        density: usize,
        num_scenarios: usize,
        /// Solver to use.
        #[arg(short, long, default_value_t=Solver::Clp)]
        solver: Solver,
    },
    ExportSchema {
        /// Path to save the JSON schema.
        out: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    setup_tracing(cli.debug)?;

    match &cli.command {
        Commands::Convert {
            input,
            output,
            stop_on_error,
            network_only,
        } => convert(input, output, *stop_on_error, *network_only)?,
        Commands::Run {
            model,
            solver,
            data_path,
            output_path,
            threads,
            ignore_feature_requirements,
        } => run(
            model,
            solver,
            data_path.as_deref(),
            output_path.as_deref(),
            *threads,
            *ignore_feature_requirements,
        ),
        Commands::RunMulti {
            model,
            solver,
            data_path,
            output_path,
            threads: _,
        } => run_multi(model, solver, data_path.as_deref(), output_path.as_deref()),
        Commands::RunRandom {
            num_systems,
            density,
            num_scenarios,
            solver,
        } => run_random(*num_systems, *density, *num_scenarios, solver),
        Commands::ExportSchema { out } => export_schema(out)?,
    }

    Ok(())
}

fn convert(in_path: &Path, out_path: &Path, stop_on_error: bool, network_only: bool) -> Result<()> {
    if in_path.is_dir() {
        if !out_path.is_dir() {
            bail!("Output path must be an existing directory when input path is a directory");
        }

        for entry in in_path
            .read_dir()
            .with_context(|| format!("Failed to read directory: {in_path:?}",))?
            .flatten()
        {
            let path = entry.path();

            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "json" {
                        let out_fn = out_path.join(
                            path.file_name()
                                .with_context(|| "Failed to determine output filename.".to_string())?,
                        );

                        v1_to_v2(&path, &out_fn, stop_on_error, network_only)?;
                    }
                }
            }
        }
    } else {
        if out_path.is_dir() {
            bail!("Output path must be a file when input path is a file");
        }

        v1_to_v2(in_path, out_path, stop_on_error, network_only)?;
    }

    Ok(())
}

fn v1_to_v2(in_path: &Path, out_path: &Path, stop_on_error: bool, network_only: bool) -> Result<()> {
    info!("Converting file: {}", in_path.display());

    let data = std::fs::read_to_string(in_path).with_context(|| format!("Failed to read file: {in_path:?}",))?;

    if network_only {
        let schema: pywr_v1_schema::PywrNetwork = serde_json::from_str(data.as_str())
            .with_context(|| format!("Failed deserialise Pywr v1 network file: {in_path:?}",))?;
        // Convert to v2 schema and collect any errors
        let (schema_v2, errors) = PywrNetwork::from_v1(schema);

        handle_conversion_errors(&errors, stop_on_error)?;

        std::fs::write(
            out_path,
            serde_json::to_string_pretty(&schema_v2).with_context(|| "Failed serialise Pywr v2 network".to_string())?,
        )
        .with_context(|| format!("Failed to write file: {out_path:?}",))?;
    } else {
        // Load the v1 schema
        let schema: pywr_v1_schema::PywrModel = serde_json::from_str(data.as_str())
            .with_context(|| format!("Failed deserialise Pywr v1 model file: {in_path:?}",))?;
        // Convert to v2 schema and collect any errors
        let (schema_v2, errors) = PywrModel::from_v1(schema);

        handle_conversion_errors(&errors, stop_on_error)?;

        std::fs::write(
            out_path,
            serde_json::to_string_pretty(&schema_v2).with_context(|| "Failed serialise Pywr v2 model".to_string())?,
        )
        .with_context(|| format!("Failed to write file: {out_path:?}",))?;
    }

    Ok(())
}

fn handle_conversion_errors(errors: &[ComponentConversionError], stop_on_error: bool) -> Result<()> {
    if !errors.is_empty() {
        info!("File converted with {} errors:", errors.len());
        for error in errors {
            info!("  {}", error);
        }
        if stop_on_error {
            bail!("File conversion failed with at-least one error!");
        }
    } else {
        info!("File converted with zero errors!");
    }

    Ok(())
}

fn run(
    path: &Path,
    solver: &Solver,
    data_path: Option<&Path>,
    output_path: Option<&Path>,
    threads: usize,
    ignore_feature_requirements: bool,
) {
    let data = std::fs::read_to_string(path).unwrap();
    let data_path = data_path.or_else(|| path.parent());
    let schema_v2: PywrModel = serde_json::from_str(data.as_str()).unwrap();

    let model = schema_v2.build_model(data_path, output_path).unwrap();

    match *solver {
        Solver::Clp => {
            let mut settings_builder = ClpSolverSettingsBuilder::default();
            if threads > 1 {
                settings_builder = settings_builder.parallel();
                settings_builder = settings_builder.threads(threads);
            }
            if ignore_feature_requirements {
                settings_builder = settings_builder.ignore_feature_requirements();
            }
            let settings = settings_builder.build();
            model.run::<ClpSolver>(&settings)
        }
        #[cfg(feature = "cbc")]
        Solver::Cbc => {
            let mut settings_builder = CbcSolverSettingsBuilder::default();
            if threads > 1 {
                settings_builder = settings_builder.parallel();
                settings_builder = settings_builder.threads(threads);
            }
            if ignore_feature_requirements {
                settings_builder = settings_builder.ignore_feature_requirements();
            }
            let settings = settings_builder.build();
            model.run::<CbcSolver>(&settings)
        }
        #[cfg(feature = "highs")]
        Solver::Highs => {
            let mut settings_builder = HighsSolverSettingsBuilder::default();
            if threads > 1 {
                settings_builder = settings_builder.parallel();
                settings_builder = settings_builder.threads(threads);
            }
            if ignore_feature_requirements {
                settings_builder = settings_builder.ignore_feature_requirements();
            }
            let settings = settings_builder.build();
            model.run::<HighsSolver>(&settings)
        }
        #[cfg(feature = "ipm-ocl")]
        Solver::CLIPMF32 => {
            let mut settings_builder = ClIpmSolverSettingsBuilder::default();
            if threads > 1 {
                settings_builder = settings_builder.parallel();
                settings_builder = settings_builder.threads(threads);
            }
            if ignore_feature_requirements {
                settings_builder = settings_builder.ignore_feature_requirements();
            }

            let settings = settings_builder.build();
            model.run_multi_scenario::<ClIpmF32Solver>(&settings)
        }
        #[cfg(feature = "ipm-ocl")]
        Solver::CLIPMF64 => {
            let mut settings_builder = ClIpmSolverSettingsBuilder::default();
            if threads > 1 {
                settings_builder = settings_builder.parallel();
                settings_builder = settings_builder.threads(threads);
            }
            if ignore_feature_requirements {
                settings_builder = settings_builder.ignore_feature_requirements();
            }

            let settings = settings_builder.build();
            model.run_multi_scenario::<ClIpmF64Solver>(&settings)
        }
        #[cfg(feature = "ipm-simd")]
        Solver::IpmSimd => {
            let mut settings_builder = SimdIpmSolverSettingsBuilder::default();
            if threads > 1 {
                settings_builder = settings_builder.parallel();
                settings_builder = settings_builder.threads(threads);
            }
            if ignore_feature_requirements {
                settings_builder = settings_builder.ignore_feature_requirements();
            }

            let settings = settings_builder.build();
            model.run_multi_scenario::<SimdIpmF64Solver>(&settings)
        }
        Solver::Microlp => {
            let mut settings_builder = MicroLpSolverSettingsBuilder::default();
            if threads > 1 {
                settings_builder = settings_builder.parallel();
                settings_builder = settings_builder.threads(threads);
            }
            if ignore_feature_requirements {
                settings_builder = settings_builder.ignore_feature_requirements();
            }
            let settings = settings_builder.build();
            model.run::<MicroLpSolver>(&settings)
        }
    }
    .unwrap();
}

fn run_multi(path: &Path, solver: &Solver, data_path: Option<&Path>, output_path: Option<&Path>) {
    let data = std::fs::read_to_string(path).unwrap();
    let data_path = data_path.or_else(|| path.parent());

    let schema_v2: PywrMultiNetworkModel = serde_json::from_str(data.as_str()).unwrap();

    let model = schema_v2.build_model(data_path, output_path).unwrap();

    match *solver {
        Solver::Clp => model.run::<ClpSolver>(&ClpSolverSettings::default()),
        #[cfg(feature = "highs")]
        Solver::Highs => model.run::<HighsSolver>(&HighsSolverSettings::default()),
        #[cfg(feature = "cbc")]
        Solver::Cbc => model.run::<CbcSolver>(&CbcSolverSettings::default()),
        #[cfg(feature = "ipm-ocl")]
        Solver::CLIPMF32 => model.run_multi_scenario::<ClIpmF32Solver>(&ClIpmSolverSettings::default()),
        #[cfg(feature = "ipm-ocl")]
        Solver::CLIPMF64 => model.run_multi_scenario::<ClIpmF64Solver>(&ClIpmSolverSettings::default()),
        #[cfg(feature = "ipm-simd")]
        Solver::IpmSimd => model.run_multi_scenario::<SimdIpmF64Solver>(&SimdIpmSolverSettings::default()),
        #[cfg(feature = "microlp")]
        Solver::Microlp => model.run::<MicroLpSolver>(&MicroLpSolverSettings::default()),
    }
    .unwrap();
}

fn run_random(num_systems: usize, density: usize, num_scenarios: usize, solver: &Solver) {
    let mut rng = ChaCha8Rng::seed_from_u64(0);
    let model = make_random_model(num_systems, density, num_scenarios, &mut rng).unwrap();

    match *solver {
        Solver::Clp => model.run::<ClpSolver>(&ClpSolverSettings::default()),
        #[cfg(feature = "highs")]
        Solver::Highs => model.run::<HighsSolver>(&HighsSolverSettings::default()),
        #[cfg(feature = "cbc")]
        Solver::Cbc => model.run::<CbcSolver>(&CbcSolverSettings::default()),
        #[cfg(feature = "ipm-ocl")]
        Solver::CLIPMF32 => model.run_multi_scenario::<ClIpmF32Solver>(&ClIpmSolverSettings::default()),
        #[cfg(feature = "ipm-ocl")]
        Solver::CLIPMF64 => model.run_multi_scenario::<ClIpmF64Solver>(&ClIpmSolverSettings::default()),
        #[cfg(feature = "ipm-simd")]
        Solver::IpmSimd => model.run_multi_scenario::<SimdIpmF64Solver>(&SimdIpmSolverSettings::default()),
        #[cfg(feature = "microlp")]
        Solver::Microlp => model.run::<MicroLpSolver>(&MicroLpSolverSettings::default()),
    }
    .unwrap();
}

fn export_schema(out_path: &Path) -> Result<()> {
    let schema = schema_for!(PywrModel);
    std::fs::write(
        out_path,
        serde_json::to_string_pretty(&schema).with_context(|| "Failed serialise Pywr schema".to_string())?,
    )
    .with_context(|| format!("Failed to write file: {out_path:?}",))?;

    Ok(())
}
