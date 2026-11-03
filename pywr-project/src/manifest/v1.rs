use crate::composition::{ComposedModel, ComposedModelBuilder, ComposedNetwork, ComposedNetworkPath, PositionOffset};
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
    #[error("The definition `{definition}` refers to the network definition `{network}`, which does not exist.")]
    NetworkNotFound { definition: String, network: String },
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
        let sets = self.resolve_sets(root, &mut problems);
        let mut networks = NetworkExpander::new(self, &sets);
        for model in &self.models {
            model.resolve(root, &base_model, &mut networks, &mut problems);
        }
        // Network definitions that no model uses are still validated; each is reported once.
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
        let sets = self.resolve_sets(root, &mut problems);
        model.resolve(root, &base_model, &mut NetworkExpander::new(self, &sets), &mut problems);
        into_result(problems)
    }

    /// Validate a network definition against the project manifest.
    pub fn validate_network(&self, root: &Path, network_name: &str) -> Result<(), ProjectManifestValidationError> {
        if self.find_network(network_name).is_none() {
            return into_result(vec![ProjectManifestProblem::NetworkDefinitionNotFound {
                network: network_name.to_string(),
            }]);
        }
        let mut problems = self.manifest_problems();
        let sets = self.resolve_sets(root, &mut problems);
        NetworkExpander::new(self, &sets).expand(network_name, &mut problems);
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
        let sets = self.resolve_sets(root, &mut problems);
        let resolved = model.resolve(root, &base_model, &mut NetworkExpander::new(self, &sets), &mut problems);

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
        let sets = self.resolve_sets(root, &mut problems);
        let files = NetworkExpander::new(self, &sets).expand(network_name, &mut problems);

        match problems.into_iter().next() {
            Some(problem) => Err(problem.into()),
            None => Ok(ComposedNetwork::new(
                network_name.to_string(),
                files.into_iter().map(SelectedFile::into_composed).collect(),
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
            &self.network_sets,
            |set| &set.name,
            |name, count| ProjectManifestProblem::DuplicateNetworkSet { set: name, count },
            &mut problems,
        );
        for set in &self.network_sets {
            if let (Some(min_files), Some(max_files)) = (set.min_files, set.max_files)
                && min_files > max_files
            {
                problems.push(ProjectManifestProblem::InvalidFileConstraints {
                    set: set.name.clone(),
                    min_files,
                    max_files,
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

    /// Resolve the network sets against the filesystem, appending any problems found.
    fn resolve_sets(&self, root: &Path, problems: &mut Vec<ProjectManifestProblem>) -> ResolvedSets {
        let mut sets = Vec::new();
        let mut unresolved = HashSet::new();
        for set in &self.network_sets {
            match resolve_network_set(root, set) {
                Ok(resolved) => sets.push(resolved),
                Err(error) => {
                    problems.push(error.into());
                    unresolved.insert(set.name.clone());
                }
            }
        }
        ResolvedSets { sets, unresolved }
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

/// The network sets of a manifest resolved against the filesystem, shared by every definition.
struct ResolvedSets {
    /// Network sets that resolved, in declaration order.
    sets: Vec<ResolvedNetworkSet>,
    /// Names of network sets that failed to resolve. Their problems are already reported, so
    /// selections of them are not reported again as missing sets.
    unresolved: HashSet<String>,
}

impl ResolvedSets {
    fn get(&self, name: &str) -> Option<&ResolvedNetworkSet> {
        self.sets.iter().find(|set| set.name == name)
    }
}

/// A network fragment selected by a definition, before composition.
#[derive(Clone)]
struct SelectedFile<'a> {
    set: &'a ResolvedNetworkSet,
    file: &'a ResolvedNetworkFile,
    position_offset: Option<PositionOffset>,
    /// The network definition whose `include` selected this file, if any.
    network: Option<&'a str>,
}

impl SelectedFile<'_> {
    fn into_composed(self) -> ComposedNetworkPath {
        ComposedNetworkPath {
            path: self.file.path.clone(),
            position_offset: self.position_offset,
            source: ProvenanceSource {
                file: self.file.source.clone(),
                network_set: Some(self.set.name.clone()),
                network: self.network.map(str::to_string),
            },
        }
    }
}

/// Remove files reached by more than one route, keeping the first occurrence.
fn dedup_files(files: &mut Vec<SelectedFile<'_>>) {
    let mut seen = HashSet::new();
    files.retain(|selected| seen.insert(&selected.file.path));
}

/// Expands network definitions into their selected files, remembering each expansion so that a
/// definition's problems are reported once however many definitions refer to it.
struct NetworkExpander<'a> {
    manifest: &'a ProjectManifest,
    sets: &'a ResolvedSets,
    expanded: HashMap<&'a str, Vec<SelectedFile<'a>>>,
    /// The definitions currently being expanded, outermost first, used to detect cycles.
    stack: Vec<&'a str>,
}

impl<'a> NetworkExpander<'a> {
    fn new(manifest: &'a ProjectManifest, sets: &'a ResolvedSets) -> Self {
        Self {
            manifest,
            sets,
            expanded: HashMap::new(),
            stack: Vec::new(),
        }
    }

    /// The files selected by the network definition `name`, in merge order: those of each
    /// definition it extends, then its own selections.
    ///
    /// The caller must check that `name` exists; a missing name expands to no files.
    fn expand(&mut self, name: &str, problems: &mut Vec<ProjectManifestProblem>) -> Vec<SelectedFile<'a>> {
        if let Some(files) = self.expanded.get(name) {
            return files.clone();
        }
        if let Some(start) = self.stack.iter().position(|entry| *entry == name) {
            let mut cycle: Vec<String> = self.stack[start..].iter().map(|entry| entry.to_string()).collect();
            cycle.push(name.to_string());
            problems.push(ProjectManifestProblem::NetworkCycle { cycle });
            return Vec::new();
        }
        let Some(network) = self.manifest.find_network(name) else {
            return Vec::new();
        };

        self.stack.push(&network.name);
        let mut files = Vec::new();
        for parent in &network.extends {
            if self.manifest.find_network(parent).is_none() {
                problems.push(ProjectManifestProblem::NetworkNotFound {
                    definition: network.name.clone(),
                    network: parent.clone(),
                });
            }
            files.extend(self.expand(parent, problems));
        }
        files.extend(resolve_includes(
            &network.name,
            Some(&network.name),
            &network.include,
            self.sets,
            problems,
        ));
        self.stack.pop();

        dedup_files(&mut files);
        self.expanded.insert(&network.name, files.clone());
        files
    }
}

/// Resolve a definition's own `include` list, appending problems attributed to `definition`.
///
/// Returns the files selected, in selection order.
fn resolve_includes<'a>(
    definition: &str,
    network: Option<&'a str>,
    include: &'a [DefinitionSelection],
    sets: &'a ResolvedSets,
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
        let Some(set) = sets.get(&selection.set) else {
            if !sets.unresolved.contains(&selection.set) {
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
                set,
                file,
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
            ProjectManifestProblem::NetworkNotFound { definition, network } => {
                Self::NetworkNotFound { definition, network }
            }
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

/// A named, reusable composition of network fragments that resolves to a `NetworkSchema`.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetworkDefinition {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Other network definitions to include first, in order.
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

        let mut files = Vec::new();
        for network in &self.networks {
            if networks.manifest.find_network(network).is_none() {
                problems.push(ProjectManifestProblem::NetworkNotFound {
                    definition: self.name.clone(),
                    network: network.clone(),
                });
            }
            files.extend(networks.expand(network, problems));
        }
        files.extend(resolve_includes(
            &self.name,
            None,
            &self.include,
            networks.sets,
            problems,
        ));
        dedup_files(&mut files);

        // Set constraints apply to the whole model: every file selected from a set, by any route.
        for set in &networks.sets.sets {
            let count = files.iter().filter(|selected| std::ptr::eq(selected.set, set)).count();
            validate_constraints(&self.name, set, count, problems);
        }

        ResolvedModel {
            base_model,
            includes: files.into_iter().map(SelectedFile::into_composed).collect(),
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
    name: String,
    min_files: Option<usize>,
    max_files: Option<usize>,
    files: Vec<ResolvedNetworkFile>,
}

struct ResolvedNetworkFile {
    name: OsString,
    path: PathBuf,
    source: RelativePathBuf,
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

fn resolve_network_set(root: &Path, set: &NetworkSet) -> Result<ResolvedNetworkSet, ManifestResolutionError> {
    let dir = strict_relative_path(
        &format!("network set '{}' directory", set.name),
        set.dir.as_deref().unwrap_or(&set.name),
    )?;
    let candidate = dir.to_path(root);
    if !candidate.exists() {
        return Err(ManifestResolutionError::DirectoryNotFound {
            set: set.name.clone(),
            path: candidate,
        });
    }
    let root = canonicalize_contained(root, &candidate, &format!("network set '{}' directory", set.name))?;
    if !root.is_dir() {
        return Err(ManifestResolutionError::NotADirectory {
            set: set.name.clone(),
            path: root,
        });
    }
    let mut files = Vec::new();
    let entries = std::fs::read_dir(&root).map_err(|source| ManifestResolutionError::DirectoryRead {
        set: set.name.clone(),
        path: root.clone(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| ManifestResolutionError::DirectoryRead {
            set: set.name.clone(),
            path: root.clone(),
            source,
        })?;
        let name = entry.file_name();
        if Path::new(&name).extension() != Some(OsStr::new("json")) || !entry.path().is_file() {
            continue;
        }
        let path = canonicalize_contained(&root, &entry.path(), &format!("network set '{}' file", set.name))?;
        let filename =
            RelativePath::from_path(Path::new(&name)).map_err(|_| ManifestResolutionError::InvalidRelativePath {
                field: format!("network set '{}' file", set.name),
                path: entry.path(),
            })?;
        let source = dir.join(filename);
        files.push(ResolvedNetworkFile { name, path, source });
    }
    files.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(ResolvedNetworkSet {
        name: set.name.clone(),
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
            network_sets,
            networks: vec![],
            models: vec![model("test", include)],
        }
    }
    fn model(name: &str, include: Vec<DefinitionSelection>) -> ModelDefinition {
        ModelDefinition {
            name: name.to_string(),
            base_model: None,
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
            network: None,
        };
        let a_source = ProvenanceSource {
            file: "nets/a.json".into(),
            network_set: Some("nets".into()),
            network: None,
        };
        let b_source = ProvenanceSource {
            file: "nets/b.json".into(),
            network_set: Some("nets".into()),
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
