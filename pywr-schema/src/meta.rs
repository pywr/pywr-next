use pywr_schema_macros::PywrVisitAll;
use schemars::JsonSchema;
use std::collections::HashMap;

/// Metadata shared by named network components without component-specific fields.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Default, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct NamedMeta {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub tags: HashMap<String, String>,
}
#[cfg(test)]
mod tests {
    use crate::metric_sets::MetricSet;
    use crate::outputs::Output;

    #[test]
    fn named_components_round_trip_metadata() {
        let json = serde_json::json!({
            "meta": {"name": "metrics", "comment": "a set", "tags": {"team": "planning"}},
            "filters": {"all_nodes": true}
        });
        let mut metrics: MetricSet = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(metrics.name(), "metrics");
        metrics.meta_mut().name = "renamed".to_string();
        assert_eq!(
            serde_json::to_value(&metrics).unwrap()["meta"]["tags"]["team"],
            "planning"
        );

        let mut output: Output = serde_json::from_value(serde_json::json!({
            "type": "Placeholder", "meta": {"name": "out"}
        }))
        .unwrap();
        assert_eq!(output.name(), "out");
        output.meta_mut().comment = Some("reserved".to_string());
        assert_eq!(serde_json::to_value(&output).unwrap()["meta"]["comment"], "reserved");
        assert!(serde_json::from_value::<MetricSet>(serde_json::json!({"name": "old"})).is_err());
    }
}
