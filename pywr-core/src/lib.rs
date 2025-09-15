// Requires upgrading to PyO3 v0.23 which is blocked by updating Polars
// See also: https://github.com/PyO3/pyo3/issues/4743
#![allow(unexpected_cfgs)]

extern crate core;

use crate::node::NodeIndex;
use crate::recorders::RecorderIndex;
pub use network::NetworkError;

pub mod agg_funcs;
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
