//! Tests of building and evaluating optimisation problems from a model schema and a problem config.
//!
//! The model in `problem1.json` has a supply limited to 8.0 and a variable demand (`problem1-config.json`),
//! so the delivered flow is the smaller of the demand and 8.0.
#![cfg(all(feature = "core", feature = "clp"))]

use pywr_core::network_variable_config::NetworkVariableConfigBuilderError;
use pywr_core::problem::{ModelProblem, ModelProblemBuildError};
use pywr_core::solvers::ClpSolverSettings;
use pywr_schema::{FileSystem, ModelSchema, ProblemConfig, SchemaError};
use std::path::{Path, PathBuf};

fn tests_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

fn build_model_problem(config: &ProblemConfig) -> Result<ModelProblem, SchemaError> {
    let dir = tests_dir();
    let schema = ModelSchema::from_path(dir.join("problem1.json")).unwrap();
    let model = schema
        .create_model_builder(&FileSystem, Some(&dir), None)
        .unwrap()
        .build()
        .unwrap();

    config.build_model_problem(model)
}

fn assert_values_eq(actual: &[f64], expected: &[f64]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?} != {expected:?}");
    for (a, e) in actual.iter().zip(expected) {
        assert!((a - e).abs() < 1e-6, "{actual:?} != {expected:?}");
    }
}

#[test]
fn test_build_and_evaluate_model_problem() {
    let config = ProblemConfig::from_path(tests_dir().join("problem1-config.json")).unwrap();
    let problem = build_model_problem(&config).unwrap();

    assert_eq!(problem.size().unwrap(), (1, 0));
    assert_values_eq(&problem.lower_bounds().unwrap().f64, &[0.0]);
    assert_values_eq(&problem.upper_bounds().unwrap().f64, &[20.0]);

    let solver_config = ClpSolverSettings::default();

    // The delivered flow is capped by the supply of 8.0, which satisfies the constraint
    let evaluation = problem.evaluate(&solver_config, &[10.0], &[]).unwrap();
    assert_values_eq(&evaluation.objectives, &[10.0, 8.0]);
    assert_values_eq(&evaluation.constraints, &[8.0]);
    assert_eq!(problem.constraints()[0].violation(evaluation.constraints[0]), 0.0);

    // A demand of 4.0 is below the minimum delivered flow of 5.0
    let evaluation = problem.evaluate(&solver_config, &[4.0], &[]).unwrap();
    assert_values_eq(&evaluation.objectives, &[4.0, 4.0]);
    assert!((problem.constraints()[0].violation(evaluation.constraints[0]) - 1.0).abs() < 1e-6);
}

#[test]
fn test_build_model_problem_missing_output() {
    let config: ProblemConfig = serde_json::from_str(
        r#"{
            "variables": [],
            "objectives": [{"output": "missing", "sense": "Minimise"}]
        }"#,
    )
    .unwrap();

    let result = build_model_problem(&config);
    assert!(matches!(
        result,
        Err(SchemaError::CoreModelProblemBuildError(
            ModelProblemBuildError::RecorderNotFound(_)
        ))
    ));
}

#[test]
fn test_build_model_problem_incorrect_variable_config_type() {
    // A monthly profile config can not be used with a constant parameter
    let config: ProblemConfig = serde_json::from_str(
        r#"{
            "variables": [
                {
                    "name": "demand",
                    "config": {
                        "type": "MonthlyProfile",
                        "upper_bounds": [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]
                    }
                }
            ],
            "objectives": [{"output": "mean-demand", "sense": "Minimise"}]
        }"#,
    )
    .unwrap();

    let result = build_model_problem(&config);
    assert!(matches!(
        result,
        Err(SchemaError::CoreNetworkVariableConfigBuilderError(
            NetworkVariableConfigBuilderError::InvalidConfig { .. }
        ))
    ));
}
