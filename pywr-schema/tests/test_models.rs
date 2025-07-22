#[cfg(feature = "core")]
use pywr_core::test_utils::{ExpectedOutputs, run_all_solvers};
use pywr_schema::PywrModel;
use std::fs;
use std::path::Path;
#[cfg(feature = "core")]
use std::path::PathBuf;
#[cfg(feature = "core")]
use tempfile::TempDir;

macro_rules! model_tests {
    ($($test_func:ident: $value:expr,)*) => {
    $(
        #[test]
        fn $test_func() {

            // Deserialise the schema and run it
            #[cfg(feature = "core")]
            {
                let (input, expected, solvers_without_features, solvers_to_skip): (&str, Vec<&str>, Vec<&str>, Vec<&str>) = $value;
                let input_pth = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(input);
                let expected_paths = expected.iter().map(|p| Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(p)).collect::<Vec<_>>();
                let schema = deserialise_test_model(&input_pth);
                run_test_model(&schema, &expected_paths, &solvers_without_features, &solvers_to_skip);
            }

            // Just deserialise the schema
            #[cfg(not(feature = "core"))]
            {
                let (input, _expected, _solvers_without_features, _solvers_to_skip): (&str, Vec<&str>, Vec<&str>, Vec<&str>) = $value;
                let input_pth = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(input);
                let _schema = deserialise_test_model(&input_pth);
            }
        }
    )*
    }
}

model_tests! {
    test_simple1: ("simple1.json", vec![], vec![], vec![]),
    test_csv1: ("csv1.json", vec!["csv1-outputs-long.csv", "csv1-outputs-wide.csv"], vec![], vec![]),
    test_csv2: ("csv2.json", vec!["csv2-outputs-long.csv", "csv2-outputs-wide.csv"], vec![], vec![]),
    test_csv3: ("csv3.json", vec!["csv3-outputs-long.csv"], vec![], vec![]),
    test_hdf1: ("hdf1.json", vec![], vec![], vec![]), // TODO asserting h5 results not possible with this framework
    test_memory1: ("memory1.json", vec![], vec![], vec![]),  // TODO asserting memory results not possible with this framework
    test_timeseries: ("timeseries.json", vec!["timeseries-expected.csv"], vec![], vec![]),
    test_timeseries2: ("timeseries2.json", vec!["timeseries2-expected.csv"], vec![], vec![]),
    test_timeseries3: ("timeseries3.json", vec!["timeseries3-expected.csv"], vec![], vec![]),
    test_timeseries4: ("timeseries4.json", vec!["timeseries4-expected.csv"], vec![], vec![]),
    test_timeseries5: ("timeseries5.json", vec!["timeseries5-expected.csv"], vec![], vec![]),
    test_storage_max_volumes: ("storage_max_volumes.json", vec![], vec![], vec![]),
    test_mutual_exclusivity1: ("mutual-exclusivity1.json", vec!["mutual-exclusivity1.csv"], vec!["clp", "ipm-simd", "ipm-ocl"], vec![]),
    test_mutual_exclusivity2: ("mutual-exclusivity2.json", vec!["mutual-exclusivity2.csv"], vec!["clp", "ipm-simd", "ipm-ocl"], vec![]),
    test_mutual_exclusivity3: ("mutual-exclusivity3.json", vec!["mutual-exclusivity3.csv"], vec!["clp", "ipm-simd", "ipm-ocl"], vec![]),
    test_mutual_exclusivity4: ("mutual-exclusivity4.json", vec!["mutual-exclusivity4.csv"], vec!["clp", "ipm-simd", "ipm-ocl"], vec![]),
    test_link_with_soft_min: ("link_with_soft_min.json", vec![], vec!["ipm-simd", "ipm-ocl"], vec![]),
    test_link_with_soft_max: ("link_with_soft_max.json", vec![], vec!["ipm-simd", "ipm-ocl"], vec![]),
    test_delay1: ("delay1.json", vec!["delay1-expected.csv"], vec![], vec![]),
    test_loss_link1: ("loss_link1.json", vec!["loss_link1-expected.csv"], vec!["ipm-simd", "ipm-ocl"], vec![]),
    test_loss_link2: ("loss_link2.json", vec!["loss_link2-expected.csv"], vec!["ipm-simd", "ipm-ocl"], vec![]),
    // TODO this asserted internal flows in the previous test
    test_piecewise_link1: ("piecewise_link1.json", vec!["piecewise-link1-nodes.csv", "piecewise-link1-edges.csv"], vec![], vec![]),
    // TODO not sure why this is failing in IPM solvers (https://github.com/pywr/pywr-next/issues/293)
    test_piecewise_storage1: ("piecewise_storage1.json", vec!["piecewise_storage1-expected.csv"], vec![], vec!["ipm-simd", "ipm-ocl"]),
    // TODO not sure why this is failing in IPM solvers (https://github.com/pywr/pywr-next/issues/293)
    test_piecewise_storage2: ("piecewise_storage2.json", vec!["piecewise_storage2-expected.csv"], vec![], vec!["ipm-simd", "ipm-ocl"]),
    test_reservoir_with_river: ("reservoir_with_river.json", vec![], vec![], vec![]),
    test_reservoir_with_spill: ("reservoir_with_spill.json", vec![], vec![], vec![]),
    test_river_loss1: ("river_loss1.json", vec!["river_loss1-expected.csv"], vec!["ipm-simd", "ipm-ocl"], vec![]),
    // TODO not sure why this is failing in IPM solvers (https://github.com/pywr/pywr-next/issues/293)
    test_river_gauge1: ("river_gauge1.json", vec![], vec![], vec!["ipm-simd", "ipm-ocl"]),
    test_river_split_with_gauge1: ("river_split_with_gauge1.json", vec![], vec![], vec![]),
    test_thirty_day_licence: ("30-day-licence.json", vec![], vec!["ipm-simd", "ipm-ocl"], vec![]),
    test_wtw1: ("wtw1.json", vec!["wtw1-expected.csv"], vec!["ipm-simd", "ipm-ocl"], vec![]),
    test_wtw2: ("wtw2.json", vec!["wtw2-expected.csv"], vec!["ipm-simd", "ipm-ocl"], vec![]),
    test_local_parameter1: ("local-parameter1.json", vec!["local-parameter1-expected.csv"], vec![], vec![]),
    test_local_parameter2: ("local-parameter2.json", vec!["local-parameter2-expected.csv"], vec![], vec![]),
}

/// Test Pandas backend for reading timeseries data.
///
/// This test requires Python environment with Pandas
#[test]
#[cfg(feature = "test-python")]
fn test_timeseries_pandas() {
    let input = "timeseries_pandas.json";
    let input_pth = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(input);
    let expected = ["timeseries-expected.csv"];
    let expected_paths = expected
        .iter()
        .map(|p| Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(p))
        .collect::<Vec<_>>();
    let schema = deserialise_test_model(&input_pth);
    run_test_model(&schema, &expected_paths, &[], &[]);
}

fn deserialise_test_model(model_path: &Path) -> PywrModel {
    let data = fs::read_to_string(model_path).expect("Unable to read file");
    serde_json::from_str(&data).expect("Failed to deserialize model")
}

#[cfg(feature = "core")]
fn run_test_model(
    schema: &PywrModel,
    result_paths: &[PathBuf],
    solvers_without_features: &[&str],
    solvers_to_skip: &[&str],
) {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let model = schema.build_model(Some(&data_dir), Some(temp_dir.path())).unwrap();
    // After model run there should be an output file.
    let expected_outputs: Vec<_> = result_paths
        .iter()
        .map(|pth| {
            ExpectedOutputs::new(
                temp_dir.path().join(pth.file_name().unwrap()),
                fs::read_to_string(pth).unwrap_or_else(|_| panic!("Failed to read expected output: {}", pth.display())),
            )
        })
        .collect();

    // Test all solvers
    run_all_solvers(&model, solvers_without_features, solvers_to_skip, &expected_outputs);
}

macro_rules! convert_tests {
    ($($func_name:ident: $value:expr,)*) => {
    $(

        #[test]
        fn $func_name() {
            let (v1, v2) = $value;
            let v1_pth = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(v1);
            let v2_pth = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(v2);
            convert_model(&v1_pth, &v2_pth);
        }
    )*
    }
}

convert_tests! {
    test_convert_timeseries: ("v1/timeseries.json", "v1/timeseries-converted.json"),
    test_convert_inline_parameter: ("v1/inline-parameter.json", "v1/inline-parameter-converted.json"),
    test_convert_river_split_with_gauge1: ("v1/river_split_with_gauge1.json", "v1/river_split_with_gauge1-converted.json"),
}

fn convert_model(v1_path: &Path, v2_path: &Path) {
    let v1_str = fs::read_to_string(v1_path).unwrap();
    let v1: pywr_v1_schema::PywrModel = serde_json::from_str(&v1_str).unwrap();

    let (v2, errors) = PywrModel::from_v1(v1);
    println!("Conversion errors: {errors:?}",);
    assert_eq!(errors.len(), 0);

    let v2_converted: serde_json::Value = serde_json::from_str(&serde_json::to_string_pretty(&v2).unwrap()).unwrap();

    let v2_expected_str = fs::read_to_string(v2_path).unwrap();
    let v2_expected: serde_json::Value = serde_json::from_str(&v2_expected_str).unwrap();
    assert_eq!(v2_converted, v2_expected);
}
