use pywr_schema::{ModelSchema, NetworkProblem, ScenarioProblem};
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
                    let found = e
                        .networks
                        .iter()
                        .flat_map(|network| &network.problems)
                        .any(|problem| matches!(problem, NetworkProblem::$expected_err { .. }));

                    if !found {
                        panic!("Expected problem: NetworkProblem::{}, but got: {:?}", stringify!($expected_err), e);
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
    duplicate_virtual_node_name: "duplicate-virtual-node-name.json", DuplicateNodeName,
    // A simple and a composite node sharing a name. The composite node expands only to
    // sub-named core nodes, so again the core builder never sees a clash. Validation is the
    // only thing standing between this model and a silently wrong network.
    duplicate_node_name_with_composite: "duplicate-node-name-with-composite.json", DuplicateNodeName,
    // Two parameters sharing a name. The core builder would refuse this too, but validation now
    // refuses it first, as it does the same clash in tables, timeseries and metric sets.
    duplicate_parameter_name: "duplicate-parameter-name.json", DuplicateParameterName,
}

/// A group of no scenarios, which `pywr-core` would build into a model that simulates nothing.
/// Validation refuses it, and so does building the model.
#[test]
fn scenario_group_size_zero() {
    let input_pth = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("invalid")
        .join("scenario-group-size-zero.json");

    let schema = deserialise_test_model(&input_pth);

    let error = schema.validate().expect_err("Expected validation to fail");

    assert!(
        error
            .scenarios
            .iter()
            .any(|problem| matches!(problem, ScenarioProblem::EmptyGroup { .. })),
        "Expected an empty scenario group, but got: {error:?}"
    );

    #[cfg(feature = "core")]
    {
        match build_test_model(&schema) {
            ModelSchemaBuildError::ScenarioValidation { source } => assert_eq!(
                source.report().to_string(),
                "The scenarios have 1 problem(s):\n\
                 - The scenario group `climate` has a size of zero, but a group must have at least one scenario."
            ),
            e => panic!("Expected ModelSchemaBuildError::ScenarioValidation, but got: {e:?}"),
        }
    }
}

/// A reference to a scenario group the model does not define. The domain itself is valid; it is
/// the network that names a group which is not there.
#[test]
fn unknown_scenario_group() {
    let input_pth = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("invalid")
        .join("unknown-scenario-group.json");

    let schema = deserialise_test_model(&input_pth);

    let error = schema.validate().expect_err("Expected validation to fail");

    assert!(
        error
            .scenarios
            .iter()
            .any(|problem| matches!(problem, ScenarioProblem::UnknownGroupReference { .. })),
        "Expected a dangling scenario group reference, but got: {error:?}"
    );

    // `pywr-core` must refuse the model too, though it resolves the parameter only at `build`.
    #[cfg(feature = "core")]
    {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("invalid");

        match schema.create_model_builder(Some(&data_dir), Some(temp_dir.path())) {
            Err(e) => panic!("Expected a `NetworkBuildError` error, but got: {e:?}"),
            Ok(builder) => match builder.build() {
                Err(e) => match e {
                    pywr_core::models::ModelBuilderError::NetworkBuildError(source) => match source {
                        pywr_core::network::NetworkBuildError::ParameterCollectionBuildError(source) => match *source {
                            pywr_core::parameters::ParameterCollectionBuilderError::ParameterBuildError {
                                source,
                                ..
                            } => {
                                match *source {
                                    pywr_core::parameters::ParameterBuildError::ScenarioGroupNotFound(_) => {
                                        // This is the expected error.
                                    }
                                    _ => panic!(
                                        "Expected `ParameterBuildError::ScenarioGroupNotFound`, but got: {source:?}"
                                    ),
                                }
                            }
                            _ => panic!(
                                "Expected `ParameterCollectionBuilderError::ParameterBuildError`, but got: {source:?}"
                            ),
                        },
                        e => panic!("Expected `NetworkBuildError::ParameterCollectionBuildError`, but got: {e:?}"),
                    },
                },
                Ok(_) => panic!("Expected the model to be refused for its unresolved scenario group!"),
            },
        };
    }
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
