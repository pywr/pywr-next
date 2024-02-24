use crate::error::SchemaError;
use pywr_core::recorders::CSVRecorder;
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct CsvOutput {
    name: String,
    filename: PathBuf,
    metric_set: String,
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

        let metric_set_idx = network.get_metric_set_index_by_name(&self.metric_set)?;
        let recorder = CSVRecorder::new(&self.name, filename, metric_set_idx);

        network.add_recorder(Box::new(recorder))?;

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

    fn csv2_str() -> &'static str {
        include_str!("../test_models/csv2.json")
    }

    #[test]
    fn test_schema() {
        let data = csv1_str();
        let schema = PywrModel::from_str(data).unwrap();

        assert_eq!(schema.network.nodes.len(), 3);
        assert_eq!(schema.network.edges.len(), 2);
        assert!(schema.network.outputs.is_some_and(|o| o.len() == 1));
    }

    #[test]
    fn test_csv1_run() {
        let data = csv1_str();
        let schema = PywrModel::from_str(data).unwrap();

        let temp_dir = TempDir::new().unwrap();

        let model = schema.build_model(None, Some(temp_dir.path())).unwrap();

        model.run::<ClpSolver>(&ClpSolverSettings::default()).unwrap();

        // After model run there should be an output file.
        let expected_path = temp_dir.path().join("outputs.csv");
        assert!(expected_path.exists());
    }

    #[test]
    fn test_csv2_run() {
        let data = csv2_str();
        let schema = PywrModel::from_str(data).unwrap();

        let temp_dir = TempDir::new().unwrap();

        let model = schema.build_model(None, Some(temp_dir.path())).unwrap();

        model.run::<ClpSolver>(&ClpSolverSettings::default()).unwrap();

        // After model run there should be an output file.
        let expected_path = temp_dir.path().join("outputs.csv");
        assert!(expected_path.exists());
    }
}
