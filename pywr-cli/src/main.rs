mod tracing;

use crate::tracing::setup_tracing;
use ::tracing::info;
use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
#[cfg(feature = "ipm-ocl")]
use pywr_core::solvers::{ClIpmF32Solver, ClIpmF64Solver, ClIpmSolverSettings};
use pywr_core::solvers::{ClpSolver, ClpSolverSettings};
#[cfg(feature = "highs")]
use pywr_core::solvers::{HighsSolver, HighsSolverSettings};
#[cfg(feature = "ipm-simd")]
use pywr_core::solvers::{SimdIpmF64Solver, SimdIpmSolverSettings};
use pywr_core::test_utils::make_random_model;
use pywr_schema::model::{PywrModel, PywrMultiNetworkModel};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

#[derive(Copy, Clone, ValueEnum)]
enum Solver {
    Clp,
    #[cfg(feature = "highs")]
    HIGHS,
    #[cfg(feature = "ipm-ocl")]
    CLIPMF32,
    #[cfg(feature = "ipm-ocl")]
    CLIPMF64,
    #[cfg(feature = "ipm-simd")]
    IpmSimd,
}

impl Display for Solver {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Solver::Clp => write!(f, "clp"),
            #[cfg(feature = "highs")]
            Solver::HIGHS => write!(f, "highs"),
            #[cfg(feature = "ipm-ocl")]
            Solver::CLIPMF32 => write!(f, "clipmf32"),
            #[cfg(feature = "ipm-ocl")]
            Solver::CLIPMF64 => write!(f, "clipmf64"),
            #[cfg(feature = "ipm-simd")]
            Solver::IpmSimd => write!(f, "ipm-simd"),
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
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Convert {
        /// Path to Pywr v1.x JSON.
        model: PathBuf,
        /// Stop if there is an error converting the model.
        #[arg(short, long, default_value_t = false)]
        stop_on_error: bool,
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
        /// Use multiple threads for simulation.
        #[arg(short, long, default_value_t = false)]
        parallel: bool,
        /// The number of threads to use in parallel simulation.
        #[arg(short, long, default_value_t = 1)]
        threads: usize,
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
        /// Use multiple threads for simulation.
        #[arg(short, long, default_value_t = false)]
        parallel: bool,
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    setup_tracing(cli.debug).unwrap();

    match &cli.command {
        Some(command) => match command {
            Commands::Convert { model, stop_on_error } => convert(model, *stop_on_error),
            Commands::Run {
                model,
                solver,
                data_path,
                output_path,
                parallel: _,
                threads: _,
            } => run(model, solver, data_path.as_deref(), output_path.as_deref()),
            Commands::RunMulti {
                model,
                solver,
                data_path,
                output_path,
                parallel: _,
                threads: _,
            } => run_multi(model, solver, data_path.as_deref(), output_path.as_deref()),
            Commands::RunRandom {
                num_systems,
                density,
                num_scenarios,
                solver,
            } => run_random(*num_systems, *density, *num_scenarios, solver),
        },
        None => {}
    }

    Ok(())
}

fn convert(path: &Path, stop_on_error: bool) {
    if path.is_dir() {
        for entry in path.read_dir().expect("read_dir call failed").flatten() {
            let path = entry.path();
            if path.is_file()
                && (path.extension().unwrap() == "json")
                && (!path.file_stem().unwrap().to_str().unwrap().contains("_v2"))
            {
                v1_to_v2(&path, stop_on_error);
            }
        }
    } else {
        v1_to_v2(path, stop_on_error);
    }
}

fn v1_to_v2(path: &Path, stop_on_error: bool) {
    info!("Model: {}", path.display());

    let data = std::fs::read_to_string(path).unwrap();
    // Load the v1 schema
    let schema: pywr_v1_schema::PywrModel = serde_json::from_str(data.as_str()).unwrap();
    // Convert to v2 schema and collect any errors
    let (schema_v2, errors) = PywrModel::from_v1(schema);

    if !errors.is_empty() {
        info!("Model converted with {} errors:", errors.len());
        for error in errors {
            info!("  {}", error);
        }
        if stop_on_error {
            return;
        }
    } else {
        info!("Model converted with zero errors!");
    }

    // There must be a better way to do this!!
    let mut new_file_name = path.file_stem().unwrap().to_os_string();
    new_file_name.push("_v2");
    let mut new_file_name = PathBuf::from(new_file_name);
    new_file_name.set_extension("json");
    let new_file_pth = path.parent().unwrap().join(new_file_name);

    std::fs::write(new_file_pth, serde_json::to_string_pretty(&schema_v2).unwrap()).unwrap();
}

fn run(path: &Path, solver: &Solver, data_path: Option<&Path>, output_path: Option<&Path>) {
    let data = std::fs::read_to_string(path).unwrap();
    let data_path = data_path.or_else(|| path.parent());
    let schema_v2: PywrModel = serde_json::from_str(data.as_str()).unwrap();

    let model = schema_v2.build_model(data_path, output_path).unwrap();

    match *solver {
        Solver::Clp => model.run::<ClpSolver>(&ClpSolverSettings::default()),
        #[cfg(feature = "highs")]
        Solver::HIGHS => model.run::<HighsSolver>(&HighsSolverSettings::default()),
        #[cfg(feature = "ipm-ocl")]
        Solver::CLIPMF32 => model.run_multi_scenario::<ClIpmF32Solver>(&ClIpmSolverSettings::default()),
        #[cfg(feature = "ipm-ocl")]
        Solver::CLIPMF64 => model.run_multi_scenario::<ClIpmF64Solver>(&ClIpmSolverSettings::default()),
        #[cfg(feature = "ipm-simd")]
        Solver::IpmSimd => model.run_multi_scenario::<SimdIpmF64Solver<4>>(&SimdIpmSolverSettings::default()),
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
        Solver::HIGHS => model.run::<HighsSolver>(&HighsSolverSettings::default()),
        #[cfg(feature = "ipm-ocl")]
        Solver::CLIPMF32 => model.run_multi_scenario::<ClIpmF32Solver>(&ClIpmSolverSettings::default()),
        #[cfg(feature = "ipm-ocl")]
        Solver::CLIPMF64 => model.run_multi_scenario::<ClIpmF64Solver>(&ClIpmSolverSettings::default()),
        #[cfg(feature = "ipm-simd")]
        Solver::IpmSimd => model.run_multi_scenario::<SimdIpmF64Solver<4>>(&SimdIpmSolverSettings::default()),
    }
    .unwrap();
}

fn run_random(num_systems: usize, density: usize, num_scenarios: usize, solver: &Solver) {
    let mut rng = ChaCha8Rng::seed_from_u64(0);
    let model = make_random_model(num_systems, density, num_scenarios, &mut rng).unwrap();

    match *solver {
        Solver::Clp => model.run::<ClpSolver>(&ClpSolverSettings::default()),
        #[cfg(feature = "highs")]
        Solver::HIGHS => model.run::<HighsSolver>(&HighsSolverSettings::default()),
        #[cfg(feature = "ipm-ocl")]
        Solver::CLIPMF32 => model.run_multi_scenario::<ClIpmF32Solver>(&ClIpmSolverSettings::default()),
        #[cfg(feature = "ipm-ocl")]
        Solver::CLIPMF64 => model.run_multi_scenario::<ClIpmF64Solver>(&ClIpmSolverSettings::default()),
        #[cfg(feature = "ipm-simd")]
        Solver::IpmSimd => model.run_multi_scenario::<SimdIpmF64Solver<4>>(&SimdIpmSolverSettings::default()),
    }
    .unwrap();
}
