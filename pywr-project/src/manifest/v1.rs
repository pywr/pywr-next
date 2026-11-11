use crate::composition::{
    ComposedModel, ComposedModelBuilder, ComposedNetwork, ComposedNetworkPath, LoadContext, PositionOffset,
};
use crate::error::{ComposeModelError, ManifestResolutionError};
use crate::manifest::DefinitionOverrides;
use pywr_schema::meta::ProvenanceSource;
use relative_path::{RelativePath, RelativePathBuf};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
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
                    let mut source = std::error::Error::source(problem);
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

#[derive(Error, Debug)]
pub enum ProjectManifestProblem {
    #[error("The model definition `{definition}` was not found in the project manifest.")]
    DefinitionNotFound { definition: String },
    #[error("The network definition `{network}` was not found in the project manifest.")]
    NetworkDefinitionNotFound { network: String },
    #[error("The name `{set}` is used by {count} network sets, but each name must be unique.")]
    DuplicateNetworkSet { set: String, count: usize },
    #[error("The name `{definition}` is used by {count} model definitions, but each name must be unique.")]
    DuplicateDefinition { definition: String, count: usize },
    #[error("The name `{network}` is used by {count} network definitions, but each name must be unique.")]
    DuplicateNetworkDefinition { network: String, count: usize },
    #[error("The name `{sub_network}` is used by {count} sub-networks, but each name must be unique.")]
    DuplicateSubNetwork { sub_network: String, count: usize },
    #[error("The {kind} name `{name}` must not contain `::`.")]
    InvalidName { kind: &'static str, name: String },
    #[error("The definition `{definition}` refers to the network definition `{network}`, which does not exist.")]
    NetworkNotFound { definition: String, network: String },
    #[error("The definition `{definition}` refers to the sub-network `{sub_network}`, which does not exist.")]
    SubNetworkNotFound { definition: String, sub_network: String },
    #[error(
        "The definition `{definition}` selects the network set `{set}`, but its sub-network `{sub_network}` is not active; list it in the model's `sub_networks` or use it as a network definition's `base`."
    )]
    SubNetworkNotActive {
        definition: String,
        set: String,
        sub_network: String,
    },
    #[error("The network definitions extend each other in a cycle: {}.", .cycle.join(" -> "))]
    NetworkCycle { cycle: Vec<String> },
    #[error(
        "The model definition `{definition}` has no base model; give one in the definition or at the top of the manifest."
    )]
    BaseModelMissing { definition: String },
    #[error(
        "The definition `{definition}` selects the network set `{set}` {count} times, but it may select each set at most once."
    )]
    DuplicateSet {
        definition: String,
        set: String,
        count: usize,
    },
    #[error("The definition `{definition}` selects the network set `{set}`, which does not exist.")]
    SetNotFound { definition: String, set: String },
    #[error("The definition `{definition}` selects the file `{file}`, which was not found in network set `{set}`.")]
    FileNotFound {
        definition: String,
        set: String,
        file: String,
    },
    #[error("The definition `{definition}` selects the file `{file}` more than once in network set `{set}`.")]
    DuplicateFile {
        definition: String,
        set: String,
        file: String,
    },
    #[error(
        "The definition `{definition}` gives metadata for file `{file}` in network set `{set}`, but that file is not selected."
    )]
    UnusedFileMeta {
        definition: String,
        set: String,
        file: String,
    },
    #[error(
        "The definition `{definition}` selects the invalid filename `{file}` in network set `{set}`; filenames must be single path components."
    )]
    InvalidFilePath {
        definition: String,
        set: String,
        file: String,
    },
    #[error(
        "The definition `{definition}` selects {actual_files} file(s) from network set `{set}`, but at least {min_files} are required."
    )]
    MinFilesNotMet {
        definition: String,
        set: String,
        min_files: usize,
        actual_files: usize,
    },
    #[error(
        "The definition `{definition}` selects {actual_files} file(s) from network set `{set}`, but at most {max_files} are allowed."
    )]
    MaxFilesExceeded {
        definition: String,
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
    #[error(transparent)]
    Resolution(#[from] ManifestResolutionError),
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
    #[serde(default)]
    pub network_sets: Vec<NetworkSet>,
    /// Named, reusable compositions of network fragments.
    #[serde(default)]
    pub networks: Vec<NetworkDefinition>,
    /// Named model definitions.
    #[serde(default)]
    pub models: Vec<ModelDefinition>,
}

impl ProjectManifest {
    /// Validate the project manifest and return a report of all structural problems found.
    pub fn validate(&self, root: &Path) -> Result<(), ProjectManifestValidationError> {
        let mut problems = self.manifest_problems();
        let base_model = self.resolve_base_model(root, &mut problems);
        let project = self.resolve_project(root, &mut problems);
        let mut networks = NetworkExpander::new(self, &project);
        for model in &self.models {
            model.resolve(root, &base_model, &mut networks, &mut problems);
        }
        // Network definitions that no model uses are still validated; each is reported once.
        // Sub-network activation is not checked here, because a model may activate it instead.
        for network in &self.networks {
            networks.expand(&network.name, &mut problems);
        }
        into_result(problems)
    }

    /// Validate a model definition against the project manifest.
    pub fn validate_model(&self, root: &Path, definition_name: &str) -> Result<(), ProjectManifestValidationError> {
        let Some(model) = self.find_model(definition_name) else {
            return into_result(vec![ProjectManifestProblem::DefinitionNotFound {
                definition: definition_name.to_string(),
            }]);
        };
        let mut problems = self.manifest_problems();
        let base_model = self.resolve_base_model(root, &mut problems);
        let project = self.resolve_project(root, &mut problems);
        model.resolve(
            root,
            &base_model,
            &mut NetworkExpander::new(self, &project),
            &mut problems,
        );
        into_result(problems)
    }

    /// Validate a network definition, composed on its own, against the project manifest.
    pub fn validate_network(&self, root: &Path, network_name: &str) -> Result<(), ProjectManifestValidationError> {
        if self.find_network(network_name).is_none() {
            return into_result(vec![ProjectManifestProblem::NetworkDefinitionNotFound {
                network: network_name.to_string(),
            }]);
        }
        let mut problems = self.manifest_problems();
        let project = self.resolve_project(root, &mut problems);
        let expansion = NetworkExpander::new(self, &project).expand(network_name, &mut problems);
        check_active(network_name, &expansion, &mut problems);
        into_result(problems)
    }

    /// Compose a model from its base model and the specified model definition.
    ///
    /// This resolves the manifest in the same way as [`Self::validate_model`], and returns the
    /// first problem found as an error.
    pub fn compose_model(&self, root: &Path, definition_name: &str) -> Result<ComposedModel, ComposeModelError> {
        let mut problems = self.manifest_problems();
        let Some(model) = self.find_model(definition_name) else {
            problems.push(ProjectManifestProblem::DefinitionNotFound {
                definition: definition_name.to_string(),
            });
            return Err(problems.swap_remove(0).into());
        };
        let base_model = self.resolve_base_model(root, &mut problems);
        let project = self.resolve_project(root, &mut problems);
        let resolved = model.resolve(
            root,
            &base_model,
            &mut NetworkExpander::new(self, &project),
            &mut problems,
        );

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

    /// Compose a network from the specified network definition.
    ///
    /// This resolves the manifest in the same way as [`Self::validate_network`], and returns the
    /// first problem found as an error.
    pub fn compose_network(&self, root: &Path, network_name: &str) -> Result<ComposedNetwork, ComposeModelError> {
        let mut problems = self.manifest_problems();
        if self.find_network(network_name).is_none() {
            problems.push(ProjectManifestProblem::NetworkDefinitionNotFound {
                network: network_name.to_string(),
            });
            return Err(problems.swap_remove(0).into());
        }
        let project = self.resolve_project(root, &mut problems);
        let expansion = NetworkExpander::new(self, &project).expand(network_name, &mut problems);
        check_active(network_name, &expansion, &mut problems);

        match problems.into_iter().next() {
            Some(problem) => Err(problem.into()),
            None => Ok(ComposedNetwork::new(
                network_name.to_string(),
                expansion.files.into_iter().map(SelectedFile::into_composed).collect(),
            )),
        }
    }

    fn find_model(&self, name: &str) -> Option<&ModelDefinition> {
        self.models.iter().find(|model| model.name == name)
    }

    fn find_network(&self, name: &str) -> Option<&NetworkDefinition> {
        self.networks.iter().find(|network| network.name == name)
    }

    /// Problems in the manifest itself, found without touching the filesystem.
    fn manifest_problems(&self) -> Vec<ProjectManifestProblem> {
        let mut problems = Vec::new();
        add_duplicate_problems(
            &self.models,
            |model| &model.name,
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
                self.networks
                    .iter()
                    .map(|network| ("network definition", &network.name)),
            )
            .chain(
                self.sub_networks
                    .iter()
                    .map(|sub_network| ("sub-network", &sub_network.name)),
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
    fn resolve_project(&self, root: &Path, problems: &mut Vec<ProjectManifestProblem>) -> ResolvedProject {
        let mut project = ResolvedProject::default();
        for set in &self.network_sets {
            project.add_set(&set.name, resolve_network_set(root, &set.name, set, None), problems);
        }
        for sub_network in &self.sub_networks {
            match resolve_sub_network(root, sub_network) {
                Ok((resolved, scope)) => {
                    for set in &sub_network.network_sets {
                        let name = qualified_name(&sub_network.name, &set.name);
                        let resolved_set = resolve_network_set(&scope.root, &name, set, Some(&scope));
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
        project
    }
}

/// Separates a sub-network's name from the name of one of its network sets, as in `upper::options`.
const NAME_SEPARATOR: &str = "::";

fn qualified_name(sub_network: &str, name: &str) -> String {
    format!("{sub_network}{NAME_SEPARATOR}{name}")
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

    fn sub_network(&self, name: &str) -> Option<&ResolvedSubNetwork> {
        self.sub_networks.iter().find(|sub_network| sub_network.name == name)
    }

    /// Look up a sub-network that a definition refers to, reporting it if it does not exist.
    ///
    /// Returns `None` both when it does not exist and when it failed to resolve.
    fn referred_sub_network(
        &self,
        definition: &str,
        name: &str,
        problems: &mut Vec<ProjectManifestProblem>,
    ) -> Option<&ResolvedSubNetwork> {
        let sub_network = self.sub_network(name);
        if sub_network.is_none() && !self.unresolved_sub_networks.contains(name) {
            problems.push(ProjectManifestProblem::SubNetworkNotFound {
                definition: definition.to_string(),
                sub_network: name.to_string(),
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
    fn into_composed(self) -> ComposedNetworkPath {
        ComposedNetworkPath {
            path: self.file.path.clone(),
            position_offset: self.position_offset,
            source: ProvenanceSource {
                file: self.file.source.clone(),
                network_set: self.set.map(|set| set.set_name.clone()),
                sub_network: self.file.sub_network.clone(),
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
    /// Sub-networks made active by the `base` of a network definition in the expansion.
    active_sub_networks: Vec<&'a str>,
}

impl<'a> Expansion<'a> {
    fn extend(&mut self, other: Expansion<'a>) {
        self.files.extend(other.files);
        self.active_sub_networks.extend(other.active_sub_networks);
    }

    /// Remove files reached by more than one route, keeping the first occurrence.
    fn dedup(&mut self) {
        let mut seen = HashSet::new();
        self.files.retain(|selected| seen.insert(&selected.file.path));
        let mut seen = HashSet::new();
        self.active_sub_networks.retain(|name| seen.insert(*name));
    }
}

/// Report each network set selected from a sub-network that the expansion does not activate.
fn check_active(definition: &str, expansion: &Expansion<'_>, problems: &mut Vec<ProjectManifestProblem>) {
    let mut reported = HashSet::new();
    for set in expansion.files.iter().filter_map(|selected| selected.set) {
        if let Some(sub_network) = &set.sub_network
            && !expansion.active_sub_networks.contains(&sub_network.as_str())
            && reported.insert(&set.name)
        {
            problems.push(ProjectManifestProblem::SubNetworkNotActive {
                definition: definition.to_string(),
                set: set.name.clone(),
                sub_network: sub_network.clone(),
            });
        }
    }
}

/// Expands network definitions into their selected files, remembering each expansion so that a
/// definition's problems are reported once however many definitions refer to it.
struct NetworkExpander<'a> {
    manifest: &'a ProjectManifest,
    project: &'a ResolvedProject,
    expanded: HashMap<&'a str, Expansion<'a>>,
    /// The definitions currently being expanded, outermost first, used to detect cycles.
    stack: Vec<&'a str>,
}

impl<'a> NetworkExpander<'a> {
    fn new(manifest: &'a ProjectManifest, project: &'a ResolvedProject) -> Self {
        Self {
            manifest,
            project,
            expanded: HashMap::new(),
            stack: Vec::new(),
        }
    }

    /// Expand the network definition `name` into its files, in merge order: its base
    /// sub-network's base network, those of each definition it extends, then its own selections.
    ///
    /// The caller must check that `name` exists; a missing name expands to nothing.
    fn expand(&mut self, name: &str, problems: &mut Vec<ProjectManifestProblem>) -> Expansion<'a> {
        if let Some(expansion) = self.expanded.get(name) {
            return expansion.clone();
        }
        if let Some(start) = self.stack.iter().position(|entry| *entry == name) {
            let mut cycle: Vec<String> = self.stack[start..].iter().map(|entry| entry.to_string()).collect();
            cycle.push(name.to_string());
            problems.push(ProjectManifestProblem::NetworkCycle { cycle });
            return Expansion::default();
        }
        let Some(network) = self.manifest.find_network(name) else {
            return Expansion::default();
        };

        self.stack.push(&network.name);
        let mut expansion = Expansion::default();
        if let Some(base) = &network.base
            && let Some(sub_network) = self.project.referred_sub_network(&network.name, base, problems)
        {
            expansion.active_sub_networks.push(&sub_network.name);
            if let Some(file) = &sub_network.base_network {
                expansion.files.push(SelectedFile {
                    file,
                    set: None,
                    position_offset: None,
                    network: Some(&network.name),
                });
            }
        }
        for parent in &network.extends {
            if self.manifest.find_network(parent).is_none() {
                problems.push(ProjectManifestProblem::NetworkNotFound {
                    definition: network.name.clone(),
                    network: parent.clone(),
                });
            }
            let parent = self.expand(parent, problems);
            expansion.extend(parent);
        }
        expansion.files.extend(resolve_includes(
            &network.name,
            Some(&network.name),
            &network.include,
            self.project,
            problems,
        ));
        self.stack.pop();

        expansion.dedup();
        self.expanded.insert(&network.name, expansion.clone());
        expansion
    }
}

/// Resolve a definition's own `include` list, appending problems attributed to `definition`.
///
/// Returns the files selected, in selection order.
fn resolve_includes<'a>(
    definition: &str,
    network: Option<&'a str>,
    include: &'a [DefinitionSelection],
    project: &'a ResolvedProject,
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
                    definition: definition.to_string(),
                    set: selection.set.clone(),
                    count: include.iter().filter(|other| other.set == selection.set).count(),
                });
            }
            continue;
        }
        let Some(set) = project.set(&selection.set) else {
            if !project.unresolved_sets.contains(&selection.set) {
                problems.push(ProjectManifestProblem::SetNotFound {
                    definition: definition.to_string(),
                    set: selection.set.clone(),
                });
            }
            continue;
        };

        let selected = resolve_selection(definition, selection, set, problems);
        for file in unused_file_meta_keys(selection, &selected) {
            problems.push(ProjectManifestProblem::UnusedFileMeta {
                definition: definition.to_string(),
                set: set.name.clone(),
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
            ProjectManifestProblem::Resolution(error) => Self::Resolution(error),
        }
    }
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
    #[serde(default)]
    pub network_sets: Vec<NetworkSet>,
}

/// A named, reusable composition of network fragments that resolves to a `NetworkSchema`.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetworkDefinition {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// A sub-network whose base network is the starting point. This also activates the sub-network.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// Other network definitions to include next, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extends: Vec<String>,
    /// Per-network-set selections for this network definition.
    #[serde(default)]
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
    #[serde(default)]
    pub include: Vec<DefinitionSelection>,
    /// Optional overrides to base model.
    #[serde(default)]
    pub overrides: Option<DefinitionOverrides>,
}

/// A model definition resolved into its base model and the networks to include in merge order.
struct ResolvedModel {
    base_model: Option<ResolvedBaseModel>,
    includes: Vec<ComposedNetworkPath>,
}

impl ModelDefinition {
    /// Resolve this definition's base model and selections.
    ///
    /// Problems are appended in the order the definition is composed, so the first one is the
    /// error that composition reports.
    fn resolve<'a>(
        &'a self,
        root: &Path,
        project_base_model: &ProjectBaseModel,
        networks: &mut NetworkExpander<'a>,
        problems: &mut Vec<ProjectManifestProblem>,
    ) -> ResolvedModel {
        let base_model = match (&self.base_model, project_base_model) {
            (Some(base_model), _) => match resolve_base_model(root, base_model) {
                Ok(base_model) => Some(base_model),
                Err(error) => {
                    problems.push(error.into());
                    None
                }
            },
            (None, ProjectBaseModel::Resolved(base_model)) => Some(base_model.clone()),
            (None, ProjectBaseModel::Unresolved) => None,
            (None, ProjectBaseModel::NotGiven) => {
                problems.push(ProjectManifestProblem::BaseModelMissing {
                    definition: self.name.clone(),
                });
                None
            }
        };

        let project = networks.project;
        let mut expansion = Expansion::default();
        for name in &self.sub_networks {
            if let Some(sub_network) = project.referred_sub_network(&self.name, name, problems) {
                expansion.active_sub_networks.push(&sub_network.name);
            }
        }
        // The base networks of the sub-networks activated here come straight after the base model.
        for sub_network in &project.sub_networks {
            if let Some(file) = &sub_network.base_network
                && expansion.active_sub_networks.contains(&sub_network.name.as_str())
            {
                expansion.files.push(SelectedFile {
                    file,
                    set: None,
                    position_offset: None,
                    network: None,
                });
            }
        }
        for network in &self.networks {
            if networks.manifest.find_network(network).is_none() {
                problems.push(ProjectManifestProblem::NetworkNotFound {
                    definition: self.name.clone(),
                    network: network.clone(),
                });
            }
            expansion.extend(networks.expand(network, problems));
        }
        expansion
            .files
            .extend(resolve_includes(&self.name, None, &self.include, project, problems));
        expansion.dedup();
        check_active(&self.name, &expansion, problems);

        // Set constraints apply to everything the model selects from a set, by any route. A
        // sub-network's sets are only constrained when the sub-network is active.
        for set in &project.sets {
            if let Some(sub_network) = &set.sub_network
                && !expansion.active_sub_networks.contains(&sub_network.as_str())
            {
                continue;
            }
            let count = expansion
                .files
                .iter()
                .filter(|selected| selected.set.is_some_and(|selected_set| std::ptr::eq(selected_set, set)))
                .count();
            validate_constraints(&self.name, set, count, problems);
        }

        ResolvedModel {
            base_model,
            includes: expansion.files.into_iter().map(SelectedFile::into_composed).collect(),
        }
    }
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
    pub dir: Option<String>,
    /// Minimum number of files required from this set (inclusive). Default: 0.
    #[serde(default, alias = "min")]
    pub min_files: Option<usize>,
    /// Maximum number of files allowed from this set (inclusive). Default: unlimited.
    #[serde(default, alias = "max")]
    pub max_files: Option<usize>,
}

struct ResolvedNetworkSet {
    /// The set's name as definitions refer to it, qualified with its sub-network's name if any.
    name: String,
    /// The set's name within its sub-network or project, as recorded in provenance.
    set_name: String,
    sub_network: Option<String>,
    min_files: Option<usize>,
    max_files: Option<usize>,
    files: Vec<ResolvedNetworkFile>,
}

struct ResolvedNetworkFile {
    name: OsString,
    path: PathBuf,
    /// The file's path relative to the project root.
    source: RelativePathBuf,
    /// The sub-network the file belongs to, if any.
    sub_network: Option<String>,
    /// Directory, relative to the project root, that the file's relative data paths are relative
    /// to, or `None` if they are already relative to the project root.
    data_dir: Option<RelativePathBuf>,
}

struct ResolvedSubNetwork {
    name: String,
    base_network: Option<ResolvedNetworkFile>,
}

/// Where a sub-network's files live, used to resolve its network sets.
struct SubNetworkScope {
    name: String,
    /// The sub-network directory relative to the project root.
    dir: RelativePathBuf,
    /// The canonical path of the sub-network directory.
    root: PathBuf,
    /// The sub-network's data directory relative to the project root.
    data_dir: RelativePathBuf,
}

fn resolve_selection<'a>(
    definition: &str,
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
                definition: definition.to_string(),
                set: set.name.clone(),
                file: file.clone(),
            });
        } else if !names.insert(file) {
            errors.push(ProjectManifestProblem::DuplicateFile {
                definition: definition.to_string(),
                set: set.name.clone(),
                file: file.clone(),
            });
        } else if let Some(resolved) = set.files.iter().find(|candidate| candidate.name == OsStr::new(file)) {
            result.push(resolved);
        } else {
            errors.push(ProjectManifestProblem::FileNotFound {
                definition: definition.to_string(),
                set: set.name.clone(),
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
    definition: &str,
    set: &ResolvedNetworkSet,
    count: usize,
    errors: &mut Vec<ProjectManifestProblem>,
) {
    if let Some(min_files) = set.min_files
        && count < min_files
    {
        errors.push(ProjectManifestProblem::MinFilesNotMet {
            definition: definition.to_string(),
            set: set.name.clone(),
            min_files,
            actual_files: count,
        });
    }
    if let Some(max_files) = set.max_files
        && count > max_files
    {
        errors.push(ProjectManifestProblem::MaxFilesExceeded {
            definition: definition.to_string(),
            set: set.name.clone(),
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
        Some(data_dir) => dir.join(strict_relative_path(
            &format!("sub-network '{name}' data directory"),
            data_dir,
        )?),
        None => dir.clone(),
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
                data_dir: Some(data_dir.clone()),
            })
        }
        None => None,
    };

    Ok((
        ResolvedSubNetwork {
            name: name.clone(),
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
/// `name` is the set's name as definitions refer to it. `scope` is the sub-network the set
/// belongs to, if any, in which case `root` is the sub-network directory.
fn resolve_network_set(
    root: &Path,
    name: &str,
    set: &NetworkSet,
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
            data_dir: scope.map(|scope| scope.data_dir.clone()),
        });
    }
    files.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(ResolvedNetworkSet {
        name: name.to_string(),
        set_name: set.name.clone(),
        sub_network: scope.map(|scope| scope.name.clone()),
        min_files: set.min_files,
        max_files: set.max_files,
        files,
    })
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
                project.validate(root.path()).unwrap_err(),
                project.validate_model(root.path(), "test").unwrap_err(),
            ] {
                assert!(report.problems.iter().any(|error| matches!(
                    error,
                    ProjectManifestProblem::UnusedFileMeta { definition, set, file }
                        if definition == "test" && set == "nets" && file == unused
                )));
            }
            assert!(matches!(
                project.compose_model(root.path(), "test"),
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
            assert!(project.validate(root.path()).is_ok());
            let composed = project.compose_model(root.path(), "test").unwrap();
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
            .compose_model(root.path(), "test")
            .unwrap()
            .load()
            .unwrap()
            .into_model_schema(&Default::default())
            .unwrap();
        let base_source = ProvenanceSource {
            file: "base.json".into(),
            network_set: None,
            sub_network: None,
            network: None,
        };
        let a_source = ProvenanceSource {
            file: "nets/a.json".into(),
            network_set: Some("nets".into()),
            sub_network: None,
            network: None,
        };
        let b_source = ProvenanceSource {
            file: "nets/b.json".into(),
            network_set: Some("nets".into()),
            sub_network: None,
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
            .compose_model(root.path(), "test")
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
                .validate(root.path())
                .unwrap_err()
                .problems
                .iter()
                .any(|error| matches!(
                    error,
                    ProjectManifestProblem::Resolution(ManifestResolutionError::InvalidRelativePath { field, .. }) if field == "network set 'nets' file"
                ))
        );
        assert!(matches!(
            project.compose_model(root.path(), "test"),
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
        let report = manifest.validate(root.path()).unwrap_err();
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

        let report = manifest.validate_model(root.path(), "test").unwrap_err();
        assert!(matches!(
            report.problems.as_slice(),
            [
                ProjectManifestProblem::FileNotFound { set, .. },
                ProjectManifestProblem::DuplicateSet { count: 2, .. }
            ] if set == "b"
        ));
        assert!(matches!(
            manifest.compose_model(root.path(), "test"),
            Err(ComposeModelError::FileNotFound { set, file }) if set == "b" && file == "missing.json"
        ));
    }

    fn network(name: &str, extends: Vec<&str>, include: Vec<DefinitionSelection>) -> NetworkDefinition {
        NetworkDefinition {
            name: name.to_string(),
            description: None,
            base: None,
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
        assert!(manifest.validate(root.path()).is_ok());

        let composed = manifest.compose_model(root.path(), "test").unwrap();
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

        let network = manifest.compose_network(root.path(), "upgraded").unwrap();
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
        assert!(manifest.validate(root.path()).is_ok());
        assert_eq!(
            manifest.compose_model(root.path(), "test").unwrap().all_paths(),
            net_paths(root.path(), &["base.json", "nets/a.json", "nets/b.json"])
        );
        assert_eq!(
            manifest.compose_network(root.path(), "diamond").unwrap().all_paths(),
            net_paths(root.path(), &["nets/a.json", "nets/b.json"])
        );
    }

    #[test]
    fn set_constraints_apply_to_the_whole_model_not_each_network() {
        let root = project_with_nets();
        let manifest = ProjectManifest {
            base_model: Some("base.json".to_string()),
            sub_networks: vec![],
            network_sets: vec![set("nets", None, None, Some(1))],
            networks: vec![
                network("left", vec![], vec![selection("nets", Some(vec!["a.json"]), false)]),
                network("right", vec![], vec![selection("nets", Some(vec!["b.json"]), false)]),
            ],
            models: vec![uses_networks(model("test", vec![]), vec!["left", "right"])],
        };
        assert!(manifest.validate_network(root.path(), "left").is_ok());
        assert!(matches!(
            manifest.validate_model(root.path(), "test").unwrap_err().problems.as_slice(),
            [ProjectManifestProblem::MaxFilesExceeded { definition, actual_files: 2, .. }] if definition == "test"
        ));
        assert!(matches!(
            manifest.compose_model(root.path(), "test"),
            Err(ComposeModelError::MaxFilesExceeded { actual_files: 2, .. })
        ));
    }

    #[test]
    fn network_cycles_are_reported_once() {
        let root = project_with_nets();
        let manifest = ProjectManifest {
            base_model: Some("base.json".to_string()),
            sub_networks: vec![],
            network_sets: vec![set("nets", None, None, None)],
            networks: vec![
                network("first", vec!["second"], vec![]),
                network("second", vec!["first"], vec![]),
            ],
            models: vec![uses_networks(model("test", vec![]), vec!["first"])],
        };
        let report = manifest.validate(root.path()).unwrap_err();
        assert!(matches!(
            report.problems.as_slice(),
            [ProjectManifestProblem::NetworkCycle { cycle }] if cycle == &["first", "second", "first"]
        ));
        assert_eq!(
            report.problems[0].to_string(),
            "The network definitions extend each other in a cycle: first -> second -> first."
        );
        assert!(matches!(
            manifest.compose_network(root.path(), "second"),
            Err(ComposeModelError::NetworkCycle { cycle }) if cycle == ["second", "first", "second"]
        ));
    }

    #[test]
    fn missing_network_references_are_reported_where_they_are_made() {
        let root = project_with_nets();
        let manifest = ProjectManifest {
            base_model: Some("base.json".to_string()),
            sub_networks: vec![],
            network_sets: vec![set("nets", None, None, None)],
            networks: vec![network("child", vec!["gone"], vec![])],
            models: vec![uses_networks(model("test", vec![]), vec!["nope"])],
        };
        let report = manifest.validate(root.path()).unwrap_err();
        let missing: Vec<_> = report
            .problems
            .iter()
            .map(|problem| match problem {
                ProjectManifestProblem::NetworkNotFound { definition, network } => {
                    (definition.as_str(), network.as_str())
                }
                other => panic!("unexpected problem: {other}"),
            })
            .collect();
        assert_eq!(missing, [("test", "nope"), ("child", "gone")]);
        assert!(matches!(
            manifest.compose_network(root.path(), "unknown"),
            Err(ComposeModelError::NetworkDefinitionNotFound { network }) if network == "unknown"
        ));
    }

    #[test]
    fn network_problems_are_reported_once_however_many_models_use_them() {
        let root = project_with_nets();
        let manifest = ProjectManifest {
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
            manifest.validate(root.path()).unwrap_err().problems.as_slice(),
            [ProjectManifestProblem::FileNotFound { definition, .. }] if definition == "shared"
        ));
    }

    #[test]
    fn base_model_comes_from_the_definition_or_the_project() {
        let root = project_with_nets();
        std::fs::copy(root.path().join("base.json"), root.path().join("other.json")).unwrap();
        let mut with_base = model("with-base", vec![]);
        with_base.base_model = Some("other.json".to_string());
        let manifest = ProjectManifest {
            base_model: None,
            sub_networks: vec![],
            network_sets: vec![set("nets", None, None, None)],
            networks: vec![network("all", vec![], vec![selection("nets", None, true)])],
            models: vec![with_base, model("without-base", vec![])],
        };

        assert!(matches!(
            manifest.validate(root.path()).unwrap_err().problems.as_slice(),
            [ProjectManifestProblem::BaseModelMissing { definition }] if definition == "without-base"
        ));
        assert_eq!(
            manifest.compose_model(root.path(), "with-base").unwrap().all_paths(),
            net_paths(root.path(), &["other.json"])
        );
        assert!(matches!(
            manifest.compose_model(root.path(), "without-base"),
            Err(ComposeModelError::BaseModelMissing { definition }) if definition == "without-base"
        ));
        assert_eq!(
            manifest.compose_network(root.path(), "all").unwrap().all_paths().len(),
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
        assert!(manifest.validate(root.path()).is_ok());

        let composed = manifest.compose_model(root.path(), "test").unwrap();
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
                network: None,
            }
        );
        assert_eq!(
            origin("lower"),
            ProvenanceSource {
                file: "lower/base.json".into(),
                network_set: None,
                sub_network: Some("lower".into()),
                network: None,
            }
        );
    }

    #[test]
    fn sub_networks_are_inactive_unless_listed_or_used_as_a_base() {
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
                    base: Some("upper".to_string()),
                    ..network("with-base", vec![], vec![upper_x()])
                },
            ],
        );

        assert!(matches!(
            manifest.validate(root.path()).unwrap_err().problems.as_slice(),
            [ProjectManifestProblem::SubNetworkNotActive { definition, set, sub_network }]
                if definition == "inactive" && set == "upper::options" && sub_network == "upper"
        ));
        assert!(matches!(
            manifest.compose_model(root.path(), "inactive"),
            Err(ComposeModelError::SubNetworkNotActive { .. })
        ));
        assert_eq!(
            manifest.compose_model(root.path(), "via-base").unwrap().all_paths(),
            net_paths(root.path(), &["base.json", "upper/base.json", "upper/options/x.json"])
        );
        assert!(manifest.validate_network(root.path(), "with-base").is_ok());
        assert!(matches!(
            manifest.compose_network(root.path(), "upper-x"),
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

        assert!(manifest.validate_model(root.path(), "without-upper").is_ok());
        assert!(matches!(
            manifest.validate_model(root.path(), "with-upper").unwrap_err().problems.as_slice(),
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
            base: Some("upper".to_string()),
            ..network("upper", vec![], vec![])
        }];

        let schema = manifest
            .compose_network(root.path(), "upper")
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
            .compose_model(root.path(), "test")
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

        let report = manifest.validate(root.path()).unwrap_err();
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
        let mut manifest = sub_network_manifest(vec![], vec![]);
        manifest.network_sets.push(set("a::b", Some("upper"), None, None));
        assert!(matches!(
            manifest.validate(root.path()).unwrap_err().problems.as_slice(),
            [ProjectManifestProblem::InvalidName { kind: "network set", name }] if name == "a::b"
        ));
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

        let report = manifest.validate(root.path()).unwrap_err();
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

        let report = manifest.validate(root.path()).unwrap_err();
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
        let report = manifest.validate(root.path()).unwrap_err();
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
            matches!(manifest.compose_model(root.path(), "test"), Err(ComposeModelError::Resolution(ManifestResolutionError::InvalidRelativePath { field, .. })) if field == "base model")
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

        let report = manifest.validate(root.path()).unwrap_err();
        assert!(report.problems.iter().any(|error| matches!(
            error,
            ProjectManifestProblem::InvalidFilePath { file, .. } if file == "../outside.json"
        )));
        assert!(matches!(
            manifest.compose_model(root.path(), "test"),
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
        let composed = project_manifest.compose_model(root.path(), "test").unwrap();
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
            duplicate.compose_model(root.path(), "test"),
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
        let report = manifest.validate(root.path()).unwrap_err();
        assert!(report.problems.iter().any(|error| matches!(
            error,
            ProjectManifestProblem::Resolution(ManifestResolutionError::PathEscapesRoot { .. })
        )));
        assert!(matches!(
            manifest.compose_model(root.path(), "test"),
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
            base_model: Some("base.json".to_string()),
            sub_networks: vec![],
            network_sets: vec![set("nets", None, Some(2), Some(1)), set("nets", None, None, None)],
            networks: vec![],
            models: vec![model("test", vec![]), model("test", vec![])],
        };
        let report = manifest.validate(root.path()).unwrap_err();
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
}
