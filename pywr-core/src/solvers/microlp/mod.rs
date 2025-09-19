mod settings;
use super::builder::{ColType, SolverBuilder};
use crate::network::Network;
use crate::solvers::builder::BuiltSolver;
use crate::solvers::{Solver, SolverFeatures, SolverSetupError, SolverSolveError, SolverTimings};
use crate::state::{ConstParameterValues, State};
use crate::timestep::Timestep;
use microlp::{ComparisonOp, OptimizationDirection, Problem};
pub use settings::{MicroLpSolverSettings, MicroLpSolverSettingsBuilder};
use std::time::Instant;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MicroLpError {
    #[error("MicroLp solver error: {0}")]
    SolveError(#[from] microlp::Error),
}

pub struct MicroLpSolver {
    builder: BuiltSolver<usize>,
}

impl MicroLpSolver {
    pub fn make_problem(&self) -> Problem {
        let mut problem = Problem::new(OptimizationDirection::Minimize);

        let variables: Vec<_> = self
            .builder
            .col_lower()
            .iter()
            .zip(self.builder.col_upper())
            .zip(self.builder.col_obj_coef())
            .zip(self.builder.col_type())
            .map(|(((lb, ub), obj_coeff), c_ty)| match c_ty {
                ColType::Continuous => problem.add_var(*obj_coeff, (*lb, *ub)),
                ColType::Integer => {
                    let ub = *ub as i32;
                    let lb = *lb as i32;

                    if lb == 0 && ub == 1 {
                        // Binary variable
                        problem.add_binary_var(*obj_coeff)
                    } else {
                        problem.add_integer_var(*obj_coeff, (lb, ub))
                    }
                }
            })
            .collect();

        for row in 0..self.builder.num_rows() {
            let row_lower = self.builder.row_lower()[row];
            let row_upper = self.builder.row_upper()[row];
            let row_start = self.builder.row_starts()[row];
            let row_end = self.builder.row_starts()[row + 1];
            let row_cols = &self.builder.columns()[row_start..row_end];
            let row_values = &self.builder.elements()[row_start..row_end];

            let expr = row_cols
                .iter()
                .zip(row_values.iter())
                .map(|(&col, &val)| (variables[col], val));

            if row_lower == row_upper {
                // If the bounds are equal we can just add a single equality constraint
                problem.add_constraint(expr.clone(), ComparisonOp::Eq, row_lower);
            } else {
                assert!(row_lower < row_upper, "Row lower bound must be less than upper bound");
                // Otherwise add two rows for each constraint (>= and <=)
                problem.add_constraint(expr.clone(), ComparisonOp::Ge, row_lower);

                if row_upper.is_finite() {
                    problem.add_constraint(expr, ComparisonOp::Le, row_upper);
                }
            }
        }

        problem
    }
}

impl Solver for MicroLpSolver {
    type Settings = MicroLpSolverSettings;

    fn name() -> &'static str {
        "microlp"
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
        model: &Network,
        values: &ConstParameterValues,
        _settings: &Self::Settings,
    ) -> Result<Box<Self>, SolverSetupError> {
        let builder = SolverBuilder::new(f64::INFINITY, f64::NEG_INFINITY);
        let built = builder.create(model, values)?;

        let solver = MicroLpSolver { builder: built };
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

        self.builder.apply_updated_coefficients();

        let now = Instant::now();
        let problem = self.make_problem();
        timings.update_constraints += now.elapsed();

        let now = Instant::now();
        let solution = problem.solve().map_err(MicroLpError::SolveError)?;

        let solution = solution.iter().map(|(_, x)| *x).collect::<Vec<f64>>();

        timings.solve = now.elapsed();

        // Create the updated network state from the results
        let network_state = state.get_mut_network_state();
        network_state.reset();

        let start_save_solution = Instant::now();
        for edge in model.edges().iter() {
            let col = self.builder.col_for_edge(&edge.index());
            let flow = solution[col];
            network_state.add_flow(edge, timestep, flow)?;
        }
        state.complete(model, timestep)?;
        timings.save_solution += start_save_solution.elapsed();

        Ok(timings)
    }
}
