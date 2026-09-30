pub mod v1;

use crate::composition::ComposedModel;
use crate::error::{ComposeModelError, ProjectManifestReadError};
use pywr_schema::model::{ScenarioDomain, TimeDomain};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::str::FromStr;

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "format")]
pub enum ProjectManifest {
    #[serde(rename = "v1")]
    V1(v1::ProjectManifest),
}

#[derive(Debug)]
pub enum ProjectManifestValidationError {
    V1(v1::ProjectManifestValidationReport),
}

impl ProjectManifestValidationError {
    /// A multi-line report with a summary followed by one line for each problem.
    pub fn report(&self) -> impl std::fmt::Display + '_ {
        struct Report<'a>(&'a ProjectManifestValidationError);

        impl std::fmt::Display for Report<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.0 {
                    ProjectManifestValidationError::V1(report) => report.report().fmt(f),
                }
            }
        }

        Report(self)
    }
}

impl std::fmt::Display for ProjectManifestValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V1(report) => report.fmt(f),
        }
    }
}

impl ProjectManifest {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, ProjectManifestReadError> {
        let data = std::fs::read_to_string(&path).map_err(|error| ProjectManifestReadError::IO {
            path: path.as_ref().to_path_buf(),
            error,
        })?;
        Ok(serde_json::from_str(data.as_str())?)
    }

    pub fn validate(&self, root: &Path) -> Result<(), ProjectManifestValidationError> {
        match self {
            ProjectManifest::V1(manifest) => manifest.validate(root).map_err(ProjectManifestValidationError::V1),
        }
    }

    pub fn compose(&self, root: &Path, definition_name: &str) -> Result<ComposedModel, ComposeModelError> {
        match self {
            ProjectManifest::V1(manifest) => manifest.compose_model(root, definition_name),
        }
    }
}

impl FromStr for ProjectManifest {
    type Err = ProjectManifestReadError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(serde_json::from_str(s)?)
    }
}

#[derive(Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct DefinitionOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<TimeDomain>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scenarios: Option<ScenarioDomain>,
}
