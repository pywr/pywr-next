#[cfg(feature = "core")]
use crate::error::SchemaError;
#[cfg(feature = "core")]
use pywr_core::recorders::HDF5Recorder;
use pywr_schema_macros::PywrVisitPaths;
use schemars::JsonSchema;
#[cfg(feature = "core")]
use std::path::Path;
use std::path::PathBuf;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitPaths)]
pub struct Hdf5Output {
    pub name: String,
    pub filename: PathBuf,
    /// The metric set to save
    pub metric_set: String,
}

#[cfg(feature = "core")]
impl Hdf5Output {
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

        let recorder = HDF5Recorder::new(&self.name, filename, metric_set_idx);

        network.add_recorder(Box::new(recorder))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::visit::VisitPaths;
    use crate::PywrModel;
    #[cfg(feature = "core")]
    use pywr_core::solvers::{ClpSolver, ClpSolverSettings};
    use std::path::PathBuf;
    use std::str::FromStr;
    #[cfg(feature = "core")]
    use tempfile::TempDir;

    fn model_str() -> &'static str {
        include_str!("../test_models/hdf1.json")
    }

    #[test]
    fn test_schema() {
        let data = model_str();
        let schema = PywrModel::from_str(data).unwrap();

        assert_eq!(schema.network.nodes.len(), 3);
        assert_eq!(schema.network.edges.len(), 2);

        let num_outputs = schema.network.outputs.as_ref().map(|o| o.len());
        assert_eq!(num_outputs, Some(1));

        let expected_paths = vec![PathBuf::from_str("outputs.h5").unwrap()];
        let mut found_paths = Vec::new();
        schema.visit_paths(&mut |path| {
            found_paths.push(path.to_path_buf());
        });
        assert_eq!(found_paths, expected_paths);
    }

    #[test]
    #[cfg(feature = "core")]
    fn test_run() {
        let data = model_str();
        let schema = PywrModel::from_str(data).unwrap();

        let temp_dir = TempDir::new().unwrap();

        let model = schema.build_model(None, Some(temp_dir.path())).unwrap();

        model.run::<ClpSolver>(&ClpSolverSettings::default()).unwrap();

        // After model run there should be an output file.
        let expected_path = temp_dir.path().join("outputs.h5");
        assert!(expected_path.exists());
    }
}
