use crate::error::ComposeModelError;
use crate::manifest::{ProjectManifest, ResolveOptions};
use crate::{
    Composed, ComposedModel, ComposedMultiModel, ComposedNetwork, ProjectError, ProjectManifestValidationError,
};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// A simple project schema that defines how to build a model from multiple JSON fragments.
pub struct Project {
    /// Path (relative to root) to the manifest JSON file (this file).
    manifest_filename: OsString,
    /// Base directory of the project (can be inferred from file location if None).
    root: PathBuf,
    manifest: ProjectManifest,
    options: ResolveOptions,
}

impl Project {
    /// Open a project from a manifest JSON file, allowing it to depend on any other project.
    pub fn open<P: AsRef<Path>>(manifest_path: P) -> Result<Self, ProjectError> {
        Self::open_with(manifest_path, ResolveOptions::default())
    }

    /// Open a project from a manifest JSON file, resolving it with `options`.
    pub fn open_with<P: AsRef<Path>>(manifest_path: P, options: ResolveOptions) -> Result<Self, ProjectError> {
        let manifest_path = manifest_path.as_ref();
        let manifest_path = if manifest_path.is_absolute() {
            manifest_path.to_path_buf()
        } else {
            std::env::current_dir()?.join(manifest_path)
        };
        let manifest_path = manifest_path.canonicalize()?;
        let manifest_fn = manifest_path.file_name().ok_or_else(|| {
            ProjectError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Manifest path has no file name",
            ))
        })?;
        let root = manifest_path
            .parent()
            .ok_or_else(|| {
                ProjectError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Manifest path has no parent directory",
                ))
            })?
            .to_path_buf();

        let manifest_json = std::fs::read_to_string(&manifest_path)?;
        let manifest: ProjectManifest = serde_json::from_str(&manifest_json)?;

        // Determine the root directory
        Ok(Self {
            manifest_filename: manifest_fn.to_os_string(),
            root,
            manifest,
            options,
        })
    }

    /// Get a reference to the project manifest.
    pub fn manifest(&self) -> &ProjectManifest {
        &self.manifest
    }

    /// Get the manifest filename (the name of the JSON file).
    pub fn manifest_filename(&self) -> &OsString {
        &self.manifest_filename
    }

    /// Get the root directory of the project.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Validate the project manifest and return a report of all structural problems found.
    pub fn validate(&self) -> Result<(), ProjectManifestValidationError> {
        self.manifest.validate(&self.root, &self.options)
    }

    /// Compose a model from the project manifest and a definition name.
    pub fn compose_model(&self, definition_name: &str) -> Result<ComposedModel, ComposeModelError> {
        self.manifest.compose_model(&self.root, definition_name, &self.options)
    }

    /// Compose a model or multi-model from the project manifest and a definition name.
    pub fn compose(&self, definition_name: &str) -> Result<Composed, ComposeModelError> {
        self.manifest.compose(&self.root, definition_name, &self.options)
    }

    /// Compose a multi-network model from the project manifest and a multi-model definition name.
    pub fn compose_multi_model(&self, definition_name: &str) -> Result<ComposedMultiModel, ComposeModelError> {
        self.manifest
            .compose_multi_model(&self.root, definition_name, &self.options)
    }

    /// Compose a network from the project manifest and a network definition name.
    pub fn compose_network(&self, network_name: &str) -> Result<ComposedNetwork, ComposeModelError> {
        self.manifest.compose_network(&self.root, network_name, &self.options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{ProjectManifest, v1};
    use std::ffi::OsStr;
    use tempfile::tempdir;

    #[test]
    fn test_open_project() {
        let manifest = ProjectManifest::V1(v1::ProjectManifest {
            base_model: Some("base_model.json".to_string()),
            sub_networks: vec![],
            multi_models: vec![],
            dependencies: vec![],
            exports: None,
            network_sets: vec![],
            networks: vec![],
            models: vec![],
        });

        let tmp_dir = tempdir().unwrap();
        let manifest_path = tmp_dir.path().join("project.json");

        std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();

        let project = Project::open(manifest_path).unwrap();
        assert_eq!(project.root, tmp_dir.path().canonicalize().unwrap());
        assert_eq!(project.manifest_filename(), OsStr::new("project.json"));
    }
}
