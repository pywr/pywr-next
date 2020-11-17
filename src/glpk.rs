use libc::{c_double, c_int, size_t};
use std::time::Instant;
use thiserror::Error;

#[repr(C)]
struct GlpProbPtr {
    _private: [u8; 0],
}
#[repr(C)]
struct GlpSmcp {
    msg_lev: c_int,    /* message level: */
    meth: c_int,       /* simplex method option: */
    pricing: c_int,    /* pricing technique: */
    r_test: c_int,     /* ratio test technique: */
    tol_bnd: c_double, /* primal feasibility tolerance */
    tol_dj: c_double,  /* dual feasibility tolerance */
    tol_piv: c_double, /* pivot tolerance */
    obj_ll: c_double,  /* lower objective limit */
    obj_ul: c_double,  /* upper objective limit */
    it_lim: c_int,     /* simplex iteration limit */
    tm_lim: c_int,     /* time limit, ms */
    out_frq: c_int,    /* display output frequency, ms */
    out_dly: c_int,    /* display output delay, ms */
    presolve: c_int,   /* enable/disable using LP presolver */
    excl: c_int,       /* exclude fixed non-basic variables */
    shift: c_int,      /* shift bounds of variables to zero */
    aorn: c_int,       /* option to use A or N: */
    foo_bar: [c_double; 33], /* (reserved) */
                       //
                       // #define GLP_PRIMAL         1  /* use primal simplex */
                       // #define GLP_DUALP          2  /* use dual; if it fails, use primal */
                       // #define GLP_DUAL           3  /* use dual simplex */
                       //
                       // #define GLP_PT_STD      0x11  /* standard (Dantzig's rule) */
                       // #define GLP_PT_PSE      0x22  /* projected steepest edge */
                       //
                       // #define GLP_RT_STD      0x11  /* standard (textbook) */
                       // #define GLP_RT_HAR      0x22  /* Harris' two-pass ratio test */
                       // #if 1 /* 16/III-2016 */
                       // #define GLP_RT_FLIP     0x33  /* long-step (flip-flop) ratio test */
                       // #endif
                       //
                       // #if 1 /* 11/VII-2017 (not documented yet) */
                       //
                       // #define GLP_USE_AT         1  /* use A matrix in row-wise format */
                       // #define GLP_USE_NT         2  /* use N matrix in row-wise format */
}

impl GlpSmcp {
    fn new() -> Self {
        let mut smcp = Self {
            msg_lev: 0,
            meth: 0,
            pricing: 0,
            r_test: 0,
            tol_bnd: 0.0,
            tol_dj: 0.0,
            tol_piv: 0.0,
            obj_ll: 0.0,
            obj_ul: 0.0,
            it_lim: 0,
            tm_lim: 0,
            out_frq: 0,
            out_dly: 0,
            presolve: 0,
            excl: 0,
            shift: 0,
            aorn: 0,
            foo_bar: [0.0; 33],
        };

        unsafe {
            glp_init_smcp(&mut smcp);
        }
        // TODO add methods to set these settings; currently just hard turn off messages
        smcp.msg_lev = GLP_MSG_OFF;
        smcp
    }
}

#[link(name = "glpk")]
extern "C" {
    fn glp_create_prob() -> *mut GlpProbPtr;
    fn glp_init_smcp(parm: *mut GlpSmcp);
    fn glp_add_rows(p: *mut GlpProbPtr, nrs: c_int) -> c_int;
    fn glp_add_cols(p: *mut GlpProbPtr, ncs: c_int) -> c_int;
    fn glp_set_mat_row(p: *mut GlpProbPtr, i: c_int, len: c_int, ind: *const i32, val: *const f64);
    fn glp_set_mat_col(p: *mut GlpProbPtr, j: c_int, len: c_int, ind: *const i32, val: *const f64);
    fn glp_set_row_bnds(p: *mut GlpProbPtr, i: c_int, _type: c_int, lb: f64, ub: f64);
    fn glp_set_col_bnds(p: *mut GlpProbPtr, j: c_int, _type: c_int, lb: f64, ub: f64);
    fn glp_set_obj_coef(p: *mut GlpProbPtr, j: c_int, coef: f64);
    fn glp_set_obj_dir(p: *mut GlpProbPtr, dir: c_int);
    fn glp_get_obj_val(p: *mut GlpProbPtr) -> f64;
    fn glp_simplex(p: *mut GlpProbPtr, parm: *const GlpSmcp) -> c_int;
    fn glp_get_row_prim(p: *mut GlpProbPtr, i: c_int) -> f64;
    fn glp_get_col_prim(p: *mut GlpProbPtr, j: c_int) -> f64;
    fn glp_get_status(p: *mut GlpProbPtr) -> c_int;
    fn glp_erase_prob(p: *mut GlpProbPtr);
    fn glp_delete_prob(p: *mut GlpProbPtr);
}

// GLPK Macro constants
// optimization direction flag:
const GLP_MIN: c_int = 1; // minimization
const GLP_MAX: c_int = 2; // maximization

// type of auxiliary/structural variable:
const GLP_FR: c_int = 1; // free (unbounded) variable
const GLP_LO: c_int = 2; // variable with lower bound
const GLP_UP: c_int = 3; // variable with upper bound
const GLP_DB: c_int = 4; // double-bounded variable
const GLP_FX: c_int = 5; // fixed variable

/* solution status: */
const GLP_UNDEF: c_int = 1; /* solution is undefined */
const GLP_FEAS: c_int = 2; /* solution is feasible */
const GLP_INFEAS: c_int = 3; /* solution is infeasible */
const GLP_NOFEAS: c_int = 4; /* no feasible solution exists */
const GLP_OPT: c_int = 5; /* solution is optimal */
const GLP_UNBND: c_int = 6; /* solution is unbounded */

/* return codes: */
const GLP_EBADB: c_int = 0x01; /* invalid basis */
const GLP_ESING: c_int = 0x02; /* singular matrix */
const GLP_ECOND: c_int = 0x03; /* ill-conditioned matrix */
const GLP_EBOUND: c_int = 0x04; /* invalid bounds */
const GLP_EFAIL: c_int = 0x05; /* solver failed */
const GLP_EOBJLL: c_int = 0x06; /* objective lower limit reached */
const GLP_EOBJUL: c_int = 0x07; /* objective upper limit reached */
const GLP_EITLIM: c_int = 0x08; /* iteration limit exceeded */
const GLP_ETMLIM: c_int = 0x09; /* time limit exceeded */
const GLP_ENOPFS: c_int = 0x0A; /* no primal feasible solution */
const GLP_ENODFS: c_int = 0x0B; /* no dual feasible solution */

/* message level: */
const GLP_MSG_OFF: c_int = 0; /* no output */
const GLP_MSG_ERR: c_int = 1; /* warning and error messages only */
const GLP_MSG_ON: c_int = 2; /* normal output */
const GLP_MSG_ALL: c_int = 3; /* full output */
const GLP_MSG_DBG: c_int = 4; /* debug output */

pub struct GlpProb {
    ptr: *mut GlpProbPtr, // Reference to the C struct
    smcp: GlpSmcp,
}

pub enum Direction {
    Min,
    Max,
}

#[derive(Debug)]
pub enum Bounds {
    Free,
    Lower(f64),
    Upper(f64),
    Double(f64, f64),
    Fixed(f64),
}

#[derive(Debug, PartialEq)]
pub enum SolutionStatus {
    Undefined,
    Feasible,
    Infeasible,
    NoFeasibleSolutionExists,
    Optimal,
    Unbounded,
    Unknown(i32),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SimplexStatus {
    Solved,
    InvalidBasis,
    SingularMatrix,
    IllConditionedMatrix,
    InvalidBounds,
    SolverFailed,
    ObjectiveLowerLimitReached,
    ObjectiveUpperLimitReached,
    IterationLimitExceeded,
    TimeLimitExceeded,
    NoPrimalFeasibleSolution,
    NoDualFeasibleSolution,
    Unknown(i32),
}

// Errors

#[derive(Error, Debug, PartialEq)]
pub enum GlpkError {
    #[error("an unknown error occurred in GLPK.")]
    UnknownError,
    #[error("an error occurred in the simplex method.")]
    SimplexError { status: SimplexStatus },
    #[error("row `{0}` is invalid.")]
    InvalidRowNumber(i32),
    #[error("unequal matrix vectors; indices and values must be equal length")]
    UnequalLengthMatrixVectors,
}

// Delete the problem data on destruction!
impl Drop for GlpProb {
    fn drop(&mut self) {
        unsafe { glp_delete_prob(self.ptr) }
    }
}

impl GlpProb {
    /// Initialise a new GLPK Problem object
    pub fn create(direction: Direction) -> Result<GlpProb, GlpkError> {
        let prob: GlpProb;

        unsafe {
            let ptr = glp_create_prob();
            let smcp = GlpSmcp::new();
            // smcp.msg_lev = GLP_MSG_OFF;
            prob = GlpProb { ptr: ptr, smcp }
        }
        // Initialise direction
        prob.set_obj_dir(direction)?;

        Ok(prob)
    }

    // /// Erase the problem content
    // pub fn erase(&mut self) -> Result<(), String> {
    //     // TODO handle an erased problem in Rust
    //     unsafe { glp_erase_prob(self.ptr) };
    //     Ok(())
    // }

    /// Set the objective direction
    pub fn set_obj_dir(&self, direction: Direction) -> Result<(), GlpkError> {
        match direction {
            Direction::Min => unsafe {
                glp_set_obj_dir(self.ptr, GLP_MIN);
            },
            Direction::Max => unsafe {
                glp_set_obj_dir(self.ptr, GLP_MAX);
            },
        };

        Ok(())
    }

    /// Set the objective coefficient for a column
    pub fn set_obj_coefficient(&self, col: usize, coef: f64) -> Result<(), GlpkError> {
        unsafe { glp_set_obj_coef(self.ptr, col as i32 + 1, coef) };
        Ok(())
    }

    /// Get the objective value for the problem
    pub fn get_objective_value(&self) -> f64 {
        let value = unsafe { glp_get_obj_val(self.ptr) };
        value
    }

    /// Add a number of rows to the problem
    pub fn add_rows(&self, nrows: usize) -> Result<usize, GlpkError> {
        let row = unsafe { glp_add_rows(self.ptr, nrows as i32) };
        Ok(row as usize - 1)
    }

    /// Add a number of columns to the problem
    pub fn add_columns(&self, ncols: usize) -> Result<usize, GlpkError> {
        let col = unsafe { glp_add_cols(self.ptr, ncols as i32) };
        Ok(col as usize - 1)
    }

    /// Set the bounds on a column
    pub fn set_col_bounds(&self, col: usize, bounds: Bounds) -> Result<(), GlpkError> {
        match bounds {
            Bounds::Free => unsafe {
                glp_set_col_bnds(self.ptr, col as i32 + 1, GLP_FR, 0.0, 0.0);
            },
            Bounds::Lower(lb) => unsafe {
                glp_set_col_bnds(self.ptr, col as i32 + 1, GLP_LO, lb, 0.0);
            },
            Bounds::Upper(ub) => unsafe {
                glp_set_col_bnds(self.ptr, col as i32 + 1, GLP_UP, 0.0, ub);
            },
            Bounds::Double(lb, ub) => unsafe {
                glp_set_col_bnds(self.ptr, col as i32 + 1, GLP_DB, lb, ub);
            },
            Bounds::Fixed(b) => unsafe {
                glp_set_col_bnds(self.ptr, col as i32 + 1, GLP_FX, b, b);
            },
        };

        Ok(())
    }

    /// Set the bounds on a row
    pub fn set_row_bounds(&self, row: usize, bounds: Bounds) -> Result<(), GlpkError> {
        match bounds {
            Bounds::Free => unsafe {
                glp_set_row_bnds(self.ptr, row as i32 + 1, GLP_FR, 0.0, 0.0);
            },
            Bounds::Lower(lb) => unsafe {
                glp_set_row_bnds(self.ptr, row as i32 + 1, GLP_LO, lb, 0.0);
            },
            Bounds::Upper(ub) => unsafe {
                glp_set_row_bnds(self.ptr, row as i32 + 1, GLP_UP, 0.0, ub);
            },
            Bounds::Double(lb, ub) => unsafe {
                glp_set_row_bnds(self.ptr, row as i32 + 1, GLP_DB, lb, ub);
            },
            Bounds::Fixed(b) => unsafe {
                glp_set_row_bnds(self.ptr, row as i32 + 1, GLP_FX, b, b);
            },
        };

        Ok(())
    }

    /// Set the values for a row in the matrix
    pub fn set_matrix_row(&self, row: usize, indices: &Vec<usize>, values: &Vec<f64>) -> Result<(), GlpkError> {
        if indices.len() != values.len() {
            return Err(GlpkError::UnequalLengthMatrixVectors);
        }
        let mut padded_indices: Vec<i32> = vec![0];
        padded_indices.extend(indices.iter().map(|&i| i as i32 + 1));
        let mut padded_values: Vec<f64> = vec![0.0];
        padded_values.extend(values);
        // Set the value in
        unsafe {
            glp_set_mat_row(
                self.ptr,
                row as i32 + 1,
                indices.len() as i32,
                padded_indices.as_ptr(),
                padded_values.as_ptr(),
            );
        }
        Ok(())
    }

    pub fn simplex(&self) -> Result<SimplexStatus, GlpkError> {
        let now = Instant::now();
        let result = unsafe { glp_simplex(self.ptr, &self.smcp) };

        // TODO handle the error conditions more explicitly.
        match result {
            0 => Ok(SimplexStatus::Solved),
            GLP_EBADB => Err(GlpkError::SimplexError {
                status: SimplexStatus::InvalidBasis,
            }), /* invalid basis */
            GLP_ESING => Err(GlpkError::SimplexError {
                status: SimplexStatus::SingularMatrix,
            }), /* singular matrix */
            GLP_ECOND => Err(GlpkError::SimplexError {
                status: SimplexStatus::IllConditionedMatrix,
            }), /* ill-conditioned matrix */
            GLP_EBOUND => Err(GlpkError::SimplexError {
                status: SimplexStatus::InvalidBounds,
            }), /* invalid bounds */
            GLP_EFAIL => Err(GlpkError::SimplexError {
                status: SimplexStatus::SolverFailed,
            }), /* solver failed */
            GLP_EOBJLL => Err(GlpkError::SimplexError {
                status: SimplexStatus::ObjectiveLowerLimitReached,
            }), /* objective lower limit reached */
            GLP_EOBJUL => Err(GlpkError::SimplexError {
                status: SimplexStatus::ObjectiveUpperLimitReached,
            }), /* objective upper limit reached */
            GLP_EITLIM => Err(GlpkError::SimplexError {
                status: SimplexStatus::IterationLimitExceeded,
            }), /* iteration limit exceeded */
            GLP_ETMLIM => Err(GlpkError::SimplexError {
                status: SimplexStatus::TimeLimitExceeded,
            }), /* time limit exceeded */
            GLP_ENOPFS => Err(GlpkError::SimplexError {
                status: SimplexStatus::NoPrimalFeasibleSolution,
            }), /* no primal feasible solution */
            GLP_ENODFS => Err(GlpkError::SimplexError {
                status: SimplexStatus::NoDualFeasibleSolution,
            }), /* no dual feasible solution */
            val => Err(GlpkError::SimplexError {
                status: SimplexStatus::Unknown(val),
            }),
        }
    }

    pub fn get_solution_status(&self) -> SolutionStatus {
        match unsafe { glp_get_status(self.ptr) } {
            GLP_UNDEF => SolutionStatus::Undefined,   /* solution is undefined */
            GLP_FEAS => SolutionStatus::Feasible,     /* solution is feasible */
            GLP_INFEAS => SolutionStatus::Infeasible, /* solution is infeasible */
            GLP_NOFEAS => SolutionStatus::NoFeasibleSolutionExists, /* no feasible solution exists */
            GLP_OPT => SolutionStatus::Optimal,       /* solution is optimal */
            GLP_UNBND => SolutionStatus::Unbounded,   /* solution is unbounded */
            val => SolutionStatus::Unknown(val),
        }
    }

    // pub fn get_row_primal(&self, row: usize) -> Result<f64, GlpkError> {
    //     let value = unsafe { glp_get_row_prim(self.ptr, row as i32 + 1) };
    //     Ok(value)
    // }

    pub fn get_col_primal(&self, col: usize) -> Result<f64, GlpkError> {
        let value = unsafe { glp_get_col_prim(self.ptr, col as i32 + 1) };
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glpk::GlpkError::SimplexError;

    #[test]
    /// Test solving a simple linear programme
    fn test_solve_lp() {
        let mut problem = GlpProb::create(Direction::Min).unwrap();

        let _ = problem.add_columns(3).unwrap();
        problem.set_col_bounds(0, Bounds::Double(0.0, 2.0)).unwrap();
        problem.set_col_bounds(1, Bounds::Lower(0.0)).unwrap();
        problem.set_col_bounds(2, Bounds::Double(0.0, 4.0)).unwrap();
        problem.set_obj_coefficient(0, 1.0);
        problem.set_obj_coefficient(1, 0.0);
        problem.set_obj_coefficient(2, 4.0);

        let _ = problem.add_rows(2).unwrap();
        // Row1
        problem.set_matrix_row(0, &vec![0, 2], &vec![1.0, 1.0]).unwrap();
        problem.set_row_bounds(0, Bounds::Lower(2.0)).unwrap();
        // Row2
        problem.set_matrix_row(1, &vec![0, 1, 2], &vec![1.0, -5.0, 1.0]).unwrap();
        problem.set_row_bounds(1, Bounds::Fixed(1.0)).unwrap();

        let simplex_status = problem.simplex().unwrap();
        let solution_status = problem.get_solution_status();
        assert_eq!(simplex_status, SimplexStatus::Solved);
        assert_eq!(solution_status, SolutionStatus::Optimal);
        assert_eq!(problem.get_objective_value(), 2.0);
    }
}
