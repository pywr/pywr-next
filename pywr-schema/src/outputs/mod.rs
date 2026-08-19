mod csv;
mod hdf;
mod memory;
mod placeholder;

pub use self::csv::CsvOutput;
#[cfg(feature = "core")]
use crate::error::SchemaError;
pub use hdf::Hdf5Output;
pub use memory::MemoryOutput;
pub use placeholder::PlaceholderOutput;
use pywr_schema_macros::PywrVisitPaths;
use schemars::JsonSchema;
#[cfg(feature = "core")]
use std::path::Path;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};

#[derive(
    serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitPaths, Display, EnumDiscriminants,
)]
#[serde(tag = "type")]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(OutputType))]
pub enum Output {
    CSV(CsvOutput),
    HDF5(Hdf5Output),
    Memory(Box<MemoryOutput>),
    Placeholder(PlaceholderOutput),
}

impl Output {
    pub fn name(&self) -> &str {
        match self {
            Self::CSV(o) => &o.name,
            Self::HDF5(o) => &o.name,
            Self::Memory(o) => &o.name,
            Self::Placeholder(o) => &o.name,
        }
    }
    pub fn is_placeholder(&self) -> bool {
        matches!(self, Self::Placeholder(_))
    }
}

#[cfg(feature = "core")]
impl Output {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        data_path: Option<&Path>,
        output_path: Option<&Path>,
    ) -> Result<(), SchemaError> {
        match self {
            Self::CSV(o) => o.add_to_network(network, output_path),
            Self::HDF5(o) => o.add_to_network(network, output_path),
            Self::Memory(o) => o.add_to_network(network, data_path),
            Self::Placeholder(o) => o.add_to_network(),
        }
    }
}
