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
//! scenarios ([`SCENARIOS_KEY`]) and scenario groups ([`SCENARIO_GROUPS_KEY`]) as JSON.
//!
//! The `arrow` feature adds `MetricColumnExtension`, an implementation of Arrow's
//! `ExtensionType` for metric columns, at the cost of a dependency on `arrow-schema`.
use serde::{Deserialize, Serialize};

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
    /// The index of this scenario within each scenario group.
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
    fn parses_scenario_metadata() {
        let scenarios: Vec<ScenarioMetadata> =
            serde_json::from_str(r#"[{"simulation_id":0,"simulation_indices":[1],"scenario_labels":["wet"]}]"#)
                .unwrap();
        assert_eq!(scenarios[0].scenario_labels, ["wet"]);
        let groups: Vec<ScenarioGroupMetadata> = serde_json::from_str(r#"[{"name":"case","size":2}]"#).unwrap();
        assert_eq!(groups[0].size, 2);
    }
}
