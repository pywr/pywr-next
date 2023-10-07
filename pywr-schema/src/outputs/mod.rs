mod csv;
mod hdf;

pub use self::csv::CsvOutput;
use crate::error::SchemaError;
pub use hdf::Hdf5Output;
use std::path::Path;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum Output {
    CSV(CsvOutput),
    HDF5(Hdf5Output),
}

impl Output {
    pub fn add_to_model(
        &self,
        model: &mut pywr_core::model::Model,
        output_path: Option<&Path>,
    ) -> Result<(), SchemaError> {
        match self {
            Self::CSV(o) => o.add_to_model(model, output_path),
            Self::HDF5(o) => o.add_to_model(model, output_path),
        }
    }
}
