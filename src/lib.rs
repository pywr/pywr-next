use thiserror::Error;

pub mod edge;
mod glpk;
pub mod model;
pub mod node;
pub mod parameters;
pub mod python;
mod scenario;
mod solvers;
pub mod state;
mod timestep;

use crate::edge::{Edge, EdgeIndex};
use crate::node::{Node, NodeIndex};
use crate::parameters::ParameterIndex;
use crate::state::{EdgeState, NetworkState, NodeState, ParameterState};
use chrono::ParseError;

#[derive(Error, Debug, PartialEq)]
pub enum PywrError {
    #[error("invalid node connect")]
    InvalidNodeConnection,
    #[error("connection to node is already defined")]
    NodeConnectionAlreadyExists,
    #[error("node index not found")]
    NodeIndexNotFound,
    #[error("edge index not found")]
    EdgeIndexNotFound,
    #[error("parameter index not found")]
    ParameterIndexNotFound,
    #[error("node name `{0}` already exists on node {1}")]
    NodeNameAlreadyExists(String, NodeIndex),
    #[error("parameter name `{0}` already exists on parameter {1}")]
    ParameterNameAlreadyExists(String, ParameterIndex),
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
    #[error("glpk error")]
    GlpkError(#[from] glpk::GlpkError),
    #[error("solver not initialised")]
    SolverNotSetup,
    #[error("no edges defined")]
    NoEdgesDefined,
    #[error("Python error")]
    PythonError,
    #[error("Unrecognised solver")]
    UnrecognisedSolver,
    #[error("Solve failed")]
    SolveFailed,
    #[error("atleast one parameter is required")]
    AtleastOneParameterRequired,
    #[error("scenario state not found")]
    ScenarioStateNotFound,
}
