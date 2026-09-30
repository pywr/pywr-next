use pywr_schema_macros::PywrVisitAll;
use relative_path::RelativePathBuf;
use schemars::JsonSchema;
use std::collections::HashMap;

/// The project-relative source of a component in a composed model.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq, Eq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceSource {
    #[schemars(with = "String")]
    pub file: RelativePathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_set: Option<String>,
}

/// Origin of a definition and any sources subsequently merged into it.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq, Eq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentProvenance {
    pub origin: ProvenanceSource,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contributors: Vec<ProvenanceSource>,
}

// Provenance paths describe source files; they are not model data paths to rewrite.
impl crate::visit::VisitPaths for ComponentProvenance {}
impl crate::visit::VisitMetrics for ComponentProvenance {}
impl crate::visit::VisitReferences for ComponentProvenance {}

impl ComponentProvenance {
    pub fn new(origin: ProvenanceSource) -> Self {
        Self {
            origin,
            contributors: Vec::new(),
        }
    }

    pub fn merge(&mut self, other: Self) {
        for source in std::iter::once(other.origin).chain(other.contributors) {
            if source != self.origin && !self.contributors.contains(&source) {
                self.contributors.push(source);
            }
        }
    }
}

/// Access to composition metadata across different component metadata layouts.
pub trait ComponentMeta {
    fn provenance(&self) -> Option<&ComponentProvenance>;
    fn set_provenance(&mut self, provenance: ComponentProvenance);
    fn clear_provenance(&mut self);
}

/// Metadata shared by named network components without component-specific fields.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Default, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct NamedMeta {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub tags: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<ComponentProvenance>,
}

impl ComponentMeta for NamedMeta {
    fn provenance(&self) -> Option<&ComponentProvenance> {
        self.provenance.as_ref()
    }

    fn set_provenance(&mut self, provenance: ComponentProvenance) {
        self.provenance = Some(provenance);
    }

    fn clear_provenance(&mut self) {
        self.provenance = None;
    }
}
#[cfg(test)]
mod tests {
    use super::ProvenanceSource;
    use crate::metric_sets::MetricSet;
    use crate::outputs::Output;

    #[test]
    fn provenance_file_round_trips_as_a_portable_relative_path() {
        let source: ProvenanceSource = serde_json::from_str(r#"{"file":"nested/set/network.json"}"#).unwrap();
        assert_eq!(source.file.as_str(), "nested/set/network.json");
        assert_eq!(
            serde_json::to_value(&source).unwrap()["file"],
            "nested/set/network.json"
        );
        assert_eq!(
            serde_json::from_value::<ProvenanceSource>(serde_json::to_value(&source).unwrap()).unwrap(),
            source
        );
    }

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
