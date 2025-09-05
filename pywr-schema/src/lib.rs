//! Schema for a Pywr model.
//!
//! Schema definition for a Pywr model.
//!
//! Serializing and deserializing is accomplished using [`serde`].
//!
pub mod agg_funcs;
pub mod data_tables;
pub mod edge;
mod error;
pub mod metric;
pub mod metric_sets;
pub mod model;
mod network;
pub mod nodes;
pub mod outputs;
pub mod parameters;
pub mod timeseries;
mod v1;
mod visit;

pub use error::{ComponentConversionError, ConversionError, SchemaError};
pub use model::{PywrModel, PywrModelReadError, PywrMultiNetworkModel};
#[cfg(feature = "core")]
pub use model::{PywrModelBuildError, PywrMultiNetworkModelBuildError};
#[cfg(feature = "core")]
pub use network::{LoadArgs, PywrNetworkBuildError};
pub use network::{PywrNetwork, PywrNetworkReadError, PywrNetworkRef};
pub use v1::{ConversionData, TryFromV1, TryIntoV2};
pub use visit::{VisitMetrics, VisitPaths};
