extern crate core;

use crate::node::NodeIndex;
use crate::parameters::{IndexParameterIndex, ParameterIndex};
use crate::recorders::RecorderIndex;
use thiserror::Error;

pub mod aggregated_node;
mod aggregated_storage_node;
pub mod edge;
mod metric;
pub mod model;
pub mod node;
pub mod parameters;
pub mod python;
mod recorders;
mod scenario;
pub mod schema;
pub mod solvers;
pub mod state;
pub mod test_utils;
pub mod timestep;
mod virtual_storage;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum PywrError {
    #[error("failed to load schema: {0}")]
    SchemaLoad(String),
    #[error("invalid node connect")]
    InvalidNodeConnection,
    #[error("connection to node is already defined")]
    NodeConnectionAlreadyExists,
    #[error("node index not found")]
    NodeIndexNotFound,
    #[error("node with name {0} not found")]
    NodeNotFound(String),
    #[error("edge index not found")]
    EdgeIndexNotFound,
    #[error("unexpected parameter type: {0}")]
    UnexpectedParameterType(String),
    #[error("parameter index {0} not found")]
    ParameterIndexNotFound(ParameterIndex),
    #[error("index parameter index {0} not found")]
    IndexParameterIndexNotFound(IndexParameterIndex),
    #[error("parameter {0} not found")]
    ParameterNotFound(String),
    #[error("recorder index not found")]
    RecorderIndexNotFound,
    #[error("recorder not found")]
    RecorderNotFound,
    #[error("node name `{0}` already exists")]
    NodeNameAlreadyExists(String),
    #[error("parameter name `{0}` already exists on parameter {1}")]
    ParameterNameAlreadyExists(String, ParameterIndex),
    #[error("index parameter name `{0}` already exists on index parameter {1}")]
    IndexParameterNameAlreadyExists(String, IndexParameterIndex),
    #[error("recorder name `{0}` already exists on parameter {1}")]
    RecorderNameAlreadyExists(String, RecorderIndex),
    #[error("connections from output nodes are invalid. node: {0}")]
    InvalidNodeConnectionFromOutput(String),
    #[error("connections to input nodes are invalid. node: {0}")]
    InvalidNodeConnectionToInput(String),
    #[error("flow constraints are undefined for this node")]
    FlowConstraintsUndefined,
    #[error("storage constraints are undefined for this node")]
    StorageConstraintsUndefined,
    #[error("missing initial volume for node: {0}")]
    MissingInitialVolume(String),
    #[error("invalid date format description")]
    InvalidDateFormatDescription(#[from] time::error::InvalidFormatDescription),
    #[error("failed to parse date")]
    DateParse(#[from] time::error::Parse),
    #[error("invalid date component range")]
    InvalidDateComponentRange(#[from] time::error::ComponentRange),
    #[error("timestep index out of range")]
    TimestepIndexOutOfRange,
    #[error("solver not initialised")]
    SolverNotSetup,
    #[error("no edges defined")]
    NoEdgesDefined,
    #[error("Python error: {0}")]
    PythonError(String),
    #[error("Unrecognised metric")]
    UnrecognisedMetric,
    #[error("Unrecognised solver")]
    UnrecognisedSolver,
    #[error("Solve failed")]
    SolveFailed,
    #[error("atleast one parameter is required")]
    AtleastOneParameterRequired,
    #[error("scenario state not found")]
    ScenarioStateNotFound,
    #[error("scenario not found: {0}")]
    ScenarioNotFound(String),
    #[error("clp error")]
    ClpError(#[from] solvers::ClpError),
    #[error("metric not defined")]
    MetricNotDefinedForNode,
    #[error("invalid metric type: {0}")]
    InvalidMetricType(String),
    #[error("recorder not initialised")]
    RecorderNotInitialised,
    #[error("hdf5 error: {0}")]
    HDF5Error(String),
    #[error("not implemented by recorder")]
    NotSupportedByRecorder,
    #[error("invalid constraint value: {0}")]
    InvalidConstraintValue(String),
    #[error("invalid constraint type: {0}")]
    InvalidConstraintType(String),
    #[error("invalid aggregated function: {0}")]
    InvalidAggregationFunction(String),
    #[error("data out of range")]
    DataOutOfRange,
    #[error("internal parameter error: {0}")]
    InternalParameterError(String),
    #[error("conversion from v1 schema error: {0}")]
    V1SchemaConversion(String),
    #[error("data table error: {0}")]
    DataTable(#[from] schema::data_tables::TableError),
}
