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
    #[error("The definition `{definition}` was not found in the project manifest.")]
    DefinitionNotFound { definition: String },
    #[error("The network set `{set}` was not found in the project manifest.")]
    SetNotFound { set: String },
    #[error("The name `{set}` is used by more than one network set, but each name must be unique.")]
    DuplicateNetworkSet { set: String },
    #[error("The name `{definition}` is used by more than one definition, but each name must be unique.")]
    DuplicateDefinition { definition: String },
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
