use crate::network::Network;
use crate::state::{ConstParameterValues, State};
use crate::timestep::Timestep;
use crate::PywrError;
use std::ops::{Add, AddAssign};
use std::time::Duration;

mod builder;

#[cfg(feature = "cbc")]
mod cbc;
mod clp;
mod col_edge_map;
#[cfg(feature = "highs")]
mod highs;
#[cfg(feature = "ipm-ocl")]
mod ipm_ocl;
#[cfg(feature = "ipm-simd")]
mod ipm_simd;

#[cfg(feature = "ipm-ocl")]
pub use self::ipm_ocl::{ClIpmF32Solver, ClIpmF64Solver, ClIpmSolverSettings, ClIpmSolverSettingsBuilder};
#[cfg(feature = "ipm-simd")]
pub use self::ipm_simd::{SimdIpmF64Solver, SimdIpmSolverSettings, SimdIpmSolverSettingsBuilder};
#[cfg(feature = "cbc")]
pub use cbc::{CbcError, CbcSolver, CbcSolverSettings, CbcSolverSettingsBuilder};
pub use clp::{ClpError, ClpSolver, ClpSolverSettings, ClpSolverSettingsBuilder};
#[cfg(feature = "highs")]
pub use highs::{HighsSolver, HighsSolverSettings, HighsSolverSettingsBuilder};

#[derive(Default, Debug)]
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
#[derive(PartialEq, Eq, Hash)]
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
}

pub trait Solver: Send {
    type Settings;

    fn name() -> &'static str;
    /// An array of features that this solver provides.
    fn features() -> &'static [SolverFeatures];
    fn setup(model: &Network, values: &ConstParameterValues, settings: &Self::Settings)
        -> Result<Box<Self>, PywrError>;
    fn solve(&mut self, model: &Network, timestep: &Timestep, state: &mut State) -> Result<SolverTimings, PywrError>;
}

pub trait MultiStateSolver: Send {
    type Settings;

    fn name() -> &'static str;
    /// An array of features that this solver provides.
    fn features() -> &'static [SolverFeatures];
    fn setup(model: &Network, num_scenarios: usize, settings: &Self::Settings) -> Result<Box<Self>, PywrError>;
    fn solve(&mut self, model: &Network, timestep: &Timestep, states: &mut [State])
        -> Result<SolverTimings, PywrError>;
}
