use pywr_schema::template::{LookupError, TemplateError};
use pywr_schema::{ModelSchema, ModelSchemaReadError, NetworkSchema, NetworkSchemaReadError};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

fn tests_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

/// A test model with the url of its first table templated.
fn templated_model() -> Value {
    let path = tests_dir().join("daily-profile1.json");
    let mut model: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    *model.pointer_mut("/network/tables/0/url").unwrap() = json!("${dataset}/daily-profile-365.csv");
    model
}

fn write_json(dir: &Path, name: &str, value: &Value) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, serde_json::to_string_pretty(value).unwrap()).unwrap();
    path
}

fn dataset(name: &str) -> Result<String, LookupError> {
    match name {
        "dataset" => Ok("data/stoch-01".to_string()),
        _ => Err(LookupError::Undefined),
    }
}

/// Without templates, reading through a `serde_json::Value` must give the same schema as reading
/// the text directly.
#[test]
fn models_without_templates_load_as_they_do_without_variables() {
    let mut compared = 0;
    for entry in fs::read_dir(tests_dir()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        // Skip files that are not v2 models, such as v1 models and multi-network models.
        let Ok(expected) = ModelSchema::from_path(&path) else {
            continue;
        };
        let actual = ModelSchema::from_path_with(&path, |used| {
            panic!(
                "`{}` uses variable `{}` at `{}`",
                path.display(),
                used.name,
                used.pointer
            )
        })
        .unwrap_or_else(|error| panic!("`{}` failed to load: {error:?}", path.display()));
        assert_eq!(
            serde_json::to_value(&actual).unwrap(),
            serde_json::to_value(&expected).unwrap(),
            "`{}` loaded differently",
            path.display()
        );
        compared += 1;
    }
    assert!(compared > 20, "only {compared} models were compared");
}

#[test]
fn templates_in_models_and_networks_are_filled_in() {
    let dir = tempfile::tempdir().unwrap();
    let model = templated_model();
    let model_path = write_json(dir.path(), "model.json", &model);
    let network_path = write_json(dir.path(), "network.json", &model["network"]);

    let model = ModelSchema::from_path_with(&model_path, |used| dataset(&used.name)).unwrap();
    assert_eq!(
        serde_json::to_value(&model).unwrap().pointer("/network/tables/0/url"),
        Some(&json!("data/stoch-01/daily-profile-365.csv"))
    );

    let network = NetworkSchema::from_path_with(&network_path, |used| dataset(&used.name)).unwrap();
    assert_eq!(
        serde_json::to_value(&network).unwrap().pointer("/tables/0/url"),
        Some(&json!("data/stoch-01/daily-profile-365.csv"))
    );

    // Read without variables, the template is left as it is.
    let unfilled = ModelSchema::from_path(&model_path).unwrap();
    assert_eq!(
        serde_json::to_value(&unfilled)
            .unwrap()
            .pointer("/network/tables/0/url"),
        Some(&json!("${dataset}/daily-profile-365.csv"))
    );
}

#[test]
fn template_errors_name_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let model_path = write_json(dir.path(), "model.json", &templated_model());
    let undefined = TemplateError::Undefined {
        pointer: "/network/tables/0/url".to_string(),
        name: "dataset".to_string(),
    };

    match ModelSchema::from_path_with(&model_path, |_| Err(LookupError::Undefined)) {
        Err(ModelSchemaReadError::Template { path, source }) => {
            assert_eq!(path, model_path);
            assert_eq!(source, undefined);
        }
        other => panic!("expected a template error, got {:?}", other.err()),
    }

    let network_path = write_json(dir.path(), "network.json", &templated_model()["network"]);
    match NetworkSchema::from_path_with(&network_path, |_| Err(LookupError::Undefined)) {
        Err(NetworkSchemaReadError::Template { path, source }) => {
            assert_eq!(path, network_path);
            assert_eq!(
                source,
                TemplateError::Undefined {
                    pointer: "/tables/0/url".to_string(),
                    name: "dataset".to_string(),
                }
            );
        }
        other => panic!("expected a template error, got {:?}", other.err()),
    }
}
