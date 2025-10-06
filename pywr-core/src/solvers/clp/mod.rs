mod settings;

use super::builder::SolverBuilder;
use crate::network::Network;
use crate::solvers::builder::BuiltSolver;
use crate::solvers::{Solver, SolverFeatures, SolverSetupError, SolverSolveError, SolverTimings};
use crate::state::{ConstParameterValues, State};
use crate::timestep::Timestep;
use coin_or_sys::clp::*;
use libc::{c_double, c_int};
#[cfg(feature = "pyo3")]
pub use settings::build_clp_settings_py;
pub use settings::{ClpSolverSettings, ClpSolverSettingsBuilder};
use std::ffi::CString;
use std::fmt::Display;
use std::slice;
use std::time::Instant;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ClpSolveStatusError {
    #[error("The problem is primal infeasible. Secondary status: {secondary}")]
    PrimalInfeasible { secondary: ClpSecondaryStatus },
    #[error("The problem is dual infeasible. Secondary status: {secondary}")]
    DualInfeasible { secondary: ClpSecondaryStatus },
    #[error("The problem is stopped on iterations. Secondary status: {secondary}")]
    StoppedOnIterations { secondary: ClpSecondaryStatus },
    #[error("The problem is stopped due to errors. Secondary status: {secondary}")]
    StoppedOnErrors { secondary: ClpSecondaryStatus },
    #[error("An unknown error occurred in Clp with code {code}. Secondary status: {secondary}")]
    Unknown { code: c_int, secondary: ClpSecondaryStatus },
}

/// Convert Clp return codes to Result
fn to_clp_result(code: c_int, secondary: c_int) -> Result<(), ClpSolveStatusError> {
    let secondary_status = ClpSecondaryStatus::from(secondary);
    match code {
        0 => Ok(()),
        1 => Err(ClpSolveStatusError::PrimalInfeasible {
            secondary: secondary_status,
        }),
        2 => Err(ClpSolveStatusError::DualInfeasible {
            secondary: secondary_status,
        }),
        3 => Err(ClpSolveStatusError::StoppedOnIterations {
            secondary: secondary_status,
        }),
        4 => Err(ClpSolveStatusError::StoppedOnErrors {
            secondary: secondary_status,
        }),
        other => Err(ClpSolveStatusError::Unknown {
            code: other,
            secondary: secondary_status,
        }),
    }
}

/// Secondary status codes from Clp
#[derive(Debug)]
pub enum ClpSecondaryStatus {
    NotSet,
    MaybePrimalInfeasible,
    ScaledOptimalPrimalInfeasible,
    ScaledOptimalDualInfeasible,
    ScaledOptimalPrimalAndDualInfeasible,
    GivingUpPrimal,
    EmptyProblemCheck,
    PostSolveNotOptimal,
    BadElementCheck,
    StoppedOnTime,
    StoppedAsPrimalFeasible,
    PresolveInfeasibleOrUnbounded,
    Unknown(c_int),
}

impl From<c_int> for ClpSecondaryStatus {
    fn from(code: c_int) -> Self {
        match code {
            0 => ClpSecondaryStatus::NotSet,
            1 => ClpSecondaryStatus::MaybePrimalInfeasible,
            2 => ClpSecondaryStatus::ScaledOptimalPrimalInfeasible,
            3 => ClpSecondaryStatus::ScaledOptimalDualInfeasible,
            4 => ClpSecondaryStatus::ScaledOptimalPrimalAndDualInfeasible,
            5 => ClpSecondaryStatus::GivingUpPrimal,
            6 => ClpSecondaryStatus::EmptyProblemCheck,
            7 => ClpSecondaryStatus::PostSolveNotOptimal,
            8 => ClpSecondaryStatus::BadElementCheck,
            9 => ClpSecondaryStatus::StoppedOnTime,
            10 => ClpSecondaryStatus::StoppedAsPrimalFeasible,
            11 => ClpSecondaryStatus::PresolveInfeasibleOrUnbounded,
            other => ClpSecondaryStatus::Unknown(other),
        }
    }
}

impl Display for ClpSecondaryStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClpSecondaryStatus::NotSet => write!(f, "No secondary status set"),
            ClpSecondaryStatus::MaybePrimalInfeasible => write!(
                f,
                "Primal infeasible because dual limit reached OR (probably primal infeasible but can't prove it - main status was 4)"
            ),
            ClpSecondaryStatus::ScaledOptimalPrimalInfeasible => write!(
                f,
                "Scaled problem optimal - unscaled problem has primal infeasibilities"
            ),
            ClpSecondaryStatus::ScaledOptimalDualInfeasible => {
                write!(f, "Scaled problem optimal - unscaled problem has dual infeasibilities")
            }
            ClpSecondaryStatus::ScaledOptimalPrimalAndDualInfeasible => write!(
                f,
                "Scaled problem optimal - unscaled problem has primal and dual infeasibilities"
            ),
            ClpSecondaryStatus::GivingUpPrimal => {
                write!(f, "Giving up in primal with flagged variables")
            }
            ClpSecondaryStatus::EmptyProblemCheck => write!(f, "Failed due to empty problem check"),
            ClpSecondaryStatus::PostSolveNotOptimal => write!(f, "PostSolve says not optimal"),
            ClpSecondaryStatus::BadElementCheck => write!(f, "Failed due to bad element check"),
            ClpSecondaryStatus::StoppedOnTime => write!(f, "Status was 3 and stopped on time"),
            ClpSecondaryStatus::StoppedAsPrimalFeasible => {
                write!(f, "Status was 3 but stopped as primal feasible")
            }
            ClpSecondaryStatus::PresolveInfeasibleOrUnbounded => {
                write!(f, "Status was 1/2 from presolve found infeasible or unbounded")
            }
            ClpSecondaryStatus::Unknown(code) => {
                write!(f, "An unknown secondary status error occurred in Clp with code {code}")
            }
        }
    }
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
    #[allow(dead_code)]
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

    pub fn modify_coefficient(&mut self, row: c_int, column: c_int, new_element: c_double) {
        unsafe {
            Clp_modifyCoefficient(self.ptr, row, column, new_element, true);
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

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    fn initial_primal_solve(&mut self) {
        unsafe {
            Clp_initialPrimalSolve(self.ptr);
        }
    }

    fn dual_solve(&mut self) -> Result<(), ClpSolveStatusError> {
        unsafe {
            let _ret = Clp_dual(self.ptr, 0);
            let primary = Clp_status(self.ptr);
            let secondary = Clp_secondaryStatus(self.ptr);
            to_clp_result(primary, secondary)
        }
    }

    #[allow(dead_code)]
    fn primal_solve(&mut self) -> Result<(), ClpSolveStatusError> {
        unsafe {
            let _ret = Clp_primal(self.ptr, 0);
            let primary = Clp_status(self.ptr);
            let secondary = Clp_secondaryStatus(self.ptr);
            to_clp_result(primary, secondary)
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

    #[allow(dead_code)]
    fn get_objective_coefficients(&mut self, number: usize) -> Vec<c_double> {
        let coef: Vec<c_double>;
        unsafe {
            let data_ptr = Clp_getObjCoefficients(self.ptr);
            coef = slice::from_raw_parts(data_ptr, number).to_vec()
        }
        coef
    }

    #[allow(dead_code)]
    fn get_row_upper(&mut self, number: usize) -> Vec<c_double> {
        let ub: Vec<c_double>;
        unsafe {
            let data_ptr = Clp_getRowUpper(self.ptr);
            ub = slice::from_raw_parts(data_ptr, number).to_vec()
        }
        ub
    }

    #[allow(dead_code)]
    fn objective_value(&self) -> c_double {
        unsafe { Clp_objectiveValue(self.ptr) }
    }

    #[allow(dead_code)]
    fn write_mps(&mut self, filename: &str) {
        let c_filename = CString::new(filename).expect("CString::new failed");
        unsafe {
            Clp_writeMps(self.ptr, c_filename.as_ptr(), 0, 1, 0.0);
        }
    }
}

pub struct ClpSolver {
    builder: BuiltSolver<c_int>,
    clp_simplex: ClpSimplex,
}

impl ClpSolver {
    fn from_builder(builder: BuiltSolver<c_int>) -> Self {
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

    fn solve(&mut self) -> Result<Vec<c_double>, ClpSolveStatusError> {
        self.clp_simplex.dual_solve()?;

        let num_cols = self.builder.num_cols() as usize;

        Ok(self.clp_simplex.primal_column_solution(num_cols))
    }
}

impl Solver for ClpSolver {
    type Settings = ClpSolverSettings;

    fn name() -> &'static str {
        "clp"
    }

    fn features() -> &'static [SolverFeatures] {
        &[
            SolverFeatures::AggregatedNode,
            SolverFeatures::AggregatedNodeFactors,
            SolverFeatures::AggregatedNodeDynamicFactors,
            SolverFeatures::VirtualStorage,
        ]
    }

    fn setup(
        model: &Network,
        values: &ConstParameterValues,
        _settings: &Self::Settings,
    ) -> Result<Box<Self>, SolverSetupError> {
        let builder = SolverBuilder::new(f64::MAX, f64::MIN);
        let built = builder.create(model, values)?;

        let solver = ClpSolver::from_builder(built);
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
        self.clp_simplex
            .change_objective_coefficients(self.builder.col_obj_coef());
        timings.update_objective += now.elapsed();

        let now = Instant::now();
        self.clp_simplex.change_row_lower(self.builder.row_lower());
        self.clp_simplex.change_row_upper(self.builder.row_upper());

        for (row, column, coefficient) in self.builder.coefficients_to_update() {
            self.clp_simplex.modify_coefficient(*row, *column, *coefficient)
        }

        timings.update_constraints += now.elapsed();

        // self.write_mps(&format!("model_{}.mps", timestep.index));

        let now = Instant::now();

        let solution = self.solve()?;
        timings.solve = now.elapsed();

        // Create the updated network state from the results
        let network_state = state.get_mut_network_state();
        network_state.reset();

        let start_save_solution = Instant::now();
        for edge in model.edges().iter() {
            let col = self.builder.col_for_edge(&edge.index()) as usize;
            let flow = solution[col];
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
        lp.dual_solve().unwrap();

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
        lp.dual_solve().unwrap();

        assert!(approx_eq!(f64, lp.objective_value(), -40.0));
        assert_eq!(lp.primal_column_solution(3), vec![0.0, 0.0, 10.0]);
    }
}
