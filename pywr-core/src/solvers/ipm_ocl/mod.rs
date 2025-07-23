mod settings;

use crate::edge::EdgeIndex;
use crate::network::Network;
use crate::node::{Node, NodeBounds, NodeType};
use crate::solvers::col_edge_map::{ColumnEdgeMap, ColumnEdgeMapBuilder};
use crate::solvers::{MultiStateSolver, SolverFeatures, SolverSetupError, SolverSolveError, SolverTimings};
use crate::state::State;
use crate::timestep::Timestep;
use ipm_ocl::{GetClProgram, PathFollowingDirectClSolver};
use rayon::iter::IndexedParallelIterator;
use rayon::iter::ParallelIterator;
use rayon::prelude::ParallelSliceMut;
pub use settings::{ClIpmSolverSettings, ClIpmSolverSettingsBuilder};
use std::collections::BTreeMap;
use std::f64;
use std::num::NonZeroUsize;
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

struct Lp {
    inequality: Matrix,
    equality: Matrix,
    num_lps: usize,
    num_cols: usize,
    row_upper: Vec<f64>,
    col_obj_coef: Vec<f64>,
}

impl Lp {
    /// Zero all objective coefficients.
    fn zero_obj_coefficients(&mut self) {
        self.col_obj_coef.fill(0.0);
    }

    pub fn add_obj_coefficient(&mut self, col: usize, obj_coef: &[f64]) {
        let first_col_id = col * self.num_lps;
        if obj_coef.len() != self.num_lps {
            panic!("Objective coefficient slice must be the same length as the number of LPs.");
        }

        for (i, &v) in obj_coef.iter().enumerate() {
            self.col_obj_coef[first_col_id + i] += v;
        }
    }

    /// Reset the row bounds to `FMIN` and `FMAX` for all rows with a mask.
    fn reset_row_bounds(&mut self) {
        for ub in self.row_upper.iter_mut().take(self.inequality.nrows() * self.num_lps) {
            *ub = B_MAX
        }
    }

    pub fn apply_row_bounds(&mut self, row: usize, ub: &[f64]) {
        let first_row_id = row * self.num_lps;

        if ub.len() != self.num_lps {
            panic!("Upper bound slice must be the same length as the number of LPs.");
        }

        for (i, v) in ub.iter().enumerate() {
            self.row_upper[first_row_id + i] = self.row_upper[first_row_id + i].min(*v);
        }
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

struct LpBuilder {
    inequality: Vec<RowBuilder>,
    equality: Vec<RowBuilder>,
    num_lps: usize,
    num_cols: usize,
}

impl LpBuilder {
    fn new(num_lps: usize) -> Self {
        Self {
            inequality: Vec::new(),
            equality: Vec::new(),
            num_lps,
            num_cols: 0,
            // row_upper: Vec::new(),
            // Pre-allocate array for the objective coefficients
            // col_obj_coef: vec![0.0; num_lps * num_cols],
        }
    }

    fn add_column(&mut self) {
        self.num_cols += 1;
    }

    fn add_row(&mut self, row: RowBuilder) -> Option<usize> {
        match &row.upper {
            Bounds::Upper => {
                // let row_id = self.inequality.len();
                // self.inequality.push(row);
                // Some(row_id)

                match self.inequality.iter().position(|r| r == &row) {
                    Some(row_id) => Some(row_id),
                    None => {
                        // No row found, add a new one.
                        let row_id = self.inequality.len();
                        self.inequality.push(row);
                        Some(row_id)
                    }
                }
            }
            Bounds::Fixed => {
                self.equality.push(row);
                None
            }
        }
    }

    /// Build the LP into a final sparse form
    fn build(self) -> Lp {
        let num_rows = self.equality.len() + self.inequality.len();
        let row_upper = vec![0.0; num_rows * self.num_lps];
        let col_obj_coef = vec![0.0; self.num_cols * self.num_lps];

        // println!("Number of columns: {}", self.num_cols);
        // println!("Number of rows: {num_rows}");
        // println!("Number of inequality rows: {}", self.inequality.len());
        // println!("Number of equality rows: {}", self.equality.len());
        // println!("Number of LPs: {}", self.num_lps);

        // Build the two matrices
        let mut inequality = Matrix::default();
        let mut equality = Matrix::default();

        for row in self.inequality.into_iter() {
            // Current last entry of the inequality bounds
            // let idx = inequality.nrows() * self.num_lps;
            // Add the row to the matrix
            inequality.add_row(row);
            // Extend the inequality bounds before the equality bounds
            // let values = vec![B_MAX; self.num_lps];
            // row_upper.splice(idx..idx, values.into_iter());
        }

        for row in self.equality.into_iter() {
            equality.add_row(row);
            // Equality constraints default to zero bounds
            // row_upper.extend(vec![0.0; self.num_lps]);
        }

        // println!("Inequality: {:?}", inequality);
        // println!("Equality: {:?}", equality);

        Lp {
            inequality,
            equality,
            num_lps: self.num_lps,
            num_cols: self.num_cols,
            row_upper,
            col_obj_coef,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
enum Bounds {
    Upper,
    Fixed,
}

#[derive(Debug, Clone, PartialEq)]
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
        if !value.is_finite() {
            panic!("Row factor is non-finite.");
        }
        *self.columns.entry(column).or_insert(0.0) += value;
    }
}

struct BuiltSolver {
    lp: Lp,
    col_edge_map: ColumnEdgeMap<usize>,
    node_constraints_row_ids: Vec<usize>,
}

impl BuiltSolver {
    pub fn col_obj_coef(&self) -> &[f64] {
        &self.lp.col_obj_coef
    }

    pub fn row_upper(&self) -> &[f64] {
        &self.lp.row_upper
    }

    pub fn col_for_edge(&self, edge_index: &EdgeIndex) -> usize {
        self.col_edge_map.col_for_edge(edge_index)
    }

    fn update(
        &mut self,
        network: &Network,
        timestep: &Timestep,
        states: &[State],
        timings: &mut SolverTimings,
    ) -> Result<(), SolverSolveError> {
        let start_objective_update = Instant::now();
        self.update_edge_objectives(network, states)?;
        timings.update_objective += start_objective_update.elapsed();

        let start_constraint_update = Instant::now();

        self.lp.reset_row_bounds();
        self.update_node_constraint_bounds(network, timestep, states)?;
        // self.update_aggregated_node_constraint_bounds(network, state)?;
        timings.update_constraints += start_constraint_update.elapsed();

        Ok(())
    }

    /// Update edge objective coefficients
    fn update_edge_objectives(&mut self, network: &Network, states: &[State]) -> Result<(), SolverSolveError> {
        self.lp.zero_obj_coefficients();
        for edge in network.edges().deref() {
            // Collect all of the costs for all states together
            let cost = states
                .iter()
                .map(|s| {
                    edge.cost(network.nodes(), network, s)
                        .map(|c| if c != 0.0 { -c } else { 0.0 })
                })
                .collect::<Result<Vec<f64>, _>>()
                .map_err(|source| {
                    let from_node = match network.get_node(&edge.from_node_index()) {
                        Some(n) => n,
                        None => return SolverSolveError::NodeIndexNotFound(edge.from_node_index()),
                    };

                    let to_node = match network.get_node(&edge.to_node_index()) {
                        Some(n) => n,
                        None => return SolverSolveError::NodeIndexNotFound(edge.to_node_index()),
                    };

                    SolverSolveError::EdgeError {
                        from_name: from_node.name().to_string(),
                        from_sub_name: from_node.sub_name().map(|s| s.to_string()),
                        to_name: to_node.name().to_string(),
                        to_sub_name: to_node.sub_name().map(|s| s.to_string()),
                        source,
                    }
                })?;

            let col = self.col_for_edge(&edge.index());
            self.lp.add_obj_coefficient(col, &cost);
        }
        Ok(())
    }

    /// Update node constraints
    fn update_node_constraint_bounds(
        &mut self,
        network: &Network,
        timestep: &Timestep,
        states: &[State],
    ) -> Result<(), SolverSolveError> {
        let mut row_ids = self.node_constraints_row_ids.iter();

        let dt = timestep.days();

        for node in network.nodes().deref() {
            match node.node_type() {
                NodeType::Input | NodeType::Output | NodeType::Link => {
                    if !node.is_max_flow_unconstrained().unwrap() {
                        // Flow nodes will only respect the upper bounds
                        let ub: Vec<f64> = states
                            .iter()
                            .map(|state| {
                                // TODO check for non-zero lower bounds and error?
                                match node.get_bounds(network, state).expect("Failed to get node bounds.") {
                                    NodeBounds::Flow(bounds) => bounds.max_flow.min(B_MAX),
                                    _ => panic!("Flow bounds expected for Input, Output and Link nodes."),
                                }
                            })
                            .collect();
                        // Apply the bounds to LP
                        self.lp.apply_row_bounds(*row_ids.next().unwrap(), ub.as_slice());
                    }
                }
                NodeType::Storage => {
                    // Storage nodes instead have two constraints for available and missing volume.
                    let (avail, missing): (Vec<_>, Vec<_>) = states
                        .iter()
                        .map(
                            |state| match node.get_bounds(network, state).expect("Failed to get node bounds.") {
                                NodeBounds::Volume(bounds) => (bounds.available / dt, bounds.missing / dt),
                                _ => panic!("Volume bounds expected for Storage nodes."),
                            },
                        )
                        .unzip();
                    // Storage nodes add two rows the LP. First is the bounds on increase
                    // in volume. The second is the bounds on decrease in volume.
                    self.lp.apply_row_bounds(*row_ids.next().unwrap(), missing.as_slice());

                    self.lp.apply_row_bounds(*row_ids.next().unwrap(), avail.as_slice());
                }
            }
        }

        Ok(())
    }
}

struct SolverBuilder {
    builder: LpBuilder,
    col_edge_map: ColumnEdgeMapBuilder<usize>,
    // start_node_constraints: Option<usize>,
    // start_agg_node_constraints: Option<usize>,
    // start_agg_node_factor_constraints: Option<usize>,
    // start_virtual_storage_constraints: Option<usize>,
}

impl SolverBuilder {
    fn new(num_lps: usize) -> Self {
        Self {
            builder: LpBuilder::new(num_lps),
            col_edge_map: ColumnEdgeMapBuilder::default(),
        }
    }

    pub fn col_for_edge(&self, edge_index: &EdgeIndex) -> usize {
        self.col_edge_map.col_for_edge(edge_index)
    }

    fn create(mut self, network: &Network) -> Result<BuiltSolver, SolverSetupError> {
        // Create the columns
        self.create_columns(network)?;

        // Create edge mass balance constraints
        self.create_mass_balance_constraints(network);
        // Create the nodal constraints
        let node_constraints_row_ids = self.create_node_constraints(network);
        // // Create the aggregated node constraints
        // builder.create_aggregated_node_constraints(network);
        // // Create the aggregated node factor constraints
        // builder.create_aggregated_node_factor_constraints(network);
        // // Create virtual storage constraints
        // builder.create_virtual_storage_constraints(network);

        Ok(BuiltSolver {
            lp: self.builder.build(),
            col_edge_map: self.col_edge_map.build(),
            node_constraints_row_ids,
        })
    }

    /// Create the columns in the linear program.
    ///
    /// Typically each edge will have its own column. However, we use the mass-balance information
    /// to collapse edges (and their columns) where they are trivially the same. I.e. if there
    /// is a single incoming edge and outgoing edge at a link node.
    fn create_columns(&mut self, network: &Network) -> Result<(), SolverSetupError> {
        // One column per edge
        let ncols = network.edges().len();
        if ncols < 1 {
            return Err(SolverSetupError::NoEdgesDefined);
        }

        for edge in network.edges().iter() {
            let edge_index = edge.index();
            let from_node = network
                .get_node(&edge.from_node_index)
                .ok_or(SolverSetupError::NodeIndexNotFound(edge.from_node_index))?;

            if let NodeType::Link = from_node.node_type() {
                // We only look at link nodes; there should be no output nodes as a
                // "from_node" and input nodes will have no upstream edges
                let incoming_edges = from_node.get_incoming_edges()?;
                // NB `edge` should be one of these outgoing edges
                let outgoing_edges = from_node.get_outgoing_edges()?;
                assert!(outgoing_edges.contains(&edge_index));
                if (incoming_edges.len() == 1) && (outgoing_edges.len() == 1) {
                    // Because of the mass-balance constraint these two edges must be equal to
                    // one another.
                    self.col_edge_map.add_equal_edges(edge_index, incoming_edges[0]);
                } else {
                    // Otherwise this edge has a more complex relationship with its upstream
                    self.col_edge_map.add_simple_edge(edge_index);
                }
            } else {
                // Other upstream node types mean the edge is added normally
                self.col_edge_map.add_simple_edge(edge_index);
            }
        }

        // Add columns set the columns as x >= 0.0 (i.e. no upper bounds)
        for _ in 0..self.col_edge_map.ncols() {
            self.builder.add_column();
        }

        Ok(())
    }

    /// Create mass balance constraints for each edge
    fn create_mass_balance_constraints(&mut self, network: &Network) {
        for node in network.nodes().deref() {
            // Only link nodes create mass-balance constraints

            if let NodeType::Link = node.node_type() {
                let mut row = RowBuilder::fixed();
                let incoming_edges = node.get_incoming_edges().unwrap();
                let outgoing_edges = node.get_outgoing_edges().unwrap();

                // TODO check for length >= 1

                for edge in incoming_edges {
                    let column = self.col_for_edge(edge);
                    row.add_element(column, 1.0);
                }
                for edge in outgoing_edges {
                    let column = self.col_for_edge(edge);
                    row.add_element(column, -1.0);
                }

                if row.columns.is_empty() {
                    panic!("Row contains no columns!")
                } else if row.columns.len() == 1 {
                    // Skip this row because the edges must be mapped to the same column
                } else {
                    self.builder.add_row(row);
                }
            }
        }
    }

    fn add_node(&mut self, node: &Node, factor: f64, row: &mut RowBuilder) {
        match node.node_type() {
            NodeType::Link => {
                for edge in node.get_outgoing_edges().unwrap() {
                    let column = self.col_for_edge(edge);
                    row.add_element(column, factor);
                }
            }
            NodeType::Input => {
                for edge in node.get_outgoing_edges().unwrap() {
                    let column = self.col_for_edge(edge);
                    row.add_element(column, factor);
                }
            }
            NodeType::Output => {
                for edge in node.get_incoming_edges().unwrap() {
                    let column = self.col_for_edge(edge);
                    row.add_element(column, factor);
                }
            }
            NodeType::Storage => {
                for edge in node.get_incoming_edges().unwrap() {
                    let column = self.col_for_edge(edge);
                    row.add_element(column, factor);
                }
                for edge in node.get_outgoing_edges().unwrap() {
                    let column = self.col_for_edge(edge);
                    row.add_element(column, -factor);
                }
            }
        }
    }

    /// Create node constraints
    ///
    /// One constraint is created per node to enforce any constraints (flow or storage)
    /// that it may define.
    fn create_node_constraints(&mut self, network: &Network) -> Vec<usize> {
        let mut row_ids = Vec::with_capacity(network.nodes().len());

        for node in network.nodes().deref() {
            match node.node_type() {
                NodeType::Input | NodeType::Output | NodeType::Link => {
                    // Only create node constraints for nodes that could become constrained
                    if !node.is_max_flow_unconstrained().unwrap() {
                        // Create empty arrays to store the matrix data
                        let mut row = RowBuilder::upper();
                        self.add_node(node, 1.0, &mut row);

                        let row_id = self.builder.add_row(row.clone()).unwrap();
                        row_ids.push(row_id);
                    }
                }
                NodeType::Storage => {
                    // Storage nodes have a different type of constraint
                    let mut row = RowBuilder::upper();
                    self.add_node(node, 1.0, &mut row);
                    let row_id = self.builder.add_row(row.clone()).unwrap();
                    row_ids.push(row_id);

                    let neg_row = row.clone_negative();
                    let row_id = self.builder.add_row(neg_row).unwrap();
                    row_ids.push(row_id);
                }
            }
        }

        row_ids
    }
}

pub struct ClIpmF32Solver {
    built: Vec<BuiltSolver>,
    ipm: Vec<PathFollowingDirectClSolver<f32>>,
    chunk_size: NonZeroUsize,
    max_iterations: NonZeroUsize,
    queue: ocl::Queue,
}

impl MultiStateSolver for ClIpmF32Solver {
    type Settings = ClIpmSolverSettings;

    fn name() -> &'static str {
        "ipm-ocl"
    }

    fn features() -> &'static [SolverFeatures] {
        &[]
    }

    fn setup(
        network: &Network,
        num_scenarios: usize,
        settings: &Self::Settings,
    ) -> Result<Box<Self>, SolverSetupError> {
        let platform = ocl::Platform::default();
        let device = ocl::Device::first(platform).expect("Failed to get OpenCL device.");
        let context = ocl::Context::builder()
            .platform(platform)
            .devices(device)
            .build()
            .expect("Failed to create OpenCL context.");

        let program =
            f32::get_cl_program(&context, &device, &settings.tolerances()).expect("Failed to create OpenCL program.");
        let queue = ocl::Queue::new(&context, device, None).expect("Failed to create OpenCL queue.");

        let mut built_solvers = Vec::new();
        let mut ipms = Vec::new();

        let num_chunks = settings.num_chunks();
        let chunk_size = NonZeroUsize::new(num_scenarios / num_chunks).unwrap();

        for chunk_scenarios in (0..num_scenarios).collect::<Vec<_>>().chunks(chunk_size.get()) {
            let builder = SolverBuilder::new(chunk_scenarios.len());
            let built = builder.create(network)?;

            let matrix = built.lp.get_full_matrix();
            let num_rows = matrix.row_starts.len() - 1;
            let num_cols = built.lp.num_cols;

            // TODO handle the error better
            let ipm = PathFollowingDirectClSolver::from_data(
                &queue,
                &program,
                num_rows,
                num_cols,
                matrix.row_starts,
                matrix.columns,
                matrix.elements.into_iter().map(|v| v as f32).collect(),
                built.lp.inequality.nrows() as u32,
                chunk_scenarios.len() as u32,
            )
            .expect("Failed to create the OpenCL IPM solver from the given LP data.");

            built_solvers.push(built);
            ipms.push(ipm)
        }

        Ok(Box::new(Self {
            built: built_solvers,
            ipm: ipms,
            chunk_size,
            max_iterations: settings.max_iterations(),
            queue,
        }))
    }

    fn solve(
        &mut self,
        network: &Network,
        timestep: &Timestep,
        states: &mut [State],
    ) -> Result<SolverTimings, SolverSolveError> {
        // TODO complete the timings
        let timings = SolverTimings::default();

        states
            .par_chunks_mut(self.chunk_size.get())
            .zip(&mut self.built)
            .zip(&mut self.ipm)
            .for_each(|((chunk_states, built), ipm)| {
                let mut timings = SolverTimings::default();

                built.update(network, timestep, chunk_states, &mut timings).unwrap();

                let now = Instant::now();
                let row_upper: Vec<_> = built.row_upper().iter().map(|&v| v as f32).collect();
                let col_obj_coef: Vec<_> = built.col_obj_coef().iter().map(|&v| v as f32).collect();

                let solution = ipm
                    .solve(&self.queue, &row_upper, &col_obj_coef, self.max_iterations)
                    .expect("Solve failed with the OpenCL IPM solver.");
                timings.solve = now.elapsed();

                let start_save_solution = Instant::now();
                let num_states = chunk_states.len();
                for (i, state) in chunk_states.iter_mut().enumerate() {
                    let network_state = state.get_mut_network_state();
                    network_state.reset();

                    for edge in network.edges().deref() {
                        let col = built.col_for_edge(&edge.index());
                        let flow = solution[col * num_states + i];
                        network_state.add_flow(edge, timestep, flow as f64).unwrap();
                    }
                }
                timings.save_solution += start_save_solution.elapsed();
            });

        Ok(timings)
    }
}

pub struct ClIpmF64Solver {
    built: Vec<BuiltSolver>,
    ipm: Vec<PathFollowingDirectClSolver<f64>>,
    chunk_size: NonZeroUsize,
    max_iterations: NonZeroUsize,
    queues: Vec<ocl::Queue>,
}

impl MultiStateSolver for ClIpmF64Solver {
    type Settings = ClIpmSolverSettings;

    fn name() -> &'static str {
        "ipm-ocl"
    }

    fn features() -> &'static [SolverFeatures] {
        &[]
    }

    fn setup(
        network: &Network,
        num_scenarios: usize,
        settings: &Self::Settings,
    ) -> Result<Box<Self>, SolverSetupError> {
        let platform = ocl::Platform::default();
        let device = ocl::Device::first(platform).expect("Failed to get OpenCL device.");
        let context = ocl::Context::builder()
            .platform(platform)
            .devices(device)
            .build()
            .expect("Failed to create OpenCL context.");

        let program =
            f64::get_cl_program(&context, &device, &settings.tolerances()).expect("Failed to create OpenCL program.");

        let mut built_solvers = Vec::new();
        let mut ipms = Vec::new();
        let mut queues = Vec::new();

        let num_chunks = settings.num_chunks();
        let chunk_size = NonZeroUsize::new(num_scenarios / num_chunks).unwrap_or(NonZeroUsize::MIN);

        for chunk_scenarios in (0..num_scenarios).collect::<Vec<_>>().chunks(chunk_size.get()) {
            // Create a queue per chunk.
            let queue = ocl::Queue::new(&context, device, None).expect("Failed to create OpenCL queue.");

            let builder = SolverBuilder::new(chunk_scenarios.len());
            let built = builder.create(network)?;

            let matrix = built.lp.get_full_matrix();
            let num_rows = matrix.row_starts.len() - 1;
            let num_cols = built.lp.num_cols;

            // TODO handle the error better
            let ipm = PathFollowingDirectClSolver::from_data(
                &queue,
                &program,
                num_rows,
                num_cols,
                matrix.row_starts,
                matrix.columns,
                matrix.elements,
                built.lp.inequality.nrows() as u32,
                chunk_scenarios.len() as u32,
            )
            .expect("Failed to create the OpenCL IPM solver from the given LP data.");

            built_solvers.push(built);
            ipms.push(ipm);
            queues.push(queue);
        }

        Ok(Box::new(Self {
            built: built_solvers,
            ipm: ipms,
            chunk_size,
            max_iterations: settings.max_iterations(),
            queues,
        }))
    }

    fn solve(
        &mut self,
        network: &Network,
        timestep: &Timestep,
        states: &mut [State],
    ) -> Result<SolverTimings, SolverSolveError> {
        // TODO complete the timings
        let timings = SolverTimings::default();

        states
            .par_chunks_mut(self.chunk_size.get())
            .zip(&mut self.built)
            .zip(&mut self.ipm)
            .zip(&self.queues)
            .for_each(|(((chunk_states, built), ipm), queue)| {
                let mut timings = SolverTimings::default();

                built.update(network, timestep, chunk_states, &mut timings).unwrap();

                let now = Instant::now();

                let solution = ipm
                    .solve(queue, built.row_upper(), built.col_obj_coef(), self.max_iterations)
                    .expect("Solve failed with the OpenCL IPM solver.");
                timings.solve = now.elapsed();

                let start_save_solution = Instant::now();
                let num_states = chunk_states.len();
                for (i, state) in chunk_states.iter_mut().enumerate() {
                    let network_state = state.get_mut_network_state();
                    network_state.reset();

                    for edge in network.edges().deref() {
                        let col = built.col_for_edge(&edge.index());
                        let flow = solution[col * num_states + i];
                        network_state.add_flow(edge, timestep, flow).unwrap();
                    }
                }
                timings.save_solution += start_save_solution.elapsed();
            });

        Ok(timings)
    }
}
