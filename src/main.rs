use clap::{Parser, Subcommand};
use pywr::model::Model;
use pywr::schema::model::PywrModel;
use pywr::solvers::{ClIpmF32Solver, ClIpmF64Solver, ClpSolver, HighsSolver};
use pywr::timestep::Timestepper;
use std::path::{Path, PathBuf};

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
    /// does testing things
    Convert {
        /// lists test values
        #[arg(short, long, value_name = "FILE")]
        model: PathBuf,
    },

    Run {
        /// lists test values
        #[arg(short, long, value_name = "FILE")]
        model: PathBuf,
        #[arg(short, long, value_name = "SOLVER")]
        solver: String,
        #[arg(short, long, value_name = "PATH")]
        data_path: Option<PathBuf>,
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
            } => run(model, solver.as_str(), data_path.as_deref()),
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

fn run(path: &Path, solver: &str, data_path: Option<&Path>) {
    let data = std::fs::read_to_string(path).unwrap();
    let schema_v2: PywrModel = serde_json::from_str(data.as_str()).unwrap();

    let (model, timestepper): (Model, Timestepper) = schema_v2.try_into_model(data_path).unwrap();

    match solver {
        "clp" => model.run::<ClpSolver>(&timestepper),
        "highs" => model.run::<HighsSolver>(&timestepper),
        "clipm-f32" => model.run_multi_scenario::<ClIpmF32Solver>(&timestepper),
        "clipm-f64" => model.run_multi_scenario::<ClIpmF64Solver>(&timestepper),
        _ => panic!("Solver {} not recognised.", solver),
    }
    .unwrap();
}
