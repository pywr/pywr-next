use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use log::info;
use pywr_core::models::RunMetadata;
#[cfg(feature = "cbc")]
use pywr_core::solvers::{CbcSolverSettings, CbcSolverSettingsBuilder};
#[cfg(feature = "ipm-ocl")]
use pywr_core::solvers::{ClIpmF32Settings, ClIpmF32SettingsBuilder, ClIpmF64Settings, ClIpmF64SettingsBuilder};
use pywr_core::solvers::{ClpSolverSettings, ClpSolverSettingsBuilder};
#[cfg(feature = "highs")]
use pywr_core::solvers::{HighsSolverSettings, HighsSolverSettingsBuilder};
#[cfg(feature = "microlp")]
use pywr_core::solvers::{MicroLpSolverSettings, MicroLpSolverSettingsBuilder};
#[cfg(feature = "ipm-simd")]
use pywr_core::solvers::{SimdIpmSolverSettings, SimdIpmSolverSettingsBuilder};
use pywr_core::test_utils::make_random_model_builder;
use pywr_runner_service::RunnerServiceConfig;
use pywr_schema::{ComponentConversionError, FileSystem, ModelSchema, MultiNetworkModelSchema, NetworkSchema};
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

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum RunServerMode {
    /// Listen for connections on a namespaced local socket.
    LocalSocket,
    /// Serve one framed connection using process stdin and stdout.
    Stdio,
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Enabled debug level logging for Pywr.
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
        /// Name of the run, recorded in the metadata of the run's outputs.
        #[arg(long)]
        run_name: Option<String>,
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
        /// Name of the run, recorded in the metadata of the run's outputs.
        #[arg(long)]
        run_name: Option<String>,
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
    /// Run the Pywr model runner service over a local socket or standard I/O.
    RunServer {
        /// IPC transport to use.
        #[arg(long, value_enum, default_value_t = RunServerMode::LocalSocket)]
        mode: RunServerMode,
        /// Portable local-socket namespace name.
        ///
        /// This option applies only when `--mode local-socket` is selected.
        #[arg(long, default_value = "pywr-runner")]
        socket_name: String,
    },
    RunProject {
        /// Path to Pywr project JSON.
        project: PathBuf,
        /// Name of the model or multi-model definition to build.
        #[arg(short = 'd', long = "definition")]
        definition: String,
        /// Solver to use.
        #[arg(short, long, default_value_t=Solver::Clp)]
        solver: Solver,
        #[arg(long)]
        data_path: Option<PathBuf>,
        #[arg(short, long)]
        output_path: Option<PathBuf>,
        /// The number of threads to use in parallel simulation.
        #[arg(short, long, default_value_t = 1)]
        threads: usize,
        /// Ignore the feature requirements of a solver.
        #[arg(short, long, default_value_t = false)]
        ignore_feature_requirements: bool,
        /// Name of the run, recorded in the metadata of the run's outputs.
        #[arg(long)]
        run_name: Option<String>,
        /// Value for a project variable, overriding every definition's. Name a variable of a
        /// dependency as `<dependency>::<name>`. For a variable with choices, give a choice name;
        /// for a path variable without choices, a path relative to the project's directory. May be
        /// given more than once.
        #[arg(long = "var", value_name = "NAME=VALUE", value_parser = parse_variable)]
        variables: Vec<(String, String)>,
    },
    // InitProject {
    //     /// Location to initialise the project directory.
    //     location: PathBuf,
    //     /// Project filename (e.g. pywr-project.json).
    //     #[arg(short = 'f', long = "file", default_value = "pywr-project.json")]
    //     project_filename: String,
    //     /// Relative path for base model JSON.
    //     #[arg(short = 'b', long = "base", default_value = "models/base.json")]
    //     base_model_rel: String,
    //     /// Option set directories in the project (comma-separated name=dir pairs, e.g. "upgrades=options/upgrades,policy=options/policy").
    //     #[arg(short = 'o', long = "options")]
    //     option_sets: Vec<String>,
    // },
}

fn build_logger(debug: bool) -> env_logger::Logger {
    let mut builder = env_logger::Builder::new();

    builder.format_timestamp_micros().format_level(true);

    let level = if debug {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };

    builder
        .target(env_logger::Target::Stderr)
        .filter_module("pywr_v1_schema", level)
        .filter_module("pywr_core", level)
        .filter_module("pywr_schema", level)
        .filter_module("pywr_runner_service", level)
        .filter_module("pywr_cli", level);

    builder.build()
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    pywr_runner_service::install_log_router_with(Box::new(build_logger(cli.debug)))
        .map_err(|_| anyhow::anyhow!("a global logger has already been installed"))?;

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
            run_name,
        } => run(
            model,
            solver,
            data_path.as_deref(),
            output_path.as_deref(),
            *threads,
            *ignore_feature_requirements,
            run_name.clone(),
        )?,
        Commands::RunMulti {
            model,
            solver,
            data_path,
            output_path,
            threads: _,
            run_name,
        } => run_multi(
            model,
            solver,
            data_path.as_deref(),
            output_path.as_deref(),
            run_name.clone(),
        )?,
        Commands::RunRandom {
            num_systems,
            density,
            num_scenarios,
            solver,
        } => run_random(*num_systems, *density, *num_scenarios, solver)?,
        Commands::ExportSchema { out } => export_schema(out)?,
        Commands::RunServer { mode, socket_name } => {
            run_server(*mode, socket_name)?;
        }
        Commands::RunProject {
            project,
            definition,
            solver,
            data_path,
            output_path,
            threads,
            ignore_feature_requirements,
            run_name,
            variables,
        } => run_project(
            project,
            definition,
            solver,
            data_path.as_deref(),
            output_path.as_deref(),
            *threads,
            *ignore_feature_requirements,
            run_name.clone(),
            variables,
        )?,
        // Commands::InitProject {
        //     location,
        //     project_filename,
        //     base_model_rel,
        //     option_sets,
        // } => init_project_cli(location, project_filename, base_model_rel, option_sets)?,
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

            if path.is_file()
                && let Some(ext) = path.extension()
                && ext == "json"
            {
                let out_fn = out_path.join(
                    path.file_name()
                        .with_context(|| "Failed to determine output filename.".to_string())?,
                );

                v1_to_v2(&path, &out_fn, stop_on_error, network_only)?;
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
        let (schema_v2, errors) = NetworkSchema::from_v1(schema);

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
        let (schema_v2, errors) = ModelSchema::from_v1(schema);

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

/// Run `$model`, a `Model` or `MultiNetworkModel`, with the given solver settings.
macro_rules! run_with_solver {
    ($model:expr, $solver:expr, $threads:expr, $ignore_feature_requirements:expr) => {
        match *$solver {
            Solver::Clp => {
                let mut settings_builder = ClpSolverSettingsBuilder::default();
                if $threads > 1 {
                    settings_builder = settings_builder.parallel();
                    settings_builder = settings_builder.threads($threads);
                }
                if $ignore_feature_requirements {
                    settings_builder = settings_builder.ignore_feature_requirements();
                }
                let settings = settings_builder.build();
                $model.run(&settings)
            }
            #[cfg(feature = "cbc")]
            Solver::Cbc => {
                let mut settings_builder = CbcSolverSettingsBuilder::default();
                if $threads > 1 {
                    settings_builder = settings_builder.parallel();
                    settings_builder = settings_builder.threads($threads);
                }
                if $ignore_feature_requirements {
                    settings_builder = settings_builder.ignore_feature_requirements();
                }
                let settings = settings_builder.build();
                $model.run(&settings)
            }
            #[cfg(feature = "highs")]
            Solver::Highs => {
                let mut settings_builder = HighsSolverSettingsBuilder::default();
                if $threads > 1 {
                    settings_builder = settings_builder.parallel();
                    settings_builder = settings_builder.threads($threads);
                }
                if $ignore_feature_requirements {
                    settings_builder = settings_builder.ignore_feature_requirements();
                }
                let settings = settings_builder.build();
                $model.run(&settings)
            }
            #[cfg(feature = "ipm-ocl")]
            Solver::CLIPMF32 => {
                let mut settings_builder = ClIpmF32SettingsBuilder::default();
                if $threads > 1 {
                    settings_builder = settings_builder.parallel();
                    settings_builder = settings_builder.threads($threads);
                }
                if $ignore_feature_requirements {
                    settings_builder = settings_builder.ignore_feature_requirements();
                }

                let settings = settings_builder.build();
                $model.run_multi_scenario(&settings)
            }
            #[cfg(feature = "ipm-ocl")]
            Solver::CLIPMF64 => {
                let mut settings_builder = ClIpmF64SettingsBuilder::default();
                if $threads > 1 {
                    settings_builder = settings_builder.parallel();
                    settings_builder = settings_builder.threads($threads);
                }
                if $ignore_feature_requirements {
                    settings_builder = settings_builder.ignore_feature_requirements();
                }

                let settings = settings_builder.build();
                $model.run_multi_scenario(&settings)
            }
            #[cfg(feature = "ipm-simd")]
            Solver::IpmSimd => {
                let mut settings_builder = SimdIpmSolverSettingsBuilder::default();
                if $threads > 1 {
                    settings_builder = settings_builder.parallel();
                    settings_builder = settings_builder.threads($threads);
                }
                if $ignore_feature_requirements {
                    settings_builder = settings_builder.ignore_feature_requirements();
                }

                let settings = settings_builder.build();
                $model.run_multi_scenario(&settings)
            }
            #[cfg(feature = "microlp")]
            Solver::Microlp => {
                let mut settings_builder = MicroLpSolverSettingsBuilder::default();
                if $threads > 1 {
                    settings_builder = settings_builder.parallel();
                    settings_builder = settings_builder.threads($threads);
                }
                if $ignore_feature_requirements {
                    settings_builder = settings_builder.ignore_feature_requirements();
                }
                let settings = settings_builder.build();
                $model.run(&settings)
            }
        }
    };
}

/// Fail with every problem a model's validation found, one per line.
///
/// Building a model validates it too, but its error only gives the number of problems.
fn check_schema(validation: std::result::Result<(), pywr_schema::ValidationError>) -> Result<()> {
    validation.map_err(|error| anyhow::anyhow!("{}", error.report()))
}

/// Add every problem of a network that failed validation while the model was created.
///
/// [`check_schema`] can't see a network a multi-network model gives by path, as the network is
/// only read when the model is created, and that error only gives the number of problems.
fn with_network_report(error: anyhow::Error) -> anyhow::Error {
    let report = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<pywr_schema::NetworkValidationError>())
        .map(|validation| validation.report().to_string());

    match report {
        Some(report) => error.context(report),
        None => error,
    }
}

fn run(
    path: &Path,
    solver: &Solver,
    data_path: Option<&Path>,
    output_path: Option<&Path>,
    threads: usize,
    ignore_feature_requirements: bool,
    run_name: Option<String>,
) -> Result<()> {
    create_output_dir(output_path)?;
    let schema_v2 = ModelSchema::from_path(path).with_context(|| format!("Failed to load model: {path:?}"))?;
    let data_path = data_path.or_else(|| path.parent());

    check_schema(schema_v2.validate())?;
    let mut builder = schema_v2
        .create_model_builder(&FileSystem, data_path, output_path)
        .context("Failed to create the model from its schema")
        .map_err(with_network_report)?;
    if run_name.is_some() {
        builder.run_metadata(RunMetadata {
            run_name,
            ..Default::default()
        });
    }
    let model = builder.build().context("Failed to build the model")?;

    run_with_solver!(model, solver, threads, ignore_feature_requirements).context("Failed to run the model")?;

    Ok(())
}

fn run_multi(
    path: &Path,
    solver: &Solver,
    data_path: Option<&Path>,
    output_path: Option<&Path>,
    run_name: Option<String>,
) -> Result<()> {
    create_output_dir(output_path)?;
    let schema_v2 =
        MultiNetworkModelSchema::from_path(path).with_context(|| format!("Failed to load model: {path:?}"))?;
    let data_path = data_path.or_else(|| path.parent());

    check_schema(schema_v2.validate())?;
    let mut builder = schema_v2
        .create_model_builder(&FileSystem, data_path, output_path)
        .context("Failed to create the model from its schema")
        .map_err(with_network_report)?;
    if run_name.is_some() {
        builder.run_metadata(RunMetadata {
            run_name,
            ..Default::default()
        });
    }
    let model = builder.build().context("Failed to build the model")?;

    match *solver {
        Solver::Clp => model.run(&ClpSolverSettings::default()),
        #[cfg(feature = "highs")]
        Solver::Highs => model.run(&HighsSolverSettings::default()),
        #[cfg(feature = "cbc")]
        Solver::Cbc => model.run(&CbcSolverSettings::default()),
        #[cfg(feature = "ipm-ocl")]
        Solver::CLIPMF32 => model.run_multi_scenario(&ClIpmF32Settings::default()),
        #[cfg(feature = "ipm-ocl")]
        Solver::CLIPMF64 => model.run_multi_scenario(&ClIpmF64Settings::default()),
        #[cfg(feature = "ipm-simd")]
        Solver::IpmSimd => model.run_multi_scenario(&SimdIpmSolverSettings::default()),
        #[cfg(feature = "microlp")]
        Solver::Microlp => model.run(&MicroLpSolverSettings::default()),
    }
    .context("Failed to run the model")?;

    Ok(())
}

/// Create the output directory, if one is given, so outputs can be written into it.
fn create_output_dir(output_path: Option<&Path>) -> Result<()> {
    if let Some(output_path) = output_path {
        std::fs::create_dir_all(output_path)
            .with_context(|| format!("Failed to create output directory {output_path:?}"))?;
    }
    Ok(())
}

fn run_random(num_systems: usize, density: usize, num_scenarios: usize, solver: &Solver) -> Result<()> {
    let mut rng = ChaCha8Rng::seed_from_u64(0);
    let builder = make_random_model_builder(num_systems, density, num_scenarios, &mut rng);
    let model = builder.build().context("Failed to build the random model")?;

    match *solver {
        Solver::Clp => model.run(&ClpSolverSettings::default()),
        #[cfg(feature = "highs")]
        Solver::Highs => model.run(&HighsSolverSettings::default()),
        #[cfg(feature = "cbc")]
        Solver::Cbc => model.run(&CbcSolverSettings::default()),
        #[cfg(feature = "ipm-ocl")]
        Solver::CLIPMF32 => model.run_multi_scenario(&ClIpmF32Settings::default()),
        #[cfg(feature = "ipm-ocl")]
        Solver::CLIPMF64 => model.run_multi_scenario(&ClIpmF64Settings::default()),
        #[cfg(feature = "ipm-simd")]
        Solver::IpmSimd => model.run_multi_scenario(&SimdIpmSolverSettings::default()),
        #[cfg(feature = "microlp")]
        Solver::Microlp => model.run(&MicroLpSolverSettings::default()),
    }
    .context("Failed to run the model")?;

    Ok(())
}

fn export_schema(out_path: &Path) -> Result<()> {
    let schema = schema_for!(ModelSchema);
    std::fs::write(
        out_path,
        serde_json::to_string_pretty(&schema).with_context(|| "Failed serialise Pywr schema".to_string())?,
    )
    .with_context(|| format!("Failed to write file: {out_path:?}",))?;

    Ok(())
}

fn run_server(mode: RunServerMode, socket_name: &str) -> Result<()> {
    use pywr_runner_service::{RunnerServiceConfigBuilder, run_local_socket_server, run_stdio_server};

    let config: RunnerServiceConfig = RunnerServiceConfigBuilder::new().build();

    match mode {
        RunServerMode::LocalSocket => {
            info!("Starting Pywr runner service on socket: {socket_name}");
            run_local_socket_server(socket_name, config)
                .with_context(|| "Failed to run Pywr runner service".to_string())?;
        }
        RunServerMode::Stdio => {
            info!("Starting Pywr runner service over standard input/output");
            run_stdio_server(config).with_context(|| "Failed to run Pywr runner service".to_string())?;
        }
    }

    Ok(())
}

/// Parse a `--var` argument, `NAME=VALUE`, splitting at the first `=`.
fn parse_variable(argument: &str) -> std::result::Result<(String, String), String> {
    match argument.split_once('=') {
        Some((name, value)) if !name.is_empty() => Ok((name.to_string(), value.to_string())),
        _ => Err(format!("expected `NAME=VALUE`, got `{argument}`")),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_project(
    path: &Path,
    definition: &str,
    solver: &Solver,
    data_path: Option<&Path>,
    output_path: Option<&Path>,
    threads: usize,
    ignore_feature_requirements: bool,
    run_name: Option<String>,
    variables: &[(String, String)],
) -> Result<()> {
    let mut compose_options = pywr_project::ComposeOptions::default();
    for (name, value) in variables {
        if compose_options.variables.insert(name.clone(), value.clone()).is_some() {
            bail!("The variable `{name}` is given more than once with `--var`.");
        }
    }
    create_output_dir(output_path)?;
    let project = pywr_project::Project::open(path).with_context(|| format!("Failed to open project: {path:?}"))?;
    let data_path = data_path.or_else(|| path.parent());
    let options = pywr_schema::NetworkMergeOptions::default();

    let composed = project
        .compose_with(definition, &compose_options)
        .with_context(|| format!("Failed to compose definition `{definition}`"))?;
    // Hash the composed files just before loading them. Loading reads them again, so a file
    // changed in between would not match its recorded hash.
    let run_metadata = RunMetadata {
        run_name,
        project: Some(
            project
                .run_metadata(&composed)
                .context("Failed to hash the project's files for the run metadata")?,
        ),
        ..Default::default()
    };
    match composed {
        pywr_project::Composed::Model(composed) => {
            let schema = composed
                .load()
                .with_context(|| format!("Failed to load the schemas of model definition `{definition}`"))?
                .into_model_schema(&options)
                .with_context(|| format!("Failed to merge the networks of model definition `{definition}`"))?;
            check_schema(schema.validate())?;
            let mut builder = schema
                .create_model_builder(&FileSystem, data_path, output_path)
                .context("Failed to create the model from its schema")
                .map_err(with_network_report)?;
            builder.run_metadata(run_metadata);
            let model = builder.build().context("Failed to build the model")?;
            run_with_solver!(model, solver, threads, ignore_feature_requirements).context("Failed to run the model")?;
        }
        pywr_project::Composed::MultiModel(composed) => {
            let schema = composed
                .load()
                .with_context(|| format!("Failed to load the schemas of multi-model definition `{definition}`"))?
                .into_multi_model_schema(&options)
                .with_context(|| format!("Failed to merge the networks of multi-model definition `{definition}`"))?;
            check_schema(schema.validate())?;
            let mut builder = schema
                .create_model_builder(&FileSystem, data_path, output_path)
                .context("Failed to create the model from its schema")
                .map_err(with_network_report)?;
            builder.run_metadata(run_metadata);
            let model = builder.build().context("Failed to build the model")?;
            run_with_solver!(model, solver, threads, ignore_feature_requirements).context("Failed to run the model")?;
        }
    }

    Ok(())
}

// fn init_project_cli(
//     location: &Path,
//     project_filename: &str,
//     base_model_rel: &str,
//     option_sets: &[String],
// ) -> Result<()> {
//     use pywr_project::NetworkSet;
//     // Parse option set definitions from name=dir strings
//     let sets: Vec<NetworkSet> = option_sets
//         .iter()
//         .map(|s| {
//             let (name, dir) = s.split_once('=').unwrap_or((s.as_str(), s.as_str()));
//             NetworkSet {
//                 name: name.to_string(),
//                 dir: PathBuf::from(dir),
//                 min_files: None,
//                 max_files: None,
//             }
//         })
//         .collect();
//     let project_path =
//         pywr_project::Project::init_project(location, project_filename, Path::new(base_model_rel), &sets)?;
//     info!("Initialised project at {}", project_path.display());
//     Ok(())
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_server_mode_defaults_to_local_socket() {
        let cli = Cli::try_parse_from(["pywr", "run-server"]).unwrap();
        let Commands::RunServer { mode, socket_name } = cli.command else {
            panic!("expected run-server command");
        };
        assert_eq!(mode, RunServerMode::LocalSocket);
        assert_eq!(socket_name, "pywr-runner");
    }

    #[test]
    fn run_server_accepts_stdio_mode() {
        let cli = Cli::try_parse_from(["pywr", "run-server", "--mode", "stdio"]).unwrap();
        let Commands::RunServer { mode, .. } = cli.command else {
            panic!("expected run-server command");
        };
        assert_eq!(mode, RunServerMode::Stdio);
    }

    #[test]
    fn run_project_takes_repeated_variables() {
        let cli = Cli::try_parse_from([
            "pywr",
            "run-project",
            "pywr-project.json",
            "-d",
            "baseline",
            "--var",
            "dataset=stoch-01",
            "--var",
            "org::column=a=b",
        ])
        .unwrap();
        let Commands::RunProject { variables, .. } = cli.command else {
            panic!("expected run-project command");
        };
        assert_eq!(
            variables,
            [
                ("dataset".to_string(), "stoch-01".to_string()),
                ("org::column".to_string(), "a=b".to_string())
            ]
        );
        for invalid in ["dataset", "=x"] {
            assert!(Cli::try_parse_from(["pywr", "run-project", "p.json", "-d", "d", "--var", invalid]).is_err());
        }
    }

    /// A project whose model's inflow is read from the dataset chosen by its `dataset`
    /// variable: `a`, the default, with an inflow of 1, or `b`, with an inflow of 2.
    fn project_with_datasets(root: &Path) {
        let write = |path: &str, contents: String| {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        };
        for (dataset, inflow) in [("a", 1), ("b", 2)] {
            let rows: String = (1..=3).map(|day| format!("2021-01-0{day},{inflow}\n")).collect();
            write(&format!("data/{dataset}/inflow.csv"), format!("date,inflow1\n{rows}"));
        }
        let model = serde_json::json!({
            "metadata": {"title": "Datasets"},
            "time": {"start": "2021-01-01", "end": "2021-01-03", "timestep": {"type": "Days", "days": 1}},
            "network": {
                "nodes": [
                    {
                        "meta": {"name": "input"},
                        "type": "Input",
                        "max_flow": {"type": "TimeSeries", "name": "inflow", "columns": {"type": "Column", "name": "inflow1"}}
                    },
                    {
                        "meta": {"name": "output"},
                        "type": "Output",
                        "cost": {"type": "Literal", "value": -10.0},
                        "max_flow": {"type": "Literal", "value": 100.0}
                    }
                ],
                "edges": [{"from_node": "input", "to_node": "output"}],
                "time_series": [
                    {"meta": {"name": "inflow"}, "type": "Arrow", "format": "CSV", "time_col": "date", "path": "${dataset}/inflow.csv"}
                ],
                "metric_sets": [{"meta": {"name": "nodes"}, "metrics": [{"type": "Node", "name": "output"}]}],
                "outputs": [
                    {"meta": {"name": "nodes"}, "type": "CSV", "format": "Long", "filename": "flows.csv", "metric_set": "nodes"}
                ]
            }
        });
        write("base.json", model.to_string());
        let manifest = serde_json::json!({
            "format": "v1",
            "base_model": "base.json",
            "variables": [
                {"name": "dataset", "type": "path", "default": "a", "choices": {"a": "data/a", "b": "data/b"}}
            ],
            "models": [{"name": "baseline"}]
        });
        write("pywr-project.json", manifest.to_string());
    }

    /// Run the project in `root` with `variables`, returning the flow to the output node on each day.
    fn run_flows(root: &Path, variables: &[(&str, &str)]) -> Result<Vec<f64>> {
        let output = root.join(format!("outputs-{}", variables.len()));
        let variables: Vec<_> = variables
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        run_project(
            &root.join("pywr-project.json"),
            "baseline",
            &Solver::Clp,
            None,
            Some(&output),
            1,
            false,
            None,
            &variables,
        )?;
        let flows = std::fs::read_to_string(output.join("flows.csv")).unwrap();
        Ok(flows
            .lines()
            .skip(1)
            .map(|line| line.rsplit(',').next().unwrap().parse().unwrap())
            .collect())
    }

    #[test]
    fn run_project_reads_the_dataset_a_variable_chooses() {
        let root = tempfile::tempdir().unwrap();
        project_with_datasets(root.path());
        assert_eq!(run_flows(root.path(), &[]).unwrap(), [1.0, 1.0, 1.0]);
        assert_eq!(run_flows(root.path(), &[("dataset", "b")]).unwrap(), [2.0, 2.0, 2.0]);

        let error = run_flows(root.path(), &[("dataset", "c")]).unwrap_err();
        assert!(format!("{error:#}").contains("binds the variable `dataset` to `c`, which is not one of its choices"));
        let error = run_flows(root.path(), &[("dataset", "a"), ("dataset", "b")]).unwrap_err();
        assert!(error.to_string().contains("given more than once"), "{error:#}");
    }
}
