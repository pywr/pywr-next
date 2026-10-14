mod arrow_stream;
mod csv;
mod hdf;
mod memory;
mod placeholder;

pub use self::csv::{CsvFormat, CsvMetricSet, CsvMetricSetType, CsvOutput};
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::meta::NamedMeta;
pub use arrow_stream::ArrowStreamOutput;
pub use hdf::Hdf5Output;
pub use memory::{MemoryAggregation, MemoryAggregationOrder, MemoryOutput};
pub use placeholder::PlaceholderOutput;
use pywr_schema_macros::{PywrVisitPaths, PywrVisitReferences};
use schemars::JsonSchema;
#[cfg(feature = "core")]
use std::path::Path;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};

#[derive(
    serde::Deserialize,
    serde::Serialize,
    Debug,
    Clone,
    JsonSchema,
    PywrVisitPaths,
    PywrVisitReferences,
    Display,
    EnumDiscriminants,
)]
#[serde(tag = "type")]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(OutputType))]
pub enum Output {
    ArrowStream(ArrowStreamOutput),
    CSV(CsvOutput),
    HDF5(Hdf5Output),
    Memory(Box<MemoryOutput>),
    Placeholder(PlaceholderOutput),
}

impl Output {
    pub fn name(&self) -> &str {
        &self.meta().name
    }

    pub fn meta(&self) -> &NamedMeta {
        match self {
            Self::ArrowStream(o) => &o.meta,
            Self::CSV(o) => &o.meta,
            Self::HDF5(o) => &o.meta,
            Self::Memory(o) => &o.meta,
            Self::Placeholder(o) => &o.meta,
        }
    }

    pub fn meta_mut(&mut self) -> &mut NamedMeta {
        match self {
            Self::ArrowStream(o) => &mut o.meta,
            Self::CSV(o) => &mut o.meta,
            Self::HDF5(o) => &mut o.meta,
            Self::Memory(o) => &mut o.meta,
            Self::Placeholder(o) => &mut o.meta,
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
            Self::ArrowStream(o) => o.add_to_network(network, output_path),
            Self::CSV(o) => o.add_to_network(network, output_path),
            Self::HDF5(o) => o.add_to_network(network, output_path),
            Self::Memory(o) => o.add_to_network(network, data_path),
            Self::Placeholder(o) => o.add_to_network(),
        }
    }
}
