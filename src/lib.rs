use chrono::ParseError;
use thiserror::Error;

use crate::edge::{Edge, EdgeIndex};
use crate::node::{Node, NodeIndex};
use crate::parameters::ParameterIndex;
use crate::recorders::RecorderIndex;
use crate::state::{NetworkState, ParameterState};

pub mod edge;
mod metric;
pub mod model;
pub mod node;
pub mod parameters;
pub mod python;
mod recorders;
mod scenario;
mod solvers;
pub mod state;
mod timestep;
mod utils;

#[derive(Error, Debug, PartialEq)]
pub enum PywrError {
    #[error("invalid node connect")]
    InvalidNodeConnection,
    #[error("connection to node is already defined")]
    NodeConnectionAlreadyExists,
    #[error("node index not found")]
    NodeIndexNotFound,
    #[error("node not found")]
    NodeNotFound,
    #[error("edge index not found")]
    EdgeIndexNotFound,
    #[error("parameter index not found")]
    ParameterIndexNotFound,
    #[error("parameter not found")]
    ParameterNotFound,
    #[error("recorder index not found")]
    RecorderIndexNotFound,
    #[error("recorder not found")]
    RecorderNotFound,
    #[error("node name `{0}` already exists")]
    NodeNameAlreadyExists(String),
    #[error("parameter name `{0}` already exists on parameter {1}")]
    ParameterNameAlreadyExists(String, ParameterIndex),
    #[error("recorder name `{0}` already exists on parameter {1}")]
    RecorderNameAlreadyExists(String, RecorderIndex),
    #[error("connections from output nodes are invalid")]
    InvalidNodeConnectionFromOutput,
    #[error("connections to input nodes are invalid")]
    InvalidNodeConnectionToInput,
    #[error("flow constraints are undefined for this node")]
    FlowConstraintsUndefined,
    #[error("storage constraints are undefined for this node")]
    StorageConstraintsUndefined,
    #[error("unable to parse date")]
    ParseError(#[from] ParseError),
    #[error("timestep index out of range")]
    TimestepIndexOutOfRange,
    #[error("solver not initialised")]
    SolverNotSetup,
    #[error("no edges defined")]
    NoEdgesDefined,
    #[error("Python error")]
    PythonError,
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
    #[error("clp error")]
    ClpError(#[from] solvers::clp::ClpError),
    #[error("metric not defined")]
    MetricNotDefinedForNode,
    #[error("recorder not initialised")]
    RecorderNotInitialised,
    #[error("hdf5 error")]
    HDF5Error(String),
    #[error("not implemented by recorder")]
    NotSupportedByRecorder,
}
