//! Schema for a Pywr model.
//!
//! Schema definition for a Pywr model.
//!
//! Serializing and deserializing is accomplished using [`serde`].
//!
pub mod agg_funcs;
pub mod data_tables;
mod digest;
pub mod edge;
mod error;
mod files;
pub mod json_schema;
mod mermaid;
pub mod meta;
pub mod metric;
pub mod metric_sets;
pub mod model;
mod network;
pub mod nodes;
pub mod outputs;
pub mod parameters;
mod py_utils;
pub mod time_series;
mod util;
mod v1;
mod validation;
mod visit;

pub use digest::{Checksum, ChecksumError};
pub use error::{ComponentConversionError, ConversionError, SchemaError};
pub use files::{FileProvider, FileSystem, InputFile, MemoryFiles};
pub use model::{ModelSchema, ModelSchemaReadError, MultiNetworkModelSchema};
#[cfg(feature = "core")]
pub use model::{ModelSchemaBuildError, MultiNetworkModelSchemaBuildError};
#[cfg(feature = "core")]
pub use network::{LoadArgs, NetworkSchemaBuildError};
pub use network::{NetworkMergeError, NetworkMergeOptions, NetworkSchema, NetworkSchemaReadError, NetworkSchemaRef};
pub use py_utils::{PythonSource, PythonSourceType, PythonSourceTypeIter};
pub use v1::{ConversionData, TryFromV1, TryIntoV2};
pub use validation::{
    DuplicateNodeName, EdgeProblem, EdgeValidationError, InitialVolumeProblem, MemberProblem, MetricSetProblem,
    ModelProblem, NetworkProblem, NetworkValidationError, NodeProblem, NodeReferenceProblem, ParameterProblem,
    ParameterReferenceProblem, PointsProblem, ProblemOwner, ScenarioProblem, ScenarioValidationError,
    TableReferenceProblem, ValidationError, VirtualNodeProblem,
};
pub use visit::{Owner, Reference, ReferenceMut, VisitMetrics, VisitPaths, VisitReferences};
