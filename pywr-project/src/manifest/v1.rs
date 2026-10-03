use crate::composition::{ComposedModel, ComposedModelBuilder, ComposedNetworkPath, PositionOffset};
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
    #[error("The definition `{definition}` was not found in the project manifest.")]
    DefinitionNotFound { definition: String },
    #[error("The name `{set}` is used by {count} network sets, but each name must be unique.")]
    DuplicateNetworkSet { set: String, count: usize },
    #[error("The name `{definition}` is used by {count} definitions, but each name must be unique.")]
    DuplicateDefinition { definition: String, count: usize },
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
    /// Path (relative to root) to the base ModelSchema JSON.
    pub base_model: String,
    /// Named sets of NetworkSchema fragments.
    #[serde(default)]
    pub network_sets: Vec<NetworkSet>,
    /// Multiple named definitions (scenarios).
    #[serde(default)]
    pub definitions: Vec<Definition>,
}

impl ProjectManifest {
    /// Validate the project manifest and return a report of all structural problems found.
    pub fn validate(&self, root: &Path) -> Result<(), ProjectManifestValidationError> {
        let mut problems = self.manifest_problems();
        let sets = self.resolve_network_sets(root, &mut problems);
        let base_model = self.resolve_base_model(root, &mut problems);

        for definition in &self.definitions {
            definition.validate(&sets, &mut problems);
        }

        // Resolving here exercises the same path policy used by composition even if there are no definitions.
        let _ = base_model;
        if !problems.is_empty() {
            Err(ProjectManifestValidationError { problems })
        } else {
            Ok(())
        }
    }

    /// Validate a definition against the project manifest.
    pub fn validate_model(&self, root: &Path, definition_name: &str) -> Result<(), ProjectManifestValidationError> {
        let problems = match self.definitions.iter().find(|d| d.name == definition_name) {
            Some(definition) => {
                let mut problems = self.manifest_problems();
                let sets = self.resolve_network_sets(root, &mut problems);
                self.resolve_base_model(root, &mut problems);
                definition.validate(&sets, &mut problems);
                problems
            }
            None => {
                vec![ProjectManifestProblem::DefinitionNotFound {
                    definition: definition_name.to_string(),
                }]
            }
        };

        if !problems.is_empty() {
            Err(ProjectManifestValidationError { problems })
        } else {
            Ok(())
        }
    }

    /// Compose a model from the base model and the specified definition.
    pub fn compose_model(&self, root: &Path, definition_name: &str) -> Result<ComposedModel, ComposeModelError> {
        ensure_unique(
            &self.definitions,
            |definition| &definition.name,
            |name| ComposeModelError::DuplicateDefinition { definition: name },
        )?;
        ensure_unique(
            &self.network_sets,
            |network_set| &network_set.name,
            |name| ComposeModelError::DuplicateNetworkSet { set: name },
        )?;

        let definition = self
            .definitions
            .iter()
            .find(|d| d.name == definition_name)
            .ok_or_else(|| ComposeModelError::DefinitionNotFound {
                definition: definition_name.to_string(),
            })?;
        let base_model = resolve_base_model(root, &self.base_model)?;
        let sets = self
            .network_sets
            .iter()
            .map(|set| Ok((set.name.clone(), resolve_network_set(root, set)?)))
            .collect::<Result<HashMap<_, _>, ComposeModelError>>()?;
        let base_file = strict_relative_path("base model", &self.base_model)?;
        definition.compose_model(base_model, base_file, &sets)
    }

    fn manifest_problems(&self) -> Vec<ProjectManifestProblem> {
        let mut problems = Vec::new();
        add_duplicate_problems(
            &self.network_sets,
            |set| &set.name,
            |name, count| ProjectManifestProblem::DuplicateNetworkSet { set: name, count },
            &mut problems,
        );
        add_duplicate_problems(
            &self.definitions,
            |definition| &definition.name,
            |name, count| ProjectManifestProblem::DuplicateDefinition {
                definition: name,
                count,
            },
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

    fn resolve_base_model(&self, root: &Path, problems: &mut Vec<ProjectManifestProblem>) -> Option<PathBuf> {
        match resolve_base_model(root, &self.base_model) {
            Ok(path) => Some(path),
            Err(error) => {
                problems.push(error.into());
                None
            }
        }
    }

    fn resolve_network_sets(
        &self,
        root: &Path,
        problems: &mut Vec<ProjectManifestProblem>,
    ) -> HashMap<String, ResolvedNetworkSet> {
        let mut resolved = HashMap::new();
        for set in &self.network_sets {
            match resolve_network_set(root, set) {
                Ok(value) => {
                    resolved.entry(set.name.clone()).or_insert(value);
                }
                Err(error) => problems.push(error.into()),
            }
        }
        resolved
    }
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub name: String,
    /// Per-network-set selections for this definition.
    pub include: Vec<DefinitionSelection>,
    /// Optional overrides to base model.
    #[serde(default)]
    pub overrides: Option<DefinitionOverrides>,
}

impl Definition {
    fn validate(&self, sets: &HashMap<String, ResolvedNetworkSet>, problems: &mut Vec<ProjectManifestProblem>) {
        let mut counts = HashMap::new();
        for selection in &self.include {
            *counts.entry(selection.set.clone()).or_insert(0) += 1;
        }

        let mut duplicate_sets: Vec<_> = counts.into_iter().filter(|(_, count)| *count > 1).collect();
        duplicate_sets.sort_by(|(set_a, _), (set_b, _)| set_a.cmp(set_b));

        problems.extend(
            duplicate_sets
                .into_iter()
                .map(|(set, count)| ProjectManifestProblem::DuplicateSet {
                    definition: self.name.clone(),
                    set: set.clone(),
                    count,
                }),
        );

        for set in sets.values() {
            let selected = match self.include.iter().find(|selection| selection.set == set.name) {
                Some(selection) => {
                    let selected = resolve_selection(self, selection, set, problems);
                    for file in unused_file_meta_keys(selection, &selected) {
                        problems.push(ProjectManifestProblem::UnusedFileMeta {
                            definition: self.name.clone(),
                            set: set.name.clone(),
                            file: file.to_string(),
                        });
                    }
                    selected
                }
                None => Vec::new(),
            };
            validate_constraints(self, set, selected.len(), problems);
        }
        for selection in &self.include {
            if !sets.contains_key(&selection.set) {
                problems.push(ProjectManifestProblem::SetNotFound {
                    definition: self.name.clone(),
                    set: selection.set.clone(),
                });
            }
        }
    }

    fn compose_model(
        &self,
        base_model: PathBuf,
        base_file: RelativePathBuf,
        sets: &HashMap<String, ResolvedNetworkSet>,
    ) -> Result<ComposedModel, ComposeModelError> {
        let mut selected_sets = HashSet::new();
        let mut builder = ComposedModelBuilder::new(self.name.clone(), base_model, base_file);
        for selection in &self.include {
            if !selected_sets.insert(&selection.set) {
                return Err(ComposeModelError::DuplicateSelection {
                    set: selection.set.clone(),
                });
            }
            let set = sets.get(&selection.set).ok_or_else(|| ComposeModelError::SetNotFound {
                set: selection.set.clone(),
            })?;
            if let (Some(min_files), Some(max_files)) = (set.min_files, set.max_files)
                && min_files > max_files
            {
                return Err(ComposeModelError::InvalidFileConstraints {
                    set: set.name.clone(),
                    min_files,
                    max_files,
                });
            }
            let files = resolve_selection_for_composition(selection, set)?;
            if let Some(file) = unused_file_meta_keys(selection, &files).into_iter().next() {
                return Err(ComposeModelError::UnusedFileMeta {
                    set: set.name.clone(),
                    file: file.to_string(),
                });
            }
            if let Some(min_files) = set.min_files
                && files.len() < min_files
            {
                return Err(ComposeModelError::MinFilesNotMet {
                    set: set.name.clone(),
                    min_files,
                    actual_files: files.len(),
                });
            }
            if let Some(max_files) = set.max_files
                && files.len() > max_files
            {
                return Err(ComposeModelError::MaxFilesExceeded {
                    set: set.name.clone(),
                    max_files,
                    actual_files: files.len(),
                });
            }

            for file in files {
                let position_offset = file
                    .name
                    .to_str()
                    .and_then(|name| selection.file_meta.as_ref()?.get(name))
                    .and_then(|meta| meta.position_offset.clone());

                let composed_path = ComposedNetworkPath {
                    path: file.path.clone(),
                    position_offset: position_offset.map(PositionOffset::from),
                    source: ProvenanceSource {
                        file: file.source.clone(),
                        network_set: Some(set.name.clone()),
                    },
                };
                builder.add_include(composed_path);
            }
        }
        // Constraints also apply to sets omitted by this definition.
        for set in sets.values() {
            if !selected_sets.contains(&set.name)
                && let Some(min_files) = set.min_files
                && min_files > 0
            {
                return Err(ComposeModelError::MinFilesNotMet {
                    set: set.name.clone(),
                    min_files,
                    actual_files: 0,
                });
            }
        }
        if let Some(overrides) = &self.overrides {
            builder.overrides(overrides.clone());
        }
        Ok(builder.build())
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
    definition: &Definition,
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
                definition: definition.name.clone(),
                set: set.name.clone(),
                file: file.clone(),
            });
        } else if !names.insert(file) {
            errors.push(ProjectManifestProblem::DuplicateFile {
                definition: definition.name.clone(),
                set: set.name.clone(),
                file: file.clone(),
            });
        } else if let Some(resolved) = set.files.iter().find(|candidate| candidate.name == OsStr::new(file)) {
            result.push(resolved);
        } else {
            errors.push(ProjectManifestProblem::FileNotFound {
                definition: definition.name.clone(),
                set: set.name.clone(),
                file: file.clone(),
            });
        }
    }
    result
}

fn resolve_selection_for_composition<'a>(
    selection: &DefinitionSelection,
    set: &'a ResolvedNetworkSet,
) -> Result<Vec<&'a ResolvedNetworkFile>, ComposeModelError> {
    if selection.include_all.unwrap_or(false) {
        return Ok(set.files.iter().collect());
    }
    let mut result = Vec::new();
    let mut names = HashSet::new();
    for file in selection.files.as_deref().unwrap_or_default() {
        if !is_filename(file) {
            Err(ManifestResolutionError::InvalidRelativePath {
                field: format!("selected file in network set '{}'", set.name),
                path: PathBuf::from(file),
            })?;
        }
        if !names.insert(file) {
            return Err(ComposeModelError::DuplicateFile {
                set: set.name.clone(),
                file: file.clone(),
            });
        }
        let resolved = set
            .files
            .iter()
            .find(|candidate| candidate.name == OsStr::new(file))
            .ok_or_else(|| ComposeModelError::FileNotFound {
                set: set.name.clone(),
                file: file.clone(),
            })?;
        result.push(resolved);
    }
    Ok(result)
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
    definition: &Definition,
    set: &ResolvedNetworkSet,
    count: usize,
    errors: &mut Vec<ProjectManifestProblem>,
) {
    if let Some(min_files) = set.min_files
        && count < min_files
    {
        errors.push(ProjectManifestProblem::MinFilesNotMet {
            definition: definition.name.clone(),
            set: set.name.clone(),
            min_files,
            actual_files: count,
        });
    }
    if let Some(max_files) = set.max_files
        && count > max_files
    {
        errors.push(ProjectManifestProblem::MaxFilesExceeded {
            definition: definition.name.clone(),
            set: set.name.clone(),
            max_files,
            actual_files: count,
        });
    }
}

fn resolve_base_model(root: &Path, base_model: &str) -> Result<PathBuf, ManifestResolutionError> {
    let candidate = strict_relative_path("base model", base_model)?;
    let candidate = candidate.to_path(root);
    if !candidate.exists() {
        return Err(ManifestResolutionError::BaseModelNotFound { path: candidate });
    }
    let path = canonicalize_contained(root, &candidate, "base model")?;
    if !path.is_file() {
        return Err(ManifestResolutionError::BaseModelNotAFile { path });
    }
    Ok(path)
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

fn ensure_unique<T, F, E>(items: &[T], key: F, error: E) -> Result<(), ComposeModelError>
where
    F: Fn(&T) -> &String,
    E: Fn(String) -> ComposeModelError,
{
    let mut seen = HashSet::new();
    for item in items {
        let name = key(item);
        if !seen.insert(name) {
            return Err(error(name.clone()));
        }
    }
    Ok(())
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
            base_model: base_model.to_string(),
            network_sets,
            definitions: vec![Definition {
                name: "test".to_string(),
                include,
                overrides: None,
            }],
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
        };
        let a_source = ProvenanceSource {
            file: "nets/a.json".into(),
            network_set: Some("nets".into()),
        };
        let b_source = ProvenanceSource {
            file: "nets/b.json".into(),
            network_set: Some("nets".into()),
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
            base_model: "base.json".to_string(),
            network_sets: vec![set("nets", None, Some(2), Some(1)), set("nets", None, None, None)],
            definitions: vec![
                Definition {
                    name: "test".to_string(),
                    include: vec![],
                    overrides: None,
                },
                Definition {
                    name: "test".to_string(),
                    include: vec![],
                    overrides: None,
                },
            ],
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
