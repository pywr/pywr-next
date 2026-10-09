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
/// `models` (v1 multi-model), `network` (v2 model) or `networks` (v2 multi-network model). Each kind
/// forbids the other kinds' properties so that a file mixing them is rejected. The definitions of each version are
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
            let others: Map<String, Value> = branches
                .iter()
                .filter(|other| other.key != branch.key)
                .map(|other| (other.key.to_string(), Value::Bool(false)))
                .collect();
            json!({
                "title": format!("Pywr {} document with `{}`", branch.version, branch.key),
                "required": [branch.key],
                "properties": others,
                "$ref": format!("{DEFS_POINTER}{}", branch.root),
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
    Schema::try_from(schema).map_err(|_| JsonSchemaError::UnionNotAnObject)
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
        let schema = serde_json::to_value(pywr_model_schema(custom_types).expect("the union is built"))
            .expect("schema serialises");
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
