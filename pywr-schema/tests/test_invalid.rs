use pywr_schema::{ModelSchema, ValidationError};
#[cfg(feature = "core")]
use pywr_schema::{ModelSchemaBuildError, NetworkSchemaBuildError};
use std::fs;
use std::path::Path;
#[cfg(feature = "core")]
use tempfile::TempDir;

macro_rules! invalid_tests {
    ($($test_func:ident: $value:expr, $expected_err:ident,)*) => {
    $(
        #[test]
        fn $test_func() {
            // Deserialise the schema and run it
            #[cfg(feature = "core")]
            {
                let input: &str = $value;
                let input_pth = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("invalid").join(input);

                let schema = deserialise_test_model(&input_pth);
                let err = build_test_model(&schema);
                if !matches!(err, ModelSchemaBuildError::$expected_err { .. }) {
                    panic!("Expected error: PywrModelBuildError::{}, but got: {:?}", stringify!($expected_err), err);
                };
            }

            // Just deserialise the schema
            #[cfg(not(feature = "core"))]
            {
                let input: &str = $value;
                let input_pth = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("invalid").join(input);
                let _schema = deserialise_test_model(&input_pth);
            }
        }
    )*
    }
}

invalid_tests! {
    agg_storage_with_flow_node: "agg-storage-with-flow-node.json", NetworkBuildError,
}

/// Models that are rejected by [`ModelSchema::validate`].
macro_rules! invalid_schema_tests {
    ($($test_func:ident: $value:expr, $expected_err:ident,)*) => {
    $(
        #[test]
        fn $test_func() {
            let input: &str = $value;
            let input_pth = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("invalid").join(input);

            let schema = deserialise_test_model(&input_pth);

            match schema.validate() {
                Ok(()) => panic!("Expected validation to fail, but the schema was valid!"),
                Err(e) => {
                    if !matches!(e, ValidationError::$expected_err { .. }) {
                        panic!("Expected error: ValidationError::{}, but got: {:?}", stringify!($expected_err), e);
                    }
                }
            }

            // The same error must also stop the model being built.
            #[cfg(feature = "core")]
            {
                match build_test_model(&schema) {
                    ModelSchemaBuildError::NetworkBuildError { source } => {
                        if !matches!(*source, NetworkSchemaBuildError::Validation { .. }) {
                            panic!("Expected a validation error when building, but got: {:?}", source);
                        }
                    }
                    e => panic!("Expected ModelSchemaBuildError::NetworkBuildError, but got: {e:?}"),
                }
            }
        }
    )*
    }
}

invalid_schema_tests! {
    // Two virtual nodes sharing a name. The two are built into separate pywr-core collections,
    // so the core builder never sees a clash.
    duplicate_virtual_node_name: "duplicate-virtual-node-name.json", DuplicateNodeNames,
    // A simple and a composite node sharing a name. The composite node expands only to
    // sub-named core nodes, so again the core builder never sees a clash. Validation is the
    // only thing standing between this model and a silently wrong network.
    duplicate_node_name_with_composite: "duplicate-node-name-with-composite.json", DuplicateNodeNames,
}

fn deserialise_test_model(model_path: &Path) -> ModelSchema {
    let data = fs::read_to_string(model_path).expect("Unable to read file");
    serde_json::from_str(&data).expect("Failed to deserialize model")
}

#[cfg(feature = "core")]
fn build_test_model(schema: &ModelSchema) -> ModelSchemaBuildError {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("invalid");
    match schema.create_model_builder(Some(&data_dir), Some(temp_dir.path())) {
        Ok(_) => panic!("Expected an error, but model built successfully!"),
        Err(e) => e,
    }
}
