#![cfg_attr(feature = "ipm-simd", feature(portable_simd))]

extern crate core;

use crate::derived_metric::DerivedMetricIndex;
use crate::models::MultiNetworkTransferIndex;
use crate::node::NodeIndex;
use crate::parameters::{
    ConstParameterIndex, GeneralParameterIndex, InterpolationError, ParameterIndex, SimpleParameterIndex,
};
use crate::recorders::{AggregationError, MetricSetIndex, RecorderIndex};
use crate::state::MultiValue;
use crate::virtual_storage::VirtualStorageIndex;
use pyo3::exceptions::{PyException, PyRuntimeError};
use pyo3::{create_exception, PyErr};
use thiserror::Error;

pub mod aggregated_node;
mod aggregated_storage_node;
pub mod derived_metric;
pub mod edge;
pub mod metric;
pub mod models;
pub mod network;
pub mod node;
pub mod parameters;
pub mod recorders;
pub mod scenario;
pub mod solvers;
pub mod state;
pub mod test_utils;
pub mod timestep;
pub mod utils;
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
    #[error("virtual storage index {0} not found")]
    VirtualStorageIndexNotFound(VirtualStorageIndex),
    #[error("parameter index {0} not found")]
    ParameterIndexNotFound(ParameterIndex<f64>),
    #[error("index parameter index {0} not found")]
    IndexParameterIndexNotFound(ParameterIndex<usize>),
    #[error("multi-value parameter index {0} not found")]
    MultiValueParameterIndexNotFound(ParameterIndex<MultiValue>),
    #[error("parameter index {0} not found")]
    GeneralParameterIndexNotFound(GeneralParameterIndex<f64>),
    #[error("index parameter index {0} not found")]
    GeneralIndexParameterIndexNotFound(GeneralParameterIndex<usize>),
    #[error("multi-value parameter index {0} not found")]
    GeneralMultiValueParameterIndexNotFound(GeneralParameterIndex<MultiValue>),
    #[error("parameter index {0} not found")]
    SimpleParameterIndexNotFound(SimpleParameterIndex<f64>),
    #[error("index parameter index {0} not found")]
    SimpleIndexParameterIndexNotFound(SimpleParameterIndex<usize>),
    #[error("multi-value parameter index {0} not found")]
    SimpleMultiValueParameterIndexNotFound(SimpleParameterIndex<MultiValue>),
    #[error("parameter index {0} not found")]
    ConstParameterIndexNotFound(ConstParameterIndex<f64>),
    #[error("index parameter index {0} not found")]
    ConstIndexParameterIndexNotFound(ConstParameterIndex<usize>),
    #[error("multi-value parameter index {0} not found")]
    ConstMultiValueParameterIndexNotFound(ConstParameterIndex<MultiValue>),
    #[error("multi-value parameter key {0} not found")]
    MultiValueParameterKeyNotFound(String),
    #[error("inter-network parameter state not initialised")]
    InterNetworkParameterStateNotInitialised,
    #[error("inter-network parameter index {0} not found")]
    MultiNetworkTransferIndexNotFound(MultiNetworkTransferIndex),
    #[error("parameter {0} not found")]
    ParameterNotFound(String),
    #[error("metric set index {0} not found")]
    MetricSetIndexNotFound(MetricSetIndex),
    #[error("metric set with name {0} not found")]
    MetricSetNotFound(String),
    #[error("recorder index not found")]
    RecorderIndexNotFound,
    #[error("recorder not found")]
    RecorderNotFound,
    #[error("derived metric not found")]
    DerivedMetricNotFound,
    #[error("derived metric index {0} not found")]
    DerivedMetricIndexNotFound(DerivedMetricIndex),
    #[error("node name `{0}` already exists")]
    NodeNameAlreadyExists(String),
    #[error("parameter name `{0}` already exists")]
    ParameterNameAlreadyExists(String),
    #[error("index parameter name `{0}` already exists at index {1}")]
    IndexParameterNameAlreadyExists(String, ParameterIndex<usize>),
    #[error("multi-value parameter name `{0}` already exists at index {1}")]
    MultiValueParameterNameAlreadyExists(String, ParameterIndex<MultiValue>),
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
    #[error("No more timesteps")]
    EndOfTimesteps,
    #[error("can not add virtual storage node to a storage node")]
    NoVirtualStorageOnStorageNode,
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
    #[error("scenario group index not found: {0}")]
    ScenarioGroupIndexNotFound(usize),
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
    #[error("recorder does not supported aggregation")]
    RecorderDoesNotSupportAggregation,
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
    #[error("interpolation error: {0}")]
    Interpolation(#[from] InterpolationError),
    #[error("network {0} not found")]
    NetworkNotFound(String),
    #[error("network index ({0}) not found")]
    NetworkIndexNotFound(usize),
    #[error("network name `{0}` already exists.")]
    NetworkNameAlreadyExists(String),
    #[error("parameters do not provide an initial value")]
    ParameterNoInitialValue,
    #[error("parameter state not found for parameter index {0}")]
    ParameterStateNotFound(ParameterIndex<f64>),
    #[error("parameter state not found for parameter index {0}")]
    GeneralParameterStateNotFound(GeneralParameterIndex<f64>),
    #[error("Could not create timestep range due to following error: {0}")]
    TimestepRangeGenerationError(String),
    #[error("Could not create timesteps for frequency '{0}'")]
    TimestepGenerationError(String),
    #[error("Pywr does not currently support timesteps of varying duration")]
    TimestepDurationMismatch,
    #[error("aggregation error: {0}")]
    Aggregation(#[from] AggregationError),
    #[error("cannot simplify metric")]
    CannotSimplifyMetric,
    #[error("Negative factor is not allowed")]
    NegativeFactor,
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
