mod settings;

use crate::network::Network;
use crate::solvers::builder::{BuiltSolver, ColType, SolverBuilder};
use crate::solvers::{Solver, SolverFeatures, SolverSetupError, SolverSolveError, SolverTimings};
use crate::state::{ConstParameterValues, State};
use crate::timestep::Timestep;
use highs_sys::{
    Highs_addCols, Highs_addRows, Highs_changeCoeff, Highs_changeColIntegrality, Highs_changeColsCostByRange,
    Highs_changeObjectiveSense, Highs_changeRowsBoundsByMask, Highs_clearSolver, Highs_create,
    Highs_getDoubleInfoValue, Highs_getModelStatus, Highs_getSolution, Highs_run, Highs_setBoolOptionValue,
    Highs_setStringOptionValue, Highs_writeModel, HighsInt, OBJECTIVE_SENSE_MINIMIZE, STATUS_OK,
    kHighsModelStatusInfeasible, kHighsModelStatusInterrupt, kHighsModelStatusIterationLimit,
    kHighsModelStatusLoadError, kHighsModelStatusModelEmpty, kHighsModelStatusModelError, kHighsModelStatusNotset,
    kHighsModelStatusObjectiveBound, kHighsModelStatusObjectiveTarget, kHighsModelStatusOptimal,
    kHighsModelStatusPostsolveError, kHighsModelStatusPresolveError, kHighsModelStatusSolutionLimit,
    kHighsModelStatusSolveError, kHighsModelStatusTimeLimit, kHighsModelStatusUnbounded,
    kHighsModelStatusUnboundedOrInfeasible, kHighsModelStatusUnknown, kHighsStatusError, kHighsStatusOk,
    kHighsStatusWarning, kHighsVarTypeContinuous, kHighsVarTypeInteger,
};
use libc::c_void;
#[cfg(feature = "pyo3")]
pub use settings::build_highs_settings_py;
pub use settings::{HighsSolverSettings, HighsSolverSettingsBuilder};
use std::ffi::CString;
use std::ops::Deref;
use std::ptr::null;
use std::time::Instant;
use thiserror::Error;

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

#[derive(Error, Debug)]
#[error("Error in Highs function: {function}")]
pub struct HighsStatusError {
    function: String,
}

fn to_highs_result(ret: i32, function: &str) -> Result<(), HighsStatusError> {
    match ret {
        r if r == kHighsStatusOk => Ok(()),
        r if r == kHighsStatusWarning => {
            // Log a warning, but continue
            tracing::warn!("Highs warning in {function}: {ret}");
            Ok(())
        }
        r if r == kHighsStatusError => {
            // Log an error and return an error
            tracing::error!("Highs error in {function}: {ret}");
            Err(HighsStatusError {
                function: function.to_string(),
            })
        }
        _ => {
            // Log an unknown status and return an error
            tracing::error!("Highs unknown status in {function}: {ret}");
            panic!("Highs unknown status in {function}: {ret}");
        }
    }
}

/// Non-optimal model status (i.e. errors)
#[derive(Error, Debug)]
pub enum HighsModelError {
    #[error("Model status not set")]
    Notset,
    #[error("Model load error")]
    LoadError,
    #[error("Model error")]
    ModelError,
    #[error("Model presolve error")]
    PresolveError,
    #[error("Model solve error")]
    SolveError,
    #[error("Model postsolve error")]
    PostsolveError,
    #[error("Model is empty")]
    ModelEmpty,
    #[error("Model is infeasible")]
    Infeasible,
    #[error("Model is unbounded or infeasible")]
    UnboundedOrInfeasible,
    #[error("Model is unbounded")]
    Unbounded,
    #[error("Model objective bound reached")]
    ObjectiveBound,
    #[error("Model objective target reached")]
    ObjectiveTarget,
    #[error("Model time limit reached")]
    TimeLimit,
    #[error("Model iteration limit reached")]
    IterationLimit,
    #[error("Model status is unknown")]
    Unknown,
    #[error("Model solution limit reached")]
    SolutionLimit,
    #[error("Model interrupted")]
    Interrupt,
}

fn to_highs_model_result(status: i32) -> Result<(), HighsModelError> {
    match status {
        s if s == kHighsModelStatusNotset => Err(HighsModelError::Notset),
        s if s == kHighsModelStatusLoadError => Err(HighsModelError::LoadError),
        s if s == kHighsModelStatusModelError => Err(HighsModelError::ModelError),
        s if s == kHighsModelStatusPresolveError => Err(HighsModelError::PresolveError),
        s if s == kHighsModelStatusSolveError => Err(HighsModelError::SolveError),
        s if s == kHighsModelStatusPostsolveError => Err(HighsModelError::PostsolveError),
        s if s == kHighsModelStatusModelEmpty => Err(HighsModelError::ModelEmpty),
        s if s == kHighsModelStatusOptimal => Ok(()),
        s if s == kHighsModelStatusInfeasible => Err(HighsModelError::Infeasible),
        s if s == kHighsModelStatusUnboundedOrInfeasible => Err(HighsModelError::UnboundedOrInfeasible),
        s if s == kHighsModelStatusUnbounded => Err(HighsModelError::Unbounded),
        s if s == kHighsModelStatusObjectiveBound => Err(HighsModelError::ObjectiveBound),
        s if s == kHighsModelStatusObjectiveTarget => Err(HighsModelError::ObjectiveTarget),
        s if s == kHighsModelStatusTimeLimit => Err(HighsModelError::TimeLimit),
        s if s == kHighsModelStatusIterationLimit => Err(HighsModelError::IterationLimit),
        s if s == kHighsModelStatusUnknown => Err(HighsModelError::Unknown),
        s if s == kHighsModelStatusSolutionLimit => Err(HighsModelError::SolutionLimit),
        s if s == kHighsModelStatusInterrupt => Err(HighsModelError::Interrupt),
        _ => {
            // Log an unknown status and return an error
            tracing::error!("Highs unknown model status: {status}");
            panic!("Highs unknown model status in: {status}");
        }
    }
}

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
    ) -> Result<(), HighsStatusError> {
        // Add all the columns
        let ret = unsafe {
            Highs_addCols(
                self.ptr,
                ncols,
                col_obj_coef.as_ptr(),
                col_lower.as_ptr(),
                col_upper.as_ptr(),
                0,
                null(),
                null(),
                null(),
            )
        };
        to_highs_result(ret, "addCols")?;

        // Now change the column types
        for (i, &ctype) in col_type.iter().enumerate() {
            let ctype_int: HighsInt = match ctype {
                ColType::Continuous => kHighsVarTypeContinuous,
                ColType::Integer => kHighsVarTypeInteger,
            };

            let ret = unsafe { Highs_changeColIntegrality(self.ptr, i as HighsInt, ctype_int) };
            to_highs_result(ret, "changeColIntegrality")?;
        }

        Ok(())
    }

    pub fn add_rows(
        &mut self,
        row_lower: &[f64],
        row_upper: &[f64],
        nnz: HighsInt,
        row_starts: &[HighsInt],
        columns: &[HighsInt],
        elements: &[f64],
    ) -> Result<(), HighsStatusError> {
        let ret = unsafe {
            Highs_addRows(
                self.ptr,
                row_upper.len() as HighsInt,
                row_lower.as_ptr(),
                row_upper.as_ptr(),
                nnz,
                row_starts.as_ptr(),
                columns.as_ptr(),
                elements.as_ptr(),
            )
        };
        to_highs_result(ret, "addRows")
    }

    pub fn change_objective_coefficients(
        &mut self,
        obj_coefficients: &[f64],
        numcols: HighsInt,
    ) -> Result<(), HighsStatusError> {
        let ret = unsafe { Highs_changeColsCostByRange(self.ptr, 0, numcols - 1, obj_coefficients.as_ptr()) };
        to_highs_result(ret, "changeColsCostByRange")
    }

    pub fn change_row_bounds(
        &mut self,
        mask: &[HighsInt],
        lower: &[f64],
        upper: &[f64],
    ) -> Result<(), HighsStatusError> {
        let ret = unsafe { Highs_changeRowsBoundsByMask(self.ptr, mask.as_ptr(), lower.as_ptr(), upper.as_ptr()) };
        to_highs_result(ret, "changeRowsBoundsByMask")
    }

    pub fn change_coefficient(&mut self, row: HighsInt, col: HighsInt, value: f64) -> Result<(), HighsStatusError> {
        let ret = unsafe { Highs_changeCoeff(self.ptr, row, col, value) };
        to_highs_result(ret, "changeCoeff")
    }

    pub fn run(&mut self) -> Result<(), HighsModelError> {
        let ret = unsafe { Highs_run(self.ptr) };
        to_highs_result(ret, "run").map_err(|_| HighsModelError::Unknown)?;

        // Check the status of the solve
        let status = unsafe { Highs_getModelStatus(self.ptr) };
        to_highs_model_result(status)
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

    pub fn primal_column_solution(&mut self, numcol: usize, numrow: usize) -> Result<Vec<f64>, HighsStatusError> {
        let colvalue: &mut [f64] = &mut vec![0.; numcol];
        let coldual: &mut [f64] = &mut vec![0.; numcol];
        let rowvalue: &mut [f64] = &mut vec![0.; numrow];
        let rowdual: &mut [f64] = &mut vec![0.; numrow];

        let ret = unsafe {
            // Get the primal and dual solution
            Highs_getSolution(
                self.ptr,
                colvalue.as_mut_ptr(),
                coldual.as_mut_ptr(),
                rowvalue.as_mut_ptr(),
                rowdual.as_mut_ptr(),
            )
        };
        to_highs_result(ret, "getSolution")?;
        Ok(colvalue.to_vec())
    }

    fn write_model(&mut self, filename: &str) -> Result<(), HighsStatusError> {
        let c_filename = CString::new(filename).unwrap();
        let ret = unsafe { Highs_writeModel(self.ptr, c_filename.as_ptr()) };
        to_highs_result(ret, "writeModel")
    }

    fn clear_solver(&mut self) -> Result<(), HighsStatusError> {
        let ret = unsafe { Highs_clearSolver(self.ptr) };
        to_highs_result(ret, "clearSolver")
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
            SolverFeatures::AggregatedNodeDynamicFactors,
            SolverFeatures::VirtualStorage,
        ]
    }

    fn setup(
        network: &Network,
        values: &ConstParameterValues,
        _settings: &Self::Settings,
    ) -> Result<Box<Self>, SolverSetupError> {
        let builder: SolverBuilder<HighsInt> = SolverBuilder::new(f64::MAX, f64::MIN);
        let built = builder.create(network, values)?;

        let num_cols = built.num_cols();
        let num_nz = built.num_non_zero();

        let mut highs_lp = Highs::default();
        highs_lp.presolve("on");

        highs_lp.add_cols(
            built.col_lower(),
            built.col_upper(),
            built.col_obj_coef(),
            built.col_type(),
            num_cols,
        )?;

        highs_lp.add_rows(
            built.row_lower(),
            built.row_upper(),
            num_nz,
            built.row_starts(),
            built.columns(),
            built.elements(),
        )?;

        Ok(Box::new(Self {
            builder: built,
            highs: highs_lp,
        }))
    }
    fn solve(
        &mut self,
        network: &Network,
        timestep: &Timestep,
        state: &mut State,
    ) -> Result<SolverTimings, SolverSolveError> {
        let mut timings = SolverTimings::default();
        self.builder.update(network, timestep, state, &mut timings)?;

        let num_cols = self.builder.num_cols();
        let num_rows = self.builder.num_rows();

        let now = Instant::now();
        self.highs
            .change_objective_coefficients(self.builder.col_obj_coef(), num_cols)?;
        timings.update_objective += now.elapsed();

        let now = Instant::now();

        self.highs.change_row_bounds(
            self.builder.row_mask(),
            self.builder.row_lower(),
            self.builder.row_upper(),
        )?;

        for (row, column, coefficient) in self.builder.coefficients_to_update() {
            // Highs only accepts coefficients in the range -1e10 to 1e10
            self.highs
                .change_coefficient(*row, *column, coefficient.clamp(-1e10, 1e10))?;
        }

        timings.update_constraints += now.elapsed();

        let now = Instant::now();

        if let Err(e) = self.highs.run() {
            let result = if let HighsModelError::Unknown = e {
                // An unknown error occurred, try a resolve after clearing solve information
                // See https://github.com/ERGO-Code/HiGHS/issues/1607
                self.highs.clear_solver()?;
                self.highs.run()
            } else {
                Err(e)
            };

            if let Err(e) = result {
                self.highs.write_model("pywr_lp.mps")?;

                return Err(SolverSolveError::HighsModelError(e));
            }
        }
        let solution = self
            .highs
            .primal_column_solution(num_cols as usize, num_rows as usize)?;
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
        state.complete(network, timestep)?;
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

        lp.add_cols(&col_lower, &col_upper, &col_obj_coef, &col_type, 2)
            .unwrap();

        let row_lower: Vec<f64> = vec![0.0];
        let row_upper: Vec<f64> = vec![2.0];
        let row_starts: Vec<HighsInt> = vec![0, 2];
        let columns: Vec<HighsInt> = vec![0, 1];
        let elements: Vec<f64> = vec![1.0, 1.0];

        lp.add_rows(&row_lower, &row_upper, 2, &row_starts, &columns, &elements)
            .unwrap();
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

        lp.add_cols(&col_lower, &col_upper, &col_obj_coef, &col_type, ncols)
            .unwrap();

        lp.add_rows(&row_lower, &row_upper, nnz, &row_starts, &columns, &elements)
            .unwrap();
        lp.run().unwrap();

        assert!(approx_eq!(f64, lp.objective_value(), -20.0));
        assert_eq!(
            lp.primal_column_solution(ncols as usize, nrows as usize).unwrap(),
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

        lp.add_cols(&col_lower, &col_upper, &col_obj_coef, &col_type, ncols)
            .unwrap();

        lp.add_rows(&row_lower, &row_upper, nnz, &row_starts, &columns, &elements)
            .unwrap();
        lp.run().unwrap();

        assert!(approx_eq!(f64, lp.objective_value(), -40.0));
        assert_eq!(
            lp.primal_column_solution(ncols as usize, nrows as usize).unwrap(),
            vec![0.0, 0.0, 10.0]
        );
    }
}
