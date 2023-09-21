use crate::recorders::CSVRecorder;
use crate::PywrError;
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct CsvOutput {
    name: String,
    filename: PathBuf,
    metric_set: String,
}

impl CsvOutput {
    pub fn add_to_model(&self, model: &mut crate::model::Model, output_path: Option<&Path>) -> Result<(), PywrError> {
        let filename = match (output_path, self.filename.is_relative()) {
            (Some(odir), true) => odir.join(&self.filename),
            _ => self.filename.to_path_buf(),
        };

        let metric_set_idx = model.get_metric_set_index_by_name(&self.metric_set)?;
        let recorder = CSVRecorder::new(&self.name, filename, metric_set_idx);

        model.add_recorder(Box::new(recorder))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::model::RunOptions;
    use crate::schema::PywrModel;
    use crate::solvers::ClpSolver;
    use tempfile::TempDir;

    fn model_str() -> &'static str {
        include_str!("../test_models/csv1.json")
    }

    #[test]
    fn test_schema() {
        let data = model_str();
        let schema = PywrModel::from_str(data).unwrap();

        assert_eq!(schema.nodes.len(), 3);
        assert_eq!(schema.edges.len(), 2);
        assert!(schema.outputs.is_some_and(|o| o.len() == 1));
    }

    #[test]
    fn test_run() {
        let data = model_str();
        let schema = PywrModel::from_str(data).unwrap();

        let temp_dir = TempDir::new().unwrap();

        let (model, timestepper): (crate::model::Model, crate::timestep::Timestepper) =
            schema.build_model(None, Some(temp_dir.path())).unwrap();

        model.run::<ClpSolver>(&timestepper, &RunOptions::default()).unwrap();

        // After model run there should be an output file.
        let expected_path = temp_dir.path().join("outputs.csv");
        assert!(expected_path.exists());
    }
}
