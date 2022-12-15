use crate::model::Model;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use std::ops::{Add, AddAssign};
use std::time::Duration;

pub mod clp;

#[derive(Default)]
pub struct SolverTimings {
    pub update_objective: Duration,
    pub update_constraints: Duration,
    pub solve: Duration,
    pub save_solution: Duration,
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

pub trait Solver {
    fn setup(&mut self, model: &Model) -> Result<(), PywrError>;
    fn solve(&mut self, model: &Model, timestep: &Timestep, state: &mut State) -> Result<SolverTimings, PywrError>;
}
