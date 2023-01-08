use crate::model::Model;
use crate::node::NodeType;
use crate::solvers::{MultiStateSolver, SolverTimings};
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use clipm::PathFollowingDirectClSolver;
use std::collections::BTreeMap;
use std::ops::Deref;
use std::time::Instant;

const B_MAX: f64 = 999999.0;

#[derive(Debug)]
struct Matrix {
    row_starts: Vec<usize>,
    columns: Vec<usize>,
    elements: Vec<f64>,
}

impl Default for Matrix {
    fn default() -> Self {
        Self {
            row_starts: vec![0usize],
            columns: Vec::new(),
            elements: Vec::new(),
        }
    }
}

impl Matrix {
    fn add_row(&mut self, row: RowBuilder) {
        let prev_row_start = *self.row_starts.get(&self.row_starts.len() - 1).unwrap();
        self.row_starts.push(prev_row_start + row.columns.len());
        for (column, value) in row.columns {
            self.columns.push(column);
            self.elements.push(value);
        }
    }

    pub fn nrows(&self) -> usize {
        self.row_starts.len() - 1
    }
}

struct LpBuilder {
    inequality: Matrix,
    equality: Matrix,
    num_lps: usize,
    num_cols: usize,
    row_upper: Vec<f64>,
    col_obj_coef: Vec<f64>,
}

impl LpBuilder {
    fn new(num_lps: usize, num_cols: usize) -> Self {
        Self {
            inequality: Matrix::default(),
            equality: Matrix::default(),
            num_lps,
            num_cols,
            row_upper: Vec::new(),
            // Pre-allocate array for the objective coefficients
            col_obj_coef: vec![0.0; num_lps * num_cols],
        }
    }

    pub fn add_row(&mut self, row: RowBuilder) {
        match &row.upper {
            Bounds::Upper => {
                // Current last entry of the inequality bounds
                let idx = self.inequality.nrows() * self.num_lps;
                // Add the row to the matrix
                self.inequality.add_row(row);
                // Extend the inequality bounds before the equality bounds
                let values = vec![B_MAX; self.num_lps];
                self.row_upper.splice(idx..idx, values.into_iter());
            }
            Bounds::Fixed => {
                self.equality.add_row(row);
                // Equality constraints default to zero bounds
                self.row_upper.extend(vec![0.0; self.num_lps]);
            }
        }
    }

    pub fn set_obj_coefficient(&mut self, col: usize, obj_coef: &[f64]) {
        let i = col * self.num_lps;
        let j = (col + 1) * self.num_lps;
        self.col_obj_coef[i..j].copy_from_slice(obj_coef);
    }

    pub fn set_row_bounds(&mut self, row: usize, ub: &[f64]) {
        let i = row * self.num_lps;
        let j = (row + 1) * self.num_lps;

        self.row_upper[i..j].copy_from_slice(ub);
    }

    fn get_full_matrix(&self) -> Matrix {
        // Start with the inequality matrix
        // Remove last entry from the row starts, this will be the offset added to the second matrix
        let (last, row_starts) = self.inequality.row_starts.split_last().unwrap();
        let mut row_starts = row_starts.to_vec();

        let mut columns = self.inequality.columns.clone();
        let mut elements = self.inequality.elements.clone();

        // Append the equality matrix
        row_starts.extend(self.equality.row_starts.iter().map(|i| last + i));
        columns.extend(&self.equality.columns);
        elements.extend(&self.equality.elements);

        Matrix {
            row_starts,
            columns,
            elements,
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum Bounds {
    Upper,
    Fixed,
}

#[derive(Clone)]
struct RowBuilder {
    upper: Bounds,
    columns: BTreeMap<usize, f64>,
}

impl RowBuilder {
    fn fixed() -> Self {
        Self {
            upper: Bounds::Fixed,
            columns: BTreeMap::new(),
        }
    }

    fn upper() -> Self {
        Self {
            upper: Bounds::Upper,
            columns: BTreeMap::new(),
        }
    }

    fn clone_negative(&self) -> Self {
        Self {
            upper: self.upper,
            columns: self.columns.iter().map(|(k, v)| (*k, -v)).collect(),
        }
    }

    /// Add an element to the row
    ///
    /// If the column already exists `value` will be added to the existing coefficient.
    fn add_element(&mut self, column: usize, value: f64) {
        *self.columns.entry(column).or_insert(0.0) += value;
    }
}

struct SolverBuilder {
    builder: LpBuilder,
    start_node_constraints: Option<usize>,
    start_agg_node_constraints: Option<usize>,
    start_agg_node_factor_constraints: Option<usize>,
    start_virtual_storage_constraints: Option<usize>,
}

impl SolverBuilder {
    fn new(num_lps: usize, num_cols: usize) -> Self {
        Self {
            builder: LpBuilder::new(num_lps, num_cols),
            start_node_constraints: None,
            start_agg_node_constraints: None,
            start_agg_node_factor_constraints: None,
            start_virtual_storage_constraints: None,
        }
    }

    fn create(model: &Model, num_scenarios: usize) -> Result<Self, PywrError> {
        let mut builder = Self::new(num_scenarios, model.edges.len());
        // Create edge mass balance constraints
        builder.create_mass_balance_constraints(model);
        // Create the nodal constraints
        builder.create_node_constraints(model);
        // // Create the aggregated node constraints
        // builder.create_aggregated_node_constraints(model);
        // // Create the aggregated node factor constraints
        // builder.create_aggregated_node_factor_constraints(model);
        // // Create virtual storage constraints
        // builder.create_virtual_storage_constraints(model);

        Ok(builder)
    }

    /// Create mass balance constraints for each edge
    fn create_mass_balance_constraints(&mut self, model: &Model) {
        for node in model.nodes.deref() {
            // Only link nodes create mass-balance constraints

            if let NodeType::Link = node.node_type() {
                let mut row = RowBuilder::fixed();
                let incoming_edges = node.get_incoming_edges().unwrap();
                let outgoing_edges = node.get_outgoing_edges().unwrap();

                // TODO check for length >= 1

                for edge in incoming_edges {
                    row.add_element(*edge.deref(), 1.0);
                }
                for edge in outgoing_edges {
                    row.add_element(*edge.deref(), -1.0);
                }

                self.builder.add_row(row);
            }
        }
    }

    /// Create node constraints
    ///
    /// One constraint is created per node to enforce any constraints (flow or storage)
    /// that it may define.
    fn create_node_constraints(&mut self, model: &Model) {
        let start_row = self.builder.inequality.nrows();

        for node in model.nodes.deref() {
            // Create empty arrays to store the matrix data
            let mut row = RowBuilder::upper();
            let mut add_negative_copy = false;

            match node.node_type() {
                NodeType::Link => {
                    for edge in node.get_outgoing_edges().unwrap() {
                        row.add_element(*edge.deref(), 1.0);
                    }
                }
                NodeType::Input => {
                    for edge in node.get_outgoing_edges().unwrap() {
                        row.add_element(*edge.deref(), 1.0);
                    }
                }
                NodeType::Output => {
                    for edge in node.get_incoming_edges().unwrap() {
                        row.add_element(*edge.deref(), 1.0);
                    }
                }
                NodeType::Storage => {
                    // Make two rows for Storage nodes
                    add_negative_copy = true;
                    for edge in node.get_incoming_edges().unwrap() {
                        row.add_element(*edge.deref(), 1.0);
                    }
                    for edge in node.get_outgoing_edges().unwrap() {
                        row.add_element(*edge.deref(), -1.0);
                    }
                }
            }

            self.builder.add_row(row.clone());
            if add_negative_copy {
                let neg_row = row.clone_negative();
                self.builder.add_row(neg_row);
            }
        }
        self.start_node_constraints = Some(start_row);
    }

    fn update(
        &mut self,
        model: &Model,
        timestep: &Timestep,
        states: &[State],
        timings: &mut SolverTimings,
    ) -> Result<(), PywrError> {
        let start_objective_update = Instant::now();
        self.update_edge_objectives(model, states)?;
        timings.update_objective += start_objective_update.elapsed();

        let start_constraint_update = Instant::now();
        self.update_node_constraint_bounds(model, timestep, states)?;
        // self.update_aggregated_node_constraint_bounds(model, state)?;
        timings.update_constraints += start_constraint_update.elapsed();

        Ok(())
    }

    /// Update edge objective coefficients
    fn update_edge_objectives(&mut self, model: &Model, states: &[State]) -> Result<(), PywrError> {
        for edge in model.edges.deref() {
            // Collect all of the costs for all states together
            let cost = states
                .iter()
                .map(|s| edge.cost(&model.nodes, s).map(|c| if c != 0.0 { -c } else { 0.0 }))
                .collect::<Result<Vec<f64>, _>>()?;

            self.builder.set_obj_coefficient(*edge.index().deref(), &cost);
        }
        Ok(())
    }

    /// Update node constraints
    fn update_node_constraint_bounds(
        &mut self,
        model: &Model,
        timestep: &Timestep,
        states: &[State],
    ) -> Result<(), PywrError> {
        let mut row = match self.start_node_constraints {
            Some(r) => r,
            None => return Err(PywrError::SolverNotSetup),
        };

        let dt = timestep.days();

        for node in model.nodes.deref() {
            match node.node_type() {
                NodeType::Input | NodeType::Output | NodeType::Link => {
                    // Flow nodes will only respect the upper bounds
                    let ub: Vec<f64> = states
                        .iter()
                        .map(|state| {
                            // TODO check for non-zero lower bounds and error?
                            node.get_current_flow_bounds(state)
                                .expect("Flow bounds expected for Input, Output and Link nodes.")
                                .1
                                .min(B_MAX)
                        })
                        .collect();
                    // Apply the bounds to LP
                    self.builder.set_row_bounds(row, ub.as_slice());
                    row += 1;
                }
                NodeType::Storage => {
                    // Storage nodes instead have two constraints for availale and missing volume.
                    let (avail, missing): (Vec<_>, Vec<_>) = states
                        .iter()
                        .map(|state| {
                            let (avail, missing) = node
                                .get_current_available_volume_bounds(state)
                                .expect("Volumes bounds expected for Storage nodes.");
                            (avail / dt, missing / dt)
                        })
                        .unzip();
                    // Storage nodes add two rows the LP. First is the bounds on increase
                    // in volume. The second is the bounds on decrease in volume.
                    self.builder.set_row_bounds(row, missing.as_slice());
                    row += 1;
                    self.builder.set_row_bounds(row, avail.as_slice());
                    row += 1;
                }
            }
        }

        Ok(())
    }
}

pub struct ClIpmSolver {
    builder: SolverBuilder,
    ipm: PathFollowingDirectClSolver,
}

impl MultiStateSolver for ClIpmSolver {
    fn setup(model: &Model, num_scenarios: usize) -> Result<Box<Self>, PywrError> {
        let builder = SolverBuilder::create(model, num_scenarios)?;

        let matrix = builder.builder.get_full_matrix();
        let num_rows = matrix.row_starts.len() - 1;
        let num_cols = builder.builder.num_cols;

        // TODO handle the error better
        let ipm = PathFollowingDirectClSolver::from_data(
            num_rows,
            num_cols,
            matrix.row_starts,
            matrix.columns,
            matrix.elements,
            builder.builder.inequality.nrows() as u32,
            num_scenarios as u32,
        )
        .expect("Failed to create the OpenCL IPM solver from the given LP data.");

        Ok(Box::new(Self { builder, ipm }))
    }

    fn solve(&mut self, model: &Model, timestep: &Timestep, states: &mut [State]) -> Result<SolverTimings, PywrError> {
        // TODO complete the timings
        let mut timings = SolverTimings::default();

        self.builder.update(model, timestep, states, &mut timings)?;

        let solution = self
            .ipm
            .solve(
                self.builder.builder.row_upper.as_slice(),
                self.builder.builder.col_obj_coef.as_slice(),
            )
            .expect("Solve failed with the OpenCL IPM solver.");

        let start_save_solution = Instant::now();
        let num_states = states.len();
        for (i, state) in states.iter_mut().enumerate() {
            let network_state = state.get_mut_network_state();
            network_state.reset();

            for edge in model.edges.deref() {
                let flow = solution[*edge.index().deref() * num_states + i];
                network_state.add_flow(edge, timestep, flow)?;
            }
        }
        timings.save_solution += start_save_solution.elapsed();

        Ok(timings)
    }
}
