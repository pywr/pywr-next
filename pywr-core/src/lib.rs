use crate::node::NodeIndex;
use crate::recorders::RecorderIndex;
pub use network::NetworkError;

pub mod agg_funcs;
pub mod aggregated_node;
mod aggregated_storage_node;
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

/// Absolute tolerance for floating-point equality checks.
/// For values with large magnitudes, consider using relative tolerance instead.
const FLOAT_EQ_TOLERANCE: f64 = 1e-6;

/// Absolute tolerance for detecting mass-balance discrepancies on storage nodes.
const STORAGE_MASS_BALANCE_TOLERANCE: f64 = 1e-6;
