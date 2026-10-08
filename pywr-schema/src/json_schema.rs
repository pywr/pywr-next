//! A single JSON Schema covering the Pywr v1 and v2 model file formats.

use pywr_v1_schema::json_schema::{
    CustomTypes, model_schema as v1_model_schema, multi_model_schema as v1_multi_model_schema,
};
use schemars::{JsonSchema, Schema, schema_for};
use serde_json::{Map, Value, json};

const DEFS_POINTER: &str = "#/$defs/";

/// A document kind, identified by the property that only that kind has.
struct Branch {
    /// Prefix that namespaces the definitions of every document of one version.
    prefix: &'static str,
    /// The property that identifies the document kind.
    key: &'static str,
    schema: Schema,
}

/// Prefix every `#/$defs/<name>` reference in `value` with `prefix`.
fn prefix_refs(value: &mut Value, prefix: &str) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                match child {
                    Value::String(reference) if key == "$ref" => {
                        let name = reference
                            .strip_prefix(DEFS_POINTER)
                            .unwrap_or_else(|| panic!("unexpected $ref form: {reference}"));
                        *reference = format!("{DEFS_POINTER}{prefix}{name}");
                    }
                    _ => prefix_refs(child, prefix),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| prefix_refs(item, prefix)),
        _ => {}
    }
}

/// Move the definitions of `schema` into `defs` under `prefix`, with its root as one more definition.
///
/// Returns the name of the definition that holds the root. Two documents of one version share
/// definitions, which must then be identical.
fn add_namespaced(defs: &mut Map<String, Value>, schema: Schema, prefix: &str) -> String {
    let Value::Object(mut root) = schema.to_value() else {
        panic!("a root schema is an object");
    };
    root.remove("$schema");
    let Some(Value::Object(definitions)) = root.remove("$defs") else {
        panic!("a root schema has definitions");
    };
    let Some(Value::String(title)) = root.remove("title") else {
        panic!("a root schema has a title");
    };

    let root_name = format!("{prefix}{title}");
    let definitions = definitions
        .into_iter()
        .map(|(name, definition)| (format!("{prefix}{name}"), definition))
        .chain([(root_name.clone(), Value::Object(root))]);
    for (name, mut definition) in definitions {
        prefix_refs(&mut definition, prefix);
        if let Some(existing) = defs.insert(name.clone(), definition.clone()) {
            assert_eq!(
                existing, definition,
                "definition {name} differs between documents of one version"
            );
        }
    }
    root_name
}

fn v2_schema<T: JsonSchema>() -> Schema {
    schema_for!(T)
}

/// The JSON Schema for any Pywr model file, v1 or v2.
///
/// A document is one of four kinds, told apart by its identifying property: `nodes` (v1 model),
/// `models` (v1 multi-model), `network` (v2 model) or `networks` (v2 multi-network model). The
/// definitions of each version are namespaced `v1_` or `v2_` so that the two cannot collide, and
/// each kind forbids the other kinds' properties so that a file mixing them is rejected.
///
/// `v1_custom_types` selects how the v1 schemas treat nodes and parameters that fail to match a
/// core definition; see [`CustomTypes`].
pub fn pywr_model_schema(v1_custom_types: CustomTypes) -> Schema {
    let branches = [
        Branch {
            prefix: "v1_",
            key: "nodes",
            schema: v1_model_schema(v1_custom_types),
        },
        Branch {
            prefix: "v1_",
            key: "models",
            schema: v1_multi_model_schema(v1_custom_types),
        },
        Branch {
            prefix: "v2_",
            key: "network",
            schema: v2_schema::<crate::ModelSchema>(),
        },
        Branch {
            prefix: "v2_",
            key: "networks",
            schema: v2_schema::<crate::MultiNetworkModelSchema>(),
        },
    ];

    let mut defs = Map::new();
    let keys: Vec<&str> = branches.iter().map(|branch| branch.key).collect();
    let one_of: Vec<Value> = branches
        .into_iter()
        .map(|branch| {
            let root = add_namespaced(&mut defs, branch.schema, branch.prefix);
            let others: Map<String, Value> = keys
                .iter()
                .filter(|key| **key != branch.key)
                .map(|key| (key.to_string(), Value::Bool(false)))
                .collect();
            json!({
                "title": format!("Pywr {} document with `{}`", &branch.prefix[..2], branch.key),
                "required": [branch.key],
                "properties": others,
                "$ref": format!("{DEFS_POINTER}{root}"),
            })
        })
        .collect();

    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Pywr model (v1 or v2)",
        "description": "A Pywr model: v1 (`nodes`, or `models` for a multi-model) or v2 (`network`, or `networks` for a multi-network model).",
        "oneOf": one_of,
        "$defs": defs,
    });
    Schema::try_from(schema).expect("the union is a JSON object")
}

#[cfg(test)]
mod tests {
    use super::pywr_model_schema;
    use jsonschema::Validator;
    use pywr_v1_schema::json_schema::CustomTypes;
    use serde_json::Value;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn tests_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests")
    }

    fn validator(custom_types: CustomTypes) -> Validator {
        let schema = serde_json::to_value(pywr_model_schema(custom_types)).expect("schema serialises");
        jsonschema::validator_for(&schema).expect("the union is a valid JSON Schema")
    }

    fn read(path: &Path) -> Value {
        let data = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        serde_json::from_str(&data).unwrap_or_else(|e| panic!("parse {path:?}: {e}"))
    }

    /// The JSON files directly in `dir` that are model documents: those with a `time` property.
    fn model_files(dir: &Path) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("read {dir:?}: {e}"))
            .map(|entry| entry.expect("directory entry").path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .filter(|path| read(path).get("time").is_some())
            .collect();
        files.sort();
        assert!(!files.is_empty(), "no models in {dir:?}");
        files
    }

    #[test]
    fn v2_models_validate() {
        let validator = validator(CustomTypes::Any);
        let mut files = model_files(&tests_dir());
        files.extend(model_files(&tests_dir().join("multi1")));
        files.extend(model_files(&tests_dir().join("multi2")));
        for path in files {
            let errors: Vec<String> = validator.iter_errors(&read(&path)).map(|e| e.to_string()).collect();
            assert!(errors.is_empty(), "{path:?}: {errors:?}");
        }
    }

    /// The v1 models used to test conversion are v1 documents, so only the v1 branch accepts them.
    #[test]
    fn v1_models_validate() {
        let validator = validator(CustomTypes::Any);
        let dir = tests_dir().join("v1");
        let v1_models: Vec<PathBuf> = fs::read_dir(&dir)
            .expect("v1 test models")
            .map(|entry| entry.expect("directory entry").path())
            .filter(|path| read(path).get("nodes").is_some())
            .collect();
        assert!(!v1_models.is_empty());
        for path in v1_models {
            let errors: Vec<String> = validator.iter_errors(&read(&path)).map(|e| e.to_string()).collect();
            assert!(errors.is_empty(), "{path:?}: {errors:?}");
        }
    }

    #[test]
    fn mixed_or_unidentified_documents_are_rejected() {
        let validator = validator(CustomTypes::Any);
        let v2 = read(&tests_dir().join("simple1.json"));
        let v1 = read(&tests_dir().join("v1").join("scenarios.json"));
        assert!(validator.is_valid(&v2) && validator.is_valid(&v1));

        let mut both = v2.clone();
        both["nodes"] = v1["nodes"].clone();
        assert!(
            !validator.is_valid(&both),
            "`nodes` and `network` together must be rejected"
        );

        let mut neither = v2;
        neither.as_object_mut().expect("a model is an object").remove("network");
        assert!(
            !validator.is_valid(&neither),
            "a document with no identifying property must be rejected"
        );
    }
}
