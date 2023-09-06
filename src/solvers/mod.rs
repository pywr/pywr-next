use crate::model::Model;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use std::ops::{Add, AddAssign};
use std::time::Duration;

mod builder;
#[cfg(feature = "ipm-ocl")]
mod ipm_ocl;

mod clp;
mod col_edge_map;
#[cfg(feature = "highs")]
mod highs;
#[cfg(feature = "ipm-simd")]
mod ipm_simd;

#[cfg(feature = "ipm-ocl")]
pub use self::ipm_ocl::{ClIpmF32Solver, ClIpmF64Solver};
#[cfg(feature = "ipm-simd")]
pub use self::ipm_simd::SimdIpmF64Solver;
pub use clp::{ClpError, ClpSolver};

#[cfg(feature = "highs")]
pub use highs::HighsSolver;

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

pub trait Solver: Send {
    fn setup(model: &Model) -> Result<Box<Self>, PywrError>;
    fn solve(&mut self, model: &Model, timestep: &Timestep, state: &mut State) -> Result<SolverTimings, PywrError>;
}

pub trait MultiStateSolver: Send {
    fn setup(model: &Model, num_scenarios: usize) -> Result<Box<Self>, PywrError>;
    fn solve(&mut self, model: &Model, timestep: &Timestep, states: &mut [State]) -> Result<SolverTimings, PywrError>;
}
