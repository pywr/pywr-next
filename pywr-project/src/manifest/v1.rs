use crate::composition::{
    Composed, ComposedDomain, ComposedModel, ComposedModelBuilder, ComposedMultiModel, ComposedMultiModelNetwork,
    ComposedNetwork, ComposedNetworkPath, LoadContext, PositionOffset,
};
use crate::error::{ComposeModelError, DefinitionKind, DefinitionRef, ManifestResolutionError};
use crate::layout::{
    NetworkFileLayout, NetworkSetLayout, ProjectLayout, ProjectLocation, SubNetworkLayout, unique_paths,
};
use crate::manifest::{DefinitionOverrides, DependencyPolicy, ResolveOptions};
use pywr_schema::meta::ProvenanceSource;
use pywr_schema::model::{MultiNetworkTransfer, ScenarioDomain, TimeDomain};
use relative_path::{RelativePath, RelativePathBuf};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

/// A validation error for a project manifest.
#[derive(Debug)]
pub struct ProjectManifestValidationError {
    /// A list of all problems found during validation. Never empty.
    pub problems: Vec<ProjectManifestProblem>,
}

impl ProjectManifestValidationError {
    /// A multi-line report with a summary followed by one line for each problem.
    pub fn report(&self) -> impl std::fmt::Display {
        struct Report<'a>(&'a ProjectManifestValidationError);

        impl std::fmt::Display for Report<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.write_summary(f)?;
                write!(f, ":")?;
                for problem in &self.0.problems {
                    write!(f, "\n- {problem}")?;
                    // Make sure to include the source chain of any underlying errors, if present.
                    let mut source = std::error::Error::source(problem.innermost());
                    while let Some(error) = source {
                        write!(f, " {error}")?;
                        source = error.source();
                    }
                }
                Ok(())
            }
        }

        Report(self)
    }

    fn write_summary(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "The project manifest has {} problem(s)", self.problems.len())
    }
}

impl std::fmt::Display for ProjectManifestValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.write_summary(f)?;
        write!(f, ".")
    }
}

impl std::error::Error for ProjectManifestValidationError {}

impl ProjectManifestProblem {
    /// The problem itself or, for a problem in a dependency, the problem found there.
    fn innermost(&self) -> &Self {
        match self {
            Self::InDependency { problem, .. } => problem.innermost(),
            _ => self,
        }
    }
}

#[derive(Error, Debug)]
pub enum ProjectManifestProblem {
    #[error("The definition `{definition}` was not found in the project manifest.")]
    DefinitionNotFound { definition: String },
    #[error("The network definition `{network}` was not found in the project manifest.")]
    NetworkDefinitionNotFound { network: String },
    #[error("The name `{set}` is used by {count} network sets, but each name must be unique.")]
    DuplicateNetworkSet { set: String, count: usize },
    #[error(
        "The name `{definition}` is used by {count} model or multi-model definitions, but each name must be unique."
    )]
    DuplicateDefinition { definition: String, count: usize },
    #[error("The name `{network}` is used by {count} network definitions, but each name must be unique.")]
    DuplicateNetworkDefinition { network: String, count: usize },
    #[error("The name `{sub_network}` is used by {count} sub-networks, but each name must be unique.")]
    DuplicateSubNetwork { sub_network: String, count: usize },
    #[error("The {kind} name `{name}` must not contain `::`.")]
    InvalidName { kind: &'static str, name: String },
    #[error("The {definition} refers to the network definition `{network}`, which does not exist.")]
    NetworkNotFound { definition: DefinitionRef, network: String },
    #[error("The {definition} refers to the sub-network `{sub_network}`, which does not exist.")]
    SubNetworkNotFound {
        definition: DefinitionRef,
        sub_network: String,
    },
    #[error(
        "The {definition} selects the network set `{set}`, but its sub-network `{sub_network}` is not active; list the sub-network in `sub_networks`."
    )]
    SubNetworkNotActive {
        definition: DefinitionRef,
        set: String,
        sub_network: String,
    },
    #[error("The network definitions extend each other in a cycle: {}.", .cycle.join(" -> "))]
    NetworkCycle { cycle: Vec<String> },
    #[error("The {definition} has no base model; give one in the definition or at the top of the manifest.")]
    BaseModelMissing { definition: DefinitionRef },
    #[error("The {definition} refers to the model definition `{model}`, which does not exist.")]
    ModelNotFound { definition: DefinitionRef, model: String },
    #[error("The {definition} cannot use its base model: {error}")]
    InvalidBaseModel {
        definition: DefinitionRef,
        error: ManifestResolutionError,
    },
    #[error("The {definition} lists the network definition `{network}` {count} times, but each may be listed once.")]
    DuplicateNetworkReference {
        definition: DefinitionRef,
        network: String,
        count: usize,
    },
    #[error("The {definition} has {count} networks named `{network}`, but each name must be unique.")]
    DuplicateMultiModelNetwork {
        definition: DefinitionRef,
        network: String,
        count: usize,
    },
    #[error("The transfer `{transfer}` into `{network}` is from the network `{from_network}`, which does not exist.")]
    TransferSourceNotFound {
        network: String,
        transfer: String,
        from_network: String,
    },
    #[error("The transfer `{transfer}` into `{network}` is from the same network.")]
    TransferFromSameNetwork { network: String, transfer: String },
    #[error("The name `{transfer}` is used by {count} transfers into `{network}`, but each name must be unique.")]
    DuplicateTransfer {
        network: String,
        transfer: String,
        count: usize,
    },
    #[error("The {definition} selects the network set `{set}` {count} times, but it may select each set at most once.")]
    DuplicateSet {
        definition: DefinitionRef,
        set: String,
        count: usize,
    },
    #[error("The {definition} selects the network set `{set}`, which does not exist.")]
    SetNotFound { definition: DefinitionRef, set: String },
    #[error("The {definition} selects the file `{file}`, which was not found in network set `{set}`.")]
    FileNotFound {
        definition: DefinitionRef,
        set: String,
        file: String,
    },
    #[error("The {definition} selects the file `{file}` more than once in network set `{set}`.")]
    DuplicateFile {
        definition: DefinitionRef,
        set: String,
        file: String,
    },
    #[error("The {definition} gives metadata for file `{file}` in network set `{set}`, but that file is not selected.")]
    UnusedFileMeta {
        definition: DefinitionRef,
        set: String,
        file: String,
    },
    #[error(
        "The {definition} selects the invalid filename `{file}` in network set `{set}`; filenames must be single path components."
    )]
    InvalidFilePath {
        definition: DefinitionRef,
        set: String,
        file: String,
    },
    #[error(
        "The {definition} selects {actual_files} file(s) from network set `{set}`, but at least {min_files} are required."
    )]
    MinFilesNotMet {
        definition: DefinitionRef,
        set: String,
        min_files: usize,
        actual_files: usize,
    },
    #[error(
        "The {definition} selects {actual_files} file(s) from network set `{set}`, but at most {max_files} are allowed."
    )]
    MaxFilesExceeded {
        definition: DefinitionRef,
        set: String,
        max_files: usize,
        actual_files: usize,
    },
    #[error("The network set `{set}` has `min_files` {min_files}, which exceeds `max_files` {max_files}.")]
    InvalidFileConstraints {
        set: String,
        min_files: usize,
        max_files: usize,
    },
    #[error(
        "The network sets `{other}` and `{set}` both use the directory `{}`, but each set needs a directory of its own.", .path.display()
    )]
    SharedSetDirectory { set: String, other: String, path: PathBuf },
    #[error(
        "The network set `{set}` is in the directory of the sub-network `{sub_network}`; make it one of the sub-network's network sets instead."
    )]
    SetInSubNetwork { set: String, sub_network: String },
    #[error(
        "The directories of the sub-networks `{other}` and `{sub_network}` overlap, but each sub-network needs a directory of its own."
    )]
    OverlappingSubNetworks { sub_network: String, other: String },
    #[error(
        "The base network of the sub-network `{sub_network}` is in the directory of the network set `{set}`, but it must not also be a file of a network set."
    )]
    BaseNetworkInSet { sub_network: String, set: String },
    #[error(
        "The {definition} reaches the file `{}` through both {first} and {second}, but a file may be reached through only one.", .file.display()
    )]
    FileInSeveralSets {
        definition: DefinitionRef,
        file: PathBuf,
        first: String,
        second: String,
    },
    #[error("The {definition} selects the file `{file}` from {set} more than once, with different position offsets.")]
    ConflictingPositionOffsets {
        definition: DefinitionRef,
        file: String,
        set: String,
    },
    #[error("The name `{dependency}` is used by {count} dependencies, but each name must be unique.")]
    DuplicateDependency { dependency: String, count: usize },
    #[error("The dependency name `{dependency}` is also the name of a sub-network, but the two share one namespace.")]
    DependencyNameTaken { dependency: String },
    #[error("The project dependencies form a cycle: {}.", .cycle.iter().map(|path| format!("`{}`", path.display())).collect::<Vec<_>>().join(" -> "))]
    DependencyCycle { cycle: Vec<PathBuf> },
    #[error("The {definition} refers to the {kind} `{item}`, which the dependency `{project}` does not export.")]
    NotExported {
        definition: DefinitionRef,
        project: String,
        kind: &'static str,
        item: String,
    },
    #[error("The project exports the {kind} `{name}`, which does not exist.")]
    ExportNotFound { kind: &'static str, name: String },
    #[error("In the dependency `{dependency}`: {problem}")]
    InDependency {
        dependency: String,
        problem: Box<ProjectManifestProblem>,
    },
    #[error(transparent)]
    Resolution(#[from] ManifestResolutionError),
}

impl ProjectManifestProblem {
    /// The path at which an item was expected but not found, or found but of the wrong kind,
    /// if this problem is about one.
    pub fn unresolved_path(&self) -> Option<&Path> {
        match self {
            Self::Resolution(error) | Self::InvalidBaseModel { error, .. } => error.unresolved_path(),
            Self::InDependency { problem, .. } => problem.unresolved_path(),
            _ => None,
        }
    }
}

/// A simple project schema that defines how to build a model from multiple JSON fragments.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectManifest {
    /// Path (relative to root) to the base ModelSchema JSON used by model definitions that do not
    /// give their own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_model: Option<String>,
    /// Sub-networks, each a directory with its own base network and network sets for part of the model.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sub_networks: Vec<SubNetwork>,
    /// Named sets of NetworkSchema fragments.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub network_sets: Vec<NetworkSet>,
    /// Named, reusable compositions of network fragments.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub networks: Vec<NetworkDefinition>,
    /// Named model definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<ModelDefinition>,
    /// Named multi-network model definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub multi_models: Vec<MultiModelDefinition>,
    /// Other projects whose definitions this project may refer to as `<dependency>::<name>`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<Dependency>,
    /// What projects that depend on this one may refer to. Without it, everything is exported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exports: Option<Exports>,
}

impl ProjectManifest {
    /// Validate the project manifest and return a report of all structural problems found.
    ///
    /// Dependencies are loaded and checked as a whole; their definitions are checked as far as
    /// this project uses them.
    pub fn validate(&self, root: &Path, options: &ResolveOptions) -> Result<(), ProjectManifestValidationError> {
        let mut problems = self.manifest_problems();
        let graph = self.load_whole_graph(root, options, &mut problems);
        let mut resolver = Resolver::new(&graph);
        self.resolve_all(&mut resolver, &mut problems);
        into_result(problems)
    }

    /// Load the project graph for checking the whole project, reporting every dependency's
    /// problems whether or not anything uses it.
    fn load_whole_graph(
        &self,
        root: &Path,
        options: &ResolveOptions,
        problems: &mut Vec<ProjectManifestProblem>,
    ) -> ProjectGraph<'_> {
        let mut graph = self.load_graph(root, options, true, problems);
        problems.extend(
            std::mem::take(&mut graph.load_problems)
                .into_iter()
                .map(|found| found.problem),
        );
        graph
    }

    /// Resolve the project against the filesystem, returning where its files are together with
    /// every problem that [`validate`](Self::validate) would report.
    pub fn layout(&self, root: &Path, options: &ResolveOptions) -> ProjectLayout {
        let mut problems = self.manifest_problems();
        let graph = self.load_whole_graph(root, options, &mut problems);
        let mut resolver = Resolver::new(&graph);
        self.resolve_all(&mut resolver, &mut problems);

        // The root project's directory as given may be relative or not canonical; every other
        // path in the layout is canonical.
        let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        // Data directories are held relative to the root project's directory.
        let data_dir = |relative: Option<&Path>| {
            let dir = relative.map_or_else(|| canonical_root.clone(), |relative| canonical_root.join(relative));
            dir.canonicalize().unwrap_or(dir)
        };
        let projects = graph
            .projects
            .iter()
            .map(|project| ProjectLocation {
                alias: project.scope.alias.clone(),
                root: if project.scope.index == ROOT {
                    canonical_root.clone()
                } else {
                    project.root.clone()
                },
                manifest: project.manifest_path.clone(),
                base_model: match &project.base_model {
                    ProjectBaseModel::Resolved((path, _)) => Some(path.clone()),
                    _ => None,
                },
            })
            .collect();
        let network_sets = graph
            .projects
            .iter()
            .flat_map(|project| &project.resolved.sets)
            .map(|set| NetworkSetLayout {
                name: set.label.clone(),
                project: set.alias.clone(),
                sub_network: set.sub_network.clone(),
                set: set.set_name.clone(),
                dir: set.dir.clone(),
                data_dir: data_dir(set.data_dir.as_deref()),
                min_files: set.min_files,
                max_files: set.max_files,
                files: set.files.iter().map(|file| file.layout(Some(set))).collect(),
            })
            .collect();
        let sub_networks = graph
            .projects
            .iter()
            .flat_map(|project| {
                project
                    .resolved
                    .sub_networks
                    .iter()
                    .map(|sub_network| SubNetworkLayout {
                        name: project.label(&sub_network.name),
                        project: project.scope.alias.clone(),
                        dir: sub_network.dir.clone(),
                        data_dir: data_dir(Some(&sub_network.data_dir)),
                        base_network: sub_network.base_network.as_ref().map(|file| file.layout(None)),
                    })
            })
            .collect();

        // Base models given by definitions, in a stable order, then the root project's
        // multi-model base models and transfers files. Projects' own base models are listed with
        // the projects, so are left out here.
        let mut definition_files: Vec<_> = resolver
            .model_base_models
            .values()
            .flatten()
            .map(|(path, _)| path.clone())
            .collect();
        definition_files.sort();
        for multi_model in &self.multi_models {
            if let MultiModelDomain::BaseModel(BaseModelDomain { path: base_model }) = &multi_model.domain
                && let Ok((path, _)) = resolve_base_model(root, base_model)
            {
                definition_files.push(path);
            }
            for network in &multi_model.networks {
                if let Some(path) = network
                    .transfers_file
                    .as_deref()
                    .and_then(|file| transfers_path(root, file).ok())
                {
                    definition_files.push(path);
                }
            }
        }
        let project_base_models: HashSet<_> = graph
            .projects
            .iter()
            .filter_map(|project| match &project.base_model {
                ProjectBaseModel::Resolved((path, _)) => Some(path.clone()),
                _ => None,
            })
            .collect();
        let mut seen = HashSet::new();
        definition_files.retain(|path| !project_base_models.contains(path) && seen.insert(path.clone()));

        let unresolved: Vec<_> = problems
            .iter()
            .filter_map(ProjectManifestProblem::unresolved_path)
            .map(|path| std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()))
            .collect();
        let unresolved_paths = unique_paths(unresolved.iter().map(PathBuf::as_path));

        ProjectLayout {
            projects,
            network_sets,
            sub_networks,
            definition_files,
            unresolved_paths,
            problems: into_result(problems)
                .err()
                .map(crate::ProjectManifestValidationError::V1),
        }
    }

    /// Resolve every definition, appending the problems found.
    fn resolve_all<'a>(&'a self, resolver: &mut Resolver<'a>, problems: &mut Vec<ProjectManifestProblem>) {
        for model in &self.models {
            resolver.resolve_model(model, problems);
        }
        for multi_model in &self.multi_models {
            multi_model.resolve(resolver, problems);
        }
        // Network definitions that no model uses are still validated; each is reported once.
        // Sub-network activation is not checked here, because a model may activate it instead.
        for network in &self.networks {
            resolver.expand(ROOT, &network.name, problems);
        }
    }

    /// Validate a model or multi-model definition against the project manifest.
    pub fn validate_model(
        &self,
        root: &Path,
        definition_name: &str,
        options: &ResolveOptions,
    ) -> Result<(), ProjectManifestValidationError> {
        let model = self.find_model(definition_name);
        let multi_model = self.find_multi_model(definition_name);
        if model.is_none() && multi_model.is_none() {
            return into_result(vec![ProjectManifestProblem::DefinitionNotFound {
                definition: definition_name.to_string(),
            }]);
        }
        let mut problems = self.manifest_problems();
        let mut graph = self.load_graph(root, options, true, &mut problems);
        let (at, load_problems) = (problems.len(), std::mem::take(&mut graph.load_problems));
        let mut resolver = Resolver::new(&graph);
        if let Some(model) = model {
            resolver.resolve_model(model, &mut problems);
        } else if let Some(multi_model) = multi_model {
            multi_model.resolve(&mut resolver, &mut problems);
        }
        resolver.insert_reached_load_problems(&mut problems, at, load_problems);
        into_result(problems)
    }

    /// Validate a network definition, composed on its own, against the project manifest.
    pub fn validate_network(
        &self,
        root: &Path,
        network_name: &str,
        options: &ResolveOptions,
    ) -> Result<(), ProjectManifestValidationError> {
        let Some(network) = self.find_network(network_name) else {
            return into_result(vec![ProjectManifestProblem::NetworkDefinitionNotFound {
                network: network_name.to_string(),
            }]);
        };
        let mut problems = self.manifest_problems();
        let mut graph = self.load_graph(root, options, false, &mut problems);
        let (at, load_problems) = (problems.len(), std::mem::take(&mut graph.load_problems));
        let mut resolver = Resolver::new(&graph);
        let expansion = resolver.expand(ROOT, &network.name, &mut problems);
        check_active(
            &DefinitionRef::new(DefinitionKind::Network, network_name),
            &expansion,
            &mut problems,
        );
        resolver.insert_reached_load_problems(&mut problems, at, load_problems);
        into_result(problems)
    }

    /// Compose the model or multi-model definition `definition_name`.
    pub fn compose(
        &self,
        root: &Path,
        definition_name: &str,
        options: &ResolveOptions,
    ) -> Result<Composed, ComposeModelError> {
        if self.find_multi_model(definition_name).is_some() {
            self.compose_multi_model(root, definition_name, options)
                .map(Composed::MultiModel)
        } else {
            self.compose_model(root, definition_name, options).map(Composed::Model)
        }
    }

    /// Compose a model from its base model and the specified model definition.
    ///
    /// This resolves the manifest in the same way as [`Self::validate_model`], and returns the
    /// first problem found as an error.
    pub fn compose_model(
        &self,
        root: &Path,
        definition_name: &str,
        options: &ResolveOptions,
    ) -> Result<ComposedModel, ComposeModelError> {
        let mut problems = self.manifest_problems();
        let Some(model) = self.find_model(definition_name) else {
            problems.push(ProjectManifestProblem::DefinitionNotFound {
                definition: definition_name.to_string(),
            });
            return Err(problems.swap_remove(0).into());
        };
        let mut graph = self.load_graph(root, options, true, &mut problems);
        let (at, load_problems) = (problems.len(), std::mem::take(&mut graph.load_problems));
        let mut resolver = Resolver::new(&graph);
        let resolved = resolver.resolve_model(model, &mut problems);
        resolver.insert_reached_load_problems(&mut problems, at, load_problems);

        match (problems.into_iter().next(), resolved.base_model) {
            (Some(problem), _) => Err(problem.into()),
            (None, Some((base_model, base_file))) => {
                let mut builder = ComposedModelBuilder::new(model.name.clone(), base_model, base_file);
                for include in resolved.includes {
                    builder.add_include(include);
                }
                if let Some(overrides) = &model.overrides {
                    builder.overrides(overrides.clone());
                }
                Ok(builder.build())
            }
            (None, None) => unreachable!("a missing or unresolved base model is always reported as a problem"),
        }
    }

    /// Compose a multi-network model from the specified multi-model definition.
    ///
    /// This resolves the manifest in the same way as [`Self::validate_model`], and returns the
    /// first problem found as an error.
    pub fn compose_multi_model(
        &self,
        root: &Path,
        definition_name: &str,
        options: &ResolveOptions,
    ) -> Result<ComposedMultiModel, ComposeModelError> {
        let mut problems = self.manifest_problems();
        let Some(multi_model) = self.find_multi_model(definition_name) else {
            problems.push(ProjectManifestProblem::DefinitionNotFound {
                definition: definition_name.to_string(),
            });
            return Err(problems.swap_remove(0).into());
        };
        let mut graph = self.load_graph(root, options, true, &mut problems);
        let (at, load_problems) = (problems.len(), std::mem::take(&mut graph.load_problems));
        let mut resolver = Resolver::new(&graph);
        let resolved = multi_model.resolve(&mut resolver, &mut problems);
        resolver.insert_reached_load_problems(&mut problems, at, load_problems);

        match (problems.into_iter().next(), resolved.domain) {
            (Some(problem), _) => Err(problem.into()),
            (None, Some(domain)) => Ok(ComposedMultiModel::new(
                multi_model.name.clone(),
                domain,
                resolved.overrides,
                resolved.networks,
            )),
            (None, None) => unreachable!("a domain that fails to resolve is always reported as a problem"),
        }
    }

    /// Compose a network from the specified network definition.
    ///
    /// This resolves the manifest in the same way as [`Self::validate_network`], and returns the
    /// first problem found as an error.
    pub fn compose_network(
        &self,
        root: &Path,
        network_name: &str,
        options: &ResolveOptions,
    ) -> Result<ComposedNetwork, ComposeModelError> {
        let mut problems = self.manifest_problems();
        let Some(network) = self.find_network(network_name) else {
            problems.push(ProjectManifestProblem::NetworkDefinitionNotFound {
                network: network_name.to_string(),
            });
            return Err(problems.swap_remove(0).into());
        };
        let mut graph = self.load_graph(root, options, false, &mut problems);
        let (at, load_problems) = (problems.len(), std::mem::take(&mut graph.load_problems));
        let mut resolver = Resolver::new(&graph);
        let expansion = resolver.expand(ROOT, &network.name, &mut problems);
        resolver.insert_reached_load_problems(&mut problems, at, load_problems);
        check_active(
            &DefinitionRef::new(DefinitionKind::Network, network_name),
            &expansion,
            &mut problems,
        );

        match problems.into_iter().next() {
            Some(problem) => Err(problem.into()),
            None => Ok(ComposedNetwork::new(
                network_name.to_string(),
                None,
                expansion.files.into_iter().map(SelectedFile::into_composed).collect(),
            )),
        }
    }

    /// Load this project and, transitively, its dependencies, resolving each against the
    /// filesystem.
    ///
    /// The project-level base model is only resolved if `with_base_model` is set; a dependency's
    /// always is.
    fn load_graph(
        &self,
        root: &Path,
        options: &ResolveOptions,
        with_base_model: bool,
        problems: &mut Vec<ProjectManifestProblem>,
    ) -> ProjectGraph<'_> {
        let base_model = if with_base_model {
            self.resolve_base_model(root, problems)
        } else {
            ProjectBaseModel::NotGiven
        };
        let scope = ProjectScope {
            index: ROOT,
            alias: None,
            prefix: None,
            data_dir: None,
        };
        let resolved = self.resolve_project(root, &scope, problems);
        let mut graph = ProjectGraph {
            projects: vec![LoadedProject {
                manifest: Cow::Borrowed(self),
                manifest_path: None,
                root: root.to_path_buf(),
                scope,
                base_model,
                resolved,
                dependencies: Vec::new(),
            }],
            load_problems: Vec::new(),
        };
        if !self.dependencies.is_empty() {
            let mut loader = DependencyLoader {
                root: root.canonicalize().unwrap_or_else(|_| root.to_path_buf()),
                policy: &options.dependency_policy,
                loaded: HashMap::new(),
                hashes: HashMap::new(),
                // The root project starts the chain, so a dependency leading back to it is a cycle.
                stack: options
                    .manifest_path
                    .iter()
                    .filter_map(|path| path.canonicalize().ok())
                    .collect(),
            };
            // Problems found loading each dependency are kept apart, so that a definition that
            // does not use a dependency is not stopped by its problems.
            for dependency in &self.dependencies {
                let mut found = Vec::new();
                let loaded = loader.load(&mut graph, root, dependency.name.clone(), dependency, &mut found);
                graph.projects[ROOT]
                    .dependencies
                    .push((dependency.name.clone(), loaded));
                graph.load_problems.extend(found.into_iter().map(|problem| LoadProblem {
                    dependency: dependency.name.clone(),
                    problem,
                }));
            }
        }
        graph
    }

    /// Whether this project declares the item `name` of the given kind.
    fn has_item(&self, kind: ItemKind, name: &str) -> bool {
        match kind {
            ItemKind::Network => self.find_network(name).is_some(),
            ItemKind::Model => self.find_model(name).is_some(),
            ItemKind::SubNetwork => self.sub_networks.iter().any(|sub_network| sub_network.name == name),
            ItemKind::NetworkSet => match name.split_once(NAME_SEPARATOR) {
                Some((sub_network, set)) => self.sub_networks.iter().any(|candidate| {
                    candidate.name == sub_network && candidate.network_sets.iter().any(|other| other.name == set)
                }),
                None => self.network_sets.iter().any(|set| set.name == name),
            },
        }
    }

    fn find_model(&self, name: &str) -> Option<&ModelDefinition> {
        self.models.iter().find(|model| model.name == name)
    }

    fn find_network(&self, name: &str) -> Option<&NetworkDefinition> {
        self.networks.iter().find(|network| network.name == name)
    }

    fn find_multi_model(&self, name: &str) -> Option<&MultiModelDefinition> {
        self.multi_models.iter().find(|multi_model| multi_model.name == name)
    }

    /// Problems in the manifest itself, found without touching the filesystem.
    fn manifest_problems(&self) -> Vec<ProjectManifestProblem> {
        let mut problems = Vec::new();
        // Models and multi-models share one namespace, so a definition name says what to run.
        let definitions: Vec<&String> = self
            .models
            .iter()
            .map(|model| &model.name)
            .chain(self.multi_models.iter().map(|multi_model| &multi_model.name))
            .collect();
        add_duplicate_problems(
            &definitions,
            |name| name,
            |name, count| ProjectManifestProblem::DuplicateDefinition {
                definition: name,
                count,
            },
            &mut problems,
        );
        add_duplicate_problems(
            &self.networks,
            |network| &network.name,
            |name, count| ProjectManifestProblem::DuplicateNetworkDefinition { network: name, count },
            &mut problems,
        );
        add_duplicate_problems(
            &self.sub_networks,
            |sub_network| &sub_network.name,
            |name, count| ProjectManifestProblem::DuplicateSubNetwork {
                sub_network: name,
                count,
            },
            &mut problems,
        );
        add_duplicate_problems(
            &self.dependencies,
            |dependency| &dependency.name,
            |name, count| ProjectManifestProblem::DuplicateDependency {
                dependency: name,
                count,
            },
            &mut problems,
        );
        // Sub-network and dependency names both start qualified names, so they must not clash.
        let mut taken: Vec<&String> = self
            .dependencies
            .iter()
            .map(|dependency| &dependency.name)
            .filter(|name| self.sub_networks.iter().any(|sub_network| &sub_network.name == *name))
            .collect();
        taken.sort();
        taken.dedup();
        problems.extend(
            taken
                .into_iter()
                .map(|name| ProjectManifestProblem::DependencyNameTaken {
                    dependency: name.clone(),
                }),
        );
        if let Some(exports) = &self.exports {
            let exported = [
                (ItemKind::Network, &exports.networks),
                (ItemKind::Model, &exports.models),
                (ItemKind::NetworkSet, &exports.network_sets),
                (ItemKind::SubNetwork, &exports.sub_networks),
            ];
            for (kind, names) in exported {
                for name in names.iter().filter(|name| !self.has_item(kind, name)) {
                    problems.push(ProjectManifestProblem::ExportNotFound {
                        kind: kind.name(),
                        name: name.clone(),
                    });
                }
            }
        }
        let sets: Vec<_> = self
            .network_sets
            .iter()
            .map(|set| (set.name.clone(), set))
            .chain(self.sub_networks.iter().flat_map(|sub_network| {
                sub_network
                    .network_sets
                    .iter()
                    .map(|set| (qualified_name(&sub_network.name, &set.name), set))
            }))
            .collect();
        add_duplicate_problems(
            &sets,
            |(name, _)| name,
            |name, count| ProjectManifestProblem::DuplicateNetworkSet { set: name, count },
            &mut problems,
        );
        for (name, set) in &sets {
            if let (Some(min_files), Some(max_files)) = (set.min_files, set.max_files)
                && min_files > max_files
            {
                problems.push(ProjectManifestProblem::InvalidFileConstraints {
                    set: name.clone(),
                    min_files,
                    max_files,
                });
            }
        }

        let names = self
            .models
            .iter()
            .map(|model| ("model definition", &model.name))
            .chain(
                self.multi_models
                    .iter()
                    .map(|multi_model| ("multi-model definition", &multi_model.name)),
            )
            .chain(
                self.networks
                    .iter()
                    .map(|network| ("network definition", &network.name)),
            )
            .chain(
                self.sub_networks
                    .iter()
                    .map(|sub_network| ("sub-network", &sub_network.name)),
            )
            .chain(
                self.dependencies
                    .iter()
                    .map(|dependency| ("dependency", &dependency.name)),
            )
            .chain(sets.iter().map(|(_, set)| ("network set", &set.name)));
        for (kind, name) in names {
            if name.contains(NAME_SEPARATOR) {
                problems.push(ProjectManifestProblem::InvalidName {
                    kind,
                    name: name.clone(),
                });
            }
        }
        problems
    }

    /// Resolve the project-level base model, appending a problem if it is given but cannot be resolved.
    fn resolve_base_model(&self, root: &Path, problems: &mut Vec<ProjectManifestProblem>) -> ProjectBaseModel {
        let Some(base_model) = &self.base_model else {
            return ProjectBaseModel::NotGiven;
        };
        match resolve_base_model(root, base_model) {
            Ok(base_model) => ProjectBaseModel::Resolved(base_model),
            Err(error) => {
                problems.push(error.into());
                ProjectBaseModel::Unresolved
            }
        }
    }

    /// Resolve the sub-networks and network sets against the filesystem, appending any problems found.
    fn resolve_project(
        &self,
        root: &Path,
        scope: &ProjectScope,
        problems: &mut Vec<ProjectManifestProblem>,
    ) -> ResolvedProject {
        let mut project = ResolvedProject::default();
        for set in &self.network_sets {
            let resolved_set = resolve_network_set(root, &set.name, set, scope, None);
            project.add_set(&set.name, resolved_set, problems);
        }
        for sub_network in &self.sub_networks {
            match resolve_sub_network(root, sub_network, scope) {
                Ok((resolved, sub_scope)) => {
                    for set in &sub_network.network_sets {
                        let name = qualified_name(&sub_network.name, &set.name);
                        let resolved_set = resolve_network_set(&sub_scope.root, &name, set, scope, Some(&sub_scope));
                        project.add_set(&name, resolved_set, problems);
                    }
                    project.sub_networks.push(resolved);
                }
                Err(error) => {
                    problems.push(error.into());
                    project.unresolved_sub_networks.insert(sub_network.name.clone());
                    project.unresolved_sets.extend(
                        sub_network
                            .network_sets
                            .iter()
                            .map(|set| qualified_name(&sub_network.name, &set.name)),
                    );
                }
            }
        }
        project.check_directories(problems);
        project
    }
}

/// Separates a sub-network's name from the name of one of its network sets, as in `upper::options`.
const NAME_SEPARATOR: &str = "::";

fn qualified_name(sub_network: &str, name: &str) -> String {
    format!("{sub_network}{NAME_SEPARATOR}{name}")
}

/// The name of an item as seen from the root project, given the alias of its project.
fn qualify(alias: Option<&str>, name: &str) -> String {
    match alias {
        Some(alias) => qualified_name(alias, name),
        None => name.to_string(),
    }
}

fn into_result(problems: Vec<ProjectManifestProblem>) -> Result<(), ProjectManifestValidationError> {
    if problems.is_empty() {
        Ok(())
    } else {
        Err(ProjectManifestValidationError { problems })
    }
}

/// The canonical path of a base model and its path relative to the project root.
type ResolvedBaseModel = (PathBuf, RelativePathBuf);

/// The outcome of resolving the project-level base model.
enum ProjectBaseModel {
    NotGiven,
    /// The base model was given but could not be resolved; the problem is already reported.
    Unresolved,
    Resolved(ResolvedBaseModel),
}

/// The sub-networks and network sets of a manifest resolved against the filesystem, shared by
/// every definition.
#[derive(Default)]
struct ResolvedProject {
    /// Network sets that resolved, in declaration order: the project's own sets, then each
    /// sub-network's. Sub-network sets are named by their qualified names.
    sets: Vec<ResolvedNetworkSet>,
    /// Qualified names of network sets that failed to resolve. Their problems are already
    /// reported, so selections of them are not reported again as missing sets.
    unresolved_sets: HashSet<String>,
    /// Sub-networks that resolved, in declaration order.
    sub_networks: Vec<ResolvedSubNetwork>,
    /// Names of sub-networks that failed to resolve, whose problems are already reported.
    unresolved_sub_networks: HashSet<String>,
}

impl ResolvedProject {
    fn add_set(
        &mut self,
        name: &str,
        resolved: Result<ResolvedNetworkSet, ManifestResolutionError>,
        problems: &mut Vec<ProjectManifestProblem>,
    ) {
        match resolved {
            Ok(set) => self.sets.push(set),
            Err(error) => {
                problems.push(error.into());
                self.unresolved_sets.insert(name.to_string());
            }
        }
    }

    fn set(&self, name: &str) -> Option<&ResolvedNetworkSet> {
        self.sets.iter().find(|set| set.name == name)
    }

    /// Report network sets and sub-networks whose directories overlap, so that each network file
    /// belongs to one network set, or is one sub-network's base network, and lies in at most one
    /// sub-network.
    ///
    /// Network sets may be nested, since a set holds only the files directly in its directory.
    fn check_directories(&self, problems: &mut Vec<ProjectManifestProblem>) {
        let mut reported = HashSet::new();
        for (index, set) in self.sets.iter().enumerate() {
            if let Some(other) = self.sets[..index].iter().find(|other| other.dir == set.dir) {
                reported.insert(&set.name);
                reported.insert(&other.name);
                problems.push(ProjectManifestProblem::SharedSetDirectory {
                    set: set.name.clone(),
                    other: other.name.clone(),
                    path: set.dir.clone(),
                });
            }
        }
        for set in self.sets.iter().filter(|set| set.sub_network.is_none()) {
            if !reported.contains(&set.name)
                && let Some(sub_network) = self.sub_networks.iter().find(|sub| set.dir.starts_with(&sub.dir))
            {
                problems.push(ProjectManifestProblem::SetInSubNetwork {
                    set: set.name.clone(),
                    sub_network: sub_network.name.clone(),
                });
            }
        }
        for (index, sub_network) in self.sub_networks.iter().enumerate() {
            if let Some(other) = self.sub_networks[..index]
                .iter()
                .find(|other| other.dir.starts_with(&sub_network.dir) || sub_network.dir.starts_with(&other.dir))
            {
                problems.push(ProjectManifestProblem::OverlappingSubNetworks {
                    sub_network: sub_network.name.clone(),
                    other: other.name.clone(),
                });
            }
            if let Some(base_network) = &sub_network.base_network
                && let Some(set) = self
                    .sets
                    .iter()
                    .find(|set| set.files.iter().any(|file| file.path == base_network.path))
            {
                problems.push(ProjectManifestProblem::BaseNetworkInSet {
                    sub_network: sub_network.name.clone(),
                    set: set.name.clone(),
                });
            }
        }
    }

    fn sub_network(&self, name: &str) -> Option<&ResolvedSubNetwork> {
        self.sub_networks.iter().find(|sub_network| sub_network.name == name)
    }

    /// Look up the sub-network `name` that a definition refers to as `reference`, reporting it
    /// if it does not exist.
    ///
    /// Returns `None` both when it does not exist and when it failed to resolve.
    fn referred_sub_network(
        &self,
        definition: &DefinitionRef,
        reference: &str,
        name: &str,
        problems: &mut Vec<ProjectManifestProblem>,
    ) -> Option<&ResolvedSubNetwork> {
        let sub_network = self.sub_network(name);
        if sub_network.is_none() && !self.unresolved_sub_networks.contains(name) {
            problems.push(ProjectManifestProblem::SubNetworkNotFound {
                definition: definition.clone(),
                sub_network: reference.to_string(),
            });
        }
        sub_network
    }
}

/// A network fragment selected by a definition, before composition.
#[derive(Clone)]
struct SelectedFile<'a> {
    file: &'a ResolvedNetworkFile,
    /// The network set the file was selected from; `None` for a sub-network's base network.
    set: Option<&'a ResolvedNetworkSet>,
    position_offset: Option<PositionOffset>,
    /// The network definition that selected this file, if any.
    network: Option<&'a str>,
}

impl SelectedFile<'_> {
    /// How the file was reached, for problems: through a network set, or as a sub-network's
    /// base network.
    fn route(&self) -> String {
        match (self.set, &self.file.sub_network) {
            (Some(set), _) => format!("network set `{}`", set.label),
            (None, Some(sub_network)) => format!(
                "the base network of sub-network `{}`",
                qualify(self.file.project.as_deref(), sub_network)
            ),
            (None, None) => "a base network".to_string(),
        }
    }

    fn into_composed(self) -> ComposedNetworkPath {
        ComposedNetworkPath {
            path: self.file.path.clone(),
            position_offset: self.position_offset,
            source: ProvenanceSource {
                file: self.file.source.clone(),
                network_set: self.set.map(|set| set.set_name.clone()),
                sub_network: self.file.sub_network.clone(),
                project: self.file.project.clone(),
                network: self.network.map(str::to_string),
            },
            context: LoadContext {
                data_dir: self.file.data_dir.clone(),
            },
        }
    }
}

/// The result of expanding a network definition.
#[derive(Clone, Default)]
struct Expansion<'a> {
    /// The selected files, in merge order.
    files: Vec<SelectedFile<'a>>,
    /// Sub-networks made active in the expansion, by project index and name.
    active_sub_networks: Vec<(usize, &'a str)>,
}

impl<'a> Expansion<'a> {
    fn extend(&mut self, other: Expansion<'a>) {
        self.files.extend(other.files);
        self.active_sub_networks.extend(other.active_sub_networks);
    }

    /// Merge each file selected more than once into its first selection, reporting a file
    /// reached through more than one network set, or given different position offsets.
    ///
    /// A file is often reached twice legitimately, such as through two networks that extend the
    /// same parent, so a repeat that agrees with the first selection is not a problem.
    fn dedup(&mut self, definition: &DefinitionRef, problems: &mut Vec<ProjectManifestProblem>) {
        let mut first: HashMap<&Path, &SelectedFile<'_>> = HashMap::new();
        let mut reported = HashSet::new();
        for selected in &self.files {
            let Some(earlier) = first.get(selected.file.path.as_path()) else {
                first.insert(&selected.file.path, selected);
                continue;
            };
            let same_set = match (earlier.set, selected.set) {
                (Some(earlier), Some(selected)) => std::ptr::eq(earlier, selected),
                (None, None) => true,
                _ => false,
            };
            let problem = if !same_set {
                ProjectManifestProblem::FileInSeveralSets {
                    definition: definition.clone(),
                    file: selected.file.path.clone(),
                    first: earlier.route(),
                    second: selected.route(),
                }
            } else if earlier.position_offset != selected.position_offset {
                ProjectManifestProblem::ConflictingPositionOffsets {
                    definition: definition.clone(),
                    file: selected.file.source.to_string(),
                    set: selected.route(),
                }
            } else {
                continue;
            };
            if reported.insert(&selected.file.path) {
                problems.push(problem);
            }
        }

        let mut seen = HashSet::new();
        self.files.retain(|selected| seen.insert(&selected.file.path));
        let mut seen = HashSet::new();
        self.active_sub_networks.retain(|active| seen.insert(*active));
    }
}

/// Report each network set selected from a sub-network that the expansion does not activate.
fn check_active(definition: &DefinitionRef, expansion: &Expansion<'_>, problems: &mut Vec<ProjectManifestProblem>) {
    let mut reported = HashSet::new();
    for set in expansion.files.iter().filter_map(|selected| selected.set) {
        if let Some(sub_network) = &set.sub_network
            && !expansion
                .active_sub_networks
                .contains(&(set.project, sub_network.as_str()))
            && reported.insert(&set.label)
        {
            problems.push(ProjectManifestProblem::SubNetworkNotActive {
                definition: definition.clone(),
                set: set.label.clone(),
                sub_network: qualify(set.alias.as_deref(), sub_network),
            });
        }
    }
}

/// The index of the root project in a [`ProjectGraph`].
const ROOT: usize = 0;

/// The kinds of item that one project can refer to in another.
#[derive(Clone, Copy)]
enum ItemKind {
    Network,
    Model,
    NetworkSet,
    SubNetwork,
}

impl ItemKind {
    fn name(self) -> &'static str {
        match self {
            Self::Network => "network definition",
            Self::Model => "model definition",
            Self::NetworkSet => "network set",
            Self::SubNetwork => "sub-network",
        }
    }
}

/// Where a project's files are, for provenance and for rebasing their data paths.
struct ProjectScope {
    /// The project's index in its [`ProjectGraph`].
    index: usize,
    /// The dependency names leading to the project from the root, such as `org::shared`; `None`
    /// for the root project.
    alias: Option<String>,
    /// The project's directory relative to the root project's directory (or absolute, if it has
    /// no relative path); `None` for the root project.
    prefix: Option<PathBuf>,
    /// The directory that data paths in the project's own files are relative to, given in the
    /// same way as `prefix`; `None` leaves them unchanged.
    data_dir: Option<PathBuf>,
}

/// The root project and every project it depends on, directly or indirectly.
struct ProjectGraph<'m> {
    /// The projects, root first. A project that several others depend on is loaded once.
    projects: Vec<LoadedProject<'m>>,
    /// The problems found loading the dependencies, which the caller reports in full or only
    /// for the dependencies a definition reaches.
    load_problems: Vec<LoadProblem>,
}

/// A problem found while loading the root project's dependency `dependency`, or the projects
/// loaded through it.
struct LoadProblem {
    dependency: String,
    problem: ProjectManifestProblem,
}

/// The projects a [`Resolver`] reached while resolving definitions.
#[derive(Default)]
struct Reached {
    /// The projects reached, by index.
    projects: HashSet<usize>,
    /// The names of the root project's dependencies followed, including any that failed to load.
    root_dependencies: HashSet<String>,
}

/// A project in a [`ProjectGraph`], resolved against the filesystem.
struct LoadedProject<'m> {
    manifest: Cow<'m, ProjectManifest>,
    /// The canonical path of the project's manifest; `None` for the root project, whose
    /// manifest may not have been read from a file.
    manifest_path: Option<PathBuf>,
    /// The project's directory.
    root: PathBuf,
    scope: ProjectScope,
    base_model: ProjectBaseModel,
    resolved: ResolvedProject,
    /// The project's dependencies by name, with their index in the graph, or `None` for those
    /// that failed to load. Their problems are already reported.
    dependencies: Vec<(String, Option<usize>)>,
}

impl LoadedProject<'_> {
    /// The name of an item in this project as seen from the root project.
    fn label(&self, name: &str) -> String {
        qualify(self.scope.alias.as_deref(), name)
    }

    fn dependency(&self, name: &str) -> Option<Option<usize>> {
        self.dependencies
            .iter()
            .find(|(dependency, _)| dependency == name)
            .map(|(_, index)| *index)
    }

    /// Whether projects that depend on this one may refer to the item `name`.
    fn exports(&self, kind: ItemKind, name: &str) -> bool {
        let Some(exports) = &self.manifest.exports else {
            return true;
        };
        let names = match kind {
            ItemKind::Network => &exports.networks,
            ItemKind::Model => &exports.models,
            ItemKind::NetworkSet => &exports.network_sets,
            ItemKind::SubNetwork => &exports.sub_networks,
        };
        names.iter().any(|exported| exported == name)
    }
}

/// Loads the dependencies of a project into a [`ProjectGraph`], depth first.
struct DependencyLoader<'o> {
    /// The root project's canonical directory, which other projects' paths are made relative to.
    root: PathBuf,
    policy: &'o DependencyPolicy,
    /// The projects already loaded, by canonical manifest path and data directory.
    loaded: HashMap<(PathBuf, Option<String>), usize>,
    /// The hash of each manifest read, by canonical path, so a dependency's pin is checked
    /// however many routes lead to its manifest.
    hashes: HashMap<PathBuf, String>,
    /// The canonical manifest paths of the dependencies being loaded, outermost first, used to
    /// detect cycles.
    stack: Vec<PathBuf>,
}

impl DependencyLoader<'_> {
    fn load_dependencies(
        &mut self,
        graph: &mut ProjectGraph<'_>,
        index: usize,
        problems: &mut Vec<ProjectManifestProblem>,
    ) {
        let project = &graph.projects[index];
        let dependencies = project.manifest.dependencies.clone();
        let root = project.root.clone();
        let alias = project.scope.alias.clone();
        for dependency in &dependencies {
            let alias = qualify(alias.as_deref(), &dependency.name);
            let loaded = self.load(graph, &root, alias, dependency, problems);
            graph.projects[index]
                .dependencies
                .push((dependency.name.clone(), loaded));
        }
    }

    /// Load `dependency`, declared by the project in `root`, and its own dependencies, returning
    /// its index in the graph.
    ///
    /// Problems found inside the dependency are reported as being in it.
    fn load(
        &mut self,
        graph: &mut ProjectGraph<'_>,
        root: &Path,
        alias: String,
        dependency: &Dependency,
        problems: &mut Vec<ProjectManifestProblem>,
    ) -> Option<usize> {
        let path = match self.manifest_path(root, dependency) {
            Ok(path) => path,
            Err(error) => {
                problems.push(error.into());
                return None;
            }
        };
        if let Some(start) = self.stack.iter().position(|entry| *entry == path) {
            let mut cycle = self.stack[start..].to_vec();
            cycle.push(path);
            problems.push(ProjectManifestProblem::DependencyCycle { cycle });
            return None;
        }
        let key = (path.clone(), dependency.data_dir.clone());
        if let Some(&index) = self.loaded.get(&key) {
            if let Err(error) = check_pin(&path, dependency, &self.hashes[&path]) {
                problems.push(error.into());
                return None;
            }
            return Some(index);
        }
        let manifest = match read_dependency(&path, dependency) {
            Ok((manifest, hash)) => {
                let checked = check_pin(&path, dependency, &hash);
                self.hashes.insert(path.clone(), hash);
                if let Err(error) = checked {
                    problems.push(error.into());
                    return None;
                }
                manifest
            }
            Err(error) => {
                problems.push(error.into());
                return None;
            }
        };
        let dependency_root = path
            .parent()
            .expect("a canonical file path has a parent directory")
            .to_path_buf();
        let prefix = relative_to(&self.root, &dependency_root);
        let data_dir = match &dependency.data_dir {
            Some(data_dir) => {
                let field = format!("dependency '{}' data directory", dependency.name);
                let checked = strict_relative_path(&field, data_dir).and_then(|data_dir| {
                    let owner = format!("dependency `{}`", dependency.name);
                    check_data_directory(&data_dir.to_path(&dependency_root), owner)?;
                    Ok(data_dir)
                });
                match checked {
                    Ok(data_dir) => prefix.join(data_dir.to_path("")),
                    Err(error) => {
                        problems.push(error.into());
                        return None;
                    }
                }
            }
            None => prefix.clone(),
        };

        let index = graph.projects.len();
        let scope = ProjectScope {
            index,
            alias: Some(alias.clone()),
            prefix: Some(prefix),
            data_dir: Some(data_dir),
        };
        let start = problems.len();
        problems.extend(manifest.manifest_problems());
        let base_model = manifest.resolve_base_model(&dependency_root, problems);
        let resolved = manifest.resolve_project(&dependency_root, &scope, problems);
        graph.projects.push(LoadedProject {
            manifest: Cow::Owned(manifest),
            manifest_path: Some(path.clone()),
            root: dependency_root,
            scope,
            base_model,
            resolved,
            dependencies: Vec::new(),
        });
        self.loaded.insert(key, index);

        self.stack.push(path);
        self.load_dependencies(graph, index, problems);
        self.stack.pop();
        wrap_problems(problems, start, &alias);
        Some(index)
    }

    /// The canonical path of a dependency's manifest, checked against the dependency policy.
    fn manifest_path(&self, root: &Path, dependency: &Dependency) -> Result<PathBuf, ManifestResolutionError> {
        let name = &dependency.name;
        let value = &dependency.manifest;
        // Unlike every other path in a manifest, this one may lead out of the project with `..`.
        if value.is_empty()
            || value.contains('\\')
            || Path::new(value)
                .components()
                .any(|component| !matches!(component, Component::Normal(_) | Component::ParentDir))
        {
            return Err(ManifestResolutionError::InvalidDependencyPath {
                dependency: name.clone(),
                path: value.clone(),
            });
        }
        let candidate = root.join(value);
        if !candidate.exists() {
            return Err(ManifestResolutionError::DependencyManifestNotFound {
                dependency: name.clone(),
                path: candidate,
            });
        }
        let path = candidate
            .canonicalize()
            .map_err(|source| ManifestResolutionError::UnableToCanonicalizePath {
                field: format!("dependency '{name}' manifest"),
                path: candidate.clone(),
                source,
            })?;
        if !path.is_file() {
            return Err(ManifestResolutionError::DependencyManifestNotAFile {
                dependency: name.clone(),
                path,
            });
        }
        let allowed = match self.policy {
            DependencyPolicy::Allow => true,
            DependencyPolicy::Deny => false,
            DependencyPolicy::AllowWithin(dir) => dir.canonicalize().is_ok_and(|dir| path.starts_with(dir)),
        };
        if !allowed {
            return Err(ManifestResolutionError::DependencyNotAllowed {
                dependency: name.clone(),
                path,
            });
        }
        Ok(path)
    }
}

/// Read and parse a dependency's manifest, returning it with its [`manifest_hash`].
fn read_dependency(path: &Path, dependency: &Dependency) -> Result<(ProjectManifest, String), ManifestResolutionError> {
    let data = std::fs::read(path).map_err(|source| ManifestResolutionError::DependencyManifestRead {
        dependency: dependency.name.clone(),
        path: path.to_path_buf(),
        source,
    })?;
    let manifest: crate::manifest::ProjectManifest =
        serde_json::from_slice(&data).map_err(|source| ManifestResolutionError::DependencyManifestInvalid {
            dependency: dependency.name.clone(),
            path: path.to_path_buf(),
            source,
        })?;
    let crate::manifest::ProjectManifest::V1(manifest) = manifest;
    Ok((manifest, manifest_hash(&data)))
}

/// The SHA-256 hash of a manifest's contents, as lowercase hex, with each CRLF line ending read
/// as LF. A pin then holds on any platform, whatever line endings Git checks the file out with.
fn manifest_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    let mut rest = data;
    while let Some(position) = rest.windows(2).position(|pair| pair == b"\r\n") {
        hasher.update(&rest[..position]);
        hasher.update(b"\n");
        rest = &rest[position + 2..];
    }
    hasher.update(rest);
    hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Check a dependency's `sha256` pin, if it has one, against the hash of its manifest.
fn check_pin(path: &Path, dependency: &Dependency, actual: &str) -> Result<(), ManifestResolutionError> {
    match &dependency.sha256 {
        Some(expected) if !actual.eq_ignore_ascii_case(expected) => {
            Err(ManifestResolutionError::DependencyHashMismatch {
                dependency: dependency.name.clone(),
                path: path.to_path_buf(),
                expected: expected.clone(),
                actual: actual.to_string(),
            })
        }
        _ => Ok(()),
    }
}

/// The path of `target` relative to `base`, through `..` where needed. Both must be absolute
/// and canonical. Returns `target` itself if there is no relative path, as between drives.
fn relative_to(base: &Path, target: &Path) -> PathBuf {
    let base: Vec<_> = base.components().collect();
    let target_components: Vec<_> = target.components().collect();
    let common = base
        .iter()
        .zip(&target_components)
        .take_while(|(left, right)| left == right)
        .count();
    if common == 0 {
        return target.to_path_buf();
    }
    let mut path = PathBuf::new();
    for _ in common..base.len() {
        path.push("..");
    }
    for component in &target_components[common..] {
        path.push(component);
    }
    path
}

/// Report the problems from `start` on as being in the dependency `alias`, leaving those
/// already attributed to a dependency, which is more specific.
fn wrap_problems(problems: &mut Vec<ProjectManifestProblem>, start: usize, alias: &str) {
    let found = problems.split_off(start);
    problems.extend(found.into_iter().map(|problem| match problem {
        ProjectManifestProblem::InDependency { .. } => problem,
        problem => ProjectManifestProblem::InDependency {
            dependency: alias.to_string(),
            problem: Box::new(problem),
        },
    }));
}

/// Resolves model and network definitions into their selected files, remembering each result
/// so that a definition's problems are reported once however many definitions refer to it.
struct Resolver<'a> {
    graph: &'a ProjectGraph<'a>,
    expanded: HashMap<(usize, &'a str), Expansion<'a>>,
    expanded_models: HashMap<(usize, &'a str), Expansion<'a>>,
    model_base_models: HashMap<(usize, &'a str), Option<ResolvedBaseModel>>,
    /// The model definitions already checked as whole models, by project index and name.
    checked_models: HashSet<(usize, &'a str)>,
    /// The projects reached so far through references.
    reached: RefCell<Reached>,
    /// The network definitions currently being expanded, outermost first, used to detect cycles.
    stack: Vec<(usize, &'a str)>,
}

impl<'a> Resolver<'a> {
    fn new(graph: &'a ProjectGraph<'a>) -> Self {
        Self {
            graph,
            expanded: HashMap::new(),
            expanded_models: HashMap::new(),
            model_base_models: HashMap::new(),
            checked_models: HashSet::new(),
            reached: RefCell::default(),
            stack: Vec::new(),
        }
    }

    fn project(&self, index: usize) -> &'a LoadedProject<'a> {
        &self.graph.projects[index]
    }

    /// Insert at `at` in `problems` the problems found loading the dependencies that this resolver
    /// reached, leaving out those of dependencies it did not use.
    fn insert_reached_load_problems(
        &self,
        problems: &mut Vec<ProjectManifestProblem>,
        at: usize,
        load_problems: Vec<LoadProblem>,
    ) {
        let reached = self.reached.borrow();
        let kept = load_problems.into_iter().filter(|found| {
            // A problem inside a project names it; one with no such project, such as a manifest
            // that could not be read, belongs to the dependency it was found loading.
            let project = match &found.problem {
                ProjectManifestProblem::InDependency { dependency, .. } => self
                    .graph
                    .projects
                    .iter()
                    .position(|project| project.scope.alias.as_deref() == Some(dependency.as_str())),
                _ => None,
            };
            match project {
                Some(index) => reached.projects.contains(&index),
                None => reached.root_dependencies.contains(&found.dependency),
            }
        });
        problems.splice(at..at, kept.map(|found| found.problem).collect::<Vec<_>>());
    }

    /// Report the problems from `start` on as being in project `index`, if it is a dependency.
    fn attribute(&self, index: usize, problems: &mut Vec<ProjectManifestProblem>, start: usize) {
        if let Some(alias) = &self.project(index).scope.alias {
            wrap_problems(problems, start, alias);
        }
    }

    /// Follow `reference`, made by `definition` in project `from`, through any dependency names
    /// it starts with, returning the project it leads to and the item's name there.
    ///
    /// Returns `None` if a dependency on the way failed to load, whose problem is already
    /// reported, or if the item is not exported.
    fn locate(
        &self,
        from: usize,
        definition: &DefinitionRef,
        reference: &'a str,
        kind: ItemKind,
        problems: &mut Vec<ProjectManifestProblem>,
    ) -> Option<(usize, &'a str)> {
        let mut index = from;
        let mut name = reference;
        // The dependency names followed so far, which name the project reached as `from` does: a
        // project shared by several dependencies has a different name on each route.
        let project_name = |name: &str| reference[..reference.len() - name.len()].trim_end_matches(NAME_SEPARATOR);
        while let Some((first, rest)) = name.split_once(NAME_SEPARATOR)
            && let Some(dependency) = self.project(index).dependency(first)
        {
            // A dependency's own dependencies are only visible if it exports everything.
            if index != from && self.project(index).manifest.exports.is_some() {
                problems.push(ProjectManifestProblem::NotExported {
                    definition: definition.clone(),
                    project: project_name(name).to_string(),
                    kind: "dependency",
                    item: first.to_string(),
                });
                return None;
            }
            let mut reached = self.reached.borrow_mut();
            if index == ROOT {
                reached.root_dependencies.insert(first.to_string());
            }
            index = dependency?;
            reached.projects.insert(index);
            name = rest;
        }
        if index != from && !self.project(index).exports(kind, name) {
            problems.push(ProjectManifestProblem::NotExported {
                definition: definition.clone(),
                project: project_name(name).to_string(),
                kind: kind.name(),
                item: name.to_string(),
            });
            return None;
        }
        Some((index, name))
    }

    /// Expand the network definition that `definition` in project `from` refers to.
    fn expand_reference(
        &mut self,
        from: usize,
        definition: &DefinitionRef,
        reference: &'a str,
        problems: &mut Vec<ProjectManifestProblem>,
    ) -> Expansion<'a> {
        let Some((index, name)) = self.locate(from, definition, reference, ItemKind::Network, problems) else {
            return Expansion::default();
        };
        if self.project(index).manifest.find_network(name).is_none() {
            problems.push(ProjectManifestProblem::NetworkNotFound {
                definition: definition.clone(),
                network: reference.to_string(),
            });
            return Expansion::default();
        }
        self.expand(index, name, problems)
    }

    /// Look up the sub-network that `definition` in project `from` refers to.
    fn sub_network_reference(
        &self,
        from: usize,
        definition: &DefinitionRef,
        reference: &'a str,
        problems: &mut Vec<ProjectManifestProblem>,
    ) -> Option<&'a ResolvedSubNetwork> {
        let (index, name) = self.locate(from, definition, reference, ItemKind::SubNetwork, problems)?;
        self.project(index)
            .resolved
            .referred_sub_network(definition, reference, name, problems)
    }

    /// Activate the sub-networks that `definition` in project `from` lists in its
    /// `sub_networks`, returning them with their base networks, in the order the sub-networks
    /// are declared. `network` is the network definition doing so, if any, for provenance.
    fn activate_sub_networks(
        &self,
        from: usize,
        definition: &DefinitionRef,
        references: &'a [String],
        network: Option<&'a str>,
        problems: &mut Vec<ProjectManifestProblem>,
    ) -> Expansion<'a> {
        let mut active = Vec::new();
        for reference in references {
            if let Some(sub_network) = self.sub_network_reference(from, definition, reference, problems) {
                active.push((sub_network.project, sub_network.name.as_str()));
            }
        }
        let files = self
            .graph
            .projects
            .iter()
            .flat_map(|project| &project.resolved.sub_networks)
            .filter(|sub_network| active.contains(&(sub_network.project, sub_network.name.as_str())))
            .filter_map(|sub_network| sub_network.base_network.as_ref())
            .map(|file| SelectedFile {
                file,
                set: None,
                position_offset: None,
                network,
            })
            .collect();
        Expansion {
            files,
            active_sub_networks: active,
        }
    }

    /// Resolve a root model definition's base model and files, checking them as a whole model.
    fn resolve_model(
        &mut self,
        model: &'a ModelDefinition,
        problems: &mut Vec<ProjectManifestProblem>,
    ) -> ResolvedModel {
        let base_model = self.model_base_model(ROOT, model, problems);
        let expansion = self.expand_model(ROOT, model, problems);
        self.check_model(ROOT, model, &expansion, problems);
        ResolvedModel {
            base_model,
            includes: expansion.files.into_iter().map(SelectedFile::into_composed).collect(),
        }
    }

    /// Check the expansion of a model definition in project `index` as a whole model, once
    /// however many times the model is used.
    fn check_model(
        &mut self,
        index: usize,
        model: &'a ModelDefinition,
        expansion: &Expansion<'a>,
        problems: &mut Vec<ProjectManifestProblem>,
    ) {
        if self.checked_models.insert((index, &model.name)) {
            let definition = DefinitionRef::new(DefinitionKind::Model, &self.project(index).label(&model.name));
            check_composition(&definition, expansion, self.graph, SetLimits::ModelOf(index), problems);
        }
    }

    /// The base model of a model definition in project `index`: its own, or else the project's.
    fn model_base_model(
        &mut self,
        index: usize,
        model: &'a ModelDefinition,
        problems: &mut Vec<ProjectManifestProblem>,
    ) -> Option<ResolvedBaseModel> {
        if let Some(base_model) = self.model_base_models.get(&(index, model.name.as_str())) {
            return base_model.clone();
        }
        let project = self.project(index);
        let start = problems.len();
        let definition = DefinitionRef::new(DefinitionKind::Model, &model.name);
        let base_model = match (&model.base_model, &project.base_model) {
            (Some(base_model), _) => match resolve_base_model(&project.root, base_model) {
                Ok(base_model) => Some(base_model),
                Err(error) => {
                    problems.push(ProjectManifestProblem::InvalidBaseModel { definition, error });
                    None
                }
            },
            (None, ProjectBaseModel::Resolved(base_model)) => Some(base_model.clone()),
            (None, ProjectBaseModel::Unresolved) => None,
            (None, ProjectBaseModel::NotGiven) => {
                problems.push(ProjectManifestProblem::BaseModelMissing { definition });
                None
            }
        };
        self.attribute(index, problems, start);
        self.model_base_models.insert((index, &model.name), base_model.clone());
        base_model
    }

    /// Expand a model definition in project `index` into the files merged after its base model,
    /// in merge order: the base networks of the sub-networks it activates, its networks, then
    /// its own selections.
    fn expand_model(
        &mut self,
        index: usize,
        model: &'a ModelDefinition,
        problems: &mut Vec<ProjectManifestProblem>,
    ) -> Expansion<'a> {
        if let Some(expansion) = self.expanded_models.get(&(index, model.name.as_str())) {
            return expansion.clone();
        }
        let start = problems.len();
        let definition = DefinitionRef::new(DefinitionKind::Model, &model.name);
        // The base networks of the sub-networks activated here come straight after the base model.
        let mut expansion = self.activate_sub_networks(index, &definition, &model.sub_networks, None, problems);
        for reference in unique_network_references(&definition, &model.networks, problems) {
            let network = self.expand_reference(index, &definition, reference, problems);
            expansion.extend(network);
        }
        let includes = self.resolve_includes(index, &definition, None, &model.include, problems);
        expansion.files.extend(includes);
        expansion.dedup(&definition, problems);
        self.attribute(index, problems, start);
        self.expanded_models.insert((index, &model.name), expansion.clone());
        expansion
    }

    /// Expand the network definition `name` in project `index` into its files, in merge order:
    /// its base sub-network's base network, those of each definition it extends, then its own
    /// selections.
    ///
    /// The caller must check that `name` exists; a missing name expands to nothing.
    fn expand(&mut self, index: usize, name: &'a str, problems: &mut Vec<ProjectManifestProblem>) -> Expansion<'a> {
        if let Some(expansion) = self.expanded.get(&(index, name)) {
            return expansion.clone();
        }
        let project = self.project(index);
        if let Some(start) = self.stack.iter().position(|entry| *entry == (index, name)) {
            let mut cycle: Vec<String> = self.stack[start..]
                .iter()
                .map(|(entry_index, entry)| self.project(*entry_index).label(entry))
                .collect();
            cycle.push(project.label(name));
            problems.push(ProjectManifestProblem::NetworkCycle { cycle });
            return Expansion::default();
        }
        let Some(network) = project.manifest.find_network(name) else {
            return Expansion::default();
        };

        let start = problems.len();
        self.stack.push((index, &network.name));
        let definition = DefinitionRef::new(DefinitionKind::Network, &network.name);
        let mut expansion =
            self.activate_sub_networks(index, &definition, &network.sub_networks, Some(&network.name), problems);
        for parent in unique_network_references(&definition, &network.extends, problems) {
            let parent = self.expand_reference(index, &definition, parent, problems);
            expansion.extend(parent);
        }
        let includes = self.resolve_includes(index, &definition, Some(&network.name), &network.include, problems);
        expansion.files.extend(includes);
        self.stack.pop();

        expansion.dedup(&definition, problems);
        self.attribute(index, problems, start);
        self.expanded.insert((index, &network.name), expansion.clone());
        expansion
    }

    /// Resolve the `include` list of `definition` in project `from`, appending problems
    /// attributed to `definition`.
    ///
    /// Returns the files selected, in selection order.
    fn resolve_includes(
        &self,
        from: usize,
        definition: &DefinitionRef,
        network: Option<&'a str>,
        include: &'a [DefinitionSelection],
        problems: &mut Vec<ProjectManifestProblem>,
    ) -> Vec<SelectedFile<'a>> {
        let mut files = Vec::new();
        let mut selected_sets = HashSet::new();
        let mut duplicate_sets = HashSet::new();
        for selection in include {
            if !selected_sets.insert(selection.set.as_str()) {
                // Report each duplicated set once; only its first selection is resolved.
                if duplicate_sets.insert(selection.set.as_str()) {
                    problems.push(ProjectManifestProblem::DuplicateSet {
                        definition: definition.clone(),
                        set: selection.set.clone(),
                        count: include.iter().filter(|other| other.set == selection.set).count(),
                    });
                }
                continue;
            }
            let Some((index, name)) = self.locate(from, definition, &selection.set, ItemKind::NetworkSet, problems)
            else {
                continue;
            };
            let project = &self.project(index).resolved;
            let Some(set) = project.set(name) else {
                if !project.unresolved_sets.contains(name) {
                    problems.push(ProjectManifestProblem::SetNotFound {
                        definition: definition.clone(),
                        set: selection.set.clone(),
                    });
                }
                continue;
            };

            let selected = resolve_selection(definition, selection, set, problems);
            for file in unused_file_meta_keys(selection, &selected) {
                problems.push(ProjectManifestProblem::UnusedFileMeta {
                    definition: definition.clone(),
                    set: set.label.clone(),
                    file: file.to_string(),
                });
            }

            files.extend(selected.into_iter().map(|file| {
                let position_offset = file
                    .name
                    .to_str()
                    .and_then(|name| selection.file_meta.as_ref()?.get(name))
                    .and_then(|meta| meta.position_offset.clone());
                SelectedFile {
                    file,
                    set: Some(set),
                    position_offset: position_offset.map(PositionOffset::from),
                    network,
                }
            }));
        }
        files
    }
}

impl From<ProjectManifestProblem> for ComposeModelError {
    fn from(problem: ProjectManifestProblem) -> Self {
        match problem {
            ProjectManifestProblem::DefinitionNotFound { definition } => Self::DefinitionNotFound { definition },
            ProjectManifestProblem::NetworkDefinitionNotFound { network } => {
                Self::NetworkDefinitionNotFound { network }
            }
            ProjectManifestProblem::DuplicateNetworkSet { set, .. } => Self::DuplicateNetworkSet { set },
            ProjectManifestProblem::DuplicateDefinition { definition, .. } => Self::DuplicateDefinition { definition },
            ProjectManifestProblem::DuplicateNetworkDefinition { network, .. } => {
                Self::DuplicateNetworkDefinition { network }
            }
            ProjectManifestProblem::DuplicateSubNetwork { sub_network, .. } => {
                Self::DuplicateSubNetwork { sub_network }
            }
            ProjectManifestProblem::InvalidName { kind, name } => Self::InvalidName { kind, name },
            ProjectManifestProblem::NetworkNotFound { definition, network } => {
                Self::NetworkNotFound { definition, network }
            }
            ProjectManifestProblem::SubNetworkNotFound {
                definition,
                sub_network,
            } => Self::SubNetworkNotFound {
                definition,
                sub_network,
            },
            ProjectManifestProblem::SubNetworkNotActive {
                definition,
                set,
                sub_network,
            } => Self::SubNetworkNotActive {
                definition,
                set,
                sub_network,
            },
            ProjectManifestProblem::NetworkCycle { cycle } => Self::NetworkCycle { cycle },
            ProjectManifestProblem::BaseModelMissing { definition } => Self::BaseModelMissing { definition },
            ProjectManifestProblem::ModelNotFound { definition, model } => Self::ModelNotFound { definition, model },
            ProjectManifestProblem::InvalidBaseModel { definition, error } => Self::InvalidBaseModel {
                definition,
                error: Box::new(error),
            },
            ProjectManifestProblem::DuplicateNetworkReference {
                definition, network, ..
            } => Self::DuplicateNetworkReference { definition, network },
            ProjectManifestProblem::DuplicateMultiModelNetwork {
                definition, network, ..
            } => Self::DuplicateMultiModelNetwork { definition, network },
            ProjectManifestProblem::TransferSourceNotFound {
                network,
                transfer,
                from_network,
            } => Self::TransferSourceNotFound {
                network,
                transfer,
                from_network,
            },
            ProjectManifestProblem::TransferFromSameNetwork { network, transfer } => {
                Self::TransferFromSameNetwork { network, transfer }
            }
            ProjectManifestProblem::DuplicateTransfer { network, transfer, .. } => {
                Self::DuplicateTransfer { network, transfer }
            }
            ProjectManifestProblem::DuplicateSet { set, .. } => Self::DuplicateSelection { set },
            ProjectManifestProblem::SetNotFound { set, .. } => Self::SetNotFound { set },
            ProjectManifestProblem::FileNotFound { set, file, .. } => Self::FileNotFound { set, file },
            ProjectManifestProblem::DuplicateFile { set, file, .. } => Self::DuplicateFile { set, file },
            ProjectManifestProblem::UnusedFileMeta { set, file, .. } => Self::UnusedFileMeta { set, file },
            ProjectManifestProblem::InvalidFilePath { set, file, .. } => {
                Self::Resolution(ManifestResolutionError::InvalidRelativePath {
                    field: format!("selected file in network set '{set}'"),
                    path: PathBuf::from(file),
                })
            }
            ProjectManifestProblem::MinFilesNotMet {
                set,
                min_files,
                actual_files,
                ..
            } => Self::MinFilesNotMet {
                set,
                min_files,
                actual_files,
            },
            ProjectManifestProblem::MaxFilesExceeded {
                set,
                max_files,
                actual_files,
                ..
            } => Self::MaxFilesExceeded {
                set,
                max_files,
                actual_files,
            },
            ProjectManifestProblem::InvalidFileConstraints {
                set,
                min_files,
                max_files,
            } => Self::InvalidFileConstraints {
                set,
                min_files,
                max_files,
            },
            ProjectManifestProblem::SharedSetDirectory { set, other, path } => {
                Self::SharedSetDirectory { set, other, path }
            }
            ProjectManifestProblem::SetInSubNetwork { set, sub_network } => Self::SetInSubNetwork { set, sub_network },
            ProjectManifestProblem::OverlappingSubNetworks { sub_network, other } => {
                Self::OverlappingSubNetworks { sub_network, other }
            }
            ProjectManifestProblem::BaseNetworkInSet { sub_network, set } => {
                Self::BaseNetworkInSet { sub_network, set }
            }
            ProjectManifestProblem::FileInSeveralSets {
                definition,
                file,
                first,
                second,
            } => Self::FileInSeveralSets {
                definition,
                file,
                first,
                second,
            },
            ProjectManifestProblem::ConflictingPositionOffsets { definition, file, set } => {
                Self::ConflictingPositionOffsets { definition, file, set }
            }
            ProjectManifestProblem::DuplicateDependency { dependency, .. } => Self::DuplicateDependency { dependency },
            ProjectManifestProblem::DependencyNameTaken { dependency } => Self::DependencyNameTaken { dependency },
            ProjectManifestProblem::DependencyCycle { cycle } => Self::DependencyCycle { cycle },
            ProjectManifestProblem::NotExported {
                definition,
                project,
                kind,
                item,
            } => Self::NotExported {
                definition,
                project,
                kind,
                item,
            },
            ProjectManifestProblem::ExportNotFound { kind, name } => Self::ExportNotFound { kind, name },
            ProjectManifestProblem::InDependency { dependency, problem } => Self::InDependency {
                dependency,
                error: Box::new((*problem).into()),
            },
            ProjectManifestProblem::Resolution(error) => Self::Resolution(error),
        }
    }
}

/// Another project, whose definitions this project may refer to as `<name>::<item>`.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    /// The name this project uses for the dependency.
    pub name: String,
    /// Path (relative to root) to the dependency's manifest. Unlike every other path in a
    /// manifest, it may lead out of the project with `..`.
    pub manifest: String,
    /// The expected SHA-256 hash of the dependency's manifest file, in hexadecimal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Directory (relative to the dependency's root) that the data paths in the dependency's own
    /// files are relative to. Default: the dependency's root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_dir: Option<String>,
}

/// What projects that depend on a project may refer to.
#[derive(Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Exports {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub networks: Vec<String>,
    /// Model definitions, which other projects may use only as the network of a multi-model.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
    /// Network sets, named as this project refers to them, such as `upper::options`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub network_sets: Vec<String>,
    /// Sub-networks, which other projects may use as a network definition's `base`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sub_networks: Vec<String>,
}

/// A directory holding the base network and network sets for part of a wider model.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubNetwork {
    pub name: String,
    /// Directory containing the sub-network (relative to root). Default: the sub-network's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    /// Path (relative to the sub-network directory) to the base NetworkSchema JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_network: Option<String>,
    /// Directory (relative to the sub-network directory) that the data paths in the sub-network's
    /// files are relative to. Default: the sub-network directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_dir: Option<String>,
    /// Named sets of NetworkSchema fragments, with directories relative to the sub-network directory.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub network_sets: Vec<NetworkSet>,
}

/// A named, reusable composition of network fragments that resolves to a `NetworkSchema`.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetworkDefinition {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Sub-networks to activate. Their base networks are included first, in the order the
    /// sub-networks are declared.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sub_networks: Vec<String>,
    /// Other network definitions to include next, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extends: Vec<String>,
    /// Per-network-set selections for this network definition.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<DefinitionSelection>,
}

/// A named model, composed from a base model, network definitions and further selections.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelDefinition {
    pub name: String,
    /// Path (relative to root) to the base ModelSchema JSON, overriding the project's `base_model`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_model: Option<String>,
    /// Sub-networks to activate. Their base networks are included after the base model, in the
    /// order the sub-networks are declared.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sub_networks: Vec<String>,
    /// Network definitions to include after the base model, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub networks: Vec<String>,
    /// Per-network-set selections for this definition, included after its networks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<DefinitionSelection>,
    /// Optional overrides to base model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overrides: Option<DefinitionOverrides>,
}

/// A model definition resolved into its base model and the networks to include in merge order.
struct ResolvedModel {
    base_model: Option<ResolvedBaseModel>,
    includes: Vec<ComposedNetworkPath>,
}

/// The network definitions in `references`, each once, reporting those listed more than once.
fn unique_network_references<'a>(
    definition: &DefinitionRef,
    references: &'a [String],
    problems: &mut Vec<ProjectManifestProblem>,
) -> Vec<&'a String> {
    let mut unique: Vec<&String> = Vec::with_capacity(references.len());
    for reference in references {
        if unique.contains(&reference) {
            continue;
        }
        let count = references.iter().filter(|other| *other == reference).count();
        if count > 1 {
            problems.push(ProjectManifestProblem::DuplicateNetworkReference {
                definition: definition.clone(),
                network: reference.clone(),
                count,
            });
        }
        unique.push(reference);
    }
    unique
}

/// Which network sets [`check_composition`] applies the limits of in full, rather than only
/// when something is selected from them.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SetLimits {
    /// The sets of the project with this index, as for one of its models, which its project's
    /// limits are written for. The sets of active sub-networks are included, in any project.
    ModelOf(usize),
    /// No project's sets, as for a network of a multi-model that is not built from a model, and
    /// is only one part of the multi-model. The sets of active sub-networks are still included.
    SelectedSets,
}

/// Check an expansion as the complete network of `definition`: every sub-network it selects
/// from must be active, and every network set's limits must hold.
fn check_composition(
    definition: &DefinitionRef,
    expansion: &Expansion<'_>,
    graph: &ProjectGraph<'_>,
    limits: SetLimits,
    problems: &mut Vec<ProjectManifestProblem>,
) {
    check_active(definition, expansion, problems);

    // Set constraints apply to everything selected from a set, by any route. A sub-network's
    // sets are only constrained when the sub-network is active.
    for set in graph.projects.iter().flat_map(|project| &project.resolved.sets) {
        if let Some(sub_network) = &set.sub_network
            && !expansion
                .active_sub_networks
                .contains(&(set.project, sub_network.as_str()))
        {
            continue;
        }
        let count = expansion
            .files
            .iter()
            .filter(|selected| selected.set.is_some_and(|selected_set| std::ptr::eq(selected_set, set)))
            .count();
        // An active sub-network's sets are always constrained, whichever project it is in. Other
        // sets of another project are only constrained when something is selected from them, and
        // so are the root project's own sets for a multi-model network that is not a model.
        let selected_only = set.sub_network.is_none()
            && match limits {
                SetLimits::ModelOf(project) => set.project != project,
                SetLimits::SelectedSets => true,
            };
        if selected_only && count == 0 {
            continue;
        }
        validate_constraints(definition, set, count, problems);
    }
}

/// A named multi-network model: several networks, linked by transfers, that share a time domain
/// and scenarios.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MultiModelDefinition {
    pub name: String,
    /// Where the time domain, scenarios and metadata come from.
    pub domain: MultiModelDomain,
    /// Optional overrides to the domain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overrides: Option<DefinitionOverrides>,
    /// The networks of the model, each with the transfers into it.
    pub networks: Vec<MultiModelNetwork>,
}

/// Where a multi-model takes its time domain, scenarios and metadata from, chosen by `type`.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum MultiModelDomain {
    /// A local model definition's base model, with that definition's overrides applied.
    Model(ModelDomain),
    /// A ModelSchema JSON file, whose network is ignored.
    BaseModel(BaseModelDomain),
    /// A time domain and scenarios given inline.
    Inline(InlineDomain),
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelDomain {
    /// The name of the model definition.
    pub name: String,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BaseModelDomain {
    /// Path (relative to root) to the ModelSchema JSON.
    pub path: String,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InlineDomain {
    pub time: TimeDomain,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenarios: Option<ScenarioDomain>,
}

/// One network of a multi-model.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MultiModelNetwork {
    pub name: String,
    /// The definition the network starts from. Without one, the network holds only `include`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<MultiModelNetworkSource>,
    /// Sub-networks to activate. Their base networks are included first, in the order the
    /// sub-networks are declared.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sub_networks: Vec<String>,
    /// Per-network-set selections, included after the network's source.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<DefinitionSelection>,
    /// Transfers into this network from other networks of the multi-model.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transfers: Vec<MultiNetworkTransfer>,
    /// Path (relative to root) to a JSON list of further transfers into this network.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfers_file: Option<String>,
}

/// The definition a multi-model network starts from, chosen by `type`.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum MultiModelNetworkSource {
    /// A network definition.
    Network(NetworkSource),
    /// The network part of a model definition: its base model's network and everything merged
    /// into it. The model's time domain, scenarios and overrides are not used.
    Model(ModelSource),
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetworkSource {
    /// The name of the network definition.
    pub name: String,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelSource {
    /// The name of the model definition.
    pub name: String,
}

/// A multi-model definition resolved into its domain and networks.
struct ResolvedMultiModel {
    domain: Option<ComposedDomain>,
    overrides: Vec<DefinitionOverrides>,
    networks: Vec<ComposedMultiModelNetwork>,
}

impl MultiModelDefinition {
    /// Resolve this definition's domain and networks.
    ///
    /// Problems are appended in the order the definition is composed, so the first one is the
    /// error that composition reports.
    fn resolve<'a>(
        &'a self,
        resolver: &mut Resolver<'a>,
        problems: &mut Vec<ProjectManifestProblem>,
    ) -> ResolvedMultiModel {
        let definition = DefinitionRef::new(DefinitionKind::MultiModel, &self.name);
        let mut overrides = Vec::new();
        let domain = match &self.domain {
            MultiModelDomain::Model(ModelDomain { name: model }) => {
                match resolver.project(ROOT).manifest.find_model(model) {
                    Some(definition) => {
                        overrides.extend(definition.overrides.clone());
                        resolver
                            .model_base_model(ROOT, definition, problems)
                            .map(|(path, _)| ComposedDomain::BaseModel(path))
                    }
                    None => {
                        problems.push(ProjectManifestProblem::ModelNotFound {
                            definition: definition.clone(),
                            model: model.clone(),
                        });
                        None
                    }
                }
            }
            MultiModelDomain::BaseModel(BaseModelDomain { path: base_model }) => {
                match resolve_base_model(&resolver.project(ROOT).root, base_model) {
                    Ok((path, _)) => Some(ComposedDomain::BaseModel(path)),
                    Err(error) => {
                        problems.push(ProjectManifestProblem::InvalidBaseModel {
                            definition: definition.clone(),
                            error,
                        });
                        None
                    }
                }
            }
            MultiModelDomain::Inline(InlineDomain { time, scenarios }) => Some(ComposedDomain::Inline {
                time: time.clone(),
                scenarios: scenarios.clone(),
            }),
        };
        overrides.extend(self.overrides.clone());

        add_duplicate_problems(
            &self.networks,
            |network| &network.name,
            |name, count| ProjectManifestProblem::DuplicateMultiModelNetwork {
                definition: definition.clone(),
                network: name,
                count,
            },
            problems,
        );
        let networks = self
            .networks
            .iter()
            .map(|network| network.resolve(self, resolver, problems))
            .collect();

        ResolvedMultiModel {
            domain,
            overrides,
            networks,
        }
    }
}

impl MultiModelNetwork {
    fn resolve<'a>(
        &'a self,
        multi_model: &MultiModelDefinition,
        resolver: &mut Resolver<'a>,
        problems: &mut Vec<ProjectManifestProblem>,
    ) -> ComposedMultiModelNetwork {
        // Problems in this network are reported against `<multi-model>/<network>`.
        let label = DefinitionRef::new(
            DefinitionKind::MultiModelNetwork,
            &format!("{}/{}", multi_model.name, self.name),
        );
        let mut base_model = None;
        // A model used as it is was, or will be, checked as that model. A model with additions is
        // checked against its own project's limits in full.
        let mut whole_model = None;
        let mut limits = SetLimits::SelectedSets;
        let mut expansion = resolver.activate_sub_networks(ROOT, &label, &self.sub_networks, None, problems);
        match &self.network {
            Some(MultiModelNetworkSource::Network(NetworkSource { name: network })) => {
                expansion.extend(resolver.expand_reference(ROOT, &label, network, problems));
            }
            Some(MultiModelNetworkSource::Model(ModelSource { name: model })) => {
                if let Some((index, name)) = resolver.locate(ROOT, &label, model, ItemKind::Model, problems) {
                    match resolver.project(index).manifest.find_model(name) {
                        Some(definition) => {
                            base_model = resolver
                                .model_base_model(index, definition, problems)
                                .map(|base_model| (index, base_model));
                            expansion.extend(resolver.expand_model(index, definition, problems));
                            limits = SetLimits::ModelOf(index);
                            if self.sub_networks.is_empty() && self.include.is_empty() {
                                whole_model = Some((index, definition));
                            }
                        }
                        None => problems.push(ProjectManifestProblem::ModelNotFound {
                            definition: label.clone(),
                            model: model.clone(),
                        }),
                    }
                }
            }
            None => {}
        }
        let includes = resolver.resolve_includes(ROOT, &label, None, &self.include, problems);
        expansion.files.extend(includes);
        expansion.dedup(&label, problems);
        match whole_model {
            Some((index, model)) => resolver.check_model(index, model, &expansion, problems),
            None => check_composition(&label, &expansion, resolver.graph, limits, problems),
        }

        let (transfers, transfers_file) = self.resolve_transfers(multi_model, &resolver.project(ROOT).root, problems);
        // A model's base model network is the starting point, with its data paths rebased like
        // the rest of its project's files.
        let base_model = base_model.map(|(index, (path, file))| {
            let scope = &resolver.project(index).scope;
            ComposedNetworkPath {
                path,
                position_offset: None,
                source: ProvenanceSource {
                    file,
                    network_set: None,
                    sub_network: None,
                    project: scope.alias.clone(),
                    network: None,
                },
                context: LoadContext {
                    data_dir: scope.data_dir.clone(),
                },
            }
        });
        ComposedMultiModelNetwork {
            network: ComposedNetwork::new(
                self.name.clone(),
                base_model,
                expansion.files.into_iter().map(SelectedFile::into_composed).collect(),
            ),
            transfers,
            transfers_file,
        }
    }

    /// The transfers into this network, given inline and in `transfers_file`, checked against
    /// the other networks of `multi_model`, with the canonical path of the transfers file.
    fn resolve_transfers(
        &self,
        multi_model: &MultiModelDefinition,
        root: &Path,
        problems: &mut Vec<ProjectManifestProblem>,
    ) -> (Vec<MultiNetworkTransfer>, Option<PathBuf>) {
        let mut transfers = self.transfers.clone();
        let mut transfers_path = None;
        if let Some(transfers_file) = &self.transfers_file {
            match read_transfers(root, transfers_file) {
                Ok((path, from_file)) => {
                    transfers.extend(from_file);
                    transfers_path = Some(path);
                }
                Err(error) => problems.push(error.into()),
            }
        }

        let network = format!("{}/{}", multi_model.name, self.name);
        for transfer in &transfers {
            if transfer.from_network == self.name {
                problems.push(ProjectManifestProblem::TransferFromSameNetwork {
                    network: network.clone(),
                    transfer: transfer.name.clone(),
                });
            } else if !multi_model
                .networks
                .iter()
                .any(|other| other.name == transfer.from_network)
            {
                problems.push(ProjectManifestProblem::TransferSourceNotFound {
                    network: network.clone(),
                    transfer: transfer.name.clone(),
                    from_network: transfer.from_network.clone(),
                });
            }
        }
        add_duplicate_problems(
            &transfers,
            |transfer| &transfer.name,
            |name, count| ProjectManifestProblem::DuplicateTransfer {
                network: network.clone(),
                transfer: name,
                count,
            },
            problems,
        );
        (transfers, transfers_path)
    }
}

/// The canonical path of `transfers_file`, relative to `root`.
fn transfers_path(root: &Path, transfers_file: &str) -> Result<PathBuf, ManifestResolutionError> {
    let candidate = strict_relative_path("transfers file", transfers_file)?.to_path(root);
    if !candidate.exists() {
        return Err(ManifestResolutionError::TransfersFileNotFound { path: candidate });
    }
    canonicalize_contained(root, &candidate, "transfers file")
}

/// Read a JSON list of transfers from `transfers_file`, relative to `root`, returning the file's
/// canonical path with the transfers.
fn read_transfers(
    root: &Path,
    transfers_file: &str,
) -> Result<(PathBuf, Vec<MultiNetworkTransfer>), ManifestResolutionError> {
    let candidate = strict_relative_path("transfers file", transfers_file)?.to_path(root);
    let path = transfers_path(root, transfers_file)?;
    let data = std::fs::read_to_string(&path).map_err(|source| ManifestResolutionError::TransfersFileRead {
        path: candidate.clone(),
        source,
    })?;
    let transfers = serde_json::from_str(&data).map_err(|source| ManifestResolutionError::TransfersFileJson {
        path: candidate,
        source,
    })?;
    Ok((path, transfers))
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DefinitionSelectionPositionOffset {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schematic: Option<(f32, f32)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geographic: Option<(f32, f32)>,
}

impl From<DefinitionSelectionPositionOffset> for PositionOffset {
    fn from(offset: DefinitionSelectionPositionOffset) -> Self {
        PositionOffset {
            schematic: offset.schematic,
            geographic: offset.geographic,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DefinitionSelectionFileMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position_offset: Option<DefinitionSelectionPositionOffset>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DefinitionSelection {
    /// Name of the network set.
    pub set: String,
    /// Specific filenames to include from the set directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,
    /// If true, include all JSON files in the set directory. Overrides `files` if both are specified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_all: Option<bool>,
    /// Optional metadata for each selected file.
    /// The keys are the filenames, and the values are the metadata for that file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_meta: Option<HashMap<String, DefinitionSelectionFileMeta>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetworkSet {
    pub name: String,
    /// Directory containing NetworkSchema fragments (relative to root).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    /// Minimum number of files required from this set (inclusive). Default: 0.
    #[serde(default, alias = "min", skip_serializing_if = "Option::is_none")]
    pub min_files: Option<usize>,
    /// Maximum number of files allowed from this set (inclusive). Default: unlimited.
    #[serde(default, alias = "max", skip_serializing_if = "Option::is_none")]
    pub max_files: Option<usize>,
}

struct ResolvedNetworkSet {
    /// The set's name as definitions in its project refer to it, qualified with its
    /// sub-network's name if any.
    name: String,
    /// The set's name as seen from the root project, used in problems.
    label: String,
    /// The index of the set's project.
    project: usize,
    /// The alias of the set's project, if it is a dependency.
    alias: Option<String>,
    /// The set's name within its sub-network or project, as recorded in provenance.
    set_name: String,
    sub_network: Option<String>,
    /// The canonical path of the set's directory.
    dir: PathBuf,
    /// Directory, relative to the root project's directory, that relative data paths in the
    /// set's files are relative to, or `None` if they are relative to the root project's directory.
    data_dir: Option<PathBuf>,
    min_files: Option<usize>,
    max_files: Option<usize>,
    files: Vec<ResolvedNetworkFile>,
}

struct ResolvedNetworkFile {
    name: OsString,
    path: PathBuf,
    /// The file's path relative to its project's root.
    source: RelativePathBuf,
    /// The sub-network the file belongs to, if any.
    sub_network: Option<String>,
    /// The alias of the file's project, if it is a dependency.
    project: Option<String>,
    /// Directory, relative to the root project's directory, that the file's relative data paths
    /// are relative to, or `None` if they are already relative to the root project's directory.
    data_dir: Option<PathBuf>,
}

impl ResolvedNetworkFile {
    /// This file as listed in a [`ProjectLayout`], from `set` if it is in one.
    fn layout(&self, set: Option<&ResolvedNetworkSet>) -> NetworkFileLayout {
        NetworkFileLayout {
            path: self.path.clone(),
            source: ProvenanceSource {
                file: self.source.clone(),
                network_set: set.map(|set| set.set_name.clone()),
                sub_network: self.sub_network.clone(),
                project: self.project.clone(),
                network: None,
            },
        }
    }
}

struct ResolvedSubNetwork {
    name: String,
    /// The index of the sub-network's project.
    project: usize,
    /// The canonical path of the sub-network's directory.
    dir: PathBuf,
    /// The sub-network's data directory relative to the root project's directory.
    data_dir: PathBuf,
    base_network: Option<ResolvedNetworkFile>,
}

/// Where a sub-network's files live, used to resolve its network sets.
struct SubNetworkScope {
    name: String,
    /// The sub-network directory relative to its project's root.
    dir: RelativePathBuf,
    /// The canonical path of the sub-network directory.
    root: PathBuf,
    /// The sub-network's data directory relative to the root project's directory.
    data_dir: PathBuf,
}

fn resolve_selection<'a>(
    definition: &DefinitionRef,
    selection: &DefinitionSelection,
    set: &'a ResolvedNetworkSet,
    errors: &mut Vec<ProjectManifestProblem>,
) -> Vec<&'a ResolvedNetworkFile> {
    if selection.include_all.unwrap_or(false) {
        return set.files.iter().collect();
    }
    let mut result = Vec::new();
    let mut names = HashSet::new();
    for file in selection.files.as_deref().unwrap_or_default() {
        if !is_filename(file) {
            errors.push(ProjectManifestProblem::InvalidFilePath {
                definition: definition.clone(),
                set: set.label.clone(),
                file: file.clone(),
            });
        } else if !names.insert(file) {
            errors.push(ProjectManifestProblem::DuplicateFile {
                definition: definition.clone(),
                set: set.label.clone(),
                file: file.clone(),
            });
        } else if let Some(resolved) = set.files.iter().find(|candidate| candidate.name == OsStr::new(file)) {
            result.push(resolved);
        } else {
            errors.push(ProjectManifestProblem::FileNotFound {
                definition: definition.clone(),
                set: set.label.clone(),
                file: file.clone(),
            });
        }
    }
    result
}

fn unused_file_meta_keys<'a>(
    selection: &'a DefinitionSelection,
    resolved_files: &[&ResolvedNetworkFile],
) -> Vec<&'a str> {
    let mut unused = selection
        .file_meta
        .iter()
        .flat_map(|meta| meta.keys())
        .filter(|name| !resolved_files.iter().any(|file| file.name == OsStr::new(name)))
        .map(String::as_str)
        .collect::<Vec<_>>();
    unused.sort_unstable();
    unused
}

fn validate_constraints(
    definition: &DefinitionRef,
    set: &ResolvedNetworkSet,
    count: usize,
    errors: &mut Vec<ProjectManifestProblem>,
) {
    if let Some(min_files) = set.min_files
        && count < min_files
    {
        errors.push(ProjectManifestProblem::MinFilesNotMet {
            definition: definition.clone(),
            set: set.label.clone(),
            min_files,
            actual_files: count,
        });
    }
    if let Some(max_files) = set.max_files
        && count > max_files
    {
        errors.push(ProjectManifestProblem::MaxFilesExceeded {
            definition: definition.clone(),
            set: set.label.clone(),
            max_files,
            actual_files: count,
        });
    }
}

/// Resolve the base model, returning its canonical path and its path relative to `root`.
fn resolve_base_model(root: &Path, base_model: &str) -> Result<(PathBuf, RelativePathBuf), ManifestResolutionError> {
    let file = strict_relative_path("base model", base_model)?;
    let candidate = file.to_path(root);
    if !candidate.exists() {
        return Err(ManifestResolutionError::BaseModelNotFound { path: candidate });
    }
    let path = canonicalize_contained(root, &candidate, "base model")?;
    if !path.is_file() {
        return Err(ManifestResolutionError::BaseModelNotAFile { path });
    }
    Ok((path, file))
}

/// Resolve a sub-network's directory and base network.
fn resolve_sub_network(
    root: &Path,
    sub_network: &SubNetwork,
    project: &ProjectScope,
) -> Result<(ResolvedSubNetwork, SubNetworkScope), ManifestResolutionError> {
    let name = &sub_network.name;
    let field = format!("sub-network '{name}' directory");
    let dir = strict_relative_path(&field, sub_network.dir.as_deref().unwrap_or(name))?;
    let candidate = dir.to_path(root);
    if !candidate.exists() {
        return Err(ManifestResolutionError::SubNetworkDirectoryNotFound {
            sub_network: name.clone(),
            path: candidate,
        });
    }
    let sub_root = canonicalize_contained(root, &candidate, &field)?;
    if !sub_root.is_dir() {
        return Err(ManifestResolutionError::SubNetworkNotADirectory {
            sub_network: name.clone(),
            path: sub_root,
        });
    }
    let data_dir = match &sub_network.data_dir {
        Some(data_dir) => {
            let data_dir = dir.join(strict_relative_path(
                &format!("sub-network '{name}' data directory"),
                data_dir,
            )?);
            check_data_directory(&data_dir.to_path(root), format!("sub-network `{name}`"))?;
            data_dir
        }
        None => dir.clone(),
    };
    let data_dir = match &project.prefix {
        Some(prefix) => prefix.join(data_dir.to_path("")),
        None => data_dir.to_path(""),
    };

    let base_network = match &sub_network.base_network {
        Some(base_network) => {
            let field = format!("sub-network '{name}' base network");
            let file = strict_relative_path(&field, base_network)?;
            let candidate = file.to_path(&sub_root);
            if !candidate.exists() {
                return Err(ManifestResolutionError::BaseNetworkNotFound {
                    sub_network: name.clone(),
                    path: candidate,
                });
            }
            let path = canonicalize_contained(&sub_root, &candidate, &field)?;
            if !path.is_file() {
                return Err(ManifestResolutionError::BaseNetworkNotAFile {
                    sub_network: name.clone(),
                    path,
                });
            }
            Some(ResolvedNetworkFile {
                name: OsString::from(file.file_name().unwrap_or_default()),
                path,
                source: dir.join(&file),
                sub_network: Some(name.clone()),
                project: project.alias.clone(),
                data_dir: Some(data_dir.clone()),
            })
        }
        None => None,
    };

    Ok((
        ResolvedSubNetwork {
            name: name.clone(),
            project: project.index,
            dir: sub_root.clone(),
            data_dir: data_dir.clone(),
            base_network,
        },
        SubNetworkScope {
            name: name.clone(),
            dir,
            root: sub_root,
            data_dir,
        },
    ))
}

/// Resolve a network set's directory under `root` and list its files.
///
/// `name` is the set's name as definitions in its project refer to it. `project` is the project
/// the set belongs to, and `scope` its sub-network, if any, in which case `root` is the
/// sub-network directory.
fn resolve_network_set(
    root: &Path,
    name: &str,
    set: &NetworkSet,
    project: &ProjectScope,
    scope: Option<&SubNetworkScope>,
) -> Result<ResolvedNetworkSet, ManifestResolutionError> {
    let dir = strict_relative_path(
        &format!("network set '{name}' directory"),
        set.dir.as_deref().unwrap_or(&set.name),
    )?;
    let candidate = dir.to_path(root);
    if !candidate.exists() {
        return Err(ManifestResolutionError::DirectoryNotFound {
            set: name.to_string(),
            path: candidate,
        });
    }
    let root = canonicalize_contained(root, &candidate, &format!("network set '{name}' directory"))?;
    if !root.is_dir() {
        return Err(ManifestResolutionError::NotADirectory {
            set: name.to_string(),
            path: root,
        });
    }
    // The directory relative to the project root, for provenance.
    let source_dir = match scope {
        Some(scope) => scope.dir.join(&dir),
        None => dir,
    };
    let mut files = Vec::new();
    let entries = std::fs::read_dir(&root).map_err(|source| ManifestResolutionError::DirectoryRead {
        set: name.to_string(),
        path: root.clone(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| ManifestResolutionError::DirectoryRead {
            set: name.to_string(),
            path: root.clone(),
            source,
        })?;
        let file_name = entry.file_name();
        if Path::new(&file_name).extension() != Some(OsStr::new("json")) || !entry.path().is_file() {
            continue;
        }
        let path = canonicalize_contained(&root, &entry.path(), &format!("network set '{name}' file"))?;
        let source = RelativePath::from_path(Path::new(&file_name))
            .map(|filename| source_dir.join(filename))
            .map_err(|_| ManifestResolutionError::InvalidRelativePath {
                field: format!("network set '{name}' file"),
                path: entry.path(),
            })?;
        files.push(ResolvedNetworkFile {
            name: file_name,
            path,
            source,
            sub_network: scope.map(|scope| scope.name.clone()),
            project: project.alias.clone(),
            data_dir: scope.map_or_else(|| project.data_dir.clone(), |scope| Some(scope.data_dir.clone())),
        });
    }
    files.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(ResolvedNetworkSet {
        name: name.to_string(),
        label: qualify(project.alias.as_deref(), name),
        project: project.index,
        alias: project.alias.clone(),
        set_name: set.name.clone(),
        sub_network: scope.map(|scope| scope.name.clone()),
        dir: root,
        data_dir: scope.map_or_else(|| project.data_dir.clone(), |scope| Some(scope.data_dir.clone())),
        min_files: set.min_files,
        max_files: set.max_files,
        files,
    })
}

/// Check that a data directory exists, since a mistyped one would otherwise only show up as
/// missing data files, at paths the user never wrote, when the model is built.
fn check_data_directory(path: &Path, owner: String) -> Result<(), ManifestResolutionError> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(ManifestResolutionError::DataDirectoryNotFound {
            owner,
            path: path.to_path_buf(),
        })
    }
}

fn strict_relative_path(field: &str, value: &str) -> Result<RelativePathBuf, ManifestResolutionError> {
    let path = Path::new(value);
    if value.is_empty()
        || value.contains('\\')
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || RelativePath::from_path(path).is_err()
    {
        return Err(ManifestResolutionError::InvalidRelativePath {
            field: field.to_string(),
            path: path.to_path_buf(),
        });
    }
    Ok(RelativePathBuf::from(value))
}

fn is_filename(value: &str) -> bool {
    !value.contains('\\')
        && matches!(
            Path::new(value).components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(_)]
        )
}

fn canonicalize_contained(root: &Path, candidate: &Path, field: &str) -> Result<PathBuf, ManifestResolutionError> {
    let canonical_root = root
        .canonicalize()
        .map_err(|source| ManifestResolutionError::UnableToCanonicalizePath {
            field: field.to_string(),
            path: root.to_path_buf(),
            source,
        })?;
    let resolved_path =
        candidate
            .canonicalize()
            .map_err(|source| ManifestResolutionError::UnableToCanonicalizePath {
                field: field.to_string(),
                path: candidate.to_path_buf(),
                source,
            })?;
    if !resolved_path.starts_with(&canonical_root) {
        return Err(ManifestResolutionError::PathEscapesRoot {
            field: field.to_string(),
            path: candidate.to_path_buf(),
            root: canonical_root,
            resolved_path,
        });
    }
    Ok(resolved_path)
}

fn add_duplicate_problems<T, F, P>(items: &[T], key: F, problem: P, problems: &mut Vec<ProjectManifestProblem>)
where
    F: Fn(&T) -> &String,
    P: Fn(String, usize) -> ProjectManifestProblem,
{
    let mut counts = HashMap::new();
    for item in items {
        *counts.entry(key(item).clone()).or_insert(0) += 1;
    }

    let mut duplicates: Vec<_> = counts.into_iter().filter(|(_, count)| *count > 1).collect();
    duplicates.sort_by(|(name_a, _), (name_b, _)| name_a.cmp(name_b));
    problems.extend(duplicates.into_iter().map(|(name, count)| problem(name, count)))
}

#[cfg(test)]
mod test {
    const OPTIONS: &ResolveOptions = &ResolveOptions {
        dependency_policy: DependencyPolicy::Allow,
        manifest_path: None,
    };
    use super::*;
    use pywr_schema::meta::ComponentMeta;
    use std::error::Error as _;
    use tempfile::tempdir;

    #[test]
    fn validation_report_formats_summary_and_all_problems() {
        let report = ProjectManifestValidationError {
            problems: vec![
                ProjectManifestProblem::DuplicateNetworkSet {
                    set: "nets".into(),
                    count: 2,
                },
                ProjectManifestProblem::Resolution(ManifestResolutionError::BaseModelNotFound {
                    path: PathBuf::from("base.json"),
                }),
            ],
        };
        assert_eq!(report.to_string(), "The project manifest has 2 problem(s).");
        assert_eq!(
            report.report().to_string(),
            "The project manifest has 2 problem(s):\n- The name `nets` is used by 2 network sets, but each name must be unique.\n- The base model `base.json` was not found."
        );

        let public_report = super::super::ProjectManifestValidationError::V1(report);
        assert_eq!(public_report.to_string(), "The project manifest has 2 problem(s).");
        assert_eq!(
            public_report.report().to_string(),
            "The project manifest has 2 problem(s):\n- The name `nets` is used by 2 network sets, but each name must be unique.\n- The base model `base.json` was not found."
        );
    }

    #[test]
    fn directory_read_validation_error_preserves_source() {
        let error = ProjectManifestProblem::Resolution(ManifestResolutionError::DirectoryRead {
            set: "nets".into(),
            path: PathBuf::from("nets"),
            source: std::io::Error::other("unreadable"),
        });
        assert_eq!(
            error.to_string(),
            "The directory `nets` for network set `nets` could not be read."
        );
        assert_eq!(error.source().unwrap().to_string(), "unreadable");
    }

    fn manifest(base_model: &str, network_sets: Vec<NetworkSet>, include: Vec<DefinitionSelection>) -> ProjectManifest {
        ProjectManifest {
            multi_models: vec![],
            dependencies: vec![],
            exports: None,
            base_model: Some(base_model.to_string()),
            sub_networks: vec![],
            network_sets,
            networks: vec![],
            models: vec![model("test", include)],
        }
    }
    fn model(name: &str, include: Vec<DefinitionSelection>) -> ModelDefinition {
        ModelDefinition {
            name: name.to_string(),
            base_model: None,
            sub_networks: vec![],
            networks: vec![],
            include,
            overrides: None,
        }
    }
    fn set(name: &str, dir: Option<&str>, min_files: Option<usize>, max_files: Option<usize>) -> NetworkSet {
        NetworkSet {
            name: name.to_string(),
            dir: dir.map(str::to_string),
            min_files,
            max_files,
        }
    }
    fn selection(set: &str, files: Option<Vec<&str>>, include_all: bool) -> DefinitionSelection {
        DefinitionSelection {
            set: set.to_string(),
            files: files.map(|files| files.into_iter().map(str::to_string).collect()),
            include_all: Some(include_all),
            file_meta: None,
        }
    }
    fn touch(path: &Path) {
        std::fs::write(path, "{}").unwrap();
    }

    fn with_offset(mut selection: DefinitionSelection, file: &str) -> DefinitionSelection {
        selection.file_meta = Some(HashMap::from([(
            file.to_string(),
            DefinitionSelectionFileMeta {
                position_offset: Some(DefinitionSelectionPositionOffset {
                    schematic: Some((10.0, -2.0)),
                    geographic: Some((1.0, 2.0)),
                }),
            },
        )]));
        selection
    }

    #[test]
    fn metadata_must_match_a_selected_file_with_explicit_or_all_selection() {
        let root = tempdir().unwrap();
        touch(&root.path().join("base.json"));
        std::fs::create_dir(root.path().join("nets")).unwrap();
        touch(&root.path().join("nets/a.json"));
        touch(&root.path().join("nets/b.json"));

        for (selection, unused) in [
            (selection("nets", Some(vec!["a.json"]), false), "b.json"),
            (selection("nets", None, true), "missing.json"),
        ] {
            let project = manifest(
                "base.json",
                vec![set("nets", None, None, None)],
                vec![with_offset(selection, unused)],
            );
            for report in [
                project.validate(root.path(), OPTIONS).unwrap_err(),
                project.validate_model(root.path(), "test", OPTIONS).unwrap_err(),
            ] {
                assert!(report.problems.iter().any(|error| matches!(
                    error,
                    ProjectManifestProblem::UnusedFileMeta { definition, set, file }
                        if definition.name == "test" && set == "nets" && file == unused
                )));
            }
            assert!(matches!(
                project.compose_model(root.path(), "test", OPTIONS),
                Err(ComposeModelError::UnusedFileMeta { set, file }) if set == "nets" && file == unused
            ));
        }
    }

    #[test]
    fn selected_file_metadata_applies_offsets_with_both_selection_modes() {
        let root = tempdir().unwrap();
        std::fs::write(
            root.path().join("base.json"),
            serde_json::to_vec(&pywr_schema::ModelSchema::default()).unwrap(),
        )
        .unwrap();
        std::fs::create_dir(root.path().join("nets")).unwrap();
        std::fs::write(
            root.path().join("nets/a.json"),
            r#"{"nodes":[{"type":"Input","meta":{"name":"a","position":{"schematic":[3.0,4.0],"geographic":[5.0,6.0]}}}],"edges":[],"virtual_nodes":[{"type":"Aggregated","nodes":[],"meta":{"name":"v","position":{"schematic":[0.0,1.0]}}}]}"#,
        )
        .unwrap();
        std::fs::write(
            root.path().join("nets/b.json"),
            r#"{"nodes":[{"type":"Output","meta":{"name":"b","position":{"schematic":[3.0,4.0]}}}],"edges":[]}"#,
        )
        .unwrap();

        for include_all in [false, true] {
            let project = manifest(
                "base.json",
                vec![set("nets", None, None, None)],
                vec![with_offset(
                    selection("nets", Some(vec!["a.json"]), include_all),
                    "a.json",
                )],
            );
            assert!(project.validate(root.path(), OPTIONS).is_ok());
            let composed = project.compose_model(root.path(), "test", OPTIONS).unwrap();
            let options = pywr_schema::NetworkMergeOptions {
                schematic_position_offset: Some((1.0, 1.0)),
                geographic_position_offset: Some((2.0, 3.0)),
                ..Default::default()
            };
            let merged = composed.load().unwrap().into_model_schema(&options).unwrap();
            let position = merged.network.get_node_by_name("a").unwrap().meta().position.unwrap();
            assert_eq!(position.schematic, Some((14.0, 3.0)));
            assert_eq!(position.geographic, Some((8.0, 11.0)));
            let position = merged
                .network
                .get_virtual_node_by_name("v")
                .unwrap()
                .meta()
                .position
                .unwrap();
            assert_eq!(position.schematic, Some((11.0, 0.0)));
            if include_all {
                let position = merged.network.get_node_by_name("b").unwrap().meta().position.unwrap();
                assert_eq!(position.schematic, Some((4.0, 5.0)));
            } else {
                assert!(merged.network.get_node_by_name("b").is_none());
            }
        }
    }

    #[test]
    fn composed_components_retain_sources_and_metric_set_contributors() {
        use pywr_schema::meta::{ComponentMeta, ProvenanceSource};

        let root = tempdir().unwrap();
        let base = pywr_schema::ModelSchema {
            network: serde_json::from_value(serde_json::json!({
                "nodes": [{"type": "Placeholder", "meta": {"name": "replace"}},
                          {"type": "Input", "meta": {"name": "base"}}],
                "edges": [],
                "metric_sets": [{"meta": {"name": "shared"},
                                 "metrics": [{"type": "Node", "name": "base"}]}]
            }))
            .unwrap(),
            ..Default::default()
        };
        assert!(
            serde_json::to_value(&base).unwrap()["network"]["nodes"][0]["meta"]
                .get("provenance")
                .is_none()
        );
        std::fs::write(root.path().join("base.json"), serde_json::to_vec(&base).unwrap()).unwrap();
        std::fs::create_dir(root.path().join("nets")).unwrap();
        std::fs::write(
            root.path().join("nets/a.json"),
            serde_json::to_vec(&serde_json::json!({
                "nodes": [{"type": "Input", "meta": {"name": "replace"},
                           "parameters": [{"type": "Placeholder", "meta": {"name": "local"}}]}],
                "edges": [{"from_node": "replace", "to_node": "base"}],
                "metric_sets": [{"meta": {"name": "shared"},
                                 "metrics": [{"type": "Node", "name": "replace"}]}]
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.path().join("nets/b.json"),
            serde_json::to_vec(&serde_json::json!({
                "nodes": [{"type": "Output", "meta": {"name": "included"}}],
                "edges": [],
                "metric_sets": [{"meta": {"name": "shared"},
                                 "metrics": [{"type": "Node", "name": "included"}]}]
            }))
            .unwrap(),
        )
        .unwrap();

        let manifest = manifest(
            "base.json",
            vec![set("nets", None, None, None)],
            vec![selection("nets", None, true)],
        );
        let composed = manifest
            .compose_model(root.path(), "test", OPTIONS)
            .unwrap()
            .load()
            .unwrap()
            .into_model_schema(&Default::default())
            .unwrap();
        let base_source = ProvenanceSource {
            file: "base.json".into(),
            network_set: None,
            sub_network: None,
            project: None,
            network: None,
        };
        let a_source = ProvenanceSource {
            file: "nets/a.json".into(),
            network_set: Some("nets".into()),
            sub_network: None,
            project: None,
            network: None,
        };
        let b_source = ProvenanceSource {
            file: "nets/b.json".into(),
            network_set: Some("nets".into()),
            sub_network: None,
            project: None,
            network: None,
        };
        assert_eq!(
            composed
                .network
                .get_node_by_name("base")
                .unwrap()
                .meta()
                .provenance()
                .unwrap()
                .origin,
            base_source
        );
        let replaced = composed.network.get_node_by_name("replace").unwrap();
        assert_eq!(replaced.meta().provenance().unwrap().origin, a_source);
        assert_eq!(
            replaced.local_parameters().unwrap()[0]
                .meta()
                .provenance()
                .unwrap()
                .origin,
            a_source
        );
        assert_eq!(
            composed.network.edges[0].meta().unwrap().provenance().unwrap().origin,
            a_source
        );
        assert_eq!(
            composed
                .network
                .get_node_by_name("included")
                .unwrap()
                .meta()
                .provenance()
                .unwrap()
                .origin,
            b_source
        );
        let metric_set = &composed.network.metric_sets.as_ref().unwrap()[0];
        assert_eq!(metric_set.metrics.as_ref().unwrap().len(), 3);
        assert_eq!(metric_set.meta().provenance().unwrap().origin, base_source);
        assert_eq!(
            metric_set.meta().provenance().unwrap().contributors,
            vec![a_source, b_source]
        );
    }

    #[test]
    fn nested_network_set_provenance_round_trips_without_filesystem_separators() {
        let root = tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("groups/nets")).unwrap();
        std::fs::write(
            root.path().join("base.json"),
            serde_json::to_vec(&pywr_schema::ModelSchema::default()).unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.path().join("groups/nets/a.json"),
            r#"{"nodes":[{"type":"Input","meta":{"name":"a"}}],"edges":[]}"#,
        )
        .unwrap();
        let project = manifest(
            "base.json",
            vec![set("nets", Some("groups/nets"), None, None)],
            vec![selection("nets", None, true)],
        );
        let composed = project
            .compose_model(root.path(), "test", OPTIONS)
            .unwrap()
            .load()
            .unwrap()
            .into_model_schema(&Default::default())
            .unwrap();
        let json = serde_json::to_value(&composed).unwrap();
        assert_eq!(
            json["network"]["nodes"][0]["meta"]["provenance"]["origin"]["file"],
            "groups/nets/a.json"
        );
        let round_trip: pywr_schema::ModelSchema = serde_json::from_value(json).unwrap();
        assert_eq!(
            round_trip.network.nodes[0]
                .meta()
                .provenance()
                .unwrap()
                .origin
                .file
                .as_str(),
            "groups/nets/a.json"
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_network_filename_is_rejected_instead_of_recording_lossy_provenance() {
        use std::os::unix::ffi::OsStrExt;

        let root = tempdir().unwrap();
        touch(&root.path().join("base.json"));
        std::fs::create_dir(root.path().join("nets")).unwrap();
        let name = OsStr::from_bytes(b"bad\xff.json");
        touch(&root.path().join("nets").join(name));
        let project = manifest(
            "base.json",
            vec![set("nets", None, None, None)],
            vec![selection("nets", None, true)],
        );
        assert!(
            project
                .validate(root.path(), OPTIONS)
                .unwrap_err()
                .problems
                .iter()
                .any(|error| matches!(
                    error,
                    ProjectManifestProblem::Resolution(ManifestResolutionError::InvalidRelativePath { field, .. }) if field == "network set 'nets' file"
                ))
        );
        assert!(matches!(
            project.compose_model(root.path(), "test", OPTIONS),
            Err(ComposeModelError::Resolution(ManifestResolutionError::InvalidRelativePath { field, .. })) if field == "network set 'nets' file"
        ));
    }

    #[test]
    fn validation_reports_missing_files_and_the_resulting_minimum_violation() {
        let root = tempdir().unwrap();
        touch(&root.path().join("base.json"));
        std::fs::create_dir(root.path().join("nets")).unwrap();
        let manifest = manifest(
            "base.json",
            vec![set("nets", None, Some(1), None)],
            vec![selection("nets", Some(vec!["missing.json"]), false)],
        );
        let report = manifest.validate(root.path(), OPTIONS).unwrap_err();
        assert!(matches!(
            report.problems.as_slice(),
            [
                ProjectManifestProblem::FileNotFound { .. },
                ProjectManifestProblem::MinFilesNotMet { actual_files: 0, .. }
            ]
        ));
    }

    #[test]
    fn composition_reports_the_first_problem_found_by_validation() {
        let root = tempdir().unwrap();
        touch(&root.path().join("base.json"));
        for dir in ["a", "b"] {
            std::fs::create_dir(root.path().join(dir)).unwrap();
            touch(&root.path().join(dir).join("x.json"));
        }
        let manifest = manifest(
            "base.json",
            vec![set("a", None, None, None), set("b", None, None, None)],
            vec![
                selection("b", Some(vec!["missing.json"]), false),
                selection("a", None, true),
                selection("a", Some(vec!["x.json"]), false),
            ],
        );

        let report = manifest.validate_model(root.path(), "test", OPTIONS).unwrap_err();
        assert!(matches!(
            report.problems.as_slice(),
            [
                ProjectManifestProblem::FileNotFound { set, .. },
                ProjectManifestProblem::DuplicateSet { count: 2, .. }
            ] if set == "b"
        ));
        assert!(matches!(
            manifest.compose_model(root.path(), "test", OPTIONS),
            Err(ComposeModelError::FileNotFound { set, file }) if set == "b" && file == "missing.json"
        ));
    }

    fn network(name: &str, extends: Vec<&str>, include: Vec<DefinitionSelection>) -> NetworkDefinition {
        NetworkDefinition {
            name: name.to_string(),
            description: None,
            sub_networks: vec![],
            extends: extends.into_iter().map(str::to_string).collect(),
            include,
        }
    }

    fn uses_networks(mut model: ModelDefinition, networks: Vec<&str>) -> ModelDefinition {
        model.networks = networks.into_iter().map(str::to_string).collect();
        model
    }

    /// A project root with `base.json` and a `nets` set holding `a.json`, `b.json` and `c.json`,
    /// each defining one node named after the file.
    fn project_with_nets() -> tempfile::TempDir {
        let root = tempdir().unwrap();
        std::fs::write(
            root.path().join("base.json"),
            serde_json::to_vec(&pywr_schema::ModelSchema::default()).unwrap(),
        )
        .unwrap();
        std::fs::create_dir(root.path().join("nets")).unwrap();
        for name in ["a", "b", "c"] {
            std::fs::write(
                root.path().join(format!("nets/{name}.json")),
                format!(r#"{{"nodes":[{{"type":"Input","meta":{{"name":"{name}"}}}}],"edges":[]}}"#),
            )
            .unwrap();
        }
        root
    }

    fn net_paths(root: &Path, names: &[&str]) -> Vec<PathBuf> {
        names
            .iter()
            .map(|name| root.join(name).canonicalize().unwrap())
            .collect()
    }

    #[test]
    fn network_definitions_compose_in_extension_order_and_record_provenance() {
        let root = project_with_nets();
        let manifest = ProjectManifest {
            multi_models: vec![],
            dependencies: vec![],
            exports: None,
            base_model: Some("base.json".to_string()),
            sub_networks: vec![],
            network_sets: vec![set("nets", None, None, None)],
            networks: vec![
                network("baseline", vec![], vec![selection("nets", Some(vec!["a.json"]), false)]),
                network(
                    "upgraded",
                    vec!["baseline"],
                    vec![selection("nets", Some(vec!["b.json"]), false)],
                ),
            ],
            models: vec![uses_networks(
                model("test", vec![selection("nets", Some(vec!["c.json"]), false)]),
                vec!["upgraded"],
            )],
        };
        assert!(manifest.validate(root.path(), OPTIONS).is_ok());

        let composed = manifest.compose_model(root.path(), "test", OPTIONS).unwrap();
        assert_eq!(
            composed.all_paths(),
            net_paths(root.path(), &["base.json", "nets/a.json", "nets/b.json", "nets/c.json"])
        );

        let schema = composed.load().unwrap().into_model_schema(&Default::default()).unwrap();
        for (node, network) in [("a", Some("baseline")), ("b", Some("upgraded")), ("c", None)] {
            let origin = &schema
                .network
                .get_node_by_name(node)
                .unwrap()
                .meta()
                .provenance()
                .unwrap()
                .origin;
            assert_eq!(origin.network.as_deref(), network, "node {node}");
        }

        let network = manifest.compose_network(root.path(), "upgraded", OPTIONS).unwrap();
        assert_eq!(
            network.all_paths(),
            net_paths(root.path(), &["nets/a.json", "nets/b.json"])
        );
        let schema = network
            .load()
            .unwrap()
            .into_network_schema(&Default::default())
            .unwrap();
        assert_eq!(schema.nodes.len(), 2);
    }

    #[test]
    fn files_reached_by_several_routes_are_included_once() {
        let root = project_with_nets();
        let manifest = ProjectManifest {
            multi_models: vec![],
            dependencies: vec![],
            exports: None,
            base_model: Some("base.json".to_string()),
            sub_networks: vec![],
            network_sets: vec![set("nets", None, None, None)],
            networks: vec![
                network("left", vec![], vec![selection("nets", Some(vec!["a.json"]), false)]),
                network(
                    "right",
                    vec![],
                    vec![selection("nets", Some(vec!["a.json", "b.json"]), false)],
                ),
                network("diamond", vec!["left", "right"], vec![]),
            ],
            models: vec![uses_networks(model("test", vec![]), vec!["left", "right"])],
        };
        assert!(manifest.validate(root.path(), OPTIONS).is_ok());
        assert_eq!(
            manifest
                .compose_model(root.path(), "test", OPTIONS)
                .unwrap()
                .all_paths(),
            net_paths(root.path(), &["base.json", "nets/a.json", "nets/b.json"])
        );
        assert_eq!(
            manifest
                .compose_network(root.path(), "diamond", OPTIONS)
                .unwrap()
                .all_paths(),
            net_paths(root.path(), &["nets/a.json", "nets/b.json"])
        );
    }

    #[test]
    fn each_network_file_belongs_to_one_set_and_one_sub_network() {
        let root = project_with_sub_networks();
        write_json(&root.path().join("nets/a.json"), input_network("a"));
        let manifest = ProjectManifest {
            sub_networks: vec![
                sub_network("upper", Some("options/x.json"), vec![set("options", None, None, None)]),
                sub_network("lower", Some("base.json"), vec![]),
                SubNetwork {
                    dir: Some("upper/options".to_string()),
                    ..sub_network("nested", None, vec![])
                },
            ],
            network_sets: vec![
                set("copy", Some("nets"), None, None),
                set("again", Some("nets"), None, None),
                set("inside", Some("upper"), None, None),
                set("shared", Some("upper/options"), None, None),
            ],
            ..sub_network_manifest(vec![], vec![])
        };
        let report = manifest.validate(root.path(), OPTIONS).unwrap_err();
        let messages: Vec<_> = report.problems.iter().map(ToString::to_string).collect();
        let nets = root.path().join("nets").canonicalize().unwrap();
        let options = root.path().join("upper/options").canonicalize().unwrap();
        assert_eq!(
            messages,
            [
                format!(
                    "The network sets `copy` and `again` both use the directory `{}`, but each set needs a directory of its own.",
                    nets.display()
                ),
                format!(
                    "The network sets `shared` and `upper::options` both use the directory `{}`, but each set needs a directory of its own.",
                    options.display()
                ),
                "The network set `inside` is in the directory of the sub-network `upper`; make it one of the sub-network's network sets instead.".to_string(),
                "The base network of the sub-network `upper` is in the directory of the network set `shared`, but it must not also be a file of a network set.".to_string(),
                "The directories of the sub-networks `upper` and `nested` overlap, but each sub-network needs a directory of its own.".to_string(),
            ]
        );
    }

    #[test]
    fn a_file_selected_with_different_offsets_is_reported() {
        let root = project_with_nets();
        let manifest = nets_manifest(
            vec![
                network(
                    "moved",
                    vec![],
                    vec![with_offset(selection("nets", Some(vec!["a.json"]), false), "a.json")],
                ),
                network("plain", vec![], vec![selection("nets", Some(vec!["a.json"]), false)]),
            ],
            vec![uses_networks(model("test", vec![]), vec!["moved", "plain"])],
        );
        let report = manifest.validate(root.path(), OPTIONS).unwrap_err();
        let messages: Vec<_> = report.problems.iter().map(ToString::to_string).collect();
        assert_eq!(
            messages,
            [
                "The model definition `test` selects the file `nets/a.json` from network set `nets` more than once, with different position offsets."
            ]
        );
    }

    #[test]
    fn a_file_reached_through_two_projects_sets_is_reported() {
        let root = project_with_nets();
        // A second manifest in the same directory, whose set is the same directory as `nets`.
        write_json(
            &root.path().join("twin.json"),
            serde_json::json!({"format": "v1", "network_sets": [{"name": "nets"}]}),
        );
        let mut manifest = nets_manifest(
            vec![],
            vec![model(
                "test",
                vec![
                    selection("nets", Some(vec!["a.json"]), false),
                    selection("twin::nets", Some(vec!["a.json"]), false),
                ],
            )],
        );
        manifest.dependencies = vec![dependency("twin", "twin.json")];
        let report = manifest.validate(root.path(), OPTIONS).unwrap_err();
        let messages: Vec<_> = report.problems.iter().map(ToString::to_string).collect();
        let file = root.path().join("nets/a.json").canonicalize().unwrap();
        assert_eq!(
            messages,
            [format!(
                "The model definition `test` reaches the file `{}` through both network set `nets` and network set `twin::nets`, but a file may be reached through only one.",
                file.display()
            )]
        );
    }

    #[test]
    fn set_constraints_apply_to_the_whole_model_not_each_network() {
        let root = project_with_nets();
        let manifest = ProjectManifest {
            multi_models: vec![],
            dependencies: vec![],
            exports: None,
            base_model: Some("base.json".to_string()),
            sub_networks: vec![],
            network_sets: vec![set("nets", None, None, Some(1))],
            networks: vec![
                network("left", vec![], vec![selection("nets", Some(vec!["a.json"]), false)]),
                network("right", vec![], vec![selection("nets", Some(vec!["b.json"]), false)]),
            ],
            models: vec![uses_networks(model("test", vec![]), vec!["left", "right"])],
        };
        assert!(manifest.validate_network(root.path(), "left", OPTIONS).is_ok());
        assert!(matches!(
            manifest.validate_model(root.path(), "test", OPTIONS).unwrap_err().problems.as_slice(),
            [ProjectManifestProblem::MaxFilesExceeded { definition, actual_files: 2, .. }] if definition.name == "test"
        ));
        assert!(matches!(
            manifest.compose_model(root.path(), "test", OPTIONS),
            Err(ComposeModelError::MaxFilesExceeded { actual_files: 2, .. })
        ));
    }

    #[test]
    fn network_cycles_are_reported_once() {
        let root = project_with_nets();
        let manifest = ProjectManifest {
            multi_models: vec![],
            dependencies: vec![],
            exports: None,
            base_model: Some("base.json".to_string()),
            sub_networks: vec![],
            network_sets: vec![set("nets", None, None, None)],
            networks: vec![
                network("first", vec!["second"], vec![]),
                network("second", vec!["first"], vec![]),
            ],
            models: vec![uses_networks(model("test", vec![]), vec!["first"])],
        };
        let report = manifest.validate(root.path(), OPTIONS).unwrap_err();
        assert!(matches!(
            report.problems.as_slice(),
            [ProjectManifestProblem::NetworkCycle { cycle }] if cycle == &["first", "second", "first"]
        ));
        assert_eq!(
            report.problems[0].to_string(),
            "The network definitions extend each other in a cycle: first -> second -> first."
        );
        assert!(matches!(
            manifest.compose_network(root.path(), "second", OPTIONS),
            Err(ComposeModelError::NetworkCycle { cycle }) if cycle == ["second", "first", "second"]
        ));
    }

    #[test]
    fn missing_network_references_are_reported_where_they_are_made() {
        let root = project_with_nets();
        let manifest = ProjectManifest {
            multi_models: vec![],
            dependencies: vec![],
            exports: None,
            base_model: Some("base.json".to_string()),
            sub_networks: vec![],
            network_sets: vec![set("nets", None, None, None)],
            networks: vec![network("child", vec!["gone"], vec![])],
            models: vec![uses_networks(model("test", vec![]), vec!["nope"])],
        };
        let report = manifest.validate(root.path(), OPTIONS).unwrap_err();
        let missing: Vec<_> = report
            .problems
            .iter()
            .map(|problem| match problem {
                ProjectManifestProblem::NetworkNotFound { definition, network } => {
                    (definition.name.as_str(), network.as_str())
                }
                other => panic!("unexpected problem: {other}"),
            })
            .collect();
        assert_eq!(missing, [("test", "nope"), ("child", "gone")]);
        assert!(matches!(
            manifest.compose_network(root.path(), "unknown", OPTIONS),
            Err(ComposeModelError::NetworkDefinitionNotFound { network }) if network == "unknown"
        ));
    }

    /// A manifest over [`project_with_nets`] with the given networks and models.
    fn nets_manifest(networks: Vec<NetworkDefinition>, models: Vec<ModelDefinition>) -> ProjectManifest {
        ProjectManifest {
            multi_models: vec![],
            dependencies: vec![],
            exports: None,
            base_model: Some("base.json".to_string()),
            sub_networks: vec![],
            network_sets: vec![set("nets", None, None, None)],
            networks,
            models,
        }
    }

    #[test]
    fn problems_name_the_kind_of_definition_they_are_in() {
        let root = project_with_nets();
        let missing = || vec![selection("nets", Some(vec!["missing.json"]), false)];
        let manifest = nets_manifest(vec![network("x", vec![], missing())], vec![model("x", missing())]);
        let report = manifest.validate(root.path(), OPTIONS).unwrap_err();
        let messages: Vec<_> = report.problems.iter().map(ToString::to_string).collect();
        assert_eq!(
            messages,
            [
                "The model definition `x` selects the file `missing.json`, which was not found in network set `nets`.",
                "The network definition `x` selects the file `missing.json`, which was not found in network set `nets`.",
            ]
        );
    }

    #[test]
    fn a_broken_base_model_is_reported_for_each_model_that_gives_it() {
        let root = project_with_nets();
        let with_base = |name: &str| ModelDefinition {
            base_model: Some("missing.json".to_string()),
            ..model(name, vec![])
        };
        let manifest = nets_manifest(vec![], vec![with_base("first"), with_base("second")]);
        let report = manifest.validate(root.path(), OPTIONS).unwrap_err();
        let models: Vec<_> = report
            .problems
            .iter()
            .map(|problem| match problem {
                ProjectManifestProblem::InvalidBaseModel {
                    definition,
                    error: ManifestResolutionError::BaseModelNotFound { .. },
                } => definition.name.as_str(),
                other => panic!("unexpected problem: {other}"),
            })
            .collect();
        assert_eq!(models, ["first", "second"]);
        assert!(
            report.problems[0]
                .to_string()
                .starts_with("The model definition `first` cannot use its base model: ")
        );
    }

    #[test]
    fn repeated_network_references_are_reported_and_followed_once() {
        let root = project_with_nets();
        let manifest = nets_manifest(
            vec![
                network("a", vec![], vec![selection("nets", Some(vec!["a.json"]), false)]),
                network("twice", vec!["a", "a"], vec![]),
            ],
            vec![uses_networks(model("test", vec![]), vec!["a", "twice", "a"])],
        );
        let report = manifest.validate(root.path(), OPTIONS).unwrap_err();
        let repeated: Vec<_> = report
            .problems
            .iter()
            .map(|problem| match problem {
                ProjectManifestProblem::DuplicateNetworkReference {
                    definition,
                    network,
                    count: 2,
                } => (definition.to_string(), network.as_str()),
                other => panic!("unexpected problem: {other}"),
            })
            .collect();
        assert_eq!(
            repeated,
            [
                ("model definition `test`".to_string(), "a"),
                ("network definition `twice`".to_string(), "a"),
            ]
        );
    }

    #[test]
    fn layout_reports_the_problems_of_every_dependency() {
        let workspace = workspace();
        let root = workspace.path().join("main");
        let mut manifest = main_manifest(vec![]);
        manifest.dependencies = vec![dependency("absent", "../absent/pywr-project.json")];
        let layout = manifest.layout(&root, OPTIONS);
        let problems = layout.problems.expect("the missing dependency was not reported");
        assert!(
            problems.report().to_string().contains("absent"),
            "{}",
            problems.report()
        );
    }

    #[test]
    fn layout_lists_what_resolved_alongside_the_problems() {
        let root = project_with_nets();
        let manifest = ProjectManifest {
            network_sets: vec![set("nets", None, None, None), set("missing", None, None, None)],
            ..manifest(
                "base.json",
                vec![],
                vec![selection("nets", Some(vec!["a.json"]), false)],
            )
        };
        let layout = manifest.layout(root.path(), OPTIONS);

        let sets: Vec<_> = layout.network_sets.iter().map(|set| set.name.as_str()).collect();
        assert_eq!(sets, ["nets"]);
        assert_eq!(layout.projects[0].manifest, None);
        assert_eq!(
            layout.projects[0].base_model,
            Some(root.path().canonicalize().unwrap().join("base.json"))
        );
        let Some(crate::ProjectManifestValidationError::V1(error)) = &layout.problems else {
            panic!("expected problems");
        };
        assert_eq!(
            error.to_string(),
            manifest.validate(root.path(), OPTIONS).unwrap_err().to_string()
        );
        assert!(matches!(
            error.problems.as_slice(),
            [ProjectManifestProblem::Resolution(ManifestResolutionError::DirectoryNotFound { set, .. })]
                if set == "missing"
        ));
    }

    #[test]
    fn a_layout_watches_where_missing_items_are_expected() {
        let root = project_with_nets();
        let manifest = ProjectManifest {
            network_sets: vec![set("nets", None, None, None), set("missing/deeper", None, None, None)],
            ..manifest("absent.json", vec![], vec![])
        };
        let layout = manifest.layout(root.path(), OPTIONS);
        let canonical = root.path().canonicalize().unwrap();

        assert_eq!(
            layout.unresolved_paths,
            [
                std::path::absolute(root.path().join("absent.json")).unwrap(),
                std::path::absolute(root.path().join("missing").join("deeper")).unwrap(),
            ]
        );
        // The project directory is the nearest existing parent of both, so creating either is
        // noticed.
        assert_eq!(
            layout.watch_paths(),
            [
                canonical.join("nets"),
                canonical.join("nets").join("a.json"),
                canonical.join("nets").join("b.json"),
                canonical.join("nets").join("c.json"),
                canonical.clone(),
            ]
        );
    }

    #[test]
    fn a_layouts_root_is_canonical_even_when_given_relative() {
        let root = project_with_nets();
        let manifest = manifest("base.json", vec![set("nets", None, None, None)], vec![]);
        let parent = root.path().parent().unwrap();
        let relative = parent
            .join(".")
            .join(root.path().file_name().unwrap())
            .join("nets")
            .join("..");
        let layout = manifest.layout(&relative, OPTIONS);
        assert_eq!(layout.projects[0].root, root.path().canonicalize().unwrap());
    }

    #[test]
    fn a_layout_gives_the_data_directory_of_each_set_and_sub_network() {
        let root = project_with_sub_networks();
        std::fs::create_dir(root.path().join("upper/data")).unwrap();
        write_json(&root.path().join("nets/a.json"), input_network("a"));
        let mut manifest = sub_network_manifest(vec![], vec![]);
        manifest.sub_networks[0].data_dir = Some("data".to_string());
        manifest.network_sets = vec![set("nets", None, None, None)];
        let layout = manifest.layout(root.path(), OPTIONS);
        let canonical = root.path().canonicalize().unwrap();

        let sets: Vec<_> = layout
            .network_sets
            .iter()
            .map(|set| (set.name.as_str(), set.data_dir.clone()))
            .collect();
        assert_eq!(
            sets,
            [
                ("nets", canonical.clone()),
                ("upper::options", canonical.join("upper").join("data")),
            ]
        );
        let sub_networks: Vec<_> = layout
            .sub_networks
            .iter()
            .map(|sub_network| (sub_network.name.as_str(), sub_network.data_dir.clone()))
            .collect();
        assert_eq!(
            sub_networks,
            [
                ("upper", canonical.join("upper").join("data")),
                ("lower", canonical.join("lower")),
            ]
        );
    }

    #[test]
    fn a_layout_does_not_list_a_projects_base_model_as_a_definition_file() {
        let root = project_with_nets();
        let mut manifest = multi_model_manifest(vec![]);
        manifest.multi_models[0].domain = MultiModelDomain::BaseModel(BaseModelDomain {
            path: "base.json".to_string(),
        });
        let layout = manifest.layout(root.path(), OPTIONS);
        assert_eq!(layout.definition_files, Vec::<PathBuf>::new());
    }

    #[test]
    fn network_problems_are_reported_once_however_many_models_use_them() {
        let root = project_with_nets();
        let manifest = ProjectManifest {
            multi_models: vec![],
            dependencies: vec![],
            exports: None,
            base_model: Some("base.json".to_string()),
            sub_networks: vec![],
            network_sets: vec![set("nets", None, None, None)],
            networks: vec![network(
                "shared",
                vec![],
                vec![selection("nets", Some(vec!["missing.json"]), false)],
            )],
            models: vec![
                uses_networks(model("first", vec![]), vec!["shared"]),
                uses_networks(model("second", vec![]), vec!["shared"]),
            ],
        };
        assert!(matches!(
            manifest.validate(root.path(), OPTIONS).unwrap_err().problems.as_slice(),
            [ProjectManifestProblem::FileNotFound { definition, .. }] if definition.name == "shared"
        ));
    }

    #[test]
    fn base_model_comes_from_the_definition_or_the_project() {
        let root = project_with_nets();
        std::fs::copy(root.path().join("base.json"), root.path().join("other.json")).unwrap();
        let mut with_base = model("with-base", vec![]);
        with_base.base_model = Some("other.json".to_string());
        let manifest = ProjectManifest {
            multi_models: vec![],
            dependencies: vec![],
            exports: None,
            base_model: None,
            sub_networks: vec![],
            network_sets: vec![set("nets", None, None, None)],
            networks: vec![network("all", vec![], vec![selection("nets", None, true)])],
            models: vec![with_base, model("without-base", vec![])],
        };

        assert!(matches!(
            manifest.validate(root.path(), OPTIONS).unwrap_err().problems.as_slice(),
            [ProjectManifestProblem::BaseModelMissing { definition }] if definition.name == "without-base"
        ));
        assert_eq!(
            manifest
                .compose_model(root.path(), "with-base", OPTIONS)
                .unwrap()
                .all_paths(),
            net_paths(root.path(), &["other.json"])
        );
        assert!(matches!(
            manifest.compose_model(root.path(), "without-base", OPTIONS),
            Err(ComposeModelError::BaseModelMissing { definition }) if definition.name == "without-base"
        ));
        assert_eq!(
            manifest
                .compose_network(root.path(), "all", OPTIONS)
                .unwrap()
                .all_paths()
                .len(),
            3
        );
    }

    #[test]
    fn manifest_json_uses_models_and_networks() {
        let manifest: super::super::ProjectManifest = serde_json::from_str(
            r#"{
                "format": "v1",
                "network_sets": [{ "name": "nets" }],
                "networks": [{ "name": "upgraded", "extends": ["baseline"], "include": [] }],
                "models": [{ "name": "full", "base_model": "base.json", "networks": ["upgraded"] }]
            }"#,
        )
        .unwrap();
        let super::super::ProjectManifest::V1(manifest) = manifest;
        assert!(manifest.base_model.is_none());
        assert_eq!(manifest.networks[0].extends, ["baseline"]);
        assert_eq!(manifest.models[0].networks, ["upgraded"]);

        assert!(
            serde_json::from_str::<super::super::ProjectManifest>(
                r#"{ "format": "v1", "base_model": "base.json", "definitions": [] }"#
            )
            .is_err()
        );
    }

    fn write_json(path: &Path, value: serde_json::Value) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    }

    fn input_network(name: &str) -> serde_json::Value {
        serde_json::json!({"nodes": [{"type": "Input", "meta": {"name": name}}], "edges": []})
    }

    fn sub_network(name: &str, base_network: Option<&str>, network_sets: Vec<NetworkSet>) -> SubNetwork {
        SubNetwork {
            name: name.to_string(),
            dir: None,
            base_network: base_network.map(str::to_string),
            data_dir: None,
            network_sets,
        }
    }

    /// A project root with `base.json` and two sub-networks, `upper` and `lower`, each with a base
    /// network. `upper` also has an `options` set holding `x.json`.
    fn project_with_sub_networks() -> tempfile::TempDir {
        let root = tempdir().unwrap();
        write_json(
            &root.path().join("base.json"),
            serde_json::to_value(pywr_schema::ModelSchema::default()).unwrap(),
        );
        write_json(&root.path().join("upper/base.json"), input_network("upper"));
        write_json(&root.path().join("upper/options/x.json"), input_network("x"));
        write_json(&root.path().join("lower/base.json"), input_network("lower"));
        root
    }

    fn sub_network_manifest(models: Vec<ModelDefinition>, networks: Vec<NetworkDefinition>) -> ProjectManifest {
        ProjectManifest {
            multi_models: vec![],
            dependencies: vec![],
            exports: None,
            base_model: Some("base.json".to_string()),
            sub_networks: vec![
                sub_network("upper", Some("base.json"), vec![set("options", None, None, None)]),
                sub_network("lower", Some("base.json"), vec![]),
            ],
            network_sets: vec![],
            networks,
            models,
        }
    }

    fn with_sub_networks(mut model: ModelDefinition, sub_networks: Vec<&str>) -> ModelDefinition {
        model.sub_networks = sub_networks.into_iter().map(str::to_string).collect();
        model
    }

    #[test]
    fn active_sub_network_bases_follow_the_base_model_in_declaration_order() {
        let root = project_with_sub_networks();
        let manifest = sub_network_manifest(
            vec![with_sub_networks(
                model("test", vec![selection("upper::options", Some(vec!["x.json"]), false)]),
                vec!["lower", "upper"],
            )],
            vec![],
        );
        assert!(manifest.validate(root.path(), OPTIONS).is_ok());

        let composed = manifest.compose_model(root.path(), "test", OPTIONS).unwrap();
        assert_eq!(
            composed.all_paths(),
            net_paths(
                root.path(),
                &[
                    "base.json",
                    "upper/base.json",
                    "lower/base.json",
                    "upper/options/x.json"
                ]
            )
        );

        let schema = composed.load().unwrap().into_model_schema(&Default::default()).unwrap();
        let origin = |node: &str| {
            schema
                .network
                .get_node_by_name(node)
                .unwrap()
                .meta()
                .provenance()
                .unwrap()
                .origin
                .clone()
        };
        assert_eq!(
            origin("x"),
            ProvenanceSource {
                file: "upper/options/x.json".into(),
                network_set: Some("options".into()),
                sub_network: Some("upper".into()),
                project: None,
                network: None,
            }
        );
        assert_eq!(
            origin("lower"),
            ProvenanceSource {
                file: "lower/base.json".into(),
                network_set: None,
                sub_network: Some("lower".into()),
                project: None,
                network: None,
            }
        );
    }

    #[test]
    fn sub_networks_are_inactive_unless_listed() {
        let root = project_with_sub_networks();
        let upper_x = || selection("upper::options", Some(vec!["x.json"]), false);
        let manifest = sub_network_manifest(
            vec![
                model("inactive", vec![upper_x()]),
                uses_networks(model("via-base", vec![]), vec!["with-base"]),
            ],
            vec![
                network("upper-x", vec![], vec![upper_x()]),
                NetworkDefinition {
                    sub_networks: vec!["upper".to_string()],
                    ..network("with-base", vec![], vec![upper_x()])
                },
            ],
        );

        assert!(matches!(
            manifest.validate(root.path(), OPTIONS).unwrap_err().problems.as_slice(),
            [ProjectManifestProblem::SubNetworkNotActive { definition, set, sub_network }]
                if definition.name == "inactive" && set == "upper::options" && sub_network == "upper"
        ));
        assert!(matches!(
            manifest.compose_model(root.path(), "inactive", OPTIONS),
            Err(ComposeModelError::SubNetworkNotActive { .. })
        ));
        assert_eq!(
            manifest
                .compose_model(root.path(), "via-base", OPTIONS)
                .unwrap()
                .all_paths(),
            net_paths(root.path(), &["base.json", "upper/base.json", "upper/options/x.json"])
        );
        assert!(manifest.validate_network(root.path(), "with-base", OPTIONS).is_ok());
        assert!(matches!(
            manifest.compose_network(root.path(), "upper-x", OPTIONS),
            Err(ComposeModelError::SubNetworkNotActive { .. })
        ));
    }

    #[test]
    fn sub_network_set_constraints_apply_only_when_active() {
        let root = project_with_sub_networks();
        let mut manifest = sub_network_manifest(
            vec![
                model("without-upper", vec![]),
                with_sub_networks(model("with-upper", vec![]), vec!["upper"]),
            ],
            vec![],
        );
        manifest.sub_networks[0].network_sets[0].min_files = Some(1);

        assert!(manifest.validate_model(root.path(), "without-upper", OPTIONS).is_ok());
        assert!(matches!(
            manifest.validate_model(root.path(), "with-upper", OPTIONS).unwrap_err().problems.as_slice(),
            [ProjectManifestProblem::MinFilesNotMet { set, actual_files: 0, .. }] if set == "upper::options"
        ));
    }

    #[test]
    fn sub_network_data_paths_are_rebased_but_output_paths_are_not() {
        let root = project_with_sub_networks();
        write_json(
            &root.path().join("upper/base.json"),
            serde_json::json!({
                "nodes": [{"type": "Input", "meta": {"name": "upper"}}],
                "edges": [],
                "time_series": [{"type": "Polars", "meta": {"name": "flows"}, "path": "flows.csv"}],
                "outputs": [{"type": "HDF5", "meta": {"name": "out"}, "filename": "out.h5", "metric_set": "all"}]
            }),
        );
        let mut manifest = sub_network_manifest(vec![], vec![]);
        manifest.sub_networks[0].data_dir = Some("data".to_string());
        manifest.networks = vec![NetworkDefinition {
            sub_networks: vec!["upper".to_string()],
            ..network("upper", vec![], vec![])
        }];
        let report = manifest.validate(root.path(), OPTIONS).unwrap_err();
        let messages: Vec<_> = report.problems.iter().map(ToString::to_string).collect();
        assert_eq!(
            messages,
            [format!(
                "The data directory `{}` for the sub-network `upper` was not found, or is not a directory.",
                root.path().join("upper").join("data").display()
            )]
        );
        std::fs::create_dir(root.path().join("upper/data")).unwrap();

        let schema = manifest
            .compose_network(root.path(), "upper", OPTIONS)
            .unwrap()
            .load()
            .unwrap()
            .into_network_schema(&Default::default())
            .unwrap();
        let time_series = &schema.time_series.as_ref().unwrap()[0];
        let pywr_schema::time_series::TimeSeries::Polars(time_series) = time_series else {
            panic!("expected a Polars time series");
        };
        assert_eq!(time_series.path, Path::new("upper/data/flows.csv"));
        let pywr_schema::outputs::Output::HDF5(output) = &schema.outputs.as_ref().unwrap()[0] else {
            panic!("expected an HDF5 output");
        };
        assert_eq!(output.filename, Path::new("out.h5"));
    }

    #[test]
    fn unresolved_placeholders_fail_model_composition() {
        let root = project_with_nets();
        let mut base = serde_json::to_value(pywr_schema::ModelSchema::default()).unwrap();
        base["network"]["nodes"] = serde_json::json!([{"type": "Placeholder", "meta": {"name": "missing"}}]);
        write_json(&root.path().join("base.json"), base);
        let manifest = manifest("base.json", vec![set("nets", None, None, None)], vec![]);
        let Err(error) = manifest
            .compose_model(root.path(), "test", OPTIONS)
            .unwrap()
            .load()
            .unwrap()
            .into_model_schema(&Default::default())
        else {
            panic!("expected unresolved placeholders");
        };
        assert_eq!(
            error.to_string(),
            "The composed model has unresolved placeholders: node `missing` (from `base.json`)."
        );
        let crate::ComposeToSchemaError::UnresolvedPlaceholders { placeholders } = error else {
            panic!("expected unresolved placeholders");
        };
        let [placeholder] = placeholders.as_slice() else {
            panic!("expected one placeholder");
        };
        assert_eq!(placeholder.kind, crate::PlaceholderKind::Node);
        assert_eq!(placeholder.name, "missing");
        assert_eq!(placeholder.origin.as_ref().unwrap().file.as_str(), "base.json");
        assert_eq!(placeholder.network, None);
    }

    #[test]
    fn missing_sub_network_is_reported_once_with_its_sets() {
        let root = project_with_sub_networks();
        let mut manifest = sub_network_manifest(
            vec![with_sub_networks(
                model("test", vec![selection("gone::options", None, true)]),
                vec!["gone", "nowhere"],
            )],
            vec![],
        );
        manifest
            .sub_networks
            .push(sub_network("gone", None, vec![set("options", None, None, None)]));

        let report = manifest.validate(root.path(), OPTIONS).unwrap_err();
        assert!(matches!(
            report.problems.as_slice(),
            [
                ProjectManifestProblem::Resolution(ManifestResolutionError::SubNetworkDirectoryNotFound { sub_network, .. }),
                ProjectManifestProblem::SubNetworkNotFound { sub_network: missing, .. },
            ] if sub_network == "gone" && missing == "nowhere"
        ));
    }

    #[test]
    fn names_must_not_contain_the_separator() {
        let root = project_with_sub_networks();
        write_json(&root.path().join("nets/a.json"), input_network("a"));
        let mut manifest = sub_network_manifest(vec![], vec![]);
        manifest.network_sets.push(set("a::b", Some("nets"), None, None));
        assert!(matches!(
            manifest.validate(root.path(), OPTIONS).unwrap_err().problems.as_slice(),
            [ProjectManifestProblem::InvalidName { kind: "network set", name }] if name == "a::b"
        ));
    }

    fn transfer(from_network: &str, node: &str, name: &str) -> MultiNetworkTransfer {
        serde_json::from_value(serde_json::json!({
            "from_network": from_network,
            "metric": {"type": "Node", "name": node},
            "name": name,
        }))
        .unwrap()
    }

    fn multi_network(name: &str, network: Option<MultiModelNetworkSource>) -> MultiModelNetwork {
        MultiModelNetwork {
            name: name.to_string(),
            network,
            sub_networks: vec![],
            include: vec![],
            transfers: vec![],
            transfers_file: None,
        }
    }

    fn from_model(model: &str) -> Option<MultiModelNetworkSource> {
        Some(MultiModelNetworkSource::Model(ModelSource {
            name: model.to_string(),
        }))
    }

    fn from_network(network: &str) -> Option<MultiModelNetworkSource> {
        Some(MultiModelNetworkSource::Network(NetworkSource {
            name: network.to_string(),
        }))
    }

    /// A manifest over `project_with_nets` with a model `company` (base model plus `a.json`), a
    /// network definition `river` (`b.json`) and a multi-model `joint` that links them.
    fn multi_model_manifest(networks: Vec<MultiModelNetwork>) -> ProjectManifest {
        ProjectManifest {
            multi_models: vec![MultiModelDefinition {
                name: "joint".to_string(),
                domain: MultiModelDomain::Model(ModelDomain {
                    name: "company".to_string(),
                }),
                overrides: None,
                networks,
            }],
            base_model: Some("base.json".to_string()),
            sub_networks: vec![],
            network_sets: vec![set("nets", None, None, None)],
            networks: vec![network(
                "river",
                vec![],
                vec![selection("nets", Some(vec!["b.json"]), false)],
            )],
            models: vec![model("company", vec![selection("nets", Some(vec!["a.json"]), false)])],
            dependencies: vec![],
            exports: None,
        }
    }

    fn joint_networks() -> Vec<MultiModelNetwork> {
        let mut river = multi_network("river", from_network("river"));
        river.include = vec![selection("nets", Some(vec!["c.json"]), false)];
        river.transfers = vec![transfer("company", "a", "company-a")];
        vec![multi_network("company", from_model("company")), river]
    }

    #[test]
    fn a_multi_model_network_can_activate_sub_networks_itself() {
        let root = project_with_sub_networks();
        let upper = MultiModelNetwork {
            include: vec![selection("upper::options", Some(vec!["x.json"]), false)],
            ..multi_network("upper", None)
        };
        let mut manifest = sub_network_manifest(vec![model("m", vec![])], vec![]);
        manifest.multi_models = vec![MultiModelDefinition {
            name: "joint".to_string(),
            domain: MultiModelDomain::Model(ModelDomain { name: "m".to_string() }),
            overrides: None,
            networks: vec![upper.clone(), multi_network("plain", from_model("m"))],
        }];
        let report = manifest.validate(root.path(), OPTIONS).unwrap_err();
        let messages: Vec<_> = report.problems.iter().map(ToString::to_string).collect();
        assert_eq!(
            messages,
            [
                "The multi-model network `joint/upper` selects the network set `upper::options`, but its sub-network `upper` is not active; list the sub-network in `sub_networks`."
            ]
        );

        manifest.multi_models[0].networks[0].sub_networks = vec!["upper".to_string()];
        let composed = manifest.compose_multi_model(root.path(), "joint", OPTIONS).unwrap();
        let paths: Vec<_> = composed.networks()[0]
            .network
            .includes()
            .iter()
            .map(|include| include.source.file.to_string())
            .collect();
        assert_eq!(paths, ["upper/base.json", "upper/options/x.json"]);
    }

    #[test]
    fn multi_model_networks_come_from_models_and_network_definitions() {
        let root = project_with_nets();
        let manifest = multi_model_manifest(joint_networks());
        assert!(manifest.validate(root.path(), OPTIONS).is_ok());

        let composed = manifest.compose_multi_model(root.path(), "joint", OPTIONS).unwrap();
        let paths: Vec<_> = composed
            .networks()
            .iter()
            .map(|network| network.network.all_paths())
            .collect();
        assert_eq!(
            paths,
            [
                net_paths(root.path(), &["base.json", "nets/a.json"]),
                net_paths(root.path(), &["nets/b.json", "nets/c.json"]),
            ]
        );

        let schema = composed
            .load()
            .unwrap()
            .into_multi_model_schema(&Default::default())
            .unwrap();
        assert_eq!(schema.metadata.title, "joint");
        let names: Vec<_> = schema.networks.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["company", "river"]);
        let pywr_schema::NetworkSchemaRef::Inline(river) = &schema.networks[1].network else {
            panic!("expected an inline network");
        };
        assert_eq!(river.nodes.len(), 2);
        assert_eq!(schema.networks[1].transfers[0].name, "company-a");

        assert!(matches!(
            manifest.compose(root.path(), "joint", OPTIONS),
            Ok(crate::Composed::MultiModel(_))
        ));
        assert!(matches!(
            manifest.compose(root.path(), "company", OPTIONS),
            Ok(crate::Composed::Model(_))
        ));
    }

    #[test]
    fn multi_model_domain_overrides_apply_in_order() {
        let root = project_with_nets();
        let time = |end: &str| -> pywr_schema::model::TimeDomain {
            serde_json::from_value(serde_json::json!({
                "start": "2020-01-01T00:00:00", "end": end, "timestep": {"type": "Days", "days": 1}
            }))
            .unwrap()
        };
        let overrides = |end: &str| DefinitionOverrides {
            time: Some(time(end)),
            scenarios: None,
        };
        let mut manifest = multi_model_manifest(joint_networks());
        manifest.models[0].overrides = Some(overrides("2020-02-01T00:00:00"));
        let domain_end = |manifest: &ProjectManifest| {
            let schema = manifest
                .compose_multi_model(root.path(), "joint", OPTIONS)
                .unwrap()
                .load()
                .unwrap()
                .into_multi_model_schema(&Default::default())
                .unwrap();
            serde_json::to_value(schema.time).unwrap()["end"].clone()
        };
        assert_eq!(domain_end(&manifest), "2020-02-01T00:00:00");

        manifest.multi_models[0].overrides = Some(overrides("2020-03-01T00:00:00"));
        assert_eq!(domain_end(&manifest), "2020-03-01T00:00:00");

        manifest.multi_models[0].domain = MultiModelDomain::Inline(InlineDomain {
            time: time("2020-04-01T00:00:00"),
            scenarios: None,
        });
        manifest.multi_models[0].overrides = None;
        assert_eq!(domain_end(&manifest), "2020-04-01T00:00:00");
    }

    #[test]
    fn transfers_must_come_from_another_network_and_have_unique_names() {
        let root = project_with_nets();
        let mut networks = joint_networks();
        networks[1].transfers = vec![
            transfer("river", "b", "own"),
            transfer("nowhere", "a", "lost"),
            transfer("company", "a", "twice"),
            transfer("company", "a", "twice"),
        ];
        let manifest = multi_model_manifest(networks);
        let report = manifest.validate(root.path(), OPTIONS).unwrap_err();
        let messages: Vec<_> = report.problems.iter().map(ToString::to_string).collect();
        assert_eq!(
            messages,
            [
                "The transfer `own` into `joint/river` is from the same network.",
                "The transfer `lost` into `joint/river` is from the network `nowhere`, which does not exist.",
                "The name `twice` is used by 2 transfers into `joint/river`, but each name must be unique.",
            ]
        );
    }

    #[test]
    fn transfers_are_read_from_a_transfers_file() {
        let root = project_with_nets();
        write_json(
            &root.path().join("links/transfers.json"),
            serde_json::json!([{"from_network": "company", "metric": {"type": "Node", "name": "a"}, "name": "from-file"}]),
        );
        let mut networks = joint_networks();
        networks[1].transfers_file = Some("links/transfers.json".to_string());
        let manifest = multi_model_manifest(networks);
        let composed = manifest.compose_multi_model(root.path(), "joint", OPTIONS).unwrap();
        let names: Vec<_> = composed.networks()[1]
            .transfers
            .iter()
            .map(|transfer| transfer.name.as_str())
            .collect();
        assert_eq!(names, ["company-a", "from-file"]);

        let mut networks = joint_networks();
        networks[1].transfers_file = Some("links/missing.json".to_string());
        assert!(matches!(
            multi_model_manifest(networks).compose_multi_model(root.path(), "joint", OPTIONS),
            Err(ComposeModelError::Resolution(
                ManifestResolutionError::TransfersFileNotFound { .. }
            ))
        ));
    }

    #[test]
    fn transfers_must_read_components_of_their_source_network() {
        let root = project_with_nets();
        let mut networks = joint_networks();
        networks[1].transfers = vec![transfer("company", "missing", "company-missing")];
        let Err(error) = multi_model_manifest(networks)
            .compose_multi_model(root.path(), "joint", OPTIONS)
            .unwrap()
            .load()
            .unwrap()
            .into_multi_model_schema(&Default::default())
        else {
            panic!("expected a missing transfer component");
        };
        assert_eq!(
            error.to_string(),
            "The transfer `company-missing` into `river` reads the node `missing` from `company`, which does not exist there."
        );
    }

    /// Compose `manifest`'s multi-model `joint` into a schema, returning the error.
    fn joint_schema_error(manifest: &ProjectManifest, root: &Path) -> String {
        let Err(error) = manifest
            .compose_multi_model(root, "joint", OPTIONS)
            .unwrap()
            .load()
            .unwrap()
            .into_multi_model_schema(&Default::default())
        else {
            panic!("the multi-model composed without error");
        };
        error.to_string()
    }

    #[test]
    fn transfers_must_be_declared_for_the_network_that_reads_them() {
        let root = project_with_nets();
        write_json(
            &root.path().join("nets/reader.json"),
            serde_json::json!({
                "nodes": [{"type": "Input", "meta": {"name": "reader"},
                           "max_flow": {"type": "InterNetworkTransfer", "name": "company-a"}}],
                "edges": []
            }),
        );
        let mut networks = joint_networks();
        networks[1].include = vec![selection("nets", Some(vec!["c.json", "reader.json"]), false)];
        let mut manifest = multi_model_manifest(networks);
        manifest
            .compose_multi_model(root.path(), "joint", OPTIONS)
            .unwrap()
            .load()
            .unwrap()
            .into_multi_model_schema(&Default::default())
            .unwrap();

        manifest.multi_models[0].networks[1].transfers = vec![transfer("company", "a", "company-b")];
        assert_eq!(
            joint_schema_error(&manifest, root.path()),
            "The network `river` reads the transfer `company-a`, but no transfer of that name into it is declared."
        );

        manifest.models[0].include = vec![selection("nets", Some(vec!["reader.json"]), false)];
        let Err(error) = manifest
            .compose_model(root.path(), "company", OPTIONS)
            .unwrap()
            .load()
            .unwrap()
            .into_model_schema(&Default::default())
        else {
            panic!("a model reading a transfer composed without error");
        };
        assert_eq!(
            error.to_string(),
            "The composed model reads the transfer `company-a`, but only a network of a multi-model can receive transfers."
        );
    }

    #[test]
    fn two_networks_must_not_write_the_same_output_file() {
        let root = project_with_nets();
        write_json(
            &root.path().join("nets/out.json"),
            serde_json::json!({
                "nodes": [],
                "edges": [],
                "outputs": [{"type": "HDF5", "meta": {"name": "out"}, "filename": "out.h5", "metric_set": "all"}]
            }),
        );
        let mut networks = joint_networks();
        networks[1].include = vec![selection("nets", Some(vec!["c.json", "out.json"]), false)];
        let mut manifest = multi_model_manifest(networks);
        manifest.models[0].include = vec![selection("nets", Some(vec!["a.json", "out.json"]), false)];
        assert_eq!(
            joint_schema_error(&manifest, root.path()),
            "The networks `company` and `river` both write the output file `out.h5`."
        );
    }

    #[test]
    fn multi_model_networks_are_checked_against_the_sets_they_select_from() {
        let root = project_with_nets();
        let mut manifest = multi_model_manifest(vec![
            multi_network("company", from_model("company")),
            multi_network("river", from_network("river")),
            multi_network("empty", None),
        ]);
        manifest.network_sets = vec![set("nets", None, Some(1), None)];
        if let Err(error) = manifest.validate(root.path(), OPTIONS) {
            panic!("{}", error.report());
        }

        // The model's own problems are reported once, as the model's, not again for the network
        // that uses it.
        manifest.network_sets = vec![set("nets", None, None, Some(0))];
        let expected = [
            "The model definition `company` selects 1 file(s) from network set `nets`, but at most 0 are allowed.",
            "The multi-model network `joint/river` selects 1 file(s) from network set `nets`, but at most 0 are allowed.",
        ];
        let messages = |report: ProjectManifestValidationError| -> Vec<String> {
            report.problems.iter().map(ToString::to_string).collect()
        };
        assert_eq!(messages(manifest.validate(root.path(), OPTIONS).unwrap_err()), expected);
        assert_eq!(
            messages(manifest.validate_model(root.path(), "joint", OPTIONS).unwrap_err()),
            expected
        );
    }

    #[test]
    fn a_multi_models_paths_include_its_transfers_files() {
        let root = project_with_nets();
        write_json(&root.path().join("links/transfers.json"), serde_json::json!([]));
        let mut networks = joint_networks();
        networks[1].transfers_file = Some("links/transfers.json".to_string());
        let paths = multi_model_manifest(networks)
            .compose_multi_model(root.path(), "joint", OPTIONS)
            .unwrap()
            .all_paths();
        assert_eq!(
            paths.last(),
            Some(&root.path().join("links/transfers.json").canonicalize().unwrap())
        );
    }

    #[test]
    fn multi_model_problems_are_attributed_to_the_network() {
        let root = project_with_nets();
        let mut networks = joint_networks();
        networks[0].network = from_model("absent");
        networks[1].include = vec![selection("nets", Some(vec!["missing.json"]), false)];
        networks.push(multi_network("company", None));
        let report = multi_model_manifest(networks)
            .validate(root.path(), OPTIONS)
            .unwrap_err();
        let messages: Vec<_> = report.problems.iter().map(ToString::to_string).collect();
        assert_eq!(
            messages,
            [
                "The multi-model definition `joint` has 2 networks named `company`, but each name must be unique.",
                "The multi-model network `joint/company` refers to the model definition `absent`, which does not exist.",
                "The multi-model network `joint/river` selects the file `missing.json`, which was not found in network set `nets`.",
            ]
        );
    }

    #[test]
    fn models_and_multi_models_share_names() {
        let root = project_with_nets();
        let mut manifest = multi_model_manifest(joint_networks());
        manifest.multi_models[0].name = "company".to_string();
        assert!(matches!(
            manifest.validate(root.path(), OPTIONS).unwrap_err().problems.as_slice(),
            [ProjectManifestProblem::DuplicateDefinition { definition, count: 2 }, ..] if definition == "company"
        ));
    }

    #[test]
    fn multi_model_json_accepts_each_domain_and_network_source() {
        let manifest: super::super::ProjectManifest = serde_json::from_str(
            r#"{
                "format": "v1",
                "multi_models": [
                    {
                        "name": "from-model",
                        "domain": { "type": "Model", "name": "full" },
                        "networks": [
                            { "name": "a", "network": { "type": "Model", "name": "full" } },
                            { "name": "b", "network": { "type": "Network", "name": "upgraded" }, "transfers_file": "links.json" }
                        ]
                    },
                    { "name": "from-file", "domain": { "type": "BaseModel", "path": "base.json" }, "networks": [] },
                    {
                        "name": "inline",
                        "domain": { "type": "Inline", "time": { "start": "2020-01-01T00:00:00", "end": "2020-12-31T00:00:00", "timestep": { "type": "Days", "days": 1 } } },
                        "networks": [{ "name": "c", "include": [{ "set": "nets", "include_all": true }] }]
                    }
                ]
            }"#,
        )
        .unwrap();
        let super::super::ProjectManifest::V1(manifest) = manifest;
        let [from_model, from_file, inline] = manifest.multi_models.as_slice() else {
            panic!("expected three multi-models");
        };
        assert!(matches!(from_model.domain, MultiModelDomain::Model(_)));
        assert!(matches!(
            from_model.networks[1].network,
            Some(MultiModelNetworkSource::Network(_))
        ));
        assert!(matches!(from_file.domain, MultiModelDomain::BaseModel(_)));
        assert!(matches!(inline.domain, MultiModelDomain::Inline(_)));
        assert!(inline.networks[0].network.is_none());
    }

    #[test]
    fn multi_model_json_errors_name_what_is_wrong() {
        let error = |domain: &str| {
            let json =
                format!(r#"{{"format": "v1", "multi_models": [{{"name": "m", "domain": {domain}, "networks": []}}]}}"#);
            serde_json::from_str::<super::super::ProjectManifest>(&json)
                .err()
                .expect("the manifest was accepted")
                .to_string()
        };
        assert!(
            error(r#"{"type": "Modle", "name": "x"}"#)
                .contains("unknown variant `Modle`, expected one of `Model`, `BaseModel`, `Inline`")
        );
        assert!(error(r#"{"type": "Model", "nmae": "x"}"#).contains("unknown field `nmae`"));
        assert!(error(r#"{"type": "BaseModel"}"#).contains("missing field `path`"));
        assert!(error(r#"{"model": "x"}"#).contains("missing field `type`"));
    }

    #[test]
    fn omitted_set_constraints_are_reported_in_declaration_order() {
        let root = tempdir().unwrap();
        touch(&root.path().join("base.json"));
        for dir in ["z", "a"] {
            std::fs::create_dir(root.path().join(dir)).unwrap();
        }
        let manifest = manifest(
            "base.json",
            vec![set("z", None, Some(1), None), set("a", None, Some(1), None)],
            vec![],
        );

        let report = manifest.validate(root.path(), OPTIONS).unwrap_err();
        let sets: Vec<_> = report
            .problems
            .iter()
            .map(|problem| match problem {
                ProjectManifestProblem::MinFilesNotMet {
                    set, actual_files: 0, ..
                } => set.as_str(),
                other => panic!("unexpected problem: {other}"),
            })
            .collect();
        assert_eq!(sets, ["z", "a"]);
    }

    #[test]
    fn unresolved_set_is_not_also_reported_as_missing() {
        let root = tempdir().unwrap();
        touch(&root.path().join("base.json"));
        let manifest = manifest(
            "base.json",
            vec![set("nets", None, None, None)],
            vec![selection("nets", None, true)],
        );

        let report = manifest.validate(root.path(), OPTIONS).unwrap_err();
        assert!(matches!(
            report.problems.as_slice(),
            [ProjectManifestProblem::Resolution(
                ManifestResolutionError::DirectoryNotFound { .. }
            )]
        ));
    }

    #[test]
    fn validation_reports_invalid_paths_without_aborting() {
        let root = tempdir().unwrap();
        let manifest = manifest(
            "../base.json",
            vec![set("nets", Some("../nets"), None, None)],
            vec![selection("nets", Some(vec!["../outside.json"]), false)],
        );
        let report = manifest.validate(root.path(), OPTIONS).unwrap_err();
        assert!(report.problems.iter().any(
            |error| matches!(error, ProjectManifestProblem::Resolution(ManifestResolutionError::InvalidRelativePath { field, .. }) if field == "base model"))
        );
        assert!(report.problems.iter().any(|error| matches!(error, ProjectManifestProblem::Resolution(ManifestResolutionError::InvalidRelativePath { field, .. }) if field.contains("network set 'nets' directory"))));
    }

    #[test]
    fn composition_rejects_relative_path_escapes() {
        let root = tempdir().unwrap();
        let manifest = manifest("../base.json", vec![], vec![]);
        assert!(
            matches!(manifest.compose_model(root.path(), "test", OPTIONS), Err(ComposeModelError::Resolution(ManifestResolutionError::InvalidRelativePath { field, .. })) if field == "base model")
        );
    }

    #[test]
    fn selected_filenames_must_be_single_portable_path_components() {
        let root = tempdir().unwrap();
        touch(&root.path().join("base.json"));
        std::fs::create_dir(root.path().join("nets")).unwrap();
        touch(&root.path().join("nets/valid.json"));
        let manifest = manifest(
            "base.json",
            vec![set("nets", None, None, None)],
            vec![selection("nets", Some(vec!["../outside.json"]), false)],
        );

        let report = manifest.validate(root.path(), OPTIONS).unwrap_err();
        assert!(report.problems.iter().any(|error| matches!(
            error,
            ProjectManifestProblem::InvalidFilePath { file, .. } if file == "../outside.json"
        )));
        assert!(matches!(
            manifest.compose_model(root.path(), "test", OPTIONS),
            Err(ComposeModelError::Resolution(
                ManifestResolutionError::InvalidRelativePath { .. }
            ))
        ));
    }

    #[test]
    fn include_all_is_sorted_and_composition_rejects_duplicate_set_selections() {
        let root = tempdir().unwrap();
        touch(&root.path().join("base.json"));
        std::fs::create_dir(root.path().join("nets")).unwrap();
        touch(&root.path().join("nets/z.json"));
        touch(&root.path().join("nets/a.json"));
        let project_manifest = manifest(
            "base.json",
            vec![set("nets", None, None, None)],
            vec![selection("nets", None, true)],
        );
        let composed = project_manifest.compose_model(root.path(), "test", OPTIONS).unwrap();
        assert_eq!(
            composed.all_paths(),
            vec![
                root.path().join("base.json").canonicalize().unwrap(),
                root.path().join("nets/a.json").canonicalize().unwrap(),
                root.path().join("nets/z.json").canonicalize().unwrap()
            ]
        );
        let duplicate = manifest(
            "base.json",
            vec![set("nets", None, None, None)],
            vec![
                selection("nets", None, true),
                selection("nets", Some(vec!["a.json"]), false),
            ],
        );
        assert!(matches!(
            duplicate.compose_model(root.path(), "test", OPTIONS),
            Err(ComposeModelError::DuplicateSelection { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_network_file_outside_its_set_is_rejected() {
        use std::os::unix::fs::symlink;
        let root = tempdir().unwrap();
        let outside = tempdir().unwrap();
        touch(&root.path().join("base.json"));
        std::fs::create_dir(root.path().join("nets")).unwrap();
        touch(&outside.path().join("outside.json"));
        symlink(
            outside.path().join("outside.json"),
            root.path().join("nets/escape.json"),
        )
        .unwrap();
        let manifest = manifest(
            "base.json",
            vec![set("nets", None, None, None)],
            vec![selection("nets", None, true)],
        );
        let report = manifest.validate(root.path(), OPTIONS).unwrap_err();
        assert!(report.problems.iter().any(|error| matches!(
            error,
            ProjectManifestProblem::Resolution(ManifestResolutionError::PathEscapesRoot { .. })
        )));
        assert!(matches!(
            manifest.compose_model(root.path(), "test", OPTIONS),
            Err(ComposeModelError::Resolution(
                ManifestResolutionError::PathEscapesRoot { .. }
            ))
        ));
    }

    #[test]
    fn validation_reports_duplicates_and_invalid_constraints() {
        let root = tempdir().unwrap();
        touch(&root.path().join("base.json"));
        std::fs::create_dir(root.path().join("nets")).unwrap();
        let manifest = ProjectManifest {
            multi_models: vec![],
            dependencies: vec![],
            exports: None,
            base_model: Some("base.json".to_string()),
            sub_networks: vec![],
            network_sets: vec![set("nets", None, Some(2), Some(1)), set("nets", None, None, None)],
            networks: vec![],
            models: vec![model("test", vec![]), model("test", vec![])],
        };
        let report = manifest.validate(root.path(), OPTIONS).unwrap_err();
        assert!(
            report
                .problems
                .iter()
                .any(|error| matches!(error, ProjectManifestProblem::DuplicateNetworkSet { .. }))
        );
        assert!(
            report
                .problems
                .iter()
                .any(|error| matches!(error, ProjectManifestProblem::DuplicateDefinition { .. }))
        );
        assert!(
            report
                .problems
                .iter()
                .any(|error| matches!(error, ProjectManifestProblem::InvalidFileConstraints { .. }))
        );
    }

    /// A workspace with two projects side by side: `main`, with `base.json` and a set `nets`
    /// holding `m.json`, and `org`, whose manifest is [`org_manifest`].
    fn workspace() -> tempfile::TempDir {
        let workspace = tempdir().unwrap();
        let root = workspace.path();
        let model = serde_json::to_value(pywr_schema::ModelSchema::default()).unwrap();
        write_json(&root.join("main/base.json"), model.clone());
        write_json(&root.join("main/nets/m.json"), input_network("m"));
        let mut org_model = model;
        org_model["network"] = input_network("org-base");
        write_json(&root.join("org/models/base.json"), org_model);
        write_json(
            &root.join("org/licences/l.json"),
            serde_json::json!({
                "nodes": [{"type": "Input", "meta": {"name": "licence"}}],
                "edges": [],
                "time_series": [{"type": "Polars", "meta": {"name": "flows"}, "path": "flows.csv"}]
            }),
        );
        write_json(&root.join("org/upper/base.json"), input_network("upper"));
        write_json(&root.join("org/upper/options/o.json"), input_network("o"));
        write_org_manifest(root, org_manifest());
        workspace
    }

    /// The `org` project: a set `licences`, a sub-network `upper` with a set `options`, network
    /// definitions `river` (all of `licences`) and `upper` (based on `upper`), and a model
    /// `baseline` (`river`).
    fn org_manifest() -> serde_json::Value {
        serde_json::json!({
            "format": "v1",
            "base_model": "models/base.json",
            "sub_networks": [{"name": "upper", "base_network": "base.json", "network_sets": [{"name": "options"}]}],
            "network_sets": [{"name": "licences"}],
            "networks": [
                {"name": "river", "include": [{"set": "licences", "include_all": true}]},
                {"name": "upper", "sub_networks": ["upper"]}
            ],
            "models": [{"name": "baseline", "networks": ["river"]}]
        })
    }

    fn write_org_manifest(root: &Path, manifest: serde_json::Value) {
        write_json(&root.join("org/pywr-project.json"), manifest);
    }

    fn dependency(name: &str, manifest: &str) -> Dependency {
        Dependency {
            name: name.to_string(),
            manifest: manifest.to_string(),
            sha256: None,
            data_dir: None,
        }
    }

    /// The `main` project of [`workspace`], depending on `org`, with the given models.
    fn main_manifest(models: Vec<ModelDefinition>) -> ProjectManifest {
        let mut manifest = manifest("base.json", vec![set("nets", None, None, None)], vec![]);
        manifest.models = models;
        manifest.dependencies = vec![dependency("org", "../org/pywr-project.json")];
        manifest
    }

    #[test]
    fn dependency_networks_compose_with_their_provenance_and_rebased_data_paths() {
        let workspace = workspace();
        let root = workspace.path().join("main");
        let mut manifest = main_manifest(vec![uses_networks(
            model("test", vec![selection("nets", Some(vec!["m.json"]), false)]),
            vec!["org::river"],
        )]);
        if let Err(error) = manifest.validate(&root, OPTIONS) {
            panic!("{}", error.report());
        }

        let composed = manifest.compose_model(&root, "test", OPTIONS).unwrap();
        assert_eq!(
            composed.all_paths(),
            net_paths(
                workspace.path(),
                &["main/base.json", "org/licences/l.json", "main/nets/m.json"]
            )
        );
        let schema = composed.load().unwrap().into_model_schema(&Default::default()).unwrap();
        let licence = schema.network.get_node_by_name("licence").unwrap();
        assert_eq!(
            licence.meta().provenance().unwrap().origin,
            ProvenanceSource {
                file: "licences/l.json".into(),
                network_set: Some("licences".into()),
                sub_network: None,
                project: Some("org".into()),
                network: Some("river".into()),
            }
        );
        let flows_path = |manifest: &ProjectManifest| {
            let schema = manifest
                .compose_model(&root, "test", OPTIONS)
                .unwrap()
                .load()
                .unwrap()
                .into_model_schema(&Default::default())
                .unwrap();
            let pywr_schema::time_series::TimeSeries::Polars(time_series) =
                &schema.network.time_series.as_ref().unwrap()[0]
            else {
                panic!("expected a Polars time series");
            };
            time_series.path.clone()
        };
        assert_eq!(flows_path(&manifest), Path::new("../org/flows.csv"));

        manifest.dependencies[0].data_dir = Some("data".to_string());
        assert!(matches!(
            manifest.compose_model(&root, "test", OPTIONS),
            Err(ComposeModelError::Resolution(ManifestResolutionError::DataDirectoryNotFound { owner, .. }))
                if owner == "dependency `org`"
        ));
        std::fs::create_dir(workspace.path().join("org/data")).unwrap();
        assert_eq!(flows_path(&manifest), Path::new("../org/data/flows.csv"));
    }

    #[test]
    fn dependency_sets_sub_networks_and_models_are_referred_to_by_qualified_names() {
        let workspace = workspace();
        let root = workspace.path().join("main");
        let mut manifest = main_manifest(vec![
            model("sets", vec![selection("org::licences", Some(vec!["l.json"]), false)]),
            uses_networks(model("upper", vec![]), vec!["upper-o"]),
        ]);
        manifest.networks = vec![NetworkDefinition {
            sub_networks: vec!["org::upper".to_string()],
            ..network(
                "upper-o",
                vec![],
                vec![selection("org::upper::options", Some(vec!["o.json"]), false)],
            )
        }];
        manifest.multi_models = vec![MultiModelDefinition {
            name: "joint".to_string(),
            domain: MultiModelDomain::Model(ModelDomain {
                name: "sets".to_string(),
            }),
            overrides: None,
            networks: vec![multi_network("org", from_model("org::baseline"))],
        }];
        if let Err(error) = manifest.validate(&root, OPTIONS) {
            panic!("{}", error.report());
        }

        assert_eq!(
            manifest.compose_model(&root, "sets", OPTIONS).unwrap().all_paths(),
            net_paths(workspace.path(), &["main/base.json", "org/licences/l.json"])
        );
        assert_eq!(
            manifest.compose_model(&root, "upper", OPTIONS).unwrap().all_paths(),
            net_paths(
                workspace.path(),
                &["main/base.json", "org/upper/base.json", "org/upper/options/o.json"]
            )
        );
        let joint = manifest.compose_multi_model(&root, "joint", OPTIONS).unwrap();
        assert_eq!(
            joint.networks()[0].network.all_paths(),
            net_paths(workspace.path(), &["org/models/base.json", "org/licences/l.json"])
        );
        let schema = joint
            .load()
            .unwrap()
            .into_multi_model_schema(&Default::default())
            .unwrap();
        let pywr_schema::NetworkSchemaRef::Inline(network) = &schema.networks[0].network else {
            panic!("expected an inline network");
        };
        let origin = network
            .get_node_by_name("org-base")
            .unwrap()
            .meta()
            .provenance()
            .unwrap()
            .origin
            .clone();
        assert_eq!(origin.file, RelativePathBuf::from("models/base.json"));
        assert_eq!(origin.project.as_deref(), Some("org"));
    }

    #[test]
    fn exports_limit_what_a_dependency_exposes() {
        let workspace = workspace();
        let root = workspace.path().join("main");
        let mut org = org_manifest();
        org["exports"] = serde_json::json!({"networks": ["river"], "models": ["missing"]});
        write_org_manifest(workspace.path(), org);
        let manifest = main_manifest(vec![
            uses_networks(model("river", vec![]), vec!["org::river"]),
            uses_networks(model("upper", vec![]), vec!["org::upper"]),
            model("sets", vec![selection("org::licences", Some(vec!["l.json"]), false)]),
        ]);

        let report = manifest.validate(&root, OPTIONS).unwrap_err();
        assert!(matches!(
            report.problems.as_slice(),
            [
                ProjectManifestProblem::InDependency { dependency, problem },
                ProjectManifestProblem::NotExported { definition: upper, project: upper_project, kind: "network definition", item: upper_item },
                ProjectManifestProblem::NotExported { definition: sets, kind: "network set", item: sets_item, .. },
            ] if dependency == "org"
                && matches!(problem.as_ref(), ProjectManifestProblem::ExportNotFound { kind: "model definition", name } if name == "missing")
                && upper.name == "upper" && upper_project == "org" && upper_item == "upper"
                && sets.name == "sets" && sets_item == "licences"
        ));
        assert_eq!(
            report.problems[0].to_string(),
            "In the dependency `org`: The project exports the model definition `missing`, which does not exist."
        );
    }

    #[test]
    fn dependency_problems_are_reported_once_as_in_the_dependency() {
        let workspace = workspace();
        let root = workspace.path().join("main");
        let mut org = org_manifest();
        org["networks"][0]["include"][0]["set"] = serde_json::json!("missing");
        write_org_manifest(workspace.path(), org);
        let manifest = main_manifest(vec![
            uses_networks(model("a", vec![]), vec!["org::river"]),
            uses_networks(model("b", vec![]), vec!["org::river"]),
        ]);

        assert!(matches!(
            manifest.validate(&root, OPTIONS).unwrap_err().problems.as_slice(),
            [ProjectManifestProblem::InDependency { dependency, problem }]
                if dependency == "org"
                    && matches!(problem.as_ref(), ProjectManifestProblem::SetNotFound { definition, set } if definition.name == "river" && set == "missing")
        ));
        assert!(matches!(
            manifest.compose_model(&root, "b", OPTIONS),
            Err(ComposeModelError::InDependency { dependency, error })
                if dependency == "org" && matches!(error.as_ref(), ComposeModelError::SetNotFound { .. })
        ));
    }

    #[test]
    fn shared_dependencies_load_once_and_cycles_are_reported() {
        let workspace = workspace();
        let root = workspace.path().join("main");
        write_json(&workspace.path().join("shared/nets/s.json"), input_network("s"));
        write_json(
            &workspace.path().join("shared/pywr-project.json"),
            serde_json::json!({
                "format": "v1",
                "network_sets": [{"name": "nets"}],
                "networks": [{"name": "shared", "include": [{"set": "nets", "include_all": true}]}]
            }),
        );
        let mut org = org_manifest();
        org["dependencies"] = serde_json::json!([{"name": "shared", "manifest": "../shared/pywr-project.json"}]);
        org["networks"][0]["extends"] = serde_json::json!(["shared::shared"]);
        write_org_manifest(workspace.path(), org);
        let mut manifest = main_manifest(vec![uses_networks(
            model("test", vec![]),
            vec!["shared::shared", "org::river", "org::shared::shared"],
        )]);
        manifest
            .dependencies
            .push(dependency("shared", "../shared/pywr-project.json"));

        // `shared` is reached three ways, but its file is merged once.
        assert_eq!(
            manifest.compose_model(&root, "test", OPTIONS).unwrap().all_paths(),
            net_paths(
                workspace.path(),
                &["main/base.json", "shared/nets/s.json", "org/licences/l.json"]
            )
        );

        let mut shared = serde_json::json!({"format": "v1"});
        shared["dependencies"] = serde_json::json!([{"name": "org", "manifest": "../org/pywr-project.json"}]);
        write_json(&workspace.path().join("shared/pywr-project.json"), shared);
        let org_path = workspace.path().join("org/pywr-project.json").canonicalize().unwrap();
        let shared_path = workspace
            .path()
            .join("shared/pywr-project.json")
            .canonicalize()
            .unwrap();
        let problems = manifest.validate(&root, OPTIONS).unwrap_err().problems;
        assert!(
            problems.iter().any(|problem| matches!(
                problem,
                ProjectManifestProblem::InDependency { dependency, problem }
                    if dependency == "org::shared"
                        && matches!(problem.as_ref(), ProjectManifestProblem::DependencyCycle { cycle } if *cycle == [org_path.clone(), shared_path.clone(), org_path.clone()])
            )),
            "{problems:?}"
        );
    }

    #[test]
    fn dependency_policy_and_hash_are_checked() {
        let workspace = workspace();
        let root = workspace.path().join("main");
        let mut manifest = main_manifest(vec![uses_networks(model("test", vec![]), vec!["org::river"])]);
        let not_allowed = |options: &ResolveOptions| {
            matches!(
                manifest.validate(&root, options).unwrap_err().problems.as_slice(),
                [ProjectManifestProblem::Resolution(ManifestResolutionError::DependencyNotAllowed { dependency, .. })]
                    if dependency == "org"
            )
        };
        assert!(not_allowed(&ResolveOptions {
            dependency_policy: DependencyPolicy::Deny,
            manifest_path: None,
        }));
        assert!(not_allowed(&ResolveOptions {
            dependency_policy: DependencyPolicy::AllowWithin(root.clone()),
            manifest_path: None,
        }));
        let within_workspace = ResolveOptions {
            dependency_policy: DependencyPolicy::AllowWithin(workspace.path().to_path_buf()),
            manifest_path: None,
        };
        assert!(manifest.validate(&root, &within_workspace).is_ok());

        let data = std::fs::read(workspace.path().join("org/pywr-project.json")).unwrap();
        let hash: String = Sha256::digest(&data).iter().map(|byte| format!("{byte:02x}")).collect();
        manifest.dependencies[0].sha256 = Some(hash.to_uppercase());
        assert!(manifest.validate(&root, OPTIONS).is_ok());
        manifest.dependencies[0].sha256 = Some("00".to_string());
        assert!(matches!(
            manifest.validate(&root, OPTIONS).unwrap_err().problems.as_slice(),
            [ProjectManifestProblem::Resolution(ManifestResolutionError::DependencyHashMismatch { actual, .. })]
                if *actual == hash
        ));
    }

    #[test]
    fn manifest_hashes_read_crlf_line_endings_as_lf() {
        let lf = manifest_hash(b"{\n  \"format\": \"v1\"\n}\n");
        assert_eq!(manifest_hash(b"{\r\n  \"format\": \"v1\"\r\n}\r\n"), lf);
        let plain: String = Sha256::digest(b"{\n  \"format\": \"v1\"\n}\n")
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(lf, plain);
        // A lone carriage return is content, not a line ending.
        assert_ne!(manifest_hash(b"a\rb"), manifest_hash(b"a\nb"));
    }

    #[test]
    fn a_pin_is_checked_when_its_manifest_was_already_loaded_without_one() {
        let workspace = workspace();
        let root = workspace.path().join("main");
        write_json(
            &workspace.path().join("a/pywr-project.json"),
            serde_json::json!({"format": "v1", "dependencies": [{"name": "org", "manifest": "../org/pywr-project.json"}]}),
        );
        let mut manifest = main_manifest(vec![]);
        let mut pinned = dependency("org", "../org/pywr-project.json");
        pinned.sha256 = Some("00".to_string());
        manifest.dependencies = vec![dependency("a", "../a/pywr-project.json"), pinned];
        assert!(matches!(
            manifest.validate(&root, OPTIONS).unwrap_err().problems.as_slice(),
            [ProjectManifestProblem::Resolution(ManifestResolutionError::DependencyHashMismatch { dependency, .. })]
                if dependency == "org"
        ));
    }

    #[test]
    fn a_dependency_leading_back_to_the_root_is_a_cycle_starting_at_the_root() {
        let workspace = workspace();
        let root = workspace.path().join("main");
        write_json(
            &workspace.path().join("a/pywr-project.json"),
            serde_json::json!({"format": "v1", "dependencies": [{"name": "back", "manifest": "../main/pywr-project.json"}]}),
        );
        let mut manifest = main_manifest(vec![]);
        manifest.dependencies = vec![dependency("a", "../a/pywr-project.json")];
        let manifest_path = root.join("pywr-project.json");
        std::fs::write(
            &manifest_path,
            serde_json::to_vec(&super::super::ProjectManifest::V1(manifest.clone())).unwrap(),
        )
        .unwrap();
        let options = ResolveOptions {
            dependency_policy: DependencyPolicy::Allow,
            manifest_path: Some(manifest_path.clone()),
        };
        let report = manifest.validate(&root, &options).unwrap_err();
        let [ProjectManifestProblem::InDependency { dependency, problem }] = report.problems.as_slice() else {
            panic!("{}", report.report());
        };
        let ProjectManifestProblem::DependencyCycle { cycle } = problem.as_ref() else {
            panic!("{problem}");
        };
        let canonical = |path: &Path| path.canonicalize().unwrap();
        assert_eq!(dependency, "a");
        assert_eq!(
            cycle,
            &[
                canonical(&manifest_path),
                canonical(&workspace.path().join("a/pywr-project.json")),
                canonical(&manifest_path)
            ]
        );
    }

    #[test]
    fn a_shared_project_is_named_as_the_referring_project_names_it() {
        let workspace = workspace();
        let root = workspace.path().join("main");
        write_json(
            &workspace.path().join("shared/pywr-project.json"),
            serde_json::json!({"format": "v1", "exports": {}, "networks": [{"name": "hidden"}]}),
        );
        write_json(
            &workspace.path().join("a/pywr-project.json"),
            serde_json::json!({"format": "v1", "dependencies": [{"name": "common", "manifest": "../shared/pywr-project.json"}]}),
        );
        write_json(
            &workspace.path().join("b/pywr-project.json"),
            serde_json::json!({
                "format": "v1",
                "dependencies": [{"name": "lib", "manifest": "../shared/pywr-project.json"}],
                "networks": [{"name": "uses", "extends": ["lib::hidden"]}]
            }),
        );
        let mut manifest = main_manifest(vec![uses_networks(model("test", vec![]), vec!["b::uses"])]);
        manifest.dependencies = vec![
            dependency("a", "../a/pywr-project.json"),
            dependency("b", "../b/pywr-project.json"),
        ];
        let report = manifest.validate(&root, OPTIONS).unwrap_err();
        let messages: Vec<_> = report.problems.iter().map(ToString::to_string).collect();
        assert_eq!(
            messages,
            [
                "In the dependency `b`: The network definition `uses` refers to the network definition `hidden`, which the dependency `lib` does not export."
            ]
        );
    }

    #[test]
    fn a_dependency_model_is_checked_against_its_own_projects_limits() {
        let workspace = workspace();
        let root = workspace.path().join("main");
        let model_json = serde_json::to_value(pywr_schema::ModelSchema::default()).unwrap();
        write_json(&workspace.path().join("dep/base.json"), model_json);
        std::fs::create_dir_all(workspace.path().join("dep/required")).unwrap();
        write_json(
            &workspace.path().join("dep/pywr-project.json"),
            serde_json::json!({
                "format": "v1",
                "base_model": "base.json",
                "network_sets": [{"name": "required", "min_files": 1}],
                "models": [{"name": "m"}]
            }),
        );
        let mut manifest = main_manifest(vec![model("local", vec![])]);
        manifest.dependencies = vec![dependency("dep", "../dep/pywr-project.json")];
        manifest.multi_models = vec![MultiModelDefinition {
            name: "joint".to_string(),
            domain: MultiModelDomain::Model(ModelDomain {
                name: "local".to_string(),
            }),
            overrides: None,
            networks: vec![multi_network("dep", from_model("dep::m"))],
        }];
        let report = manifest.validate(&root, OPTIONS).unwrap_err();
        let messages: Vec<_> = report.problems.iter().map(ToString::to_string).collect();
        assert_eq!(
            messages,
            [
                "The model definition `dep::m` selects 0 file(s) from network set `dep::required`, but at least 1 are required."
            ]
        );
    }

    #[test]
    fn an_active_dependency_sub_network_is_held_to_its_set_limits() {
        let workspace = workspace();
        let root = workspace.path().join("main");
        std::fs::create_dir_all(workspace.path().join("dep/upper/options")).unwrap();
        write_json(
            &workspace.path().join("dep/pywr-project.json"),
            serde_json::json!({
                "format": "v1",
                "sub_networks": [{"name": "upper", "network_sets": [{"name": "options", "min_files": 1}]}],
                "exports": {"sub_networks": ["upper"]}
            }),
        );
        let mut manifest = main_manifest(vec![with_sub_networks(model("local", vec![]), vec!["dep::upper"])]);
        manifest.dependencies = vec![dependency("dep", "../dep/pywr-project.json")];
        let mut network = multi_network("upper", None);
        network.sub_networks = vec!["dep::upper".to_string()];
        manifest.multi_models = vec![MultiModelDefinition {
            name: "joint".to_string(),
            domain: MultiModelDomain::Model(ModelDomain {
                name: "local".to_string(),
            }),
            overrides: None,
            networks: vec![network],
        }];
        let report = manifest.validate(&root, OPTIONS).unwrap_err();
        let messages: Vec<_> = report.problems.iter().map(ToString::to_string).collect();
        assert_eq!(
            messages,
            [
                "The model definition `local` selects 0 file(s) from network set `dep::upper::options`, but at least 1 are required.",
                "The multi-model network `joint/upper` selects 0 file(s) from network set `dep::upper::options`, but at least 1 are required.",
            ]
        );
    }

    #[test]
    fn a_broken_dependency_only_stops_the_definitions_that_use_it() {
        let workspace = workspace();
        let root = workspace.path().join("main");
        write_json(
            &workspace.path().join("broken/pywr-project.json"),
            serde_json::json!({
                "format": "v1",
                "network_sets": [{"name": "missing"}],
                "networks": [{"name": "n"}]
            }),
        );
        let mut manifest = main_manifest(vec![
            model("own", vec![selection("nets", Some(vec!["m.json"]), false)]),
            uses_networks(model("uses", vec![]), vec!["broken::n"]),
        ]);
        manifest.dependencies = vec![
            dependency("broken", "../broken/pywr-project.json"),
            dependency("absent", "../absent/pywr-project.json"),
        ];

        manifest.compose_model(&root, "own", OPTIONS).unwrap();
        assert!(manifest.validate_model(&root, "own", OPTIONS).is_ok());
        let in_broken = |error: &ComposeModelError| matches!(error, ComposeModelError::InDependency { dependency, .. } if dependency == "broken");
        assert!(in_broken(
            &manifest.compose_model(&root, "uses", OPTIONS).err().unwrap()
        ));

        // Validating the whole project reports every dependency's problems.
        let report = manifest.validate(&root, OPTIONS).unwrap_err();
        assert!(matches!(
            report.problems.as_slice(),
            [
                ProjectManifestProblem::InDependency { dependency, .. },
                ProjectManifestProblem::Resolution(ManifestResolutionError::DependencyManifestNotFound { dependency: absent, .. }),
            ] if dependency == "broken" && absent == "absent"
        ));
    }

    #[test]
    fn dependency_names_and_paths_are_checked() {
        let workspace = workspace();
        let root = workspace.path().join("main");
        let mut manifest = main_manifest(vec![]);
        manifest.sub_networks = vec![sub_network("org", None, vec![])];
        std::fs::create_dir(root.join("org")).unwrap();
        manifest.dependencies.extend([
            dependency("org", "../org/pywr-project.json"),
            dependency("a::b", "../org/pywr-project.json"),
            dependency("absolute", "/pywr-project.json"),
            dependency("missing", "../missing/pywr-project.json"),
        ]);

        let problems = manifest.validate(&root, OPTIONS).unwrap_err().problems;
        assert!(
            matches!(
                problems.as_slice(),
                [
                    ProjectManifestProblem::DuplicateDependency { dependency: duplicate, count: 2 },
                    ProjectManifestProblem::DependencyNameTaken { dependency: taken },
                    ProjectManifestProblem::InvalidName { kind: "dependency", name },
                    ProjectManifestProblem::Resolution(ManifestResolutionError::InvalidDependencyPath { dependency: absolute, .. }),
                    ProjectManifestProblem::Resolution(ManifestResolutionError::DependencyManifestNotFound { dependency: missing, .. }),
                ] if duplicate == "org" && taken == "org" && name == "a::b" && absolute == "absolute" && missing == "missing"
            ),
            "{problems:?}"
        );
    }

    #[test]
    fn dependency_set_constraints_apply_only_when_selected_from() {
        let workspace = workspace();
        let root = workspace.path().join("main");
        let mut org = org_manifest();
        org["network_sets"] = serde_json::json!([{"name": "licences", "min": 2}]);
        org["networks"][0]["include"] = serde_json::json!([]);
        write_org_manifest(workspace.path(), org);
        let manifest = main_manifest(vec![
            uses_networks(model("unused", vec![]), vec!["org::river"]),
            model("used", vec![selection("org::licences", Some(vec!["l.json"]), false)]),
        ]);

        assert!(matches!(
            manifest.validate(&root, OPTIONS).unwrap_err().problems.as_slice(),
            [ProjectManifestProblem::MinFilesNotMet { definition, set, actual_files: 1, .. }]
                if definition.name == "used" && set == "org::licences"
        ));
    }

    #[test]
    fn manifest_json_accepts_dependencies_and_exports() {
        let manifest: crate::manifest::ProjectManifest = serde_json::from_value(serde_json::json!({
            "format": "v1",
            "dependencies": [{"name": "org", "manifest": "../org/pywr-project.json", "sha256": "ab", "data_dir": "data"}],
            "exports": {"networks": ["a"], "models": ["b"], "network_sets": ["upper::c"], "sub_networks": ["upper"]}
        }))
        .unwrap();
        let crate::manifest::ProjectManifest::V1(manifest) = manifest;
        assert_eq!(manifest.dependencies[0].data_dir.as_deref(), Some("data"));
        assert_eq!(manifest.exports.unwrap().network_sets, ["upper::c"]);
    }
}
