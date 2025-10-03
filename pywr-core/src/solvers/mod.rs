use crate::network::Network;
use crate::state::{ConstParameterValues, State};
use crate::timestep::Timestep;
use std::ops::{Add, AddAssign};
use std::time::Duration;
use thiserror::Error;

#[cfg(any(feature = "cbc", feature = "clp", feature = "highs", feature = "microlp"))]
mod builder;

#[cfg(feature = "cbc")]
mod cbc;

#[cfg(feature = "clp")]
mod clp;

#[cfg(any(
    feature = "cbc",
    feature = "clp",
    feature = "highs",
    feature = "ipm-ocl",
    feature = "ipm-simd",
    feature = "microlp"
))]
mod col_edge_map;
#[cfg(feature = "highs")]
mod highs;
#[cfg(feature = "ipm-ocl")]
mod ipm_ocl;
#[cfg(feature = "ipm-simd")]
mod ipm_simd;
#[cfg(feature = "microlp")]
mod microlp;

#[cfg(feature = "ipm-ocl")]
pub use self::ipm_ocl::{ClIpmF32Solver, ClIpmF64Solver, ClIpmSolverSettings, ClIpmSolverSettingsBuilder};
#[cfg(all(feature = "ipm-simd", feature = "pyo3"))]
pub use self::ipm_simd::build_ipm_simd_settings_py;
#[cfg(feature = "ipm-simd")]
pub use self::ipm_simd::{SimdIpmF64Solver, SimdIpmSolverSettings, SimdIpmSolverSettingsBuilder};
use crate::aggregated_node::AggregatedNodeIndex;
use crate::node::NodeIndex;
#[cfg(all(feature = "cbc", feature = "pyo3"))]
pub use cbc::build_cbc_settings_py;
#[cfg(feature = "cbc")]
pub use cbc::{CbcError, CbcSolver, CbcSolverSettings, CbcSolverSettingsBuilder};
#[cfg(all(feature = "clp", feature = "pyo3"))]
pub use clp::build_clp_settings_py;
#[cfg(feature = "clp")]
pub use clp::{ClpSolveStatusError, ClpSolver, ClpSolverSettings, ClpSolverSettingsBuilder};
#[cfg(all(feature = "highs", feature = "pyo3"))]
pub use highs::build_highs_settings_py;
#[cfg(feature = "highs")]
pub use highs::{HighsSolver, HighsSolverSettings, HighsSolverSettingsBuilder};
#[cfg(feature = "microlp")]
pub use microlp::{MicroLpError, MicroLpSolver, MicroLpSolverSettings, MicroLpSolverSettingsBuilder};

#[derive(Default, Debug, Clone)]
pub struct SolverTimings {
    pub update_objective: Duration,
    pub update_constraints: Duration,
    pub solve: Duration,
    pub save_solution: Duration,
}

impl SolverTimings {
    pub fn total(&self) -> Duration {
        self.update_objective + self.update_constraints + self.solve + self.save_solution
    }
}

impl Add<SolverTimings> for SolverTimings {
    type Output = SolverTimings;

    fn add(self, rhs: SolverTimings) -> Self::Output {
        Self {
            update_objective: self.update_objective + rhs.update_objective,
            update_constraints: self.update_constraints + rhs.update_constraints,
            solve: self.solve + rhs.solve,
            save_solution: self.save_solution + rhs.save_solution,
        }
    }
}

impl AddAssign for SolverTimings {
    fn add_assign(&mut self, rhs: Self) {
        self.update_objective += rhs.update_objective;
        self.update_constraints += rhs.update_constraints;
        self.solve += rhs.solve;
        self.save_solution += rhs.save_solution;
    }
}

/// Features that a solver provides or a model may required.
///
/// This enum is used to ensure that a given solver implements the appropriate features
/// to solve a given model.
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum SolverFeatures {
    AggregatedNode,
    AggregatedNodeFactors,
    AggregatedNodeDynamicFactors,
    VirtualStorage,
    MutualExclusivity,
}

/// Solver settings that are common to all solvers.
pub trait SolverSettings {
    fn parallel(&self) -> bool;
    fn threads(&self) -> usize;
    fn ignore_feature_requirements(&self) -> bool;
}

/// Errors that can occur during solver setup.
#[derive(Debug, Error)]
pub enum SolverSetupError {
    #[error("Node error: {0}")]
    NodeError(#[from] crate::node::NodeError),
    #[error("Cannot create linear programme. No edges defined in the model")]
    NoEdgesDefined,
    #[error("Node index not found: {0}")]
    NodeIndexNotFound(NodeIndex),
    #[cfg(feature = "highs")]
    #[error("Highs error: {0}")]
    HighsError(#[from] highs::HighsStatusError),
}

/// Errors that can occur during solver solve.
#[derive(Debug, Error)]
pub enum SolverSolveError {
    #[error("Edge from `{from_name}` and sub-name `{}` to `{to_name}` and sub-name `{}` error: {source}", .from_sub_name.as_deref().unwrap_or("None"), .to_sub_name.as_deref().unwrap_or("None"))]
    EdgeError {
        from_name: String,
        from_sub_name: Option<String>,
        to_name: String,
        to_sub_name: Option<String>,
        #[source]
        source: crate::edge::EdgeError,
    },
    #[error("Node `{name}` and sub-name `{}` error: {source}", .sub_name.as_deref().unwrap_or("None"))]
    NodeError {
        name: String,
        sub_name: Option<String>,
        #[source]
        source: crate::node::NodeError,
    },
    #[error("Aggregated node `{name}` and sub-name `{}` error: {source}", .sub_name.as_deref().unwrap_or("None"))]
    AggregatedNodeError {
        name: String,
        sub_name: Option<String>,
        #[source]
        source: crate::aggregated_node::AggregatedNodeError,
    },
    #[error("Virtual storage error: {0}")]
    VirtualStorageError(#[from] crate::virtual_storage::VirtualStorageError),
    #[error("Node index not found: {0}")]
    NodeIndexNotFound(NodeIndex),
    #[error("Aggregated node index not found: {0}")]
    AggregatedNodeIndexNotFound(AggregatedNodeIndex),
    #[error("missing solver features")]
    MissingSolverFeatures,
    #[error("Network state error: {0}")]
    NetworkStateError(#[from] crate::state::NetworkStateError),
    #[error("State error: {0}")]
    StateError(#[from] crate::state::StateError),
    #[cfg(feature = "clp")]
    #[error("Clp error: {0}")]
    ClpSolveError(#[from] ClpSolveStatusError),
    #[cfg(feature = "highs")]
    #[error("Highs error: {0}")]
    HighsError(#[from] highs::HighsStatusError),
    #[cfg(feature = "highs")]
    #[error("Highs error: {0}")]
    HighsModelError(#[from] highs::HighsModelError),
    #[cfg(feature = "microlp")]
    #[error("MicroLP error: {0}")]
    MicroLpError(#[from] MicroLpError),
}

pub trait Solver: Send {
    type Settings;

    fn name() -> &'static str;
    /// An array of features that this solver provides.
    fn features() -> &'static [SolverFeatures];
    fn setup(
        model: &Network,
        values: &ConstParameterValues,
        settings: &Self::Settings,
    ) -> Result<Box<Self>, SolverSetupError>;
    fn solve(
        &mut self,
        model: &Network,
        timestep: &Timestep,
        state: &mut State,
    ) -> Result<SolverTimings, SolverSolveError>;
}

pub trait MultiStateSolver: Send {
    type Settings;

    fn name() -> &'static str;
    /// An array of features that this solver provides.
    fn features() -> &'static [SolverFeatures];
    fn setup(model: &Network, num_scenarios: usize, settings: &Self::Settings) -> Result<Box<Self>, SolverSetupError>;
    fn solve(
        &mut self,
        model: &Network,
        timestep: &Timestep,
        states: &mut [State],
    ) -> Result<SolverTimings, SolverSolveError>;
}
