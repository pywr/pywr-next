use crate::model::Model;
use crate::node::NodeType;
use crate::solvers::{Solver, SolverTimings};
use crate::state::ParameterState;
use crate::timestep::Timestep;
use crate::{NetworkState, PywrError};
use clp_sys::*;
use libc::{c_double, c_int};
use std::collections::HashMap;
use std::ffi::CString;
use std::ops::Deref;
use std::slice;
use std::time::{Duration, Instant};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
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

impl ClpSimplex {
    pub fn new() -> ClpSimplex {
        let model: ClpSimplex;

        unsafe {
            let ptr = Clp_newModel();
            model = ClpSimplex { ptr };
            Clp_setLogLevel(ptr, 0);
            Clp_setObjSense(ptr, 1.0);
        }

        model
    }

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

        // println!("number: {}", number);
        // println!("row_lower: {:?}", row_lower);
        // println!("row_upper: {:?}", row_upper);
        // println!("row_starts: {:?}", row_starts);
        // println!("columns: {:?}", columns);
        // println!("elements: {:?}", elements);

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

#[derive(Debug)]
pub struct ClpSolution {
    objective_value: f64,
    primal_columns: Vec<f64>,
    solve_time: Duration,
}

impl ClpSolution {
    pub fn get_solution(&self, col: usize) -> f64 {
        self.primal_columns[col]
    }
}

#[derive(Debug)]
pub enum Bounds {
    Free,
    Lower(f64),
    Upper(f64),
    Double(f64, f64),
    Fixed(f64),
}

pub struct ClpModelBuilder {
    col_lower: Vec<c_double>,
    col_upper: Vec<c_double>,
    col_obj_coef: Vec<c_double>,
    row_lower: Vec<c_double>,
    row_upper: Vec<c_double>,
    row_starts: Vec<CoinBigIndex>,
    columns: Vec<c_int>,
    elements: Vec<c_double>,
    model: Option<ClpSimplex>,
}

impl ClpModelBuilder {
    pub fn new() -> Self {
        Self {
            col_lower: Vec::new(),
            col_upper: Vec::new(),
            col_obj_coef: Vec::new(),
            row_lower: Vec::new(),
            row_upper: Vec::new(),
            row_starts: vec![0],
            columns: Vec::new(),
            elements: Vec::new(),
            model: None,
        }
    }

    pub fn add_column(&mut self, obj_coef: f64, bounds: Bounds) {
        let (lb, ub): (f64, f64) = match bounds {
            Bounds::Double(lb, ub) => (lb, ub),
            Bounds::Lower(lb) => (lb, f64::MAX),
            Bounds::Fixed(b) => (b, b),
            Bounds::Free => (f64::MIN, f64::MAX),
            Bounds::Upper(ub) => (f64::MIN, ub),
        };

        self.col_lower.push(lb);
        self.col_upper.push(ub);
        self.col_obj_coef.push(obj_coef);
    }

    pub fn set_obj_coefficient(&mut self, col: usize, obj_coef: f64) {
        self.col_obj_coef[col] = obj_coef;
    }

    pub fn set_row_bounds(&mut self, row: usize, lb: f64, ub: f64) {
        self.row_lower[row] = lb;
        self.row_upper[row] = ub;
    }

    pub fn add_row(&mut self, row: ClpRowBuilder) {
        self.row_lower.push(row.lower);
        self.row_upper.push(row.upper);
        let prev_row_start = *self.row_starts.get(&self.row_starts.len() - 1).unwrap();
        self.row_starts.push(prev_row_start + row.columns.len() as CoinBigIndex);
        for (column, value) in row.columns {
            self.columns.push(column);
            self.elements.push(value);
        }
    }

    pub fn nrows(&self) -> usize {
        self.row_upper.len()
    }

    pub fn setup(&mut self) {
        let mut model = ClpSimplex::new();
        model.resize(0, self.col_upper.len() as i32);

        model.change_column_lower(&self.col_lower);
        model.change_column_upper(&self.col_upper);
        model.change_objective_coefficients(&self.col_obj_coef);
        // println!("Adding rows ...");
        model.add_rows(
            &self.row_lower,
            &self.row_upper,
            &self.row_starts,
            &self.columns,
            &self.elements,
        );

        // println!("row_lower: {:?}", self.row_lower);
        // println!("row_upper: {:?}", self.row_upper);
        // println!("row_starts: {:?}", self.row_starts);
        // println!("columns: {:?}", self.columns);
        // println!("elements: {:?}", self.elements);
        // println!("obj_coef: {:?}", self.col_obj_coef);

        model.initial_dual_solve();

        self.model = Some(model);
    }

    pub fn solve(&mut self) -> Result<ClpSolution, ClpError> {
        // let mut model = ClpSimplex::new();
        let model = match &mut self.model {
            Some(m) => m,
            None => return Err(ClpError::SimplexNotInitialisedError),
        };

        //model.change_column_lower(&self.col_lower);
        //model.change_column_upper(&self.col_upper);
        model.change_objective_coefficients(&self.col_obj_coef);
        model.change_row_lower(&self.row_lower);
        model.change_row_upper(&self.row_upper);

        // println!("number: {}", number);
        // println!("row_lower: {:?}", self.row_lower);
        // println!("row_upper: {:?}", self.row_upper);
        // println!("row_starts: {:?}", self.row_starts);
        // println!("columns: {:?}", self.columns);
        // println!("elements: {:?}", self.elements);
        // println!("obj_coef: {:?}", self.col_obj_coef);

        // model.add_rows(
        //     &self.row_lower,
        //     &self.row_upper,
        //     &self.row_starts,
        //     &self.columns,
        //     &self.elements,
        // );
        // println!("coef: {:?}", model.get_objective_coefficients(2));
        // println!("row_upper: {:?}", model.get_row_upper(4));
        let now = Instant::now();
        model.dual_solve();
        let solve_time = now.elapsed();
        // model.primal_solve();
        // model.initial_solve();
        //let t = now.elapsed().as_secs_f64();
        // println!("dual solve: {} s; {} per s", t, 1.0/t);
        // println!("coef: {:?}", model.get_objective_coefficients(2));

        let solution = ClpSolution {
            objective_value: model.objective_value(),
            primal_columns: model.primal_column_solution(self.col_upper.len()),
            solve_time,
        };

        Ok(solution)
    }
}

pub struct ClpRowBuilder {
    lower: f64,
    upper: f64,
    columns: HashMap<i32, f64>,
}

impl ClpRowBuilder {
    pub fn new() -> Self {
        Self {
            lower: 0.0,
            upper: f64::MAX,
            columns: HashMap::new(),
        }
    }

    pub fn set_upper(&mut self, upper: f64) {
        self.upper = upper;
    }

    pub fn set_lower(&mut self, lower: f64) {
        self.lower = lower
    }

    // pub fn set_bounds(&mut self, bounds: Bounds) {
    //     let (lb, ub) = match bounds
    // }

    /// Add an element to the row
    ///
    /// If the column already exists `value` will be added to the existing coefficient.
    pub fn add_element(&mut self, column: i32, value: f64) {
        *self.columns.entry(column).or_insert(0.0) += value;
    }
}

pub struct ClpSolver {
    builder: ClpModelBuilder,
    start_node_constraints: Option<usize>,
    start_agg_node_constraints: Option<usize>,
    start_virtual_storage_constraints: Option<usize>,
}

impl ClpSolver {
    pub(crate) fn new() -> Self {
        Self {
            builder: ClpModelBuilder::new(),
            start_node_constraints: None,
            start_agg_node_constraints: None,
            start_virtual_storage_constraints: None,
        }
    }

    /// Create a column for each edge
    fn create_columns(&mut self, model: &Model) -> Result<(), PywrError> {
        // One column per edge
        let ncols = model.edges.len();
        if ncols < 1 {
            return Err(PywrError::NoEdgesDefined);
        }
        // Add columns set the columns as x >= 0.0 (i.e. no upper bounds)
        for _ in 0..ncols {
            self.builder.add_column(0.0, Bounds::Lower(0.0));
        }

        Ok(())
    }

    /// Create mass balance constraints for each edge
    fn create_mass_balance_constraints(&mut self, model: &Model) {
        for node in model.nodes.deref() {
            // Only link nodes create mass-balance constraints

            let mut row = ClpRowBuilder::new();

            if let NodeType::Link = node.node_type() {
                let incoming_edges = node.get_incoming_edges().unwrap();
                let outgoing_edges = node.get_outgoing_edges().unwrap();

                // TODO check for length >= 1

                for edge in &incoming_edges {
                    row.add_element(edge.index() as i32, 1.0);
                }
                for edge in &outgoing_edges {
                    row.add_element(edge.index() as i32, -1.0);
                }

                row.set_upper(0.0);
                row.set_lower(0.0);
            }

            self.builder.add_row(row);
        }
    }

    /// Create node constraints
    ///
    /// One constraint is created per node to enforce any constraints (flow or storage)
    /// that it may define.
    fn create_node_constraints(&mut self, model: &Model) {
        let start_row = self.builder.nrows();

        for node in model.nodes.deref() {
            // Create empty arrays to store the matrix data
            let mut row = ClpRowBuilder::new();

            match node.node_type() {
                NodeType::Link => {
                    for edge in node.get_outgoing_edges().unwrap() {
                        row.add_element(edge.index() as i32, 1.0);
                    }
                }
                NodeType::Input => {
                    for edge in node.get_outgoing_edges().unwrap() {
                        row.add_element(edge.index() as i32, 1.0);
                    }
                }
                NodeType::Output => {
                    for edge in node.get_incoming_edges().unwrap() {
                        row.add_element(edge.index() as i32, 1.0);
                    }
                }
                NodeType::Storage => {
                    for edge in node.get_incoming_edges().unwrap() {
                        row.add_element(edge.index() as i32, 1.0);
                    }
                    for edge in node.get_outgoing_edges().unwrap() {
                        row.add_element(edge.index() as i32, -1.0);
                    }
                }
            }

            self.builder.add_row(row);
        }
        self.start_node_constraints = Some(start_row);
    }

    /// Create aggregated node constraints
    ///
    /// One constraint is created per node to enforce any constraints (flow or storage)
    /// that it may define.
    fn create_aggregated_node_constraints(&mut self, model: &Model) {
        let start_row = self.builder.nrows();

        for agg_node in model.aggregated_nodes.deref() {
            // Create empty arrays to store the matrix data
            let mut row = ClpRowBuilder::new();

            for node_index in agg_node.get_nodes() {
                // TODO error handling?
                let node = model.nodes.get(&node_index).expect("Node index not found!");
                match node.node_type() {
                    NodeType::Link => {
                        for edge in node.get_outgoing_edges().unwrap() {
                            row.add_element(edge.index() as i32, 1.0);
                        }
                    }
                    NodeType::Input => {
                        for edge in node.get_outgoing_edges().unwrap() {
                            row.add_element(edge.index() as i32, 1.0);
                        }
                    }
                    NodeType::Output => {
                        for edge in node.get_incoming_edges().unwrap() {
                            row.add_element(edge.index() as i32, 1.0);
                        }
                    }
                    NodeType::Storage => {
                        for edge in node.get_incoming_edges().unwrap() {
                            row.add_element(edge.index() as i32, 1.0);
                        }
                        for edge in node.get_outgoing_edges().unwrap() {
                            row.add_element(edge.index() as i32, -1.0);
                        }
                    }
                }
            }

            self.builder.add_row(row);
        }
        self.start_agg_node_constraints = Some(start_row);
    }

    /// Create aggregated node constraints
    ///
    /// One constraint is created per node to enforce any constraints (flow or storage)
    /// that it may define.
    fn create_virtual_storage_constraints(&mut self, model: &Model) {
        let start_row = self.builder.nrows();

        for virtual_storage in model.virtual_storage_nodes.deref() {
            // Create empty arrays to store the matrix data

            if let Some(nodes) = virtual_storage.get_nodes_with_factors() {
                let mut row = ClpRowBuilder::new();
                for (node_index, factor) in nodes {
                    let node = model.nodes.get(&node_index).expect("Node index not found!");
                    match node.node_type() {
                        NodeType::Link => {
                            for edge in node.get_outgoing_edges().unwrap() {
                                row.add_element(edge.index() as i32, factor);
                            }
                        }
                        NodeType::Input => {
                            for edge in node.get_outgoing_edges().unwrap() {
                                row.add_element(edge.index() as i32, factor);
                            }
                        }
                        NodeType::Output => {
                            for edge in node.get_incoming_edges().unwrap() {
                                row.add_element(edge.index() as i32, factor);
                            }
                        }
                        NodeType::Storage => {
                            for edge in node.get_incoming_edges().unwrap() {
                                row.add_element(edge.index() as i32, factor);
                            }
                            for edge in node.get_outgoing_edges().unwrap() {
                                row.add_element(edge.index() as i32, -factor);
                            }
                        }
                    }
                }
                self.builder.add_row(row);
            }
        }
        self.start_virtual_storage_constraints = Some(start_row);
    }

    /// Update edge objective coefficients
    fn update_edge_objectives(&mut self, model: &Model, parameter_states: &ParameterState) -> Result<(), PywrError> {
        for edge in &model.edges {
            let cost: f64 = edge.cost(&model.nodes, parameter_states)?;
            self.builder.set_obj_coefficient(edge.index(), cost);
        }
        Ok(())
    }

    /// Update node constraints
    fn update_node_constraint_bounds(
        &mut self,
        model: &Model,
        timestep: &Timestep,
        network_state: &NetworkState,
        parameter_states: &ParameterState,
    ) -> Result<(), PywrError> {
        let start_row = match self.start_node_constraints {
            Some(r) => r,
            None => return Err(PywrError::SolverNotSetup),
        };

        for node in model.nodes.deref() {
            let (lb, ub): (f64, f64) = match node.get_current_flow_bounds(parameter_states) {
                Ok(bnds) => bnds,
                Err(PywrError::FlowConstraintsUndefined) => {
                    // Must be a storage node
                    let (avail, missing) = match node.get_current_available_volume_bounds(network_state) {
                        Ok(bnds) => bnds,
                        Err(e) => return Err(e),
                    };
                    let dt = timestep.days();
                    (-avail / dt, missing / dt)
                }
                Err(e) => return Err(e),
            };

            // println!("Node {:?} [{}, {}]", node, lb, ub);
            self.builder.set_row_bounds(start_row + *node.index(), lb, ub);

            // println!("Node {:?} [{}, {}]", node.name(), lb, ub);
        }

        Ok(())
    }

    /// Update aggregated node constraints
    fn update_aggregated_node_constraint_bounds(
        &mut self,
        model: &Model,
        parameter_states: &ParameterState,
    ) -> Result<(), PywrError> {
        let start_row = match self.start_agg_node_constraints {
            Some(r) => r,
            None => return Err(PywrError::SolverNotSetup),
        };

        for agg_node in model.aggregated_nodes.deref() {
            let (lb, ub): (f64, f64) = agg_node.get_current_flow_bounds(parameter_states)?;
            self.builder.set_row_bounds(start_row + *agg_node.index(), lb, ub);
        }

        Ok(())
    }
}

impl Solver for ClpSolver {
    fn setup(&mut self, model: &Model) -> Result<(), PywrError> {
        // Create the columns
        self.create_columns(model)?;
        // Create edge mass balance constraints
        self.create_mass_balance_constraints(model);
        // Create the nodal constraints
        self.create_node_constraints(model);
        // Create the aggregated node constraints
        self.create_aggregated_node_constraints(model);
        // Create virtual storage constraints
        self.create_virtual_storage_constraints(model);

        self.builder.setup();

        Ok(())
    }
    fn solve(
        &mut self,
        model: &Model,
        timestep: &Timestep,
        network_state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<(NetworkState, SolverTimings), PywrError> {
        let mut timings = SolverTimings::default();
        let start_objective_update = Instant::now();
        self.update_edge_objectives(model, parameter_state)?;
        timings.update_objective += start_objective_update.elapsed();

        let start_constraint_update = Instant::now();
        self.update_node_constraint_bounds(model, timestep, network_state, parameter_state)?;
        self.update_aggregated_node_constraint_bounds(model, parameter_state)?;
        timings.update_constraints += start_constraint_update.elapsed();

        let start_solve = Instant::now();
        let solution = self.builder.solve()?;
        //timings.solve += start_solve.elapsed();
        timings.solve += solution.solve_time;

        // println!("{:?}", solution);
        // Create the updated network state from the results
        let mut new_state = network_state.with_capacity();

        let start_save_solution = Instant::now();
        for edge in &model.edges {
            let flow = solution.get_solution(edge.index());
            new_state.add_flow(edge, timestep, flow)?;
        }
        timings.save_solution += start_save_solution.elapsed();

        Ok((new_state, timings))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use float_cmp::approx_eq;

    #[test]
    fn clp_create() {
        ClpSimplex::new();
    }

    #[test]
    fn clp_add_rows() {
        let mut model = ClpSimplex::new();
        model.resize(0, 2);

        let row_lower: Vec<c_double> = vec![0.0];
        let row_upper: Vec<c_double> = vec![2.0];
        let row_starts: Vec<CoinBigIndex> = vec![0, 2];
        let columns: Vec<c_int> = vec![0, 1];
        let elements: Vec<c_double> = vec![1.0, 1.0];

        model.add_rows(&row_lower, &row_upper, &row_starts, &columns, &elements);
    }

    #[test]
    fn model_builder_new() {
        let _builder = ClpModelBuilder::new();
    }

    #[test]
    fn builder_add_rows() {
        let mut builder = ClpModelBuilder::new();
        let mut row = ClpRowBuilder::new();
        row.add_element(0, 1.0);
        row.add_element(1, 1.0);
        row.set_lower(0.0);
        row.set_upper(2.0);
        builder.add_row(row);
    }

    #[test]
    fn builder_solve() {
        let mut builder = ClpModelBuilder::new();

        builder.add_column(1.0, Bounds::Double(0.0, 2.0));
        builder.add_column(0.0, Bounds::Lower(0.0));
        builder.add_column(4.0, Bounds::Double(0.0, 4.0));

        // Row1
        let mut row = ClpRowBuilder::new();
        row.add_element(0, 1.0);
        row.add_element(2, 1.0);
        row.set_lower(2.0);
        row.set_upper(f64::MAX);
        builder.add_row(row);

        // Row2
        let mut row = ClpRowBuilder::new();
        row.add_element(0, 1.0);
        row.add_element(1, -5.0);
        row.add_element(2, 1.0);
        row.set_lower(1.0);
        row.set_upper(1.0);
        builder.add_row(row);

        builder.setup();

        let solution = builder.solve().unwrap();

        assert!(approx_eq!(f64, solution.objective_value, 2.0));
    }

    #[test]
    fn builder_solve2() {
        let mut builder = ClpModelBuilder::new();

        builder.add_column(-2.0, Bounds::Lower(0.0));
        builder.add_column(-3.0, Bounds::Lower(0.0));
        builder.add_column(-4.0, Bounds::Lower(0.0));

        // Row1
        let mut row = ClpRowBuilder::new();
        row.add_element(0, 3.0);
        row.add_element(1, 2.0);
        row.add_element(2, 1.0);
        row.set_lower(f64::MIN);
        row.set_upper(10.0);
        builder.add_row(row);

        // Row2
        let mut row = ClpRowBuilder::new();
        row.add_element(0, 2.0);
        row.add_element(1, 5.0);
        row.add_element(2, 3.0);
        row.set_lower(f64::MIN);
        row.set_upper(15.0);
        builder.add_row(row);

        builder.setup();

        let solution = builder.solve().unwrap();

        assert!(approx_eq!(f64, solution.objective_value, -20.0));
        assert_eq!(solution.primal_columns, vec![0.0, 0.0, 5.0])
    }
}
