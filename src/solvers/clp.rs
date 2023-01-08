use super::builder::SolverBuilder;
use crate::model::Model;
use crate::node::NodeType;
use crate::solvers::{Solver, SolverTimings};
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use clp_sys::*;
use libc::{c_double, c_int};
use std::collections::HashMap;
use std::ffi::CString;
use std::ops::Deref;
use std::slice;
use std::time::Instant;
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum ClpError {
    #[error("an unknown error occurred in Clp.")]
    UnknownError,
    #[error("the simplex model has not been created")]
    SimplexNotInitialisedError,
}

pub type CoinBigIndex = c_int;

struct ClpSimplex {
    ptr: *mut Clp_Simplex,
}

unsafe impl Send for ClpSimplex {}

impl Default for ClpSimplex {
    fn default() -> Self {
        let model: ClpSimplex;

        unsafe {
            let ptr = Clp_newModel();
            model = ClpSimplex { ptr };
            Clp_setLogLevel(ptr, 0);
            Clp_setObjSense(ptr, 1.0);
        }
        model
    }
}

impl ClpSimplex {
    pub fn print(&mut self) {
        unsafe {
            let prefix = CString::new("  ").expect("CString::new failed");
            Clp_printModel(self.ptr, prefix.as_ptr());
        }
    }

    pub fn resize(&mut self, new_number_rows: c_int, new_number_columns: c_int) {
        unsafe {
            Clp_resize(self.ptr, new_number_rows, new_number_columns);
        }
    }

    pub fn change_row_lower(&mut self, row_lower: &[c_double]) {
        unsafe {
            Clp_chgRowLower(self.ptr, row_lower.as_ptr());
        }
    }

    pub fn change_row_upper(&mut self, row_upper: &[c_double]) {
        unsafe {
            Clp_chgRowUpper(self.ptr, row_upper.as_ptr());
        }
    }

    pub fn change_column_lower(&mut self, column_lower: &[c_double]) {
        unsafe {
            Clp_chgColumnLower(self.ptr, column_lower.as_ptr());
        }
    }

    pub fn change_column_upper(&mut self, column_upper: &[c_double]) {
        unsafe {
            Clp_chgColumnUpper(self.ptr, column_upper.as_ptr());
        }
    }

    pub fn change_objective_coefficients(&mut self, obj_coefficients: &[c_double]) {
        unsafe {
            Clp_chgObjCoefficients(self.ptr, obj_coefficients.as_ptr());
        }
    }

    pub fn add_rows(
        &mut self,
        row_lower: &[c_double],
        row_upper: &[c_double],
        row_starts: &[CoinBigIndex],
        columns: &[c_int],
        elements: &[c_double],
    ) {
        let number: c_int = row_lower.len() as c_int;

        unsafe {
            Clp_addRows(
                self.ptr,
                number,
                row_lower.as_ptr(),
                row_upper.as_ptr(),
                row_starts.as_ptr(),
                columns.as_ptr(),
                elements.as_ptr(),
            )
        }
    }

    fn initial_solve(&mut self) {
        unsafe {
            Clp_initialSolve(self.ptr);
        }
    }

    fn initial_dual_solve(&mut self) {
        unsafe {
            Clp_initialDualSolve(self.ptr);
        }
    }

    fn initial_primal_solve(&mut self) {
        unsafe {
            Clp_initialPrimalSolve(self.ptr);
        }
    }

    fn dual_solve(&mut self) {
        unsafe {
            Clp_dual(self.ptr, 0);
        }
    }

    fn primal_solve(&mut self) {
        unsafe {
            Clp_primal(self.ptr, 0);
        }
    }

    fn primal_column_solution(&mut self, number: usize) -> Vec<c_double> {
        let solution: Vec<c_double>;
        unsafe {
            let data_ptr = Clp_primalColumnSolution(self.ptr);
            solution = slice::from_raw_parts(data_ptr, number).to_vec()
        }
        solution
    }

    fn get_objective_coefficients(&mut self, number: usize) -> Vec<c_double> {
        let coef: Vec<c_double>;
        unsafe {
            let data_ptr = Clp_getObjCoefficients(self.ptr);
            coef = slice::from_raw_parts(data_ptr, number).to_vec()
        }
        coef
    }

    fn get_row_upper(&mut self, number: usize) -> Vec<c_double> {
        let ub: Vec<c_double>;
        unsafe {
            let data_ptr = Clp_getRowUpper(self.ptr);
            ub = slice::from_raw_parts(data_ptr, number).to_vec()
        }
        ub
    }

    fn objective_value(&self) -> c_double {
        unsafe { Clp_objectiveValue(self.ptr) }
    }
}

pub struct ClpSolver {
    builder: SolverBuilder<c_int>,
    clp_simplex: ClpSimplex,
}

impl ClpSolver {
    fn from_builder(builder: SolverBuilder<c_int>) -> Self {
        let mut clp_simplex = ClpSimplex::default();

        let num_cols = builder.num_cols();

        clp_simplex.resize(0, num_cols);

        clp_simplex.change_column_lower(builder.col_lower());
        clp_simplex.change_column_upper(builder.col_upper());
        clp_simplex.change_objective_coefficients(builder.col_obj_coef());

        clp_simplex.add_rows(
            builder.row_lower(),
            builder.row_upper(),
            builder.row_starts(),
            builder.columns(),
            builder.elements(),
        );

        clp_simplex.initial_dual_solve();

        ClpSolver { builder, clp_simplex }
    }

    fn solve(&mut self) -> Vec<c_double> {
        self.clp_simplex.dual_solve();

        let num_cols = self.builder.num_cols() as usize;

        self.clp_simplex.primal_column_solution(num_cols)
    }
}

impl Solver for ClpSolver {
    fn setup(model: &Model) -> Result<Box<Self>, PywrError> {
        let builder = SolverBuilder::create(model)?;
        let solver = ClpSolver::from_builder(builder);
        Ok(Box::new(solver))
    }

    fn solve(&mut self, model: &Model, timestep: &Timestep, state: &mut State) -> Result<SolverTimings, PywrError> {
        let mut timings = SolverTimings::default();
        self.builder.update(model, timestep, state, &mut timings)?;

        let now = Instant::now();
        self.clp_simplex
            .change_objective_coefficients(self.builder.col_obj_coef());
        timings.update_objective += now.elapsed();

        let now = Instant::now();
        self.clp_simplex.change_row_lower(self.builder.row_lower());
        self.clp_simplex.change_row_upper(self.builder.row_upper());
        timings.update_constraints += now.elapsed();

        let now = Instant::now();
        let solution = self.solve();
        timings.solve = now.elapsed();

        // Create the updated network state from the results
        let network_state = state.get_mut_network_state();
        network_state.reset();

        let start_save_solution = Instant::now();
        for edge in model.edges.deref() {
            let flow = solution[*edge.index().deref()];
            network_state.add_flow(edge, timestep, flow)?;
        }
        timings.save_solution += start_save_solution.elapsed();

        Ok(timings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use float_cmp::approx_eq;

    #[test]
    fn clp_create() {
        ClpSimplex::default();
    }

    #[test]
    fn clp_add_rows() {
        let mut model = ClpSimplex::default();
        model.resize(0, 2);

        let row_lower: Vec<c_double> = vec![0.0];
        let row_upper: Vec<c_double> = vec![2.0];
        let row_starts: Vec<CoinBigIndex> = vec![0, 2];
        let columns: Vec<c_int> = vec![0, 1];
        let elements: Vec<c_double> = vec![1.0, 1.0];

        model.add_rows(&row_lower, &row_upper, &row_starts, &columns, &elements);
    }

    #[test]
    fn simple_solve() {
        let row_upper = vec![10.0, 15.0];
        let row_lower = vec![0.0, 0.0];
        let col_lower = vec![0.0, 0.0, 0.0];
        let col_upper = vec![f64::MAX, f64::MAX, f64::MAX];
        let col_obj_coef = vec![-2.0, -3.0, -4.0];
        let row_starts = vec![0, 3, 6];
        let columns = vec![0, 1, 2, 0, 1, 2];
        let elements = vec![3.0, 2.0, 1.0, 2.0, 5.0, 3.0];

        let mut lp = ClpSimplex::default();
        lp.resize(0, col_upper.len() as c_int);

        lp.change_column_lower(&col_lower);
        lp.change_column_upper(&col_upper);
        lp.change_objective_coefficients(&col_obj_coef);

        lp.add_rows(&row_lower, &row_upper, &row_starts, &columns, &elements);
        lp.dual_solve();

        assert!(approx_eq!(f64, lp.objective_value(), -20.0));
        assert_eq!(lp.primal_column_solution(3), vec![0.0, 0.0, 5.0]);
    }

    #[test]
    fn solve_with_inf_row_bound() {
        let row_upper = vec![10.0, f64::MAX];
        let row_lower = vec![0.0, 0.0];
        let col_lower = vec![0.0, 0.0, 0.0];
        let col_upper = vec![f64::MAX, f64::MAX, f64::MAX];
        let col_obj_coef = vec![-2.0, -3.0, -4.0];
        let row_starts = vec![0, 3, 6];
        let columns = vec![0, 1, 2, 0, 1, 2];
        let elements = vec![3.0, 2.0, 1.0, 2.0, 5.0, 3.0];

        let mut lp = ClpSimplex::default();
        lp.resize(0, col_upper.len() as c_int);

        lp.change_column_lower(&col_lower);
        lp.change_column_upper(&col_upper);
        lp.change_objective_coefficients(&col_obj_coef);

        lp.add_rows(&row_lower, &row_upper, &row_starts, &columns, &elements);
        lp.dual_solve();

        assert!(approx_eq!(f64, lp.objective_value(), -40.0));
        assert_eq!(lp.primal_column_solution(3), vec![0.0, 0.0, 10.0]);
    }
}
