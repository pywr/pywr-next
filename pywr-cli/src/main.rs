mod tracing;

use crate::tracing::setup_tracing;
use anyhow::{Context, Result};
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
use pywr_schema::ConversionError;
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
    // /// Optional name to operate on
    // name: Option<String>,
    //
    // /// Sets a custom config file
    // #[arg(short, long, value_name = "FILE")]
    // config: Option<PathBuf>,
    //
    // /// Turn debugging information on
    // #[arg(short, long, action = clap::ArgAction::Count)]
    // debug: u8,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Convert {
        /// Path to Pywr v1.x JSON.
        model: PathBuf,
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
        #[arg(long, default_value_t = false)]
        debug: bool,
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
        #[arg(long, default_value_t = false)]
        debug: bool,
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

    match &cli.command {
        Some(command) => match command {
            Commands::Convert { model } => convert(model)?,
            Commands::Run {
                model,
                solver,
                data_path,
                output_path,
                parallel: _,
                threads: _,
                debug,
            } => run(model, solver, data_path.as_deref(), output_path.as_deref(), *debug),
            Commands::RunMulti {
                model,
                solver,
                data_path,
                output_path,
                parallel: _,
                threads: _,
                debug,
            } => run_multi(model, solver, data_path.as_deref(), output_path.as_deref(), *debug),
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

fn convert(path: &Path) -> Result<()> {
    if path.is_dir() {
        for entry in path.read_dir().expect("read_dir call failed").flatten() {
            let path = entry.path();
            if path.is_file()
                && (path.extension().unwrap() == "json")
                && (!path.file_stem().unwrap().to_str().unwrap().contains("_v2"))
            {
                v1_to_v2(&path).with_context(|| format!("Could not convert model: `{:?}`", &path))?;
            }
        }
    } else {
        v1_to_v2(path).with_context(|| format!("Could not convert model: `{:?}`", path))?;
    }

    Ok(())
}

fn v1_to_v2(path: &Path) -> std::result::Result<(), ConversionError> {
    println!("Model: {}", path.display());

    let data = std::fs::read_to_string(path).unwrap();
    let schema: pywr_v1_schema::PywrModel = serde_json::from_str(data.as_str()).unwrap();
    let schema_v2: PywrModel = schema.try_into()?;

    // There must be a better way to do this!!
    let mut new_file_name = path.file_stem().unwrap().to_os_string();
    new_file_name.push("_v2");
    let mut new_file_name = PathBuf::from(new_file_name);
    new_file_name.set_extension("json");
    let new_file_pth = path.parent().unwrap().join(new_file_name);

    std::fs::write(new_file_pth, serde_json::to_string_pretty(&schema_v2).unwrap()).unwrap();

    Ok(())
}

fn run(path: &Path, solver: &Solver, data_path: Option<&Path>, output_path: Option<&Path>, debug: bool) {
    setup_tracing(debug).unwrap();

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

fn run_multi(path: &Path, solver: &Solver, data_path: Option<&Path>, output_path: Option<&Path>, debug: bool) {
    setup_tracing(debug).unwrap();

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
