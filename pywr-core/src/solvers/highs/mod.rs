mod settings;

use crate::network::Network;
use crate::solvers::builder::{BuiltSolver, ColType, SolverBuilder};
use crate::solvers::{Solver, SolverFeatures, SolverTimings};
use crate::state::{ConstParameterValues, State};
use crate::timestep::Timestep;
use crate::PywrError;
use highs_sys::{
    kHighsVarTypeContinuous, kHighsVarTypeInteger, Highs_changeCoeff, HighsInt, Highs_addCols, Highs_addRows, Highs_changeCoeff,
    Highs_changeColIntegrality, Highs_changeColsCostByRange, Highs_changeObjectiveSense, Highs_changeRowsBoundsByMask,
    Highs_create, Highs_getDoubleInfoValue, Highs_getSolution, Highs_run, Highs_setBoolOptionValue,
    Highs_setStringOptionValue, OBJECTIVE_SENSE_MINIMIZE, STATUS_OK,
};
use libc::c_void;
pub use settings::{HighsSolverSettings, HighsSolverSettingsBuilder};
use std::ffi::CString;
use std::ops::Deref;
use std::ptr::null;
use std::time::Instant;

struct Highs {
    ptr: *mut c_void,
}

unsafe impl Send for Highs {}

impl Default for Highs {
    fn default() -> Self {
        let model: Highs;

        unsafe {
            let ptr = Highs_create();
            model = Self { ptr };
            let option_name = CString::new("output_flag").unwrap();
            Highs_setBoolOptionValue(ptr, option_name.as_ptr(), 0);

            // TODO - can these be put into the logging system?
            // let option_name = CString::new("log_to_console").unwrap();
            // Highs_setBoolOptionValue(ptr, option_name.as_ptr(), 1);
            // let option_name = CString::new("log_dev_level").unwrap();
            // Highs_setIntOptionValue(ptr, option_name.as_ptr(), 2);
            // model.presolve("off");

            Highs_changeObjectiveSense(ptr, OBJECTIVE_SENSE_MINIMIZE);
        }

        model
    }
}

// TODO add error handling for all Highs calls

impl Highs {
    #[allow(dead_code)]
    fn presolve(&mut self, value: &str) {
        let option_name = CString::new("presolve").unwrap();
        let option_value = CString::new(value).unwrap();
        unsafe {
            let ret = Highs_setStringOptionValue(self.ptr, option_name.as_ptr(), option_value.as_ptr());
            assert_eq!(ret, STATUS_OK);
        }
    }

    pub fn add_cols(
        &mut self,
        col_lower: &[f64],
        col_upper: &[f64],
        col_obj_coef: &[f64],
        col_type: &[ColType],
        ncols: HighsInt,
    ) {
        // Add all of the columns
        unsafe {
            let ret = Highs_addCols(
                self.ptr,
                ncols,
                col_obj_coef.as_ptr(),
                col_lower.as_ptr(),
                col_upper.as_ptr(),
                0,
                null(),
                null(),
                null(),
            );
            assert_eq!(ret, STATUS_OK);
        }

        // Now change the column types
        for (i, &ctype) in col_type.iter().enumerate() {
            let ctype_int: HighsInt = match ctype {
                ColType::Continuous => kHighsVarTypeContinuous,
                ColType::Integer => kHighsVarTypeInteger,
            };

            unsafe {
                let ret = Highs_changeColIntegrality(self.ptr, i as HighsInt, ctype_int);
                assert_eq!(ret, STATUS_OK);
            }
        }
    }

    pub fn add_rows(
        &mut self,
        row_lower: &[f64],
        row_upper: &[f64],
        nnz: HighsInt,
        row_starts: &[HighsInt],
        columns: &[HighsInt],
        elements: &[f64],
    ) {
        unsafe {
            let ret = Highs_addRows(
                self.ptr,
                row_upper.len() as HighsInt,
                row_lower.as_ptr(),
                row_upper.as_ptr(),
                nnz,
                row_starts.as_ptr(),
                columns.as_ptr(),
                elements.as_ptr(),
            );
            assert_eq!(ret, STATUS_OK);
        }
    }

    pub fn change_objective_coefficients(&mut self, obj_coefficients: &[f64], numcols: HighsInt) {
        unsafe {
            let ret = Highs_changeColsCostByRange(self.ptr, 0, numcols - 1, obj_coefficients.as_ptr());
            assert_eq!(ret, STATUS_OK);
        }
    }

    pub fn change_row_bounds(&mut self, mask: &[HighsInt], lower: &[f64], upper: &[f64]) {
        unsafe {
            let ret = Highs_changeRowsBoundsByMask(self.ptr, mask.as_ptr(), lower.as_ptr(), upper.as_ptr());
            assert_eq!(ret, STATUS_OK);
        }
    }

    pub fn change_coefficient(&mut self, row: HighsInt, col: HighsInt, value: f64) {
        unsafe {
            let ret = Highs_changeCoeff(self.ptr, row, col, value);
            assert_eq!(ret, STATUS_OK);
        }
    }

    pub fn run(&mut self) {
        unsafe {
            let status = Highs_run(self.ptr);
            assert_eq!(status, STATUS_OK);
        }
    }

    #[allow(dead_code)]
    pub fn objective_value(&mut self) -> f64 {
        let mut objective_function_value = 0.;
        unsafe {
            let info_name = CString::new("objective_function_value").unwrap();
            Highs_getDoubleInfoValue(
                self.ptr,
                info_name.as_ptr(),
                (&mut objective_function_value) as *mut f64,
            );
        }
        objective_function_value
    }

    pub fn primal_column_solution(&mut self, numcol: usize, numrow: usize) -> Vec<f64> {
        let colvalue: &mut [f64] = &mut vec![0.; numcol];
        let coldual: &mut [f64] = &mut vec![0.; numcol];
        let rowvalue: &mut [f64] = &mut vec![0.; numrow];
        let rowdual: &mut [f64] = &mut vec![0.; numrow];

        unsafe {
            // Get the primal and dual solution
            let ret = Highs_getSolution(
                self.ptr,
                colvalue.as_mut_ptr(),
                coldual.as_mut_ptr(),
                rowvalue.as_mut_ptr(),
                rowdual.as_mut_ptr(),
            );
            assert_eq!(ret, STATUS_OK);
        }
        colvalue.to_vec()
    }
}

pub struct HighsSolver {
    builder: BuiltSolver<HighsInt>,
    highs: Highs,
}

impl Solver for HighsSolver {
    type Settings = HighsSolverSettings;

    fn name() -> &'static str {
        "highs"
    }

    fn features() -> &'static [SolverFeatures] {
        &[
            SolverFeatures::VirtualStorage,
            SolverFeatures::MutualExclusivity,
            SolverFeatures::AggregatedNode,
            SolverFeatures::AggregatedNodeFactors,
        ]
    }

    fn setup(
        network: &Network,
        values: &ConstParameterValues,
        _settings: &Self::Settings,
    ) -> Result<Box<Self>, PywrError> {
        let builder: SolverBuilder<HighsInt> = SolverBuilder::default();
        let built = builder.create(network, values)?;

        let num_cols = built.num_cols();
        let num_nz = built.num_non_zero();

        let mut highs_lp = Highs::default();

        highs_lp.add_cols(
            built.col_lower(),
            built.col_upper(),
            built.col_obj_coef(),
            built.col_type(),
            num_cols,
        );

        highs_lp.add_rows(
            built.row_lower(),
            built.row_upper(),
            num_nz,
            built.row_starts(),
            built.columns(),
            built.elements(),
        );

        Ok(Box::new(Self {
            builder: built,
            highs: highs_lp,
        }))
    }
    fn solve(&mut self, network: &Network, timestep: &Timestep, state: &mut State) -> Result<SolverTimings, PywrError> {
        let mut timings = SolverTimings::default();
        self.builder.update(network, timestep, state, &mut timings)?;

        let num_cols = self.builder.num_cols();
        let num_rows = self.builder.num_rows();

        let now = Instant::now();
        self.highs
            .change_objective_coefficients(self.builder.col_obj_coef(), num_cols);
        timings.update_objective += now.elapsed();

        let now = Instant::now();

        self.highs.change_row_bounds(
            self.builder.row_mask(),
            self.builder.row_lower(),
            self.builder.row_upper(),
        );

        for (row, column, coefficient) in self.builder.coefficients_to_update() {
            // Highs only accepts coefficients in the range -1e10 to 1e10
            self.highs
                .change_coefficient(*row, *column, coefficient.clamp(-1e10, 1e10));
        }

        timings.update_constraints += now.elapsed();

        let now = Instant::now();
        self.highs.run();
        let solution = self.highs.primal_column_solution(num_cols as usize, num_rows as usize);
        timings.solve = now.elapsed();

        // Reset the network state from the results
        let network_state = state.get_mut_network_state();
        network_state.reset();
        let start_save_solution = Instant::now();

        for edge in network.edges().deref() {
            let col = self.builder.col_for_edge(&edge.index()) as usize;
            let flow = solution[col];
            network_state.add_flow(edge, timestep, flow)?;
        }
        network_state.complete(network, timestep)?;
        timings.save_solution += start_save_solution.elapsed();

        Ok(timings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use float_cmp::approx_eq;

    #[test]
    fn create() {
        Highs::default();
    }

    #[test]
    fn add_rows() {
        let mut lp = Highs::default();

        let col_lower: Vec<f64> = vec![0.0, 0.0];
        let col_upper: Vec<f64> = vec![f64::MAX, f64::MAX];
        let col_obj_coef: Vec<f64> = vec![1.0, 1.0];
        let col_type = vec![ColType::Continuous, ColType::Continuous];

        lp.add_cols(&col_lower, &col_upper, &col_obj_coef, &col_type, 2);

        let row_lower: Vec<f64> = vec![0.0];
        let row_upper: Vec<f64> = vec![2.0];
        let row_starts: Vec<HighsInt> = vec![0, 2];
        let columns: Vec<HighsInt> = vec![0, 1];
        let elements: Vec<f64> = vec![1.0, 1.0];

        lp.add_rows(&row_lower, &row_upper, 2, &row_starts, &columns, &elements);
    }

    #[test]
    fn simple_solve() {
        let row_upper = vec![10.0, 15.0];
        let row_lower = vec![0.0, 0.0];
        let col_lower = vec![0.0, 0.0, 0.0];
        let col_upper = vec![f64::MAX, f64::MAX, f64::MAX];
        let col_obj_coef = vec![-2.0, -3.0, -4.0];
        let col_type = vec![ColType::Continuous, ColType::Continuous, ColType::Continuous];
        let row_starts = vec![0, 3, 6];
        let columns = vec![0, 1, 2, 0, 1, 2];
        let elements = vec![3.0, 2.0, 1.0, 2.0, 5.0, 3.0];

        let mut lp = Highs::default();
        let ncols = col_upper.len() as HighsInt;
        let nrows = row_upper.len() as HighsInt;
        let nnz = elements.len() as HighsInt;

        lp.add_cols(&col_lower, &col_upper, &col_obj_coef, &col_type, ncols);

        lp.add_rows(&row_lower, &row_upper, nnz, &row_starts, &columns, &elements);
        lp.run();

        assert!(approx_eq!(f64, lp.objective_value(), -20.0));
        assert_eq!(
            lp.primal_column_solution(ncols as usize, nrows as usize),
            vec![0.0, 0.0, 5.0]
        );
    }

    #[test]
    fn solve_with_inf_row_bound() {
        let row_upper = vec![10.0, f64::MAX];
        let row_lower = vec![0.0, 0.0];
        let col_lower = vec![0.0, 0.0, 0.0];
        let col_upper = vec![f64::MAX, f64::MAX, f64::MAX];
        let col_obj_coef = vec![-2.0, -3.0, -4.0];
        let col_type = vec![ColType::Continuous, ColType::Continuous, ColType::Continuous];
        let row_starts = vec![0, 3, 6];
        let columns = vec![0, 1, 2, 0, 1, 2];
        let elements = vec![3.0, 2.0, 1.0, 2.0, 5.0, 3.0];

        let mut lp = Highs::default();
        let ncols = col_upper.len() as HighsInt;
        let nrows = row_upper.len() as HighsInt;
        let nnz = elements.len() as HighsInt;

        lp.add_cols(&col_lower, &col_upper, &col_obj_coef, &col_type, ncols);

        lp.add_rows(&row_lower, &row_upper, nnz, &row_starts, &columns, &elements);
        lp.run();

        assert!(approx_eq!(f64, lp.objective_value(), -40.0));
        assert_eq!(
            lp.primal_column_solution(ncols as usize, nrows as usize),
            vec![0.0, 0.0, 10.0]
        );
    }
}
