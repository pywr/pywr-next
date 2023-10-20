#![cfg_attr(feature = "ipm-simd", feature(portable_simd))]

extern crate core;

use crate::node::NodeIndex;
use crate::parameters::{IndexParameterIndex, MultiValueParameterIndex, ParameterIndex};
use crate::recorders::RecorderIndex;
use pyo3::exceptions::{PyException, PyRuntimeError};
use pyo3::{create_exception, PyErr};
use thiserror::Error;

pub mod aggregated_node;
mod aggregated_storage_node;
pub mod edge;
pub mod metric;
pub mod model;
pub mod node;
pub mod parameters;
pub mod recorders;
mod scenario;
pub mod solvers;
pub mod state;
pub mod test_utils;
pub mod timestep;
pub mod tracing;
pub mod virtual_storage;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum PywrError {
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
    #[error("parameter index {0} not found")]
    ParameterIndexNotFound(ParameterIndex),
    #[error("index parameter index {0} not found")]
    IndexParameterIndexNotFound(IndexParameterIndex),
    #[error("multi value parameter index {0} not found")]
    MultiValueParameterIndexNotFound(MultiValueParameterIndex),
    #[error("multi value parameter key {0} not found")]
    MultiValueParameterKeyNotFound(String),
    #[error("parameter {0} not found")]
    ParameterNotFound(String),
    #[error("metric set index not found")]
    MetricSetIndexNotFound,
    #[error("metric set with name {0} not found")]
    MetricSetNotFound(String),
    #[error("recorder index not found")]
    RecorderIndexNotFound,
    #[error("recorder not found")]
    RecorderNotFound,
    #[error("node name `{0}` already exists")]
    NodeNameAlreadyExists(String),
    #[error("parameter name `{0}` already exists at index {1}")]
    ParameterNameAlreadyExists(String, ParameterIndex),
    #[error("index parameter name `{0}` already exists at index {1}")]
    IndexParameterNameAlreadyExists(String, IndexParameterIndex),
    #[error("metric set name `{0}` already exists")]
    MetricSetNameAlreadyExists(String),
    #[error("recorder name `{0}` already exists at index {1}")]
    RecorderNameAlreadyExists(String, RecorderIndex),
    #[error("connections from output nodes are invalid. node: {0}")]
    InvalidNodeConnectionFromOutput(String),
    #[error("connections to input nodes are invalid. node: {0}")]
    InvalidNodeConnectionToInput(String),
    #[error("flow constraints are undefined for this node")]
    FlowConstraintsUndefined,
    #[error("storage constraints are undefined for this node")]
    StorageConstraintsUndefined,
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
    #[error("invalid metric value: {0}")]
    InvalidMetricValue(String),
    #[error("recorder not initialised")]
    RecorderNotInitialised,
    #[error("hdf5 error: {0}")]
    HDF5Error(String),
    #[error("csv error: {0}")]
    CSVError(String),
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
    #[error("parameter type does is not a valid variable")]
    ParameterTypeNotVariable,
    #[error("parameter variable is not active")]
    ParameterVariableNotActive,
    #[error("incorrect number of values for parameter variable")]
    ParameterVariableValuesIncorrectLength,
    #[error("missing solver features")]
    MissingSolverFeatures,
}

// Python errors
create_exception!(pywr, ParameterNotFoundError, PyException);

impl From<PywrError> for PyErr {
    fn from(err: PywrError) -> PyErr {
        match err {
            PywrError::ParameterNotFound(name) => ParameterNotFoundError::new_err(name),
            _ => PyRuntimeError::new_err(err.to_string()),
        }
    }
}