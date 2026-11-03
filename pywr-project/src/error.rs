use pywr_schema::{ModelSchemaReadError, NetworkMergeError, NetworkSchemaReadError};
use std::io;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("The project could not be read or written due to an I/O error.")]
    Io(#[from] io::Error),
    #[error("The project contains invalid JSON.")]
    Json(#[from] serde_json::Error),
}

/// Error type for reading a [`NetworkSchema`] network from a file or string.
#[derive(Error, Debug)]
pub enum ProjectManifestReadError {
    #[error("The project manifest at `{}` could not be read.", .path.display())]
    IO {
        path: PathBuf,
        #[source]
        error: io::Error,
    },
    #[error("The project manifest contains invalid JSON.")]
    Json(#[from] serde_json::Error),
}

#[derive(Error, Debug)]
pub enum ComposeToSchemaError {
    #[error("The base model schema could not be read.")]
    ModelRead(#[from] ModelSchemaReadError),
    #[error("A network schema could not be read.")]
    NetworkRead(#[from] NetworkSchemaReadError),
    #[error("The network schemas could not be merged.")]
    NetworkMerge(#[from] NetworkMergeError),
    #[error("The composed model has unresolved placeholders: {}.", .placeholders.join(", "))]
    UnresolvedPlaceholders { placeholders: Vec<String> },
}

/// Errors that may occur during validation of a project manifest, or during resolution of a
/// project manifest into a `ComposedModel`.
#[derive(Error, Debug)]
pub enum ManifestResolutionError {
    #[error("The path `{}` for {field} must be a non-empty strict relative path.", .path.display())]
    InvalidRelativePath { field: String, path: PathBuf },
    #[error("The path `{}` for {field} escapes its allowed root `{}`: it resolves to `{}`.", .path.display(), .root.display(), .resolved_path.display())]
    PathEscapesRoot {
        field: String,
        path: PathBuf,
        root: PathBuf,
        resolved_path: PathBuf,
    },
    #[error("The base model `{}` was not found.", .path.display())]
    BaseModelNotFound { path: PathBuf },
    #[error("The base model `{}` is not a regular file.", .path.display())]
    BaseModelNotAFile { path: PathBuf },
    #[error("The directory `{}` for network set `{set}` was not found.", .path.display())]
    DirectoryNotFound { set: String, path: PathBuf },
    #[error("The path `{}` for network set `{set}` is not a directory.", .path.display())]
    NotADirectory { set: String, path: PathBuf },
    #[error("The directory `{}` for network set `{set}` could not be read.", .path.display())]
    DirectoryRead {
        set: String,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("The directory `{}` for sub-network `{sub_network}` was not found.", .path.display())]
    SubNetworkDirectoryNotFound { sub_network: String, path: PathBuf },
    #[error("The path `{}` for sub-network `{sub_network}` is not a directory.", .path.display())]
    SubNetworkNotADirectory { sub_network: String, path: PathBuf },
    #[error("The base network `{}` for sub-network `{sub_network}` was not found.", .path.display())]
    BaseNetworkNotFound { sub_network: String, path: PathBuf },
    #[error("The base network `{}` for sub-network `{sub_network}` is not a regular file.", .path.display())]
    BaseNetworkNotAFile { sub_network: String, path: PathBuf },
    #[error("The path `{}` for {field} could not be canonicalized.", .path.display())]
    UnableToCanonicalizePath {
        field: String,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Errors that may occur during composition of a [`ComposeModel`] from a project manifest.
#[derive(Error, Debug)]
pub enum ComposeModelError {
    #[error("The model definition `{definition}` was not found in the project manifest.")]
    DefinitionNotFound { definition: String },
    #[error("The network definition `{network}` was not found in the project manifest.")]
    NetworkDefinitionNotFound { network: String },
    #[error("The network set `{set}` was not found in the project manifest.")]
    SetNotFound { set: String },
    #[error("The name `{sub_network}` is used by more than one sub-network, but each name must be unique.")]
    DuplicateSubNetwork { sub_network: String },
    #[error("The {kind} name `{name}` must not contain `::`.")]
    InvalidName { kind: &'static str, name: String },
    #[error("The definition `{definition}` refers to the sub-network `{sub_network}`, which does not exist.")]
    SubNetworkNotFound { definition: String, sub_network: String },
    #[error(
        "The definition `{definition}` selects the network set `{set}`, but its sub-network `{sub_network}` is not active."
    )]
    SubNetworkNotActive {
        definition: String,
        set: String,
        sub_network: String,
    },
    #[error("The name `{set}` is used by more than one network set, but each name must be unique.")]
    DuplicateNetworkSet { set: String },
    #[error("The name `{definition}` is used by more than one model definition, but each name must be unique.")]
    DuplicateDefinition { definition: String },
    #[error("The name `{network}` is used by more than one network definition, but each name must be unique.")]
    DuplicateNetworkDefinition { network: String },
    #[error("The definition `{definition}` refers to the network definition `{network}`, which does not exist.")]
    NetworkNotFound { definition: String, network: String },
    #[error("The network definitions extend each other in a cycle: {}.", .cycle.join(" -> "))]
    NetworkCycle { cycle: Vec<String> },
    #[error("The model definition `{definition}` has no base model.")]
    BaseModelMissing { definition: String },
    #[error("The network set `{set}` is selected more than once, but each set may be selected at most once.")]
    DuplicateSelection { set: String },
    #[error("The network set `{set}` has `min_files` {min_files}, which exceeds `max_files` {max_files}.")]
    InvalidFileConstraints {
        set: String,
        min_files: usize,
        max_files: usize,
    },
    #[error(
        "The network set `{set}` requires at least {min_files} selected file(s), but {actual_files} were selected."
    )]
    MinFilesNotMet {
        set: String,
        min_files: usize,
        actual_files: usize,
    },
    #[error("The network set `{set}` allows at most {max_files} selected file(s), but {actual_files} were selected.")]
    MaxFilesExceeded {
        set: String,
        max_files: usize,
        actual_files: usize,
    },
    #[error("The file `{file}` was not found in network set `{set}`.")]
    FileNotFound { set: String, file: String },
    #[error("The file `{file}` is selected more than once in network set `{set}`.")]
    DuplicateFile { set: String, file: String },
    #[error("The metadata for file `{file}` in network set `{set}` does not match a selected file.")]
    UnusedFileMeta { set: String, file: String },
    #[error(transparent)]
    Resolution(#[from] ManifestResolutionError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    #[test]
    fn display_omits_source_but_preserves_chain() {
        let error = ProjectError::from(io::Error::other("private cause"));
        assert_eq!(
            error.to_string(),
            "The project could not be read or written due to an I/O error."
        );
        assert_eq!(error.source().unwrap().to_string(), "private cause");

        let error = ProjectManifestReadError::IO {
            path: PathBuf::from("project.json"),
            error: io::Error::other("private cause"),
        };
        assert_eq!(
            error.to_string(),
            "The project manifest at `project.json` could not be read."
        );
        assert_eq!(error.source().unwrap().to_string(), "private cause");

        let error = ManifestResolutionError::UnableToCanonicalizePath {
            field: "my-set".to_string(),
            path: PathBuf::from("nets/a.json"),
            source: io::Error::other("private cause"),
        };
        assert_eq!(
            error.to_string(),
            "The path `nets/a.json` for my-set could not be canonicalized."
        );
        assert_eq!(error.source().unwrap().to_string(), "private cause");
    }
}
