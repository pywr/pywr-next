use clap::{Parser, Subcommand, ValueEnum};
use pywr::model::{Model, RunOptions};
use pywr::schema::model::PywrModel;
use pywr::solvers::{ClIpmF32Solver, ClIpmF64Solver, ClpSolver, HighsSolver};
use pywr::timestep::Timestepper;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

#[derive(Copy, Clone, ValueEnum)]
enum Solver {
    Clp,
    HIGHS,
    CLIPMF32,
    CLIPMF64,
}

impl Display for Solver {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Solver::Clp => write!(f, "clp"),
            Solver::HIGHS => write!(f, "highs"),
            Solver::CLIPMF32 => write!(f, "clipmf32"),
            Solver::CLIPMF64 => write!(f, "clipmf64"),
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
        /// Use multiple threads for simulation.
        #[arg(short, long, default_value_t = false)]
        parallel: bool,
        /// The number of threads to use in parallel simulation.
        #[arg(short, long, default_value_t = 1)]
        threads: usize,
    },
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Some(command) => match command {
            Commands::Convert { model } => convert(model),
            Commands::Run {
                model,
                solver,
                data_path,
                parallel,
                threads,
            } => {
                let options = if *parallel {
                    RunOptions::default().parallel().threads(*threads)
                } else {
                    RunOptions::default()
                };

                run(model, solver, data_path.as_deref(), &options)
            }
        },
        None => {}
    }
}

fn convert(path: &Path) {
    if path.is_dir() {
        for entry in path.read_dir().expect("read_dir call failed").flatten() {
            println!("{:?}", entry.path());

            let path = entry.path();
            if path.is_file()
                && (path.extension().unwrap() == "json")
                && (!path.file_stem().unwrap().to_str().unwrap().contains("_v2"))
            {
                v1_to_v2(&path);
            }
        }
    } else {
        v1_to_v2(path);
    }
}

fn v1_to_v2(path: &Path) {
    println!("Model: {}", path.display());

    let data = std::fs::read_to_string(path).unwrap();
    let schema: pywr_schema::PywrModel = serde_json::from_str(data.as_str()).unwrap();
    let schema_v2: PywrModel = schema.try_into().unwrap();

    // There must be a better way to do this!!
    let mut new_file_name = path.file_stem().unwrap().to_os_string();
    new_file_name.push("_v2");
    let mut new_file_name = PathBuf::from(new_file_name);
    new_file_name.set_extension("json");
    let new_file_pth = path.parent().unwrap().join(new_file_name);

    std::fs::write(new_file_pth, serde_json::to_string_pretty(&schema_v2).unwrap()).unwrap();
}

fn run(path: &Path, solver: &Solver, data_path: Option<&Path>, options: &RunOptions) {
    let data = std::fs::read_to_string(path).unwrap();
    let schema_v2: PywrModel = serde_json::from_str(data.as_str()).unwrap();

    let (model, timestepper): (Model, Timestepper) = schema_v2.try_into_model(data_path).unwrap();

    match *solver {
        Solver::Clp => model.run::<ClpSolver>(&timestepper, options),
        Solver::HIGHS => model.run::<HighsSolver>(&timestepper, options),
        Solver::CLIPMF32 => model.run_multi_scenario::<ClIpmF32Solver>(&timestepper),
        Solver::CLIPMF64 => model.run_multi_scenario::<ClIpmF64Solver>(&timestepper),
    }
    .unwrap();
}
