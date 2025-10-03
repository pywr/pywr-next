mod settings;

use super::builder::{ColType, SolverBuilder};
use crate::network::Network;
use crate::solvers::builder::BuiltSolver;
use crate::solvers::{Solver, SolverFeatures, SolverSetupError, SolverSolveError, SolverTimings};
use crate::state::{ConstParameterValues, State};
use crate::timestep::Timestep;
use coin_or_sys::cbc::*;
use libc::{c_double, c_int};
#[cfg(feature = "pyo3")]
pub use settings::build_cbc_settings_py;
pub use settings::{CbcSolverSettings, CbcSolverSettingsBuilder};
use std::ffi::{CString, c_char};
use std::time::Instant;
use std::{ptr, slice};
use thiserror::Error;
#[derive(Error, Debug, PartialEq, Eq)]
pub enum CbcError {
    #[error("an unknown error occurred in Cbc.")]
    UnknownError,
}

pub type CoinBigIndex = c_int;

struct Cbc {
    ptr: *mut Cbc_Model,
}

unsafe impl Send for Cbc {}

impl Default for Cbc {
    fn default() -> Self {
        let model: Cbc;

        unsafe {
            let ptr = Cbc_newModel();
            model = Cbc { ptr };
            Cbc_setLogLevel(ptr, 0);
            Cbc_setObjSense(ptr, 1.0);
        }
        model
    }
}

impl Cbc {
    #[allow(dead_code)]
    pub fn print(&mut self) {
        unsafe {
            let prefix = CString::new("  ").expect("CString::new failed");
            Cbc_printModel(self.ptr, prefix.as_ptr());
        }
    }

    pub fn change_row_lower(&mut self, row_lower: &[c_double]) {
        for (i, val) in row_lower.iter().enumerate() {
            unsafe {
                Cbc_setRowLower(self.ptr, i as c_int, *val);
            }
        }
    }

    pub fn change_row_upper(&mut self, row_upper: &[c_double]) {
        for (i, val) in row_upper.iter().enumerate() {
            unsafe {
                Cbc_setRowUpper(self.ptr, i as c_int, *val);
            }
        }
    }

    pub fn change_objective_coefficients(&mut self, obj_coefficients: &[c_double]) {
        for (i, val) in obj_coefficients.iter().enumerate() {
            unsafe {
                Cbc_setObjCoeff(self.ptr, i as c_int, *val);
            }
        }
    }

    pub fn add_cols(
        &mut self,
        col_lower: &[c_double],
        col_upper: &[c_double],
        col_type: &[ColType],
        obj_coefs: &[c_double],
    ) {
        let number: c_int = col_lower.len() as c_int;

        for col_idx in 0..number {
            let lower = col_lower[col_idx as usize];
            let upper = col_upper[col_idx as usize];
            let is_integer = match col_type[col_idx as usize] {
                ColType::Continuous => 0,
                ColType::Integer => 1,
            };
            let obj_coef = obj_coefs[col_idx as usize];

            unsafe {
                let c_name = CString::new("col").expect("Failed to create CString for column name.");
                Cbc_addCol(
                    self.ptr,
                    c_name.as_ptr(),
                    lower,
                    upper,
                    obj_coef,
                    is_integer as c_char,
                    0,
                    ptr::null_mut(),
                    ptr::null_mut(),
                );
            }
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

        for row_idx in 0..number {
            let start = row_starts[row_idx as usize];
            let end = row_starts[row_idx as usize + 1];
            let nz = end - start;
            let cols = &columns[start as usize..end as usize];
            let coefs = &elements[start as usize..end as usize];

            unsafe {
                let c_name = CString::new("row").expect("Failed to create CString for row name.");
                let sense = 'E';
                Cbc_addRow(
                    self.ptr,
                    c_name.as_ptr(),
                    nz,
                    cols.as_ptr(),
                    coefs.as_ptr(),
                    sense as c_char,
                    0.0,
                );

                Cbc_setRowUpper(self.ptr, row_idx, row_upper[row_idx as usize]);
                Cbc_setRowLower(self.ptr, row_idx, row_lower[row_idx as usize]);
            }
        }
    }

    fn solve(&mut self) {
        unsafe {
            let ret = Cbc_solve(self.ptr);
            if ret != 0 {
                panic!("Cbc solve failed with error code: {ret}");
            }
        }
    }

    fn primal_column_solution(&mut self, number: usize) -> Vec<c_double> {
        let solution: Vec<c_double>;
        unsafe {
            let data_ptr = Cbc_getColSolution(self.ptr);
            solution = slice::from_raw_parts(data_ptr, number).to_vec()
        }
        solution
    }

    #[allow(dead_code)]
    fn get_objective_coefficients(&mut self, number: usize) -> Vec<c_double> {
        let coef: Vec<c_double>;
        unsafe {
            let data_ptr = Cbc_getObjCoefficients(self.ptr);
            coef = slice::from_raw_parts(data_ptr, number).to_vec()
        }
        coef
    }

    #[allow(dead_code)]
    fn get_row_upper(&mut self, number: usize) -> Vec<c_double> {
        let ub: Vec<c_double>;
        unsafe {
            let data_ptr = Cbc_getRowUpper(self.ptr);
            ub = slice::from_raw_parts(data_ptr, number).to_vec()
        }
        ub
    }

    #[allow(dead_code)]
    fn objective_value(&self) -> c_double {
        unsafe { Cbc_getObjValue(self.ptr) }
    }
}

pub struct CbcSolver {
    builder: BuiltSolver<c_int>,
    cbc: Cbc,
}

impl CbcSolver {
    fn from_builder(builder: BuiltSolver<c_int>) -> Self {
        let mut cbc = Cbc::default();

        cbc.add_cols(
            builder.col_lower(),
            builder.col_upper(),
            builder.col_type(),
            builder.col_obj_coef(),
        );

        cbc.add_rows(
            builder.row_lower(),
            builder.row_upper(),
            builder.row_starts(),
            builder.columns(),
            builder.elements(),
        );

        CbcSolver { builder, cbc }
    }

    fn solve(&mut self) -> Vec<c_double> {
        self.cbc.solve();

        let num_cols = self.builder.num_cols() as usize;

        self.cbc.primal_column_solution(num_cols)
    }
}

impl Solver for CbcSolver {
    type Settings = CbcSolverSettings;

    fn name() -> &'static str {
        "cbc"
    }

    fn features() -> &'static [SolverFeatures] {
        &[
            SolverFeatures::AggregatedNode,
            SolverFeatures::VirtualStorage,
            SolverFeatures::AggregatedNodeFactors,
            SolverFeatures::MutualExclusivity,
        ]
    }

    fn setup(
        model: &Network,
        values: &ConstParameterValues,
        _settings: &Self::Settings,
    ) -> Result<Box<Self>, SolverSetupError> {
        let builder = SolverBuilder::new(f64::MAX, f64::MIN);
        let built = builder.create(model, values)?;

        let solver = CbcSolver::from_builder(built);
        Ok(Box::new(solver))
    }

    fn solve(
        &mut self,
        model: &Network,
        timestep: &Timestep,
        state: &mut State,
    ) -> Result<SolverTimings, SolverSolveError> {
        let mut timings = SolverTimings::default();
        self.builder.update(model, timestep, state, &mut timings)?;

        let now = Instant::now();
        self.cbc.change_objective_coefficients(self.builder.col_obj_coef());
        timings.update_objective += now.elapsed();

        let now = Instant::now();
        self.cbc.change_row_lower(self.builder.row_lower());
        self.cbc.change_row_upper(self.builder.row_upper());

        if !self.builder.coefficients_to_update().is_empty() {
            return Err(SolverSolveError::MissingSolverFeatures);
            // TODO waiting for support in CBC's C API: https://github.com/coin-or/Cbc/pull/656
            // self.cbc.modify_coefficient(*row, *column, *coefficient)
        }

        timings.update_constraints += now.elapsed();

        let now = Instant::now();
        let solution = self.solve();
        timings.solve = now.elapsed();

        // Create the updated network state from the results
        let network_state = state.get_mut_network_state();
        network_state.reset();

        let start_save_solution = Instant::now();
        for edge in model.edges().iter() {
            let col = self.builder.col_for_edge(&edge.index()) as usize;
            let flow = solution[col];
            // Round very small values to zero
            let flow = if flow.abs() < 1e-10 { 0.0 } else { flow };
            network_state.add_flow(edge, timestep, flow)?;
        }
        state.complete(model, timestep)?;
        timings.save_solution += start_save_solution.elapsed();

        Ok(timings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use float_cmp::approx_eq;

    #[test]
    fn cbc_create() {
        Cbc::default();
    }

    #[test]
    fn cbc_add_rows() {
        let mut model = Cbc::default();
        model.add_cols(
            &[0.0, 0.0],
            &[10.0, 10.0],
            &[ColType::Continuous, ColType::Continuous],
            &[0.0, 0.0],
        );

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
        let col_type = vec![ColType::Continuous, ColType::Continuous, ColType::Continuous];
        let col_obj_coef = vec![-2.0, -3.0, -4.0];
        let row_starts = vec![0, 3, 6];
        let columns = vec![0, 1, 2, 0, 1, 2];
        let elements = vec![3.0, 2.0, 1.0, 2.0, 5.0, 3.0];

        let mut lp = Cbc::default();

        lp.add_cols(&col_lower, &col_upper, &col_type, &col_obj_coef);
        lp.add_rows(&row_lower, &row_upper, &row_starts, &columns, &elements);
        lp.solve();

        assert!(approx_eq!(f64, lp.objective_value(), -20.0));
        assert_eq!(lp.primal_column_solution(3), vec![0.0, 0.0, 5.0]);
    }

    #[test]
    fn solve_with_inf_row_bound() {
        let row_upper = vec![10.0, f64::MAX];
        let row_lower = vec![0.0, 0.0];
        let col_lower = vec![0.0, 0.0, 0.0];
        let col_upper = vec![f64::MAX, f64::MAX, f64::MAX];
        let col_type = vec![ColType::Continuous, ColType::Continuous, ColType::Continuous];
        let col_obj_coef = vec![-2.0, -3.0, -4.0];
        let row_starts = vec![0, 3, 6];
        let columns = vec![0, 1, 2, 0, 1, 2];
        let elements = vec![3.0, 2.0, 1.0, 2.0, 5.0, 3.0];

        let mut lp = Cbc::default();

        lp.add_cols(&col_lower, &col_upper, &col_type, &col_obj_coef);
        lp.add_rows(&row_lower, &row_upper, &row_starts, &columns, &elements);
        lp.solve();

        assert!(approx_eq!(f64, lp.objective_value(), -40.0));
        assert_eq!(lp.primal_column_solution(3), vec![0.0, 0.0, 10.0]);
    }
}
