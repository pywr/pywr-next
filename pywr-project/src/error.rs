use pywr_schema::{ModelSchemaReadError, NetworkMergeError, NetworkSchemaReadError};
use std::io;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Error type for reading a [`NetworkSchema`] network from a file or string.
#[derive(Error, Debug)]
pub enum ProjectManifestReadError {
    #[error("IO error on path `{path}`: {error}")]
    IO { path: PathBuf, error: std::io::Error },
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Error, Debug)]
pub enum ComposeToSchemaError {
    #[error("Error reading model schema: {0}")]
    ModelRead(#[from] ModelSchemaReadError),
    #[error("Error reading network schema: {0}")]
    NetworkRead(#[from] NetworkSchemaReadError),
    #[error("Error merging network schemas: {0}")]
    NetworkMerge(#[from] NetworkMergeError),
}

#[derive(Error, Debug)]
pub enum ComposeModelError {
    #[error("Definition not found: {definition}")]
    DefinitionNotFound { definition: String },
    #[error("Network set not found: {set}")]
    SetNotFound { set: String },
    #[error("duplicate network set name: {set}")]
    DuplicateNetworkSet { set: String },
    #[error("duplicate definition name: {definition}")]
    DuplicateDefinition { definition: String },
    #[error("network set '{set}' is selected more than once")]
    DuplicateSelection { set: String },
    #[error(
        "invalid file constraints for network set '{set}': min_files ({min_files}) exceeds max_files ({max_files})"
    )]
    InvalidFileConstraints {
        set: String,
        min_files: usize,
        max_files: usize,
    },
    #[error("network set '{set}' requires at least {min_files} selected files, but {actual_files} were selected")]
    MinFilesNotMet {
        set: String,
        min_files: usize,
        actual_files: usize,
    },
    #[error("network set '{set}' allows at most {max_files} selected files, but {actual_files} were selected")]
    MaxFilesExceeded {
        set: String,
        max_files: usize,
        actual_files: usize,
    },
    #[error("path for {field} must be a non-empty strict relative path: {path}")]
    InvalidRelativePath { field: String, path: PathBuf },
    #[error("path for {field} escapes its allowed root '{root}': {path} resolves to '{resolved_path}'")]
    PathEscapesRoot {
        field: String,
        path: PathBuf,
        root: PathBuf,
        resolved_path: PathBuf,
    },
    #[error("base model not found: {path}")]
    BaseModelNotFound { path: PathBuf },
    #[error("base model is not a regular file: {path}")]
    BaseModelNotAFile { path: PathBuf },
    #[error("network set '{set}' directory not found: {path}")]
    DirectoryNotFound { set: String, path: PathBuf },
    #[error("network set '{set}' path is not a directory: {path}")]
    NotADirectory { set: String, path: PathBuf },
    #[error("failed to read network set '{set}' directory '{path}': {source}")]
    DirectoryRead {
        set: String,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("File not found in network set '{set}': {file}")]
    FileNotFound { set: String, file: String },
    #[error("file is selected more than once in network set '{set}': {file}")]
    DuplicateFile { set: String, file: String },
}

#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Definition not found: {definition}")]
    DefinitionNotFound { definition: String },
}
