use crate::model::Model;
use crate::node::{Node, NodeType};
use crate::parameters::FloatValue;
use crate::solvers::{Solver, SolverTimings};
use crate::state::State;
use crate::timestep::Timestep;
use crate::{NetworkState, PywrError};
use clp_sys::*;
use libc::{c_double, c_int, c_void};
use std::collections::HashMap;
use std::ffi::CString;
use std::ops::Deref;
use std::ptr::null;
use std::slice;
use std::time::{Duration, Instant};
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum ClpError {
    #[error("an unknown error occurred in Clp.")]
    UnknownError,
    #[error("the simplex model has not been created")]
    SimplexNotInitialisedError,
}

pub type CoinBigIndex = c_int;

pub struct ClpSimplex {
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

pub struct ClpModelBuilder<T> {
    col_lower: Vec<c_double>,
    col_upper: Vec<c_double>,
    col_obj_coef: Vec<c_double>,
    row_lower: Vec<c_double>,
    row_upper: Vec<c_double>,
    row_mask: Vec<c_int>,
    row_starts: Vec<CoinBigIndex>,
    columns: Vec<c_int>,
    elements: Vec<c_double>,
    model: Option<T>,
}

impl<T> Default for ClpModelBuilder<T> {
    fn default() -> Self {
        Self {
            col_lower: Vec::new(),
            col_upper: Vec::new(),
            col_obj_coef: Vec::new(),
            row_lower: Vec::new(),
            row_upper: Vec::new(),
            row_mask: Vec::new(),
            row_starts: vec![0],
            columns: Vec::new(),
            elements: Vec::new(),
            model: None,
        }
    }
}

impl<T> ClpModelBuilder<T> {
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
        self.row_mask.push(1);
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
}

impl ClpModelBuilder<ClpSimplex> {
    pub fn setup(&mut self) {
        let mut model = ClpSimplex::default();
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

impl ClpModelBuilder<Highs> {
    pub fn setup(&mut self) {
        let mut model = Highs::default();

        model.add_cols(
            &self.col_lower,
            &self.col_upper,
            &self.col_obj_coef,
            self.col_upper.len() as i32,
        );

        // println!("Adding rows ...");
        model.add_rows(
            self.row_upper.len() as i32,
            &self.row_lower,
            &self.row_upper,
            self.elements.len() as i32,
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

        // model.initial_dual_solve();

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

        model.change_objective_coefficients(&self.col_obj_coef, self.col_obj_coef.len() as i32);

        model.change_row_bounds(&self.row_mask, &self.row_lower, &self.row_upper);

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
        model.run();
        let solve_time = now.elapsed();
        // model.primal_solve();
        // model.initial_solve();
        //let t = now.elapsed().as_secs_f64();
        // println!("dual solve: {} s; {} per s", t, 1.0/t);
        // println!("coef: {:?}", model.get_objective_coefficients(2));

        let solution = ClpSolution {
            objective_value: model.objective_value(),
            primal_columns: model.primal_column_solution(self.col_upper.len(), self.row_upper.len()),
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

impl Default for ClpRowBuilder {
    fn default() -> Self {
        Self {
            lower: 0.0,
            upper: f64::MAX,
            columns: HashMap::new(),
        }
    }
}

impl ClpRowBuilder {
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

    fn add_node(&mut self, node: &Node, factor: f64) {
        match node.node_type() {
            NodeType::Link => {
                for edge in node.get_outgoing_edges().unwrap() {
                    self.add_element(*edge.deref() as i32, factor);
                }
            }
            NodeType::Input => {
                for edge in node.get_outgoing_edges().unwrap() {
                    self.add_element(*edge.deref() as i32, factor);
                }
            }
            NodeType::Output => {
                for edge in node.get_incoming_edges().unwrap() {
                    self.add_element(*edge.deref() as i32, factor);
                }
            }
            NodeType::Storage => {
                for edge in node.get_incoming_edges().unwrap() {
                    self.add_element(*edge.deref() as i32, factor);
                }
                for edge in node.get_outgoing_edges().unwrap() {
                    self.add_element(*edge.deref() as i32, -factor);
                }
            }
        }
    }
}

#[derive(Default)]
pub struct ClpSolver<T> {
    builder: ClpModelBuilder<T>,
    start_node_constraints: Option<usize>,
    start_agg_node_constraints: Option<usize>,
    start_agg_node_factor_constraints: Option<usize>,
    start_virtual_storage_constraints: Option<usize>,
}

impl<T> ClpSolver<T> {
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

            let mut row = ClpRowBuilder::default();

            if let NodeType::Link = node.node_type() {
                let incoming_edges = node.get_incoming_edges().unwrap();
                let outgoing_edges = node.get_outgoing_edges().unwrap();

                // TODO check for length >= 1

                for edge in incoming_edges {
                    row.add_element(*edge.deref() as i32, 1.0);
                }
                for edge in outgoing_edges {
                    row.add_element(*edge.deref() as i32, -1.0);
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
            let mut row = ClpRowBuilder::default();

            match node.node_type() {
                NodeType::Link => {
                    for edge in node.get_outgoing_edges().unwrap() {
                        row.add_element(*edge.deref() as i32, 1.0);
                    }
                }
                NodeType::Input => {
                    for edge in node.get_outgoing_edges().unwrap() {
                        row.add_element(*edge.deref() as i32, 1.0);
                    }
                }
                NodeType::Output => {
                    for edge in node.get_incoming_edges().unwrap() {
                        row.add_element(*edge.deref() as i32, 1.0);
                    }
                }
                NodeType::Storage => {
                    for edge in node.get_incoming_edges().unwrap() {
                        row.add_element(*edge.deref() as i32, 1.0);
                    }
                    for edge in node.get_outgoing_edges().unwrap() {
                        row.add_element(*edge.deref() as i32, -1.0);
                    }
                }
            }

            self.builder.add_row(row);
        }
        self.start_node_constraints = Some(start_row);
    }

    /// Create aggregated node factor constraints
    ///
    /// One constraint is created per node to enforce any factor constraints
    fn create_aggregated_node_factor_constraints(&mut self, model: &Model) {
        let start_row = self.builder.nrows();

        for agg_node in model.aggregated_nodes.deref() {
            // Only create row for nodes that have factors
            for factor_pairs in agg_node.get_norm_factor_pairs() {
                for ((n0, f0), (n1, f1)) in factor_pairs {
                    // Create rows for each node in the aggregated node pair with the first one.

                    let mut row = ClpRowBuilder::default();

                    // TODO error handling?
                    let node0 = model.nodes.get(&n0).expect("Node index not found!");
                    let node1 = model.nodes.get(&n1).expect("Node index not found!");

                    let ff0 = match f0 {
                        FloatValue::Constant(f) => f,
                        _ => panic!("Dynamic float factors not supported!"),
                    };

                    let ff1 = match f1 {
                        FloatValue::Constant(f) => f,
                        _ => panic!("Dynamic float factors not supported!"),
                    };

                    row.add_node(node0, 1.0);
                    row.add_node(node1, -ff0 / ff1);
                    // Make the row fixed at zero RHS
                    row.set_lower(0.0);
                    row.set_upper(0.0);

                    self.builder.add_row(row);
                }
            }
        }
        self.start_agg_node_factor_constraints = Some(start_row);
    }

    /// Create aggregated node constraints
    ///
    /// One constraint is created per node to enforce any constraints (flow or storage)
    /// that it may define.
    fn create_aggregated_node_constraints(&mut self, model: &Model) {
        let start_row = self.builder.nrows();

        for agg_node in model.aggregated_nodes.deref() {
            // Create empty arrays to store the matrix data
            let mut row = ClpRowBuilder::default();

            for node_index in agg_node.get_nodes() {
                // TODO error handling?
                let node = model.nodes.get(&node_index).expect("Node index not found!");
                match node.node_type() {
                    NodeType::Link => {
                        for edge in node.get_outgoing_edges().unwrap() {
                            row.add_element(*edge.deref() as i32, 1.0);
                        }
                    }
                    NodeType::Input => {
                        for edge in node.get_outgoing_edges().unwrap() {
                            row.add_element(*edge.deref() as i32, 1.0);
                        }
                    }
                    NodeType::Output => {
                        for edge in node.get_incoming_edges().unwrap() {
                            row.add_element(*edge.deref() as i32, 1.0);
                        }
                    }
                    NodeType::Storage => {
                        for edge in node.get_incoming_edges().unwrap() {
                            row.add_element(*edge.deref() as i32, 1.0);
                        }
                        for edge in node.get_outgoing_edges().unwrap() {
                            row.add_element(*edge.deref() as i32, -1.0);
                        }
                    }
                }
            }

            self.builder.add_row(row);
        }
        self.start_agg_node_constraints = Some(start_row);
    }

    /// Create virtual storage node constraints
    ///
    fn create_virtual_storage_constraints(&mut self, model: &Model) {
        let start_row = self.builder.nrows();

        for virtual_storage in model.virtual_storage_nodes.deref() {
            // Create empty arrays to store the matrix data

            if let Some(nodes) = virtual_storage.get_nodes_with_factors() {
                let mut row = ClpRowBuilder::default();
                for (node_index, factor) in nodes {
                    let node = model.nodes.get(&node_index).expect("Node index not found!");
                    match node.node_type() {
                        NodeType::Link => {
                            for edge in node.get_outgoing_edges().unwrap() {
                                row.add_element(*edge.deref() as i32, factor);
                            }
                        }
                        NodeType::Input => {
                            for edge in node.get_outgoing_edges().unwrap() {
                                row.add_element(*edge.deref() as i32, factor);
                            }
                        }
                        NodeType::Output => {
                            for edge in node.get_incoming_edges().unwrap() {
                                row.add_element(*edge.deref() as i32, factor);
                            }
                        }
                        NodeType::Storage => {
                            for edge in node.get_incoming_edges().unwrap() {
                                row.add_element(*edge.deref() as i32, factor);
                            }
                            for edge in node.get_outgoing_edges().unwrap() {
                                row.add_element(*edge.deref() as i32, -factor);
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
    fn update_edge_objectives(&mut self, model: &Model, state: &State) -> Result<(), PywrError> {
        for edge in model.edges.deref() {
            let cost: f64 = edge.cost(&model.nodes, state)?;
            self.builder.set_obj_coefficient(*edge.index().deref(), cost);
        }
        Ok(())
    }

    /// Update node constraints
    fn update_node_constraint_bounds(
        &mut self,
        model: &Model,
        timestep: &Timestep,
        state: &State,
    ) -> Result<(), PywrError> {
        let start_row = match self.start_node_constraints {
            Some(r) => r,
            None => return Err(PywrError::SolverNotSetup),
        };

        for node in model.nodes.deref() {
            let (lb, ub): (f64, f64) = match node.get_current_flow_bounds(state) {
                Ok(bnds) => bnds,
                Err(PywrError::FlowConstraintsUndefined) => {
                    // Must be a storage node
                    let (avail, missing) = match node.get_current_available_volume_bounds(state) {
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
    fn update_aggregated_node_constraint_bounds(&mut self, model: &Model, state: &State) -> Result<(), PywrError> {
        let start_row = match self.start_agg_node_constraints {
            Some(r) => r,
            None => return Err(PywrError::SolverNotSetup),
        };

        for agg_node in model.aggregated_nodes.deref() {
            let (lb, ub): (f64, f64) = agg_node.get_current_flow_bounds(state)?;
            self.builder.set_row_bounds(start_row + *agg_node.index(), lb, ub);
            // println!("Agg node {:?} [{}, {}]", agg_node.name(), lb, ub);
        }

        Ok(())
    }
}

impl Solver for ClpSolver<ClpSimplex> {
    fn setup(&mut self, model: &Model) -> Result<(), PywrError> {
        // Create the columns
        self.create_columns(model)?;
        // Create edge mass balance constraints
        self.create_mass_balance_constraints(model);
        // Create the nodal constraints
        self.create_node_constraints(model);
        // Create the aggregated node constraints
        self.create_aggregated_node_constraints(model);
        // Create the aggregated node factor constraints
        self.create_aggregated_node_factor_constraints(model);
        // Create virtual storage constraints
        self.create_virtual_storage_constraints(model);

        self.builder.setup();

        Ok(())
    }
    fn solve(&mut self, model: &Model, timestep: &Timestep, state: &mut State) -> Result<SolverTimings, PywrError> {
        let mut timings = SolverTimings::default();
        let start_objective_update = Instant::now();
        self.update_edge_objectives(model, state)?;
        timings.update_objective += start_objective_update.elapsed();

        let start_constraint_update = Instant::now();
        self.update_node_constraint_bounds(model, timestep, state)?;
        self.update_aggregated_node_constraint_bounds(model, state)?;
        timings.update_constraints += start_constraint_update.elapsed();

        let solution = self.builder.solve()?;
        //timings.solve += start_solve.elapsed();
        timings.solve += solution.solve_time;

        // println!("{:?}", solution);
        // Create the updated network state from the results
        let mut network_state = state.get_mut_network_state();
        network_state.reset();

        let start_save_solution = Instant::now();
        for edge in model.edges.deref() {
            let flow = solution.get_solution(*edge.index().deref());
            network_state.add_flow(edge, timestep, flow)?;
        }
        timings.save_solution += start_save_solution.elapsed();

        Ok(timings)
    }
}

pub struct Highs {
    ptr: *mut c_void,
}

unsafe impl Send for Highs {}

impl Default for Highs {
    fn default() -> Self {
        let model: Highs;

        unsafe {
            let ptr = highs_sys::Highs_create();
            model = Self { ptr };
            let option_name = CString::new("output_flag").unwrap();
            highs_sys::Highs_setBoolOptionValue(ptr, option_name.as_ptr(), 0);
            highs_sys::Highs_changeObjectiveSense(ptr, highs_sys::OBJECTIVE_SENSE_MINIMIZE);
        }

        model
    }
}

impl Highs {
    pub fn add_cols(
        &mut self,
        col_lower: &[c_double],
        col_upper: &[c_double],
        col_obj_coef: &[c_double],
        ncols: c_int,
    ) {
        unsafe {
            highs_sys::Highs_addCols(
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
        }
    }

    pub fn add_rows(
        &mut self,
        nrows: c_int,
        row_lower: &[c_double],
        row_upper: &[c_double],
        nnz: c_int,
        row_starts: &[CoinBigIndex],
        columns: &[c_int],
        elements: &[c_double],
    ) {
        unsafe {
            highs_sys::Highs_addRows(
                self.ptr,
                nrows,
                row_lower.as_ptr(),
                row_upper.as_ptr(),
                nnz,
                row_starts.as_ptr(),
                columns.as_ptr(),
                elements.as_ptr(),
            );
        }
    }

    pub fn change_objective_coefficients(&mut self, obj_coefficients: &[c_double], numcols: c_int) {
        unsafe {
            highs_sys::Highs_changeColsCostByRange(self.ptr, 0, numcols, obj_coefficients.as_ptr());
        }
    }

    pub fn change_row_bounds(&mut self, mask: &[c_int], lower: &[c_double], upper: &[c_double]) {
        unsafe {
            highs_sys::Highs_changeRowsBoundsByMask(self.ptr, mask.as_ptr(), lower.as_ptr(), upper.as_ptr());
        }
    }

    pub fn run(&mut self) {
        unsafe {
            let status = highs_sys::Highs_run(self.ptr);
            assert_eq!(status, highs_sys::STATUS_OK);
        }
    }

    pub fn objective_value(&mut self) -> f64 {
        let mut objective_function_value = 0.;
        unsafe {
            let info_name = CString::new("objective_function_value").unwrap();
            highs_sys::Highs_getDoubleInfoValue(
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
            highs_sys::Highs_getSolution(
                self.ptr,
                colvalue.as_mut_ptr(),
                coldual.as_mut_ptr(),
                rowvalue.as_mut_ptr(),
                rowdual.as_mut_ptr(),
            );
        }
        colvalue.to_vec()
    }
}

impl Solver for ClpSolver<Highs> {
    fn setup(&mut self, model: &Model) -> Result<(), PywrError> {
        // Create the columns
        self.create_columns(model)?;
        // Create edge mass balance constraints
        self.create_mass_balance_constraints(model);
        // Create the nodal constraints
        self.create_node_constraints(model);
        // Create the aggregated node constraints
        self.create_aggregated_node_constraints(model);
        // Create the aggregated node factor constraints
        self.create_aggregated_node_factor_constraints(model);
        // Create virtual storage constraints
        self.create_virtual_storage_constraints(model);

        self.builder.setup();

        Ok(())
    }
    fn solve(&mut self, model: &Model, timestep: &Timestep, state: &mut State) -> Result<SolverTimings, PywrError> {
        let mut timings = SolverTimings::default();
        let start_objective_update = Instant::now();
        self.update_edge_objectives(model, state)?;
        timings.update_objective += start_objective_update.elapsed();

        let start_constraint_update = Instant::now();
        self.update_node_constraint_bounds(model, timestep, state)?;
        self.update_aggregated_node_constraint_bounds(model, state)?;
        timings.update_constraints += start_constraint_update.elapsed();

        let solution = self.builder.solve()?;
        //timings.solve += start_solve.elapsed();
        timings.solve += solution.solve_time;

        // println!("{:?}", solution);
        // Reset the network state from the results
        let mut network_state = state.get_mut_network_state();
        network_state.reset();

        let start_save_solution = Instant::now();
        for edge in model.edges.deref() {
            let flow = solution.get_solution(*edge.index().deref());
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
    fn model_builder_new() {
        let _builder: ClpModelBuilder<ClpSimplex> = ClpModelBuilder::default();
    }

    #[test]
    fn builder_add_rows() {
        let mut builder: ClpModelBuilder<ClpSimplex> = ClpModelBuilder::default();
        let mut row = ClpRowBuilder::default();
        row.add_element(0, 1.0);
        row.add_element(1, 1.0);
        row.set_lower(0.0);
        row.set_upper(2.0);
        builder.add_row(row);
    }

    #[test]
    fn builder_solve() {
        let mut builder = ClpModelBuilder::<ClpSimplex>::default();

        builder.add_column(1.0, Bounds::Double(0.0, 2.0));
        builder.add_column(0.0, Bounds::Lower(0.0));
        builder.add_column(4.0, Bounds::Double(0.0, 4.0));

        // Row1
        let mut row = ClpRowBuilder::default();
        row.add_element(0, 1.0);
        row.add_element(2, 1.0);
        row.set_lower(2.0);
        row.set_upper(f64::MAX);
        builder.add_row(row);

        // Row2
        let mut row = ClpRowBuilder::default();
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
        let mut builder = ClpModelBuilder::<ClpSimplex>::default();

        builder.add_column(-2.0, Bounds::Lower(0.0));
        builder.add_column(-3.0, Bounds::Lower(0.0));
        builder.add_column(-4.0, Bounds::Lower(0.0));

        // Row1
        let mut row = ClpRowBuilder::default();
        row.add_element(0, 3.0);
        row.add_element(1, 2.0);
        row.add_element(2, 1.0);
        row.set_lower(f64::MIN);
        row.set_upper(10.0);
        builder.add_row(row);

        // Row2
        let mut row = ClpRowBuilder::default();
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
