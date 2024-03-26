use crate::error::SchemaError;
use pywr_core::recorders::{CsvLongFmtOutput, CsvWideFmtOutput, Recorder};
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Default)]
#[serde(rename_all = "lowercase")]
pub enum CsvFormat {
    Wide,
    #[default]
    Long,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum CsvMetricSet {
    Single(String),
    Multiple(Vec<String>),
}

/// Output data to a CSV file.
///
/// This output will write the output data to a CSV file. The output data is written in either
/// wide or long format. The wide format will write each metric to a separate column, while the
/// long format will write each metric to a separate row. The wide format is useful for small
/// numbers of metrics or scenarios, while the long format is useful for large numbers of metrics
/// or scenarios. For more details see the [`CsvLongFmtOutput`] and [`CsvWideFmtOutput`] types.
///
/// The long format supports either a single metric set or a list of metric sets. However,
/// the wide format only supports a single metric set.
///
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct CsvOutput {
    pub name: String,
    pub filename: PathBuf,
    pub format: CsvFormat,
    pub metric_set: CsvMetricSet,
}

impl CsvOutput {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        output_path: Option<&Path>,
    ) -> Result<(), SchemaError> {
        let filename = match (output_path, self.filename.is_relative()) {
            (Some(odir), true) => odir.join(&self.filename),
            _ => self.filename.to_path_buf(),
        };

        let recorder: Box<dyn Recorder> = match self.format {
            CsvFormat::Wide => match &self.metric_set {
                CsvMetricSet::Single(metric_set) => {
                    let metric_set_idx = network.get_metric_set_index_by_name(metric_set)?;
                    Box::new(CsvWideFmtOutput::new(&self.name, filename, metric_set_idx))
                }
                CsvMetricSet::Multiple(_) => {
                    return Err(SchemaError::MissingMetricSet(
                        "Wide format CSV output requires a single `metric_set`".to_string(),
                    ))
                }
            },
            CsvFormat::Long => {
                let metric_set_indices = match &self.metric_set {
                    CsvMetricSet::Single(metric_set) => vec![network.get_metric_set_index_by_name(metric_set)?],
                    CsvMetricSet::Multiple(metric_sets) => metric_sets
                        .iter()
                        .map(|ms| network.get_metric_set_index_by_name(ms))
                        .collect::<Result<Vec<_>, _>>()?,
                };

                Box::new(CsvLongFmtOutput::new(&self.name, filename, &metric_set_indices))
            }
        };

        network.add_recorder(recorder)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::PywrModel;
    use pywr_core::solvers::{ClpSolver, ClpSolverSettings};
    use std::str::FromStr;
    use tempfile::TempDir;

    fn csv1_str() -> &'static str {
        include_str!("../test_models/csv1.json")
    }

    fn csv1_outputs_long_str() -> &'static str {
        include_str!("../test_models/csv1-outputs-long.csv")
    }

    fn csv1_outputs_wide_str() -> &'static str {
        include_str!("../test_models/csv1-outputs-wide.csv")
    }

    fn csv2_str() -> &'static str {
        include_str!("../test_models/csv2.json")
    }

    fn csv2_outputs_long_str() -> &'static str {
        include_str!("../test_models/csv2-outputs-long.csv")
    }

    fn csv2_outputs_wide_str() -> &'static str {
        include_str!("../test_models/csv2-outputs-wide.csv")
    }

    fn csv3_str() -> &'static str {
        include_str!("../test_models/csv3.json")
    }

    fn csv3_outputs_long_str() -> &'static str {
        include_str!("../test_models/csv3-outputs-long.csv")
    }

    #[test]
    fn test_schema() {
        let data = csv1_str();
        let schema = PywrModel::from_str(data).unwrap();

        assert_eq!(schema.network.nodes.len(), 3);
        assert_eq!(schema.network.edges.len(), 2);
        assert!(schema.network.outputs.is_some_and(|o| o.len() == 2));
    }

    #[test]
    fn test_csv1_run() {
        let data = csv1_str();
        let schema = PywrModel::from_str(data).unwrap();

        let temp_dir = TempDir::new().unwrap();

        let model = schema.build_model(None, Some(temp_dir.path())).unwrap();

        model.run::<ClpSolver>(&ClpSolverSettings::default()).unwrap();

        // After model run there should be two output files.
        let expected_long_path = temp_dir.path().join("outputs-long.csv");
        assert!(expected_long_path.exists());
        let long_content = std::fs::read_to_string(&expected_long_path).unwrap();
        assert_eq!(&long_content, csv1_outputs_long_str());

        let expected_wide_path = temp_dir.path().join("outputs-wide.csv");
        assert!(expected_wide_path.exists());
        let wide_content = std::fs::read_to_string(&expected_wide_path).unwrap();
        assert_eq!(&wide_content, csv1_outputs_wide_str());
    }

    #[test]
    fn test_csv2_run() {
        let data = csv2_str();
        let schema = PywrModel::from_str(data).unwrap();

        let temp_dir = TempDir::new().unwrap();

        let model = schema.build_model(None, Some(temp_dir.path())).unwrap();

        model.run::<ClpSolver>(&ClpSolverSettings::default()).unwrap();

        // After model run there should be two output files.
        let expected_long_path = temp_dir.path().join("outputs-long.csv");
        assert!(expected_long_path.exists());
        let long_content = std::fs::read_to_string(&expected_long_path).unwrap();
        assert_eq!(&long_content, csv2_outputs_long_str());

        let expected_wide_path = temp_dir.path().join("outputs-wide.csv");
        assert!(expected_wide_path.exists());
        let wide_content = std::fs::read_to_string(&expected_wide_path).unwrap();
        assert_eq!(&wide_content, csv2_outputs_wide_str());
    }

    #[test]
    fn test_csv3_run() {
        let data = csv3_str();
        let schema = PywrModel::from_str(data).unwrap();

        let temp_dir = TempDir::new().unwrap();

        let model = schema.build_model(None, Some(temp_dir.path())).unwrap();

        model.run::<ClpSolver>(&ClpSolverSettings::default()).unwrap();

        let expected_long_path = temp_dir.path().join("outputs-long.csv");
        assert!(expected_long_path.exists());
        let long_content = std::fs::read_to_string(&expected_long_path).unwrap();
        assert_eq!(&long_content, csv3_outputs_long_str());
    }
}
