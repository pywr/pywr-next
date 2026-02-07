use pywr_schema::ModelSchema;
#[cfg(feature = "core")]
use pywr_schema::ModelSchemaBuildError;
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

fn deserialise_test_model(model_path: &Path) -> ModelSchema {
    let data = fs::read_to_string(model_path).expect("Unable to read file");
    serde_json::from_str(&data).expect("Failed to deserialize model")
}

#[cfg(feature = "core")]
fn build_test_model(schema: &ModelSchema) -> ModelSchemaBuildError {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("invalid");
    match schema.build_model(Some(&data_dir), Some(temp_dir.path())) {
        Ok(_) => panic!("Expected an error, but model built successfully!"),
        Err(e) => e,
    }
}
