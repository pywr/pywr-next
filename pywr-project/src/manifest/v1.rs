use crate::composition::{ComposedModel, ComposedModelBuilder};
use crate::error::{ComposeModelError, ValidationError};
use crate::manifest::DefinitionOverrides;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Default)]
pub struct ProjectManifestValidationReport {
    pub errors: Vec<ProjectManifestValidationError>,
}

impl ProjectManifestValidationReport {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

#[derive(Debug)]
pub enum ProjectManifestValidationError {
    DuplicateNetworkSet {
        set: String,
        count: usize,
    },
    DuplicateDefinition {
        definition: String,
        count: usize,
    },
    DuplicateSet {
        definition: String,
        set: String,
        count: usize,
    },
    SetNotFound {
        definition: String,
        set: String,
    },
    FileNotFound {
        definition: String,
        set: String,
        file: String,
    },
    DuplicateFile {
        definition: String,
        set: String,
        file: String,
    },
    InvalidFilePath {
        definition: String,
        set: String,
        file: String,
    },
    MinFilesNotMet {
        definition: String,
        set: String,
        min_files: usize,
        actual_files: usize,
    },
    MaxFilesExceeded {
        definition: String,
        set: String,
        max_files: usize,
        actual_files: usize,
    },
    InvalidFileConstraints {
        set: String,
        min_files: usize,
        max_files: usize,
    },
    BaseModelNotFound {
        path: PathBuf,
    },
    BaseModelNotAFile {
        path: PathBuf,
    },
    DirectoryNotFound {
        set: String,
        dir: PathBuf,
    },
    NotADirectory {
        set: String,
        dir: PathBuf,
    },
    InvalidRelativePath {
        field: String,
        path: PathBuf,
    },
    PathEscapesRoot {
        field: String,
        path: PathBuf,
        root: PathBuf,
        resolved_path: PathBuf,
    },
    DirectoryRead {
        set: String,
        dir: PathBuf,
        error: std::io::Error,
    },
}

/// A simple project schema that defines how to build a model from multiple JSON fragments.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
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
    /// Validate the project manifest and return a report of all structural errors found.
    pub fn validate(&self, root: &Path) -> Result<ProjectManifestValidationReport, ValidationError> {
        let mut errors = self.manifest_errors();
        let sets = self.resolve_network_sets(root, &mut errors);
        let base_model = self.resolve_base_model(root, &mut errors);

        for definition in &self.definitions {
            definition.validate(&sets, &mut errors);
        }

        // Resolving here exercises the same path policy used by composition even if there are no definitions.
        let _ = base_model;
        Ok(ProjectManifestValidationReport { errors })
    }

    /// Validate a definition against the project manifest.
    pub fn validate_model(
        &self,
        root: &Path,
        definition_name: &str,
    ) -> Result<ProjectManifestValidationReport, ValidationError> {
        let definition = self
            .definitions
            .iter()
            .find(|d| d.name == definition_name)
            .ok_or_else(|| ValidationError::DefinitionNotFound {
                definition: definition_name.to_string(),
            })?;
        let mut errors = self.manifest_errors();
        let sets = self.resolve_network_sets(root, &mut errors);
        self.resolve_base_model(root, &mut errors);
        definition.validate(&sets, &mut errors);
        Ok(ProjectManifestValidationReport { errors })
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
        definition.compose_model(base_model, &sets)
    }

    fn manifest_errors(&self) -> Vec<ProjectManifestValidationError> {
        let mut errors = Vec::new();
        add_duplicate_errors(
            &self.network_sets,
            |set| &set.name,
            |name, count| ProjectManifestValidationError::DuplicateNetworkSet { set: name, count },
            &mut errors,
        );
        add_duplicate_errors(
            &self.definitions,
            |definition| &definition.name,
            |name, count| ProjectManifestValidationError::DuplicateDefinition {
                definition: name,
                count,
            },
            &mut errors,
        );
        for set in &self.network_sets {
            if let (Some(min_files), Some(max_files)) = (set.min_files, set.max_files) {
                if min_files > max_files {
                    errors.push(ProjectManifestValidationError::InvalidFileConstraints {
                        set: set.name.clone(),
                        min_files,
                        max_files,
                    });
                }
            }
        }
        errors
    }

    fn resolve_base_model(&self, root: &Path, errors: &mut Vec<ProjectManifestValidationError>) -> Option<PathBuf> {
        match resolve_base_model(root, &self.base_model) {
            Ok(path) => Some(path),
            Err(error) => {
                errors.push(error.into_validation_error());
                None
            }
        }
    }

    fn resolve_network_sets(
        &self,
        root: &Path,
        errors: &mut Vec<ProjectManifestValidationError>,
    ) -> HashMap<String, ResolvedNetworkSet> {
        let mut resolved = HashMap::new();
        for set in &self.network_sets {
            match resolve_network_set(root, set) {
                Ok(value) => {
                    resolved.entry(set.name.clone()).or_insert(value);
                }
                Err(error) => errors.push(error.into_validation_error()),
            }
        }
        resolved
    }
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct Definition {
    pub name: String,
    /// Per-network-set selections for this definition.
    pub include: Vec<DefinitionSelection>,
    /// Optional overrides to base model.
    #[serde(default)]
    pub overrides: Option<DefinitionOverrides>,
}

impl Definition {
    fn validate(&self, sets: &HashMap<String, ResolvedNetworkSet>, errors: &mut Vec<ProjectManifestValidationError>) {
        let mut counts = HashMap::new();
        for selection in &self.include {
            *counts.entry(selection.set.clone()).or_insert(0) += 1;
        }
        for (set, count) in &counts {
            if *count > 1 {
                errors.push(ProjectManifestValidationError::DuplicateSet {
                    definition: self.name.clone(),
                    set: set.clone(),
                    count: *count,
                });
            }
        }

        for set in sets.values() {
            let selected = match self.include.iter().find(|selection| selection.set == set.name) {
                Some(selection) => resolve_selection(self, selection, set, errors),
                None => Vec::new(),
            };
            validate_constraints(self, set, selected.len(), errors);
        }
        for selection in &self.include {
            if !sets.contains_key(&selection.set) {
                errors.push(ProjectManifestValidationError::SetNotFound {
                    definition: self.name.clone(),
                    set: selection.set.clone(),
                });
            }
        }
    }

    fn compose_model(
        &self,
        base_model: PathBuf,
        sets: &HashMap<String, ResolvedNetworkSet>,
    ) -> Result<ComposedModel, ComposeModelError> {
        let mut selected_sets = HashSet::new();
        let mut builder = ComposedModelBuilder::new(self.name.clone(), base_model);
        for selection in &self.include {
            if !selected_sets.insert(&selection.set) {
                return Err(ComposeModelError::DuplicateSelection {
                    set: selection.set.clone(),
                });
            }
            let set = sets.get(&selection.set).ok_or_else(|| ComposeModelError::SetNotFound {
                set: selection.set.clone(),
            })?;
            if let (Some(min_files), Some(max_files)) = (set.min_files, set.max_files) {
                if min_files > max_files {
                    return Err(ComposeModelError::InvalidFileConstraints {
                        set: set.name.clone(),
                        min_files,
                        max_files,
                    });
                }
            }
            let files = resolve_selection_for_composition(selection, set)?;
            if let Some(min_files) = set.min_files {
                if files.len() < min_files {
                    return Err(ComposeModelError::MinFilesNotMet {
                        set: set.name.clone(),
                        min_files,
                        actual_files: files.len(),
                    });
                }
            }
            if let Some(max_files) = set.max_files {
                if files.len() > max_files {
                    return Err(ComposeModelError::MaxFilesExceeded {
                        set: set.name.clone(),
                        max_files,
                        actual_files: files.len(),
                    });
                }
            }
            for file in files {
                builder.add_include(file.path.clone());
            }
        }
        // Constraints also apply to sets omitted by this definition.
        for set in sets.values() {
            if !selected_sets.contains(&set.name) {
                if let Some(min_files) = set.min_files {
                    if min_files > 0 {
                        return Err(ComposeModelError::MinFilesNotMet {
                            set: set.name.clone(),
                            min_files,
                            actual_files: 0,
                        });
                    }
                }
            }
        }
        if let Some(overrides) = &self.overrides {
            builder.overrides(overrides.clone());
        }
        Ok(builder.build())
    }
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct DefinitionSelection {
    /// Name of the network set.
    pub set: String,
    /// Specific filenames to include from the set directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,
    /// If true, include all JSON files in the set directory. Overrides `files` if both are specified.
    pub include_all: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
}

fn resolve_selection<'a>(
    definition: &Definition,
    selection: &DefinitionSelection,
    set: &'a ResolvedNetworkSet,
    errors: &mut Vec<ProjectManifestValidationError>,
) -> Vec<&'a ResolvedNetworkFile> {
    if selection.include_all.unwrap_or(false) {
        return set.files.iter().collect();
    }
    let mut result = Vec::new();
    let mut names = HashSet::new();
    for file in selection.files.as_deref().unwrap_or_default() {
        if !is_filename(file) {
            errors.push(ProjectManifestValidationError::InvalidFilePath {
                definition: definition.name.clone(),
                set: set.name.clone(),
                file: file.clone(),
            });
        } else if !names.insert(file) {
            errors.push(ProjectManifestValidationError::DuplicateFile {
                definition: definition.name.clone(),
                set: set.name.clone(),
                file: file.clone(),
            });
        } else if let Some(resolved) = set.files.iter().find(|candidate| candidate.name == OsStr::new(file)) {
            result.push(resolved);
        } else {
            errors.push(ProjectManifestValidationError::FileNotFound {
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
            return Err(ComposeModelError::InvalidRelativePath {
                field: format!("selected file in network set '{}'", set.name),
                path: PathBuf::from(file),
            });
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

fn validate_constraints(
    definition: &Definition,
    set: &ResolvedNetworkSet,
    count: usize,
    errors: &mut Vec<ProjectManifestValidationError>,
) {
    if let Some(min_files) = set.min_files {
        if count < min_files {
            errors.push(ProjectManifestValidationError::MinFilesNotMet {
                definition: definition.name.clone(),
                set: set.name.clone(),
                min_files,
                actual_files: count,
            });
        }
    }
    if let Some(max_files) = set.max_files {
        if count > max_files {
            errors.push(ProjectManifestValidationError::MaxFilesExceeded {
                definition: definition.name.clone(),
                set: set.name.clone(),
                max_files,
                actual_files: count,
            });
        }
    }
}

fn resolve_base_model(root: &Path, base_model: &str) -> Result<PathBuf, ComposeModelError> {
    let candidate = strict_relative_path("base model", base_model)?;
    let candidate = root.join(candidate);
    if !candidate.exists() {
        return Err(ComposeModelError::BaseModelNotFound { path: candidate });
    }
    let path = canonicalize_contained(root, &candidate, "base model")?;
    if !path.is_file() {
        return Err(ComposeModelError::BaseModelNotAFile { path });
    }
    Ok(path)
}

fn resolve_network_set(root: &Path, set: &NetworkSet) -> Result<ResolvedNetworkSet, ComposeModelError> {
    let dir = strict_relative_path(
        &format!("network set '{}' directory", set.name),
        set.dir.as_deref().unwrap_or(&set.name),
    )?;
    let candidate = root.join(dir);
    if !candidate.exists() {
        return Err(ComposeModelError::DirectoryNotFound {
            set: set.name.clone(),
            path: candidate,
        });
    }
    let root = canonicalize_contained(root, &candidate, &format!("network set '{}' directory", set.name))?;
    if !root.is_dir() {
        return Err(ComposeModelError::NotADirectory {
            set: set.name.clone(),
            path: root,
        });
    }
    let mut files = Vec::new();
    let entries = std::fs::read_dir(&root).map_err(|source| ComposeModelError::DirectoryRead {
        set: set.name.clone(),
        path: root.clone(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| ComposeModelError::DirectoryRead {
            set: set.name.clone(),
            path: root.clone(),
            source,
        })?;
        let name = entry.file_name();
        if Path::new(&name).extension() != Some(OsStr::new("json")) || !entry.path().is_file() {
            continue;
        }
        let path = canonicalize_contained(&root, &entry.path(), &format!("network set '{}' file", set.name))?;
        files.push(ResolvedNetworkFile { name, path });
    }
    files.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(ResolvedNetworkSet {
        name: set.name.clone(),
        min_files: set.min_files,
        max_files: set.max_files,
        files,
    })
}

fn strict_relative_path(field: &str, value: &str) -> Result<PathBuf, ComposeModelError> {
    let path = Path::new(value);
    if value.is_empty()
        || value.contains('\\')
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ComposeModelError::InvalidRelativePath {
            field: field.to_string(),
            path: path.to_path_buf(),
        });
    }
    Ok(path.to_path_buf())
}

fn is_filename(value: &str) -> bool {
    !value.contains('\\')
        && matches!(
            Path::new(value).components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(_)]
        )
}

fn canonicalize_contained(root: &Path, candidate: &Path, field: &str) -> Result<PathBuf, ComposeModelError> {
    let canonical_root = root.canonicalize().map_err(|source| ComposeModelError::DirectoryRead {
        set: field.to_string(),
        path: root.to_path_buf(),
        source,
    })?;
    let resolved_path = candidate
        .canonicalize()
        .map_err(|source| ComposeModelError::DirectoryRead {
            set: field.to_string(),
            path: candidate.to_path_buf(),
            source,
        })?;
    if !resolved_path.starts_with(&canonical_root) {
        return Err(ComposeModelError::PathEscapesRoot {
            field: field.to_string(),
            path: candidate.to_path_buf(),
            root: canonical_root,
            resolved_path,
        });
    }
    Ok(resolved_path)
}

trait ValidationConversion {
    fn into_validation_error(self) -> ProjectManifestValidationError;
}
impl ValidationConversion for ComposeModelError {
    fn into_validation_error(self) -> ProjectManifestValidationError {
        match self {
            ComposeModelError::InvalidRelativePath { field, path } => {
                ProjectManifestValidationError::InvalidRelativePath { field, path }
            }
            ComposeModelError::PathEscapesRoot {
                field,
                path,
                root,
                resolved_path,
            } => ProjectManifestValidationError::PathEscapesRoot {
                field,
                path,
                root,
                resolved_path,
            },
            ComposeModelError::BaseModelNotFound { path } => ProjectManifestValidationError::BaseModelNotFound { path },
            ComposeModelError::BaseModelNotAFile { path } => ProjectManifestValidationError::BaseModelNotAFile { path },
            ComposeModelError::DirectoryNotFound { set, path } => {
                ProjectManifestValidationError::DirectoryNotFound { set, dir: path }
            }
            ComposeModelError::NotADirectory { set, path } => {
                ProjectManifestValidationError::NotADirectory { set, dir: path }
            }
            ComposeModelError::DirectoryRead { set, path, source } => ProjectManifestValidationError::DirectoryRead {
                set,
                dir: path,
                error: source,
            },
            _ => unreachable!("only filesystem resolution errors are converted to validation errors"),
        }
    }
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

fn add_duplicate_errors<T, F, E>(items: &[T], key: F, error: E, errors: &mut Vec<ProjectManifestValidationError>)
where
    F: Fn(&T) -> &String,
    E: Fn(String, usize) -> ProjectManifestValidationError,
{
    let mut counts = HashMap::new();
    for item in items {
        *counts.entry(key(item).clone()).or_insert(0) += 1;
    }
    for (name, count) in counts {
        if count > 1 {
            errors.push(error(name, count));
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tempfile::tempdir;

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
        }
    }
    fn touch(path: &Path) {
        std::fs::write(path, "{}").unwrap();
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
        let report = manifest.validate(root.path()).unwrap();
        assert!(matches!(
            report.errors.as_slice(),
            [
                ProjectManifestValidationError::FileNotFound { .. },
                ProjectManifestValidationError::MinFilesNotMet { actual_files: 0, .. }
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
        let report = manifest.validate(root.path()).unwrap();
        assert!(report.errors.iter().any(|error| matches!(error, ProjectManifestValidationError::InvalidRelativePath { field, .. } if field == "base model")));
        assert!(report.errors.iter().any(|error| matches!(error, ProjectManifestValidationError::InvalidRelativePath { field, .. } if field.contains("network set 'nets' directory"))));
    }

    #[test]
    fn composition_rejects_relative_path_escapes() {
        let root = tempdir().unwrap();
        let manifest = manifest("../base.json", vec![], vec![]);
        assert!(
            matches!(manifest.compose_model(root.path(), "test"), Err(ComposeModelError::InvalidRelativePath { field, .. }) if field == "base model")
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

        let report = manifest.validate(root.path()).unwrap();
        assert!(report.errors.iter().any(|error| matches!(
            error,
            ProjectManifestValidationError::InvalidFilePath { file, .. } if file == "../outside.json"
        )));
        assert!(matches!(
            manifest.compose_model(root.path(), "test"),
            Err(ComposeModelError::InvalidRelativePath { .. })
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
        let report = manifest.validate(root.path()).unwrap();
        assert!(
            report
                .errors
                .iter()
                .any(|error| matches!(error, ProjectManifestValidationError::PathEscapesRoot { .. }))
        );
        assert!(matches!(
            manifest.compose_model(root.path(), "test"),
            Err(ComposeModelError::PathEscapesRoot { .. })
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
        let report = manifest.validate(root.path()).unwrap();
        assert!(
            report
                .errors
                .iter()
                .any(|error| matches!(error, ProjectManifestValidationError::DuplicateNetworkSet { .. }))
        );
        assert!(
            report
                .errors
                .iter()
                .any(|error| matches!(error, ProjectManifestValidationError::DuplicateDefinition { .. }))
        );
        assert!(
            report
                .errors
                .iter()
                .any(|error| matches!(error, ProjectManifestValidationError::InvalidFileConstraints { .. }))
        );
    }
}
