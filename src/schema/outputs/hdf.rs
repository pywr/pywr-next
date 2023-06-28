use crate::recorders::HDF5Recorder;
use crate::PywrError;
use std::path::PathBuf;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct Hdf5Output {
    name: String,
    filename: PathBuf,
    /// The node's to save output for
    nodes: Vec<String>,
}

impl Hdf5Output {
    pub fn add_to_model(
        &self,
        model: &mut crate::model::Model,
        schema: &crate::schema::PywrModel,
    ) -> Result<(), PywrError> {
        let metrics = self
            .nodes
            .iter()
            .map(|node_name| {
                // Get the node from the schema; not the model itself

                let node = schema
                    .get_node_by_name(node_name)
                    .ok_or_else(|| PywrError::NodeNotFound(node_name.to_string()))?;

                let metric = node.default_metric(model)?;

                Ok(metric)
            })
            .collect::<Result<Vec<_>, PywrError>>()?;

        let recorder = HDF5Recorder::new(&self.name, &self.filename, metrics);

        model.add_recorder(Box::new(recorder))?;

        Ok(())
    }
}
