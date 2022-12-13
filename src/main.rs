use clap::Parser;
use pywr::model::Model;
use pywr::scenario::ScenarioGroupCollection;
use pywr::schema::model::PywrModel;
use pywr::solvers::clp::{ClpSimplex, ClpSolver};
use pywr::solvers::Solver;
use pywr::timestep::Timestepper;
use std::path::PathBuf;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, value_name = "FILE")]
    model: PathBuf,
}

fn main() {
    let args = Args::parse();

    println!("Model: {}", args.model.display());

    let data = std::fs::read_to_string(args.model.clone()).unwrap();
    let schema: pywr_schema::PywrModel = serde_json::from_str(data.as_str()).unwrap();
    let schema_v2: PywrModel = schema.try_into().unwrap();

    // There must be a better way to do this!!
    let mut new_file_name = args.model.file_stem().unwrap().to_os_string();
    new_file_name.push("_v2");
    let mut new_file_name = PathBuf::from(new_file_name);
    new_file_name.set_extension("json");
    let new_file_pth = args.model.parent().unwrap().join(new_file_name);

    std::fs::write(new_file_pth, serde_json::to_string_pretty(&schema_v2).unwrap()).unwrap();

    let data = std::fs::read_to_string("test.json").unwrap();
    let schema_v2: PywrModel = serde_json::from_str(data.as_str()).unwrap();

    // TODO this should be part of the conversion below
    let mut scenario_groups = ScenarioGroupCollection::default();
    if let Some(scenarios) = &schema_v2.scenarios {
        for scenario in scenarios {
            scenario_groups.add_group(&scenario.name, scenario.size)
        }
    }

    let (mut model, timestepper): (Model, Timestepper) = schema_v2.try_into_model(None).unwrap();

    let mut solver: Box<dyn Solver> = Box::new(ClpSolver::<ClpSimplex>::default());

    model.run(timestepper, scenario_groups, &mut solver).unwrap();
}
