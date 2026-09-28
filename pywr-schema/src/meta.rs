use pywr_schema_macros::PywrVisitAll;
use relative_path::RelativePathBuf;
use schemars::JsonSchema;
use serde::Serialize;
use std::collections::HashMap;

fn serialize_with_relative_path<S>(path: &str, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let rel_path = RelativePathBuf::from_path(path).map_err(serde::ser::Error::custom)?;
    rel_path.serialize(serializer)
}

/// The project-relative source of a component in a composed model.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq, Eq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceSource {
    #[serde(serialize_with = "serialize_with_relative_path")]
    pub file: String,
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
