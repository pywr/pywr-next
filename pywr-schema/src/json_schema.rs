//! A single JSON Schema covering the Pywr v1 and v2 model file formats.
//!
//! Only whole model files are covered. A bare network file (a v2 `NetworkSchema` or a v1 network)
//! has no identifying property and so no entry in the union.

use pywr_v1_schema::json_schema::{
    CustomTypes, model_schema as v1_model_schema, multi_model_schema as v1_multi_model_schema,
};
use pywr_v1_schema::{PywrModel, PywrMultiModel};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde_json::{Map, Value, json};
use thiserror::Error;

const DEFS_POINTER: &str = "#/$defs/";

/// Errors building the union schema.
///
/// Each variant means that a schema generator produced a shape this module does not handle, for
/// example after a `schemars` upgrade.
#[derive(Debug, Error)]
pub enum JsonSchemaError {
    #[error("the root schema of `{0}` is not a JSON object")]
    RootNotAnObject(String),
    #[error("the root schema of `{0}` has no definitions")]
    RootWithoutDefinitions(String),
    #[error("there is no definition for `{0}`")]
    MissingDefinition(String),
    #[error("unexpected `$ref` form: `{0}`")]
    UnexpectedReference(String),
    #[error("definition `{0}` differs between documents of one version")]
    ConflictingDefinition(String),
    #[error("the union is not a JSON object")]
    UnionNotAnObject,
}

/// A document kind, identified by the property that only that kind has.
struct Branch {
    /// The Pywr major version of the documents, as shown in the title.
    version: &'static str,
    /// The property that identifies the document kind.
    key: &'static str,
    /// The name of the definition that holds the root schema of the document, with its prefix.
    root: String,
}

/// Prefix every `#/$defs/<name>` reference in `value` with `prefix`.
///
/// Every string under a `$ref` key is taken to be a reference, including one inside a literal such
/// as a `default`; none of the generated schemas has such a literal.
fn prefix_refs(value: &mut Value, prefix: &str) -> Result<(), JsonSchemaError> {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                match child {
                    Value::String(reference) if key == "$ref" => {
                        let name = reference
                            .strip_prefix(DEFS_POINTER)
                            .ok_or_else(|| JsonSchemaError::UnexpectedReference(reference.clone()))?;
                        *reference = format!("{DEFS_POINTER}{prefix}{name}");
                    }
                    _ => prefix_refs(child, prefix)?,
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                prefix_refs(item, prefix)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Move `definitions` into `defs` under `prefix`.
///
/// Documents of one version share definitions, which must then be identical.
fn add_definitions(
    defs: &mut Map<String, Value>,
    definitions: Map<String, Value>,
    prefix: &str,
) -> Result<(), JsonSchemaError> {
    for (name, mut definition) in definitions {
        prefix_refs(&mut definition, prefix)?;
        let name = format!("{prefix}{name}");
        if let Some(existing) = defs.insert(name.clone(), definition.clone())
            && existing != definition
        {
            return Err(JsonSchemaError::ConflictingDefinition(name));
        }
    }
    Ok(())
}

/// Move the definitions of the root schema `schema` of `T` into `defs` under `prefix`, with the
/// root as one more definition, and return the name of that definition.
fn add_root_schema<T: JsonSchema>(
    defs: &mut Map<String, Value>,
    schema: Schema,
    prefix: &str,
) -> Result<String, JsonSchemaError> {
    let name = T::schema_name().into_owned();
    let Value::Object(mut root) = schema.to_value() else {
        return Err(JsonSchemaError::RootNotAnObject(name));
    };
    root.remove("$schema");
    // `schemars` titles a root schema with the type name, which a definition of the same type
    // (as it appears in a multi-model) does not carry; without this the two would differ.
    root.remove("title");
    let Some(Value::Object(mut definitions)) = root.remove("$defs") else {
        return Err(JsonSchemaError::RootWithoutDefinitions(name));
    };
    definitions.insert(name.clone(), Value::Object(root));
    add_definitions(defs, definitions, prefix)?;
    Ok(format!("{prefix}{name}"))
}

/// Add the v2 document types `ModelSchema` and `MultiNetworkModelSchema` to `defs`.
///
/// One generator produces both, so they share one definition namespace by construction.
fn add_v2_documents(defs: &mut Map<String, Value>) -> Result<(String, String), JsonSchemaError> {
    const PREFIX: &str = "v2_";
    let mut generator = SchemaGenerator::default();
    generator.subschema_for::<crate::ModelSchema>();
    generator.subschema_for::<crate::MultiNetworkModelSchema>();
    let definitions = generator.take_definitions(true);

    let roots = [
        crate::ModelSchema::schema_name().into_owned(),
        crate::MultiNetworkModelSchema::schema_name().into_owned(),
    ];
    if let Some(missing) = roots.iter().find(|name| !definitions.contains_key(*name)) {
        return Err(JsonSchemaError::MissingDefinition(missing.clone()));
    }
    add_definitions(defs, definitions, PREFIX)?;
    let [model, multi] = roots;
    Ok((format!("{PREFIX}{model}"), format!("{PREFIX}{multi}")))
}

/// The JSON Schema for any Pywr model file, v1 or v2.
///
/// A document is one of four kinds, told apart by its identifying property: `nodes` (v1 model),
/// `models` (v1 multi-model), `network` (v2 model) or `networks` (v2 multi-network model). Exactly
/// one of those properties must be present. The schema of the kind that is present is applied
/// through an `if`/`then`, so that an invalid document reports the errors of its own kind rather
/// than a bare "not valid under any of the given schemas". The definitions of each version are
/// namespaced `v1_` or `v2_` so that the two cannot collide.
///
/// `v1_custom_types` selects how the v1 schemas treat nodes and parameters that fail to match a
/// core definition; see [`CustomTypes`].
///
/// # Errors
///
/// Returns an error if a schema generator produces a shape this function cannot namespace; see
/// [`JsonSchemaError`].
pub fn pywr_model_schema(v1_custom_types: CustomTypes) -> Result<Schema, JsonSchemaError> {
    let mut defs = Map::new();
    let v1_model = add_root_schema::<PywrModel>(&mut defs, v1_model_schema(v1_custom_types), "v1_")?;
    let v1_multi = add_root_schema::<PywrMultiModel>(&mut defs, v1_multi_model_schema(v1_custom_types), "v1_")?;
    let (v2_model, v2_multi) = add_v2_documents(&mut defs)?;

    let branches = [
        Branch {
            version: "v1",
            key: "nodes",
            root: v1_model,
        },
        Branch {
            version: "v1",
            key: "models",
            root: v1_multi,
        },
        Branch {
            version: "v2",
            key: "network",
            root: v2_model,
        },
        Branch {
            version: "v2",
            key: "networks",
            root: v2_multi,
        },
    ];

    let one_of: Vec<Value> = branches
        .iter()
        .map(|branch| {
            json!({
                "title": format!("Pywr {} document with `{}`", branch.version, branch.key),
                "required": [branch.key],
            })
        })
        .collect();
    let all_of: Vec<Value> = branches
        .iter()
        .map(|branch| {
            let others: Map<String, Value> = branches
                .iter()
                .filter(|other| other.key != branch.key)
                .map(|other| (other.key.to_string(), Value::Bool(false)))
                .collect();
            json!({
                "if": { "required": [branch.key] },
                "then": {
                    "properties": others,
                    "$ref": format!("{DEFS_POINTER}{}", branch.root),
                },
            })
        })
        .collect();

    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Pywr model (v1 or v2)",
        "description": "A Pywr model: v1 (`nodes`, or `models` for a multi-model) or v2 (`network`, or `networks` for a multi-network model).",
        "oneOf": one_of,
        "allOf": all_of,
        "$defs": defs,
    });
    Schema::try_from(schema).map_err(|_| JsonSchemaError::UnionNotAnObject)
}

#[cfg(test)]
mod tests {
    use super::pywr_model_schema;
    use crate::{ModelSchema, MultiNetworkModelSchema};
    use jsonschema::Validator;
    use pywr_v1_schema::json_schema::CustomTypes;
    use schemars::{JsonSchema, schema_for};
    use serde_json::{Value, json};
    use std::fs;
    use std::path::{Path, PathBuf};

    fn tests_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests")
    }

    fn union_schema(custom_types: CustomTypes) -> Value {
        let schema = pywr_model_schema(custom_types).expect("the union is built");
        serde_json::to_value(schema).expect("schema serialises")
    }

    fn validator(custom_types: CustomTypes) -> Validator {
        jsonschema::validator_for(&union_schema(custom_types)).expect("the union is a valid JSON Schema")
    }

    fn read(path: &Path) -> Value {
        let data = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        serde_json::from_str(&data).unwrap_or_else(|e| panic!("parse {path:?}: {e}"))
    }

    fn errors(validator: &Validator, document: &Value) -> Vec<String> {
        validator.iter_errors(document).map(|e| e.to_string()).collect()
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

    /// The v1 models used to test conversion: the files in `tests/v1` with a `nodes` property.
    fn v1_model_files() -> Vec<PathBuf> {
        let dir = tests_dir().join("v1");
        let files: Vec<PathBuf> = fs::read_dir(&dir)
            .expect("v1 test models")
            .map(|entry| entry.expect("directory entry").path())
            .filter(|path| read(path).get("nodes").is_some())
            .collect();
        assert!(!files.is_empty());
        files
    }

    fn v2_model() -> Value {
        read(&tests_dir().join("simple1.json"))
    }

    fn v2_multi_network_model() -> Value {
        read(&tests_dir().join("multi1").join("model.json"))
    }

    fn v1_model() -> Value {
        read(&tests_dir().join("v1").join("scenarios.json"))
    }

    fn without(mut document: Value, pointer: &str) -> Value {
        let (parent, key) = pointer.rsplit_once('/').expect("a pointer with a parent");
        document
            .pointer_mut(parent)
            .and_then(Value::as_object_mut)
            .expect("the parent is an object")
            .remove(key)
            .expect("the key is present");
        document
    }

    fn with(mut document: Value, pointer: &str, value: Value) -> Value {
        *document.pointer_mut(pointer).expect("the pointer is present") = value;
        document
    }

    #[test]
    fn v2_models_validate() {
        let validator = validator(CustomTypes::Any);
        let mut files = model_files(&tests_dir());
        files.extend(model_files(&tests_dir().join("multi1")));
        files.extend(model_files(&tests_dir().join("multi2")));
        for path in files {
            let errors = errors(&validator, &read(&path));
            assert!(errors.is_empty(), "{path:?}: {errors:?}");
        }
    }

    #[test]
    fn v1_models_validate() {
        let validator = validator(CustomTypes::Any);
        for path in v1_model_files() {
            let errors = errors(&validator, &read(&path));
            assert!(errors.is_empty(), "{path:?}: {errors:?}");
        }
    }

    /// Documents without exactly one identifying property are rejected, whatever else they hold.
    #[test]
    fn documents_without_exactly_one_identifying_property_are_rejected() {
        let validator = validator(CustomTypes::Any);
        let (v1, v2, multi) = (v1_model(), v2_model(), v2_multi_network_model());
        assert!(validator.is_valid(&v1) && validator.is_valid(&v2) && validator.is_valid(&multi));

        let mut v2_with_nodes = v2.clone();
        v2_with_nodes["nodes"] = v1["nodes"].clone();
        let mut v1_with_network = v1.clone();
        v1_with_network["network"] = v2["network"].clone();
        let mut v1_with_models = v1.clone();
        v1_with_models["models"] = json!([]);
        let mut v2_with_networks = v2.clone();
        v2_with_networks["networks"] = multi["networks"].clone();
        let mut multi_with_network = multi.clone();
        multi_with_network["network"] = v2["network"].clone();

        let rejected = [
            ("an empty object", json!({})),
            ("an array", json!([])),
            ("a string", json!("x")),
            ("no identifying property", without(v2.clone(), "/network")),
            ("`nodes` and `network`", v2_with_nodes.clone()),
            ("`network` and `nodes`", v1_with_network),
            ("`nodes` and `models`", v1_with_models),
            ("`network` and `networks`", v2_with_networks),
            ("`networks` and `network`", multi_with_network),
        ];
        for (name, document) in rejected {
            assert!(!validator.is_valid(&document), "{name} must be rejected");
        }

        // A document with two identifying properties is reported against the one it is not allowed to have.
        let mixed = errors(&validator, &v2_with_nodes);
        assert!(mixed.iter().any(|e| e.contains("False schema")), "{mixed:?}");
    }

    /// An invalid document is checked against the schema of its own kind, and the errors say what is
    /// wrong with it.
    #[test]
    fn invalid_documents_are_rejected_by_their_own_kind() {
        let validator = validator(CustomTypes::Any);

        let v2_without_time = errors(&validator, &without(v2_model(), "/time"));
        assert!(
            v2_without_time
                .iter()
                .any(|e| e.contains("\"time\" is a required property")),
            "{v2_without_time:?}"
        );

        let v1_without_timestepper = errors(&validator, &without(v1_model(), "/timestepper"));
        assert!(
            v1_without_timestepper
                .iter()
                .any(|e| e.contains("\"timestepper\" is a required property")),
            "{v1_without_timestepper:?}"
        );

        let multi_without_time = errors(&validator, &without(v2_multi_network_model(), "/time"));
        assert!(
            multi_without_time
                .iter()
                .any(|e| e.contains("\"time\" is a required property")),
            "{multi_without_time:?}"
        );

        let invalid = [
            (
                "v2 model, unknown node type",
                with(v2_model(), "/network/nodes/0/type", json!("Bogus")),
            ),
            (
                "v2 model, `network` is a number",
                with(v2_model(), "/network", json!(5)),
            ),
            ("v2 model, `time` is a number", with(v2_model(), "/time", json!(5))),
            (
                "v2 multi-network model, entry `network` is a number",
                with(v2_multi_network_model(), "/networks/0/network", json!(5)),
            ),
            ("v1 model, an edge is a number", with(v1_model(), "/edges/0", json!(5))),
            ("v1 model, `nodes` is a string", with(v1_model(), "/nodes", json!("x"))),
        ];
        for (name, document) in invalid {
            assert!(!validator.is_valid(&document), "{name} must be rejected");
        }
    }

    /// A node of an unknown type is a custom node to the v1 deserialisers, so a v1 model with one is valid.
    #[test]
    fn v1_unknown_node_types_are_custom_nodes() {
        let document = with(v1_model(), "/nodes/0/type", json!("Bogus"));
        assert!(validator(CustomTypes::Any).is_valid(&document));
    }

    /// The v2 half of the union is the standalone v2 schema, with every definition under the `v2_` prefix.
    #[test]
    fn v2_definitions_match_the_standalone_schema() {
        let union = union_schema(CustomTypes::Any);
        let union_defs = union["$defs"].as_object().expect("the union has definitions");

        for (root, mut standalone) in [
            (
                ModelSchema::schema_name(),
                serde_json::to_value(schema_for!(ModelSchema)),
            ),
            (
                MultiNetworkModelSchema::schema_name(),
                serde_json::to_value(schema_for!(MultiNetworkModelSchema)),
            ),
        ]
        .map(|(name, schema)| (name, schema.expect("schema serialises")))
        {
            let standalone = standalone.as_object_mut().expect("a root schema is an object");
            standalone.remove("$schema");
            standalone.remove("title");
            let definitions = standalone.remove("$defs").expect("a root schema has definitions");
            let definitions = definitions.as_object().expect("definitions are an object");
            for (name, definition) in definitions {
                let unioned = union_defs
                    .get(&format!("v2_{name}"))
                    .expect("definition is in the union");
                assert_eq!(&strip_prefix(unioned, "v2_"), definition, "definition {name}");
            }
            let unioned_root = union_defs.get(&format!("v2_{root}")).expect("the root is in the union");
            assert_eq!(strip_prefix(unioned_root, "v2_"), Value::Object(standalone.clone()));
        }
    }

    /// Undo the namespacing of `$ref`s.
    fn strip_prefix(value: &Value, prefix: &str) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(key, child)| match child {
                        Value::String(reference) if key == "$ref" => (
                            key.clone(),
                            Value::String(reference.replacen(&format!("#/$defs/{prefix}"), "#/$defs/", 1)),
                        ),
                        _ => (key.clone(), strip_prefix(child, prefix)),
                    })
                    .collect(),
            ),
            Value::Array(items) => Value::Array(items.iter().map(|item| strip_prefix(item, prefix)).collect()),
            other => other.clone(),
        }
    }
}
