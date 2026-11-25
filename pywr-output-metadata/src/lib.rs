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
    /// What the metric reads from, such as a node or a parameter.
    pub source: MetricSource,
    /// The attribute of the source the metric reads, such as `Volume`.
    pub attribute: String,
}

/// What a metric reads from, identified in the terms of the model's schema.
///
/// Serialized with a `type` tag naming the variant, for example
/// `{"type": "CoreNode", "node": "res", "sub_name": "spill", "node_type": "Link"}`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum MetricSource {
    /// A node of the schema.
    Node { name: String, node_type: String },
    /// A virtual node of the schema.
    VirtualNode { name: String, node_type: String },
    /// A node of the solver's network, built by the schema node `node`. Its `sub_name`
    /// distinguishes it from the other core nodes that node builds; these names are internal and
    /// may change between versions.
    CoreNode {
        node: String,
        sub_name: Option<String>,
        node_type: String,
    },
    /// An edge between two nodes, with the slots it connects, if any.
    Edge {
        from_node: String,
        from_slot: Option<String>,
        to_node: String,
        to_slot: Option<String>,
    },
    /// A parameter; `parent` is the node a local parameter belongs to.
    Parameter {
        name: String,
        parent: Option<String>,
        parameter_type: String,
    },
    /// A table.
    Table { name: String },
    /// A time series.
    TimeSeries { name: String },
    /// A transfer from another network.
    InterNetworkTransfer { name: String },
}

impl MetricSource {
    /// The name of the variant, as written in the `type` tag.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Node { .. } => "Node",
            Self::VirtualNode { .. } => "VirtualNode",
            Self::CoreNode { .. } => "CoreNode",
            Self::Edge { .. } => "Edge",
            Self::Parameter { .. } => "Parameter",
            Self::Table { .. } => "Table",
            Self::TimeSeries { .. } => "TimeSeries",
            Self::InterNetworkTransfer { .. } => "InterNetworkTransfer",
        }
    }

    /// The type of node or parameter, if the source has one.
    pub fn sub_kind(&self) -> Option<&str> {
        match self {
            Self::Node { node_type, .. } | Self::VirtualNode { node_type, .. } | Self::CoreNode { node_type, .. } => {
                Some(node_type)
            }
            Self::Parameter { parameter_type, .. } => Some(parameter_type),
            Self::Edge { .. } | Self::Table { .. } | Self::TimeSeries { .. } | Self::InterNetworkTransfer { .. } => {
                None
            }
        }
    }

    /// A single-string label for outputs that cannot hold the structure, such as a CSV column.
    ///
    /// Core nodes are written `node[sub_name]`, edges `from[slot]->to[slot]` and local parameters
    /// `parent.name`.
    pub fn label(&self) -> String {
        fn with_slot(node: &str, slot: &Option<String>) -> String {
            match slot {
                Some(slot) => format!("{node}[{slot}]"),
                None => node.to_string(),
            }
        }

        match self {
            Self::Node { name, .. }
            | Self::VirtualNode { name, .. }
            | Self::Table { name }
            | Self::TimeSeries { name }
            | Self::InterNetworkTransfer { name } => name.clone(),
            Self::CoreNode { node, sub_name, .. } => with_slot(node, sub_name),
            Self::Edge {
                from_node,
                from_slot,
                to_node,
                to_slot,
            } => format!("{}->{}", with_slot(from_node, from_slot), with_slot(to_node, to_slot)),
            Self::Parameter { name, parent, .. } => match parent {
                Some(parent) => format!("{parent}.{name}"),
                None => name.clone(),
            },
        }
    }
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
    fn metric_column_metadata_tags_its_source() {
        let metadata = MetricColumnMetadata {
            metric_set: "outputs".to_string(),
            source: MetricSource::CoreNode {
                node: "res".to_string(),
                sub_name: Some("spill".to_string()),
                node_type: "Link".to_string(),
            },
            attribute: "Inflow".to_string(),
        };
        let json = serde_json::to_value(&metadata).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "metric_set": "outputs",
                "source": {"type": "CoreNode", "node": "res", "sub_name": "spill", "node_type": "Link"},
                "attribute": "Inflow",
            })
        );
        assert_eq!(serde_json::from_value::<MetricColumnMetadata>(json).unwrap(), metadata);
    }

    #[test]
    fn metric_source_labels() {
        let edge = MetricSource::Edge {
            from_node: "a".to_string(),
            from_slot: Some("x".to_string()),
            to_node: "b".to_string(),
            to_slot: None,
        };
        assert_eq!(edge.label(), "a[x]->b");
        assert_eq!(edge.kind(), "Edge");
        assert_eq!(edge.sub_kind(), None);

        let local = MetricSource::Parameter {
            name: "control".to_string(),
            parent: Some("res".to_string()),
            parameter_type: "Constant".to_string(),
        };
        assert_eq!(local.label(), "res.control");
        assert_eq!(local.sub_kind(), Some("Constant"));

        let core = MetricSource::CoreNode {
            node: "res".to_string(),
            sub_name: None,
            node_type: "Storage".to_string(),
        };
        assert_eq!(core.label(), "res");
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
