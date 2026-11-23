//! Metadata types and key names used in Pywr's Arrow outputs.
//!
//! This crate has no dependency on the Pywr solver or file-format libraries, so it can be
//! used to interpret an Arrow stream or Parquet file written by Pywr without depending on
//! `pywr-core`.
//!
//! A Parquet file written by Pywr stores the same Arrow schema as an Arrow stream, so the
//! description below applies to both. An Arrow stream written by Pywr has two timestamp columns (`time_start` and `time_end`)
//! followed by one `FixedSizeList<Float64>` column per metric, with one value per scenario.
//! Each metric field carries the [`METRIC_EXTENSION_NAME`] Arrow extension type, whose
//! metadata is a JSON-encoded [`MetricColumnMetadata`]. The schema metadata holds the
//! scenarios ([`SCENARIOS_KEY`]) and scenario groups ([`SCENARIO_GROUPS_KEY`]) as JSON, and,
//! when the run was given any, the run metadata ([`RUN_METADATA_KEY`]).
//!
//! The `arrow` feature adds `MetricColumnExtension`, an implementation of Arrow's
//! `ExtensionType` for metric columns, at the cost of a dependency on `arrow-schema`.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[cfg(feature = "arrow")]
mod extension;

#[cfg(feature = "arrow")]
pub use extension::MetricColumnExtension;

/// Name of the Arrow extension type used for Pywr metric columns.
pub const METRIC_EXTENSION_NAME: &str = "org.pywr.metric";

/// Schema metadata key holding the version of Pywr that wrote the stream.
pub const VERSION_KEY: &str = "PYWR_VERSION";

/// Schema metadata key holding a JSON array of [`ScenarioMetadata`], in column value order.
pub const SCENARIOS_KEY: &str = "PYWR_SCENARIOS";

/// Schema metadata key holding a JSON array of [`ScenarioGroupMetadata`].
pub const SCENARIO_GROUPS_KEY: &str = "PYWR_SCENARIO_GROUPS";

/// Schema metadata key holding a JSON-encoded [`RunMetadata`].
pub const RUN_METADATA_KEY: &str = "PYWR_RUN";

/// Metadata identifying a metric column in a Pywr Arrow output.
///
/// The physical Arrow type is `FixedSizeList<Float64>`; this metadata is encoded as the
/// [`METRIC_EXTENSION_NAME`] Arrow extension type on the corresponding field.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct MetricColumnMetadata {
    pub metric_set: String,
    pub name: String,
    pub attribute: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub sub_type: Option<String>,
}

/// One simulated scenario, as stored under [`SCENARIOS_KEY`].
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ScenarioMetadata {
    pub simulation_id: usize,
    /// The index of the simulated scenarios within each scenario group.
    pub simulation_indices: Vec<usize>,
    /// The label of this scenario within each scenario group.
    pub scenario_labels: Vec<String>,
}

/// One scenario group, as stored under [`SCENARIO_GROUPS_KEY`].
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ScenarioGroupMetadata {
    pub name: String,
    pub size: usize,
}

/// Metadata describing the run that wrote an output, as stored under [`RUN_METADATA_KEY`].
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct RunMetadata {
    /// The name of the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_name: Option<String>,
    /// The project definition the model was composed from, if it came from a project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectRunMetadata>,
    /// Any other metadata given for the run, recorded as given.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// The project definition a run's model was composed from.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ProjectRunMetadata {
    /// The name of the model or multi-model definition that was run.
    pub definition: String,
    /// The project's manifest followed by every file the model was composed from, in merge
    /// order. Each path is relative to the project's directory, using `/` separators, and starts
    /// with `..` for a file in a dependency outside it.
    pub files: Vec<SourceFileMetadata>,
    /// The variables whose values filled in the templates of those files, in the order they
    /// are first used.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<VariableMetadata>,
}

/// A variable used to fill in the templates of the files a run's model was composed from.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct VariableMetadata {
    /// The network of a multi-model the value is used in; `None` for a model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    /// The variable, named as the project refers to it, such as `org::dataset` for a variable
    /// of the dependency `org`.
    pub name: String,
    /// The choice name for a variable with choices, and otherwise the value as given.
    pub value: String,
    /// For a path variable, the path used, given as the paths of
    /// [`files`](ProjectRunMetadata::files) are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub source: VariableSource,
}

/// Where a variable's value came from.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VariableSource {
    /// The variable's default.
    Default,
    /// A definition that binds it.
    Definition {
        /// `model`, `network` or `multi_model_network`.
        kind: String,
        /// The definition's name, such as `org::river` or `<multi-model>/<network>`.
        name: String,
    },
    /// The command line, or whatever ran the model.
    CommandLine,
}

/// A file a run's model was composed from.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SourceFileMetadata {
    pub path: String,
    /// The SHA-256 hash of the file's contents, as lowercase hexadecimal.
    pub sha256: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_column_metadata_uses_type_key() {
        let metadata = MetricColumnMetadata {
            metric_set: "outputs".to_string(),
            name: "reservoir".to_string(),
            attribute: "volume".to_string(),
            ty: "node".to_string(),
            sub_type: Some("storage".to_string()),
        };
        let json = serde_json::to_value(&metadata).unwrap();
        assert_eq!(json["type"], "node");
        assert_eq!(serde_json::from_value::<MetricColumnMetadata>(json).unwrap(), metadata);
    }

    #[test]
    fn run_metadata_omits_unset_fields_and_round_trips() {
        assert_eq!(serde_json::to_string(&RunMetadata::default()).unwrap(), "{}");

        let metadata = RunMetadata {
            run_name: Some("baseline".to_string()),
            project: Some(ProjectRunMetadata {
                definition: "baseline-historical".to_string(),
                files: vec![SourceFileMetadata {
                    path: "pywr-project.json".to_string(),
                    sha256: "00".repeat(32),
                }],
                variables: vec![VariableMetadata {
                    network: None,
                    name: "dataset".to_string(),
                    value: "stoch".to_string(),
                    path: Some("data/stoch".to_string()),
                    source: VariableSource::Definition {
                        kind: "model".to_string(),
                        name: "baseline-historical".to_string(),
                    },
                }],
            }),
            extra: BTreeMap::from([("user".to_string(), serde_json::json!("analyst"))]),
        };
        let json = serde_json::to_string(&metadata).unwrap();
        assert_eq!(serde_json::from_str::<RunMetadata>(&json).unwrap(), metadata);
    }

    #[test]
    fn parses_scenario_metadata() {
        let scenarios: Vec<ScenarioMetadata> =
            serde_json::from_str(r#"[{"simulation_id":0,"simulation_indices":[1],"scenario_labels":["wet"]}]"#)
                .unwrap();
        assert_eq!(scenarios[0].scenario_labels, ["wet"]);
        let groups: Vec<ScenarioGroupMetadata> = serde_json::from_str(r#"[{"name":"case","size":2}]"#).unwrap();
        assert_eq!(groups[0].size, 2);
    }
}
