use crate::aggregated_node::AggregatedNodeIndex;
use crate::edge::EdgeIndex;
use crate::network::Network;
use crate::node::{Node, NodeType};
use crate::solvers::col_edge_map::{ColumnEdgeMap, ColumnEdgeMapBuilder};
use crate::solvers::SolverTimings;
use crate::state::{ConstParameterValues, State};
use crate::timestep::Timestep;
use crate::PywrError;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::ops::Deref;
use std::time::Instant;

const FMAX: f64 = f64::MAX;
const FMIN: f64 = f64::MIN;

enum Bounds {
    // Free,
    Lower(f64),
    // Upper(f64),
    // Double(f64, f64),
    // Fixed(f64),
}

/// Sparse form of a linear program.
///
/// This struct is intended to facilitate passing the LP data to a external library. Most
/// libraries accept LP construct in sparse form.
struct Lp<I> {
    col_lower: Vec<f64>,
    col_upper: Vec<f64>,
    col_obj_coef: Vec<f64>,
    row_lower: Vec<f64>,
    row_upper: Vec<f64>,
    row_mask: Vec<I>,
    row_starts: Vec<I>,
    columns: Vec<I>,
    elements: Vec<f64>,

    coefficients_to_update: Vec<(I, I, f64)>,
}

impl<I> Lp<I>
where
    I: num::PrimInt,
{
    /// Zero all objective coefficients.
    fn zero_obj_coefficients(&mut self) {
        self.col_obj_coef.fill(0.0);
    }

    /// Increment the given column's objective coefficient.
    fn add_obj_coefficient(&mut self, col: usize, obj_coef: f64) {
        self.col_obj_coef[col] += obj_coef;
    }

    /// Reset the row bounds to `FMIN` and `FMAX` for all rows with a mask.
    fn reset_row_bounds(&mut self) {
        for ((mask, lb), ub) in self
            .row_mask
            .iter()
            .zip(self.row_lower.iter_mut())
            .zip(self.row_upper.iter_mut())
        {
            if mask == &I::one() {
                *lb = FMIN;
                *ub = FMAX;
            }
        }
    }

    fn reset_coefficients_to_update(&mut self) {
        self.coefficients_to_update.clear();
    }

    /// Apply new bounds to the given. If the bounds are tighter than the current bounds
    /// then the bounds are updated. If the bounds are looser than the current bounds then they
    /// are ignored.
    fn apply_row_bounds(&mut self, row: usize, lb: f64, ub: f64) {
        self.row_lower[row] = self.row_lower[row].max(lb);
        self.row_upper[row] = self.row_upper[row].min(ub);
    }

    fn update_row_coefficients(&mut self, row: I, node: &Node, factor: f64, col_edge_map: &ColumnEdgeMap<I>) {
        match node.node_type() {
            NodeType::Link => {
                for edge in node.get_outgoing_edges().unwrap() {
                    let column = col_edge_map.col_for_edge(edge);
                    self.coefficients_to_update.push((row, column, factor))
                }
            }
            NodeType::Input => {
                for edge in node.get_outgoing_edges().unwrap() {
                    let column = col_edge_map.col_for_edge(edge);
                    self.coefficients_to_update.push((row, column, factor))
                }
            }
            NodeType::Output => {
                for edge in node.get_incoming_edges().unwrap() {
                    let column = col_edge_map.col_for_edge(edge);
                    self.coefficients_to_update.push((row, column, factor))
                }
            }
            NodeType::Storage => {
                for edge in node.get_incoming_edges().unwrap() {
                    let column = col_edge_map.col_for_edge(edge);
                    self.coefficients_to_update.push((row, column, factor))
                }
                for edge in node.get_outgoing_edges().unwrap() {
                    let column = col_edge_map.col_for_edge(edge);
                    self.coefficients_to_update.push((row, column, factor))
                }
            }
        }
    }
}

/// Helper struct for constructing a `LP<I>`
///
/// The builder facilitates constructing a linear programme one row at a time. Rows are divided
/// between variable and fixed types. In the generated `LP<I>` the user is able to modify the
/// variable rows, but not the fixed rows.
struct LpBuilder<I> {
    col_lower: Vec<f64>,
    col_upper: Vec<f64>,
    col_obj_coef: Vec<f64>,
    rows: Vec<RowBuilder<I>>,
    fixed_rows: Vec<RowBuilder<I>>,
}

impl<I> Default for LpBuilder<I>
where
    I: num::PrimInt,
{
    fn default() -> Self {
        Self {
            col_lower: Vec::new(),
            col_upper: Vec::new(),
            col_obj_coef: Vec::new(),
            rows: Vec::new(),
            fixed_rows: Vec::new(),
        }
    }
}

impl<I> LpBuilder<I>
where
    I: num::PrimInt,
{
    fn add_column(&mut self, obj_coef: f64, bounds: Bounds) {
        let (lb, ub): (f64, f64) = match bounds {
            // Bounds::Double(lb, ub) => (lb, ub),
            Bounds::Lower(lb) => (lb, FMAX),
            // Bounds::Fixed(b) => (b, b),
            // Bounds::Free => (f64::MIN, FMAX),
            // Bounds::Upper(ub) => (f64::MIN, ub),
        };

        self.col_lower.push(lb);
        self.col_upper.push(ub);
        self.col_obj_coef.push(obj_coef);
    }

    /// Add a fixed row to the LP.
    ///
    /// This row is always added to the end of the LP, and does not return its row number
    /// because it should not be changed again.
    fn add_fixed_row(&mut self, row: RowBuilder<I>) {
        self.fixed_rows.push(row);
    }

    /// Add a row to the LP or return an existing row number if the same row already exists.
    fn add_variable_row(&mut self, row: RowBuilder<I>) -> I {
        match self.rows.iter().position(|r| r == &row) {
            Some(row_id) => I::from(row_id).unwrap(),
            None => {
                // No row found, add a new one
                let row_id = self.num_variable_rows();
                self.rows.push(row);
                row_id
            }
        }
    }

    /// Return the number of variable rows
    fn num_variable_rows(&self) -> I {
        I::from(self.rows.len()).unwrap()
    }

    /// Build the LP into a final sparse form
    fn build(self) -> Lp<I> {
        let nrows = self.rows.len();
        let mut row_lower = Vec::with_capacity(nrows);
        let mut row_upper = Vec::with_capacity(nrows);
        let mut row_mask = Vec::with_capacity(nrows);
        let mut row_starts = vec![I::zero()];

        // These capacities are not big enough, but difficult to estimate the size
        // `nrows` is the minimum size.
        let mut columns = Vec::with_capacity(nrows);
        let mut elements = Vec::with_capacity(nrows);

        // Construct the sparse matrix from the rows; variable rows first
        // The mask marks the fixed rows as not requiring an update.
        for (rows, mask) in [(self.rows, I::one()), (self.fixed_rows, I::zero())] {
            for row in rows {
                row_lower.push(row.lower);
                row_upper.push(row.upper);
                row_mask.push(mask);
                let prev_row_start = *row_starts.get(&row_starts.len() - 1).unwrap();
                row_starts.push(prev_row_start + I::from(row.columns.len()).unwrap());
                for (column, value) in row.columns {
                    columns.push(column);
                    elements.push(value);
                }
            }
        }

        Lp {
            col_lower: self.col_lower,
            col_upper: self.col_upper,
            col_obj_coef: self.col_obj_coef,
            row_lower,
            row_upper,
            row_mask,
            row_starts,
            columns,
            elements,
            coefficients_to_update: Vec::new(),
        }
    }
}

#[derive(Debug, PartialEq)]
struct RowBuilder<I> {
    lower: f64,
    upper: f64,
    columns: BTreeMap<I, f64>,
}

impl<I> Default for RowBuilder<I> {
    fn default() -> Self {
        Self {
            lower: 0.0,
            upper: FMAX,
            columns: BTreeMap::new(),
        }
    }
}

impl<I> RowBuilder<I>
where
    I: num::PrimInt,
{
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
    pub fn add_element(&mut self, column: I, value: f64) {
        if !value.is_finite() {
            panic!("Row factor is non-finite.");
        }
        *self.columns.entry(column).or_insert(0.0) += value;
    }
}

struct AggNodeFactorRow<I> {
    agg_node_idx: AggregatedNodeIndex,
    // Row index for each node-pair. If `None` the row is fixed and does not need updating.
    row_indices: Vec<Option<I>>,
}

pub struct BuiltSolver<I> {
    builder: Lp<I>,
    col_edge_map: ColumnEdgeMap<I>,
    node_constraints_row_ids: Vec<usize>,
    agg_node_constraint_row_ids: Vec<usize>,
    agg_node_factor_constraint_row_ids: Vec<AggNodeFactorRow<I>>,
    virtual_storage_constraint_row_ids: Vec<usize>,
}

impl<I> BuiltSolver<I>
where
    I: num::PrimInt + Default + Debug + Copy,
{
    pub fn num_cols(&self) -> I {
        I::from(self.builder.col_upper.len()).unwrap()
    }

    pub fn num_rows(&self) -> I {
        I::from(self.builder.row_upper.len()).unwrap()
    }

    pub fn num_non_zero(&self) -> I {
        I::from(self.builder.elements.len()).unwrap()
    }

    pub fn col_lower(&self) -> &[f64] {
        &self.builder.col_lower
    }

    pub fn col_upper(&self) -> &[f64] {
        &self.builder.col_upper
    }

    pub fn col_obj_coef(&self) -> &[f64] {
        &self.builder.col_obj_coef
    }

    pub fn row_lower(&self) -> &[f64] {
        &self.builder.row_lower
    }

    pub fn row_upper(&self) -> &[f64] {
        &self.builder.row_upper
    }

    pub fn row_mask(&self) -> &[I] {
        &self.builder.row_mask
    }

    pub fn row_starts(&self) -> &[I] {
        &self.builder.row_starts
    }

    pub fn columns(&self) -> &[I] {
        &self.builder.columns
    }

    pub fn elements(&self) -> &[f64] {
        &self.builder.elements
    }

    pub fn col_for_edge(&self, edge_index: &EdgeIndex) -> I {
        self.col_edge_map.col_for_edge(edge_index)
    }

    pub fn coefficients_to_update(&self) -> &[(I, I, f64)] {
        &self.builder.coefficients_to_update
    }

    pub fn update(
        &mut self,
        network: &Network,
        timestep: &Timestep,
        state: &State,
        timings: &mut SolverTimings,
    ) -> Result<(), PywrError> {
        let start_objective_update = Instant::now();
        self.update_edge_objectives(network, state)?;
        timings.update_objective += start_objective_update.elapsed();

        let start_constraint_update = Instant::now();
        // Reset the row bounds
        self.builder.reset_row_bounds();
        self.builder.reset_coefficients_to_update();
        // Then these methods will add their bounds
        self.update_node_constraint_bounds(network, timestep, state)?;
        self.update_aggregated_node_factor_constraints(network, state)?;
        self.update_aggregated_node_constraint_bounds(network, state)?;
        self.update_virtual_storage_node_constraint_bounds(network, timestep, state)?;
        timings.update_constraints += start_constraint_update.elapsed();

        Ok(())
    }

    /// Update edge objective coefficients
    fn update_edge_objectives(&mut self, network: &Network, state: &State) -> Result<(), PywrError> {
        self.builder.zero_obj_coefficients();
        for edge in network.edges().deref() {
            let obj_coef: f64 = edge.cost(network.nodes(), network, state)?;
            let col = self.col_for_edge(&edge.index());

            self.builder.add_obj_coefficient(col.to_usize().unwrap(), obj_coef);
        }
        Ok(())
    }

    /// Update node constraints
    fn update_node_constraint_bounds(
        &mut self,
        network: &Network,
        timestep: &Timestep,
        state: &State,
    ) -> Result<(), PywrError> {
        let dt = timestep.days();

        for (row_id, node) in self.node_constraints_row_ids.iter().zip(network.nodes().deref()) {
            let (lb, ub): (f64, f64) = match node.get_current_flow_bounds(network, state) {
                Ok(bnds) => bnds,
                Err(PywrError::FlowConstraintsUndefined) => {
                    // Must be a storage node
                    let (avail, missing) = match node.get_current_available_volume_bounds(state) {
                        Ok(bnds) => bnds,
                        Err(e) => return Err(e),
                    };

                    (-avail / dt, missing / dt)
                }
                Err(e) => return Err(e),
            };

            self.builder.apply_row_bounds(*row_id, lb, ub);
        }

        Ok(())
    }

    fn update_aggregated_node_factor_constraints(&mut self, network: &Network, state: &State) -> Result<(), PywrError> {
        // Update the aggregated node factor constraints which are *not* constant
        for agg_node_row in self.agg_node_factor_constraint_row_ids.iter() {
            let agg_node = network.get_aggregated_node(&agg_node_row.agg_node_idx)?;
            // Only create row for nodes that have factors
            if let Some(node_pairs) = agg_node.get_norm_factor_pairs(network, state) {
                assert_eq!(
                    agg_node_row.row_indices.len(),
                    node_pairs.len(),
                    "Row indices and node pairs do not match!"
                );

                for (node_pair, row_idx) in node_pairs.iter().zip(agg_node_row.row_indices.iter()) {
                    // Only update pairs with a row index (i.e. not fixed)
                    if let Some(row_idx) = row_idx {
                        // Modify the constraint matrix coefficients for the nodes
                        // TODO error handling?
                        let nodes = network.nodes();
                        let node0 = nodes.get(&node_pair.node0.index).expect("Node index not found!");
                        let node1 = nodes.get(&node_pair.node1.index).expect("Node index not found!");

                        self.builder
                            .update_row_coefficients(*row_idx, node0, 1.0, &self.col_edge_map);
                        self.builder
                            .update_row_coefficients(*row_idx, node1, -node_pair.ratio(), &self.col_edge_map);

                        self.builder.apply_row_bounds(row_idx.to_usize().unwrap(), 0.0, 0.0);
                    }
                }
            } else {
                panic!("No factor pairs found for an aggregated node that was setup with factors?!");
            }
        }

        Ok(())
    }

    /// Update aggregated node constraints
    fn update_aggregated_node_constraint_bounds(&mut self, network: &Network, state: &State) -> Result<(), PywrError> {
        for (row_id, agg_node) in self
            .agg_node_constraint_row_ids
            .iter()
            .zip(network.aggregated_nodes().deref())
        {
            let (lb, ub): (f64, f64) = agg_node.get_current_flow_bounds(network, state)?;
            self.builder.apply_row_bounds(*row_id, lb, ub);
        }

        Ok(())
    }

    fn update_virtual_storage_node_constraint_bounds(
        &mut self,
        network: &Network,
        timestep: &Timestep,
        state: &State,
    ) -> Result<(), PywrError> {
        let dt = timestep.days();

        for (row_id, node) in self
            .virtual_storage_constraint_row_ids
            .iter()
            .zip(network.virtual_storage_nodes().deref())
        {
            let (avail, missing) = match node.get_current_available_volume_bounds(state) {
                Ok(bnds) => bnds,
                Err(e) => return Err(e),
            };

            let (lb, ub) = (-avail / dt, missing / dt);
            self.builder.apply_row_bounds(*row_id, lb, ub);
        }

        Ok(())
    }
}

pub struct SolverBuilder<I> {
    builder: LpBuilder<I>,
    col_edge_map: ColumnEdgeMapBuilder<I>,
}

impl<I> Default for SolverBuilder<I>
where
    I: num::PrimInt,
{
    fn default() -> Self {
        Self {
            builder: LpBuilder::default(),
            col_edge_map: ColumnEdgeMapBuilder::default(),
        }
    }
}

impl<I> SolverBuilder<I>
where
    I: num::PrimInt + Default + Debug,
{
    pub fn col_for_edge(&self, edge_index: &EdgeIndex) -> I {
        self.col_edge_map.col_for_edge(edge_index)
    }

    pub fn create(mut self, network: &Network, values: &ConstParameterValues) -> Result<BuiltSolver<I>, PywrError> {
        // Create the columns
        self.create_columns(network)?;

        // Create edge mass balance constraints
        self.create_mass_balance_constraints(network);
        // Create the nodal constraints
        let node_constraints_row_ids = self.create_node_constraints(network);
        // Create the aggregated node constraints
        let agg_node_constraint_row_ids = self.create_aggregated_node_constraints(network);
        // Create the aggregated node factor constraints
        let agg_node_factor_constraint_row_ids = self.create_aggregated_node_factor_constraints(network, values);
        // Create virtual storage constraints
        let virtual_storage_constraint_row_ids = self.create_virtual_storage_constraints(network);

        Ok(BuiltSolver {
            builder: self.builder.build(),
            col_edge_map: self.col_edge_map.build(),
            node_constraints_row_ids,
            agg_node_factor_constraint_row_ids,
            agg_node_constraint_row_ids,
            virtual_storage_constraint_row_ids,
        })
    }

    /// Create the columns in the linear program.
    ///
    /// Typically each edge will have its own column. However, we use the mass-balance information
    /// to collapse edges (and their columns) where they are trivially the same. I.e. if there
    /// is a single incoming edge and outgoing edge at a link node.
    fn create_columns(&mut self, network: &Network) -> Result<(), PywrError> {
        // One column per edge
        let ncols = network.edges().len();
        if ncols < 1 {
            return Err(PywrError::NoEdgesDefined);
        }

        for edge in network.edges().iter() {
            let edge_index = edge.index();
            let from_node = network.get_node(&edge.from_node_index)?;

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
            self.builder.add_column(0.0, Bounds::Lower(0.0));
        }

        Ok(())
    }

    /// Create mass balance constraints for each edge
    fn create_mass_balance_constraints(&mut self, network: &Network) {
        for node in network.nodes().deref() {
            // Only link nodes create mass-balance constraints

            if let NodeType::Link = node.node_type() {
                let mut row: RowBuilder<I> = RowBuilder::default();

                let incoming_edges = node.get_incoming_edges().unwrap();
                let outgoing_edges = node.get_outgoing_edges().unwrap();

                // TODO use Display for the error message
                if incoming_edges.is_empty() {
                    panic!("Node {:?} contains no incoming edges 💥", node.full_name())
                }
                if outgoing_edges.is_empty() {
                    panic!("Node {:?} contains no outgoing edges 💥", node.full_name())
                }

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
                    row.set_upper(0.0);
                    row.set_lower(0.0);

                    self.builder.add_fixed_row(row);
                }
            }
        }
    }

    fn add_node(&mut self, node: &Node, factor: f64, row: &mut RowBuilder<I>) {
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
    /// that it may define. Returns the row_ids associated with each constraint.
    fn create_node_constraints(&mut self, network: &Network) -> Vec<usize> {
        let mut row_ids = Vec::with_capacity(network.nodes().len());

        for node in network.nodes().deref() {
            // Create empty arrays to store the matrix data
            let mut row: RowBuilder<I> = RowBuilder::default();

            self.add_node(node, 1.0, &mut row);

            let row_id = self.builder.add_variable_row(row);
            row_ids.push(row_id.to_usize().unwrap())
        }
        row_ids
    }

    /// Create aggregated node factor constraints
    ///
    /// One constraint is created per node to enforce any factor constraints.
    fn create_aggregated_node_factor_constraints(
        &mut self,
        network: &Network,
        values: &ConstParameterValues,
    ) -> Vec<AggNodeFactorRow<I>> {
        let mut row_ids = Vec::new();

        for agg_node in network.aggregated_nodes().deref() {
            // Only create row for nodes that have factors
            if let Some(node_pairs) = agg_node.get_const_norm_factor_pairs(values) {
                let mut row_indices_for_agg_node = Vec::with_capacity(node_pairs.len());

                for node_pair in node_pairs {
                    // Create rows for each node in the aggregated node pair with the first one.

                    let mut row = RowBuilder::default();

                    // TODO error handling?
                    let nodes = network.nodes();
                    let node0 = nodes.get(&node_pair.node0.index).expect("Node index not found!");
                    let node1 = nodes.get(&node_pair.node1.index).expect("Node index not found!");

                    let ratio = node_pair.ratio();

                    self.add_node(node0, 1.0, &mut row);
                    self.add_node(node1, -ratio.unwrap_or(1.0), &mut row);
                    // Make the row fixed at zero RHS
                    row.set_lower(0.0);
                    row.set_upper(0.0);

                    // Row is fixed if we can compute the ratio now
                    if ratio.is_some() {
                        self.builder.add_fixed_row(row);
                        row_indices_for_agg_node.push(None)
                    } else {
                        // These rows will be updated with the correct ratio later
                        let row_idx = self.builder.add_variable_row(row);
                        row_indices_for_agg_node.push(Some(row_idx));
                    }
                }

                row_ids.push(AggNodeFactorRow {
                    agg_node_idx: agg_node.index(),
                    row_indices: row_indices_for_agg_node,
                })
            }
        }

        row_ids
    }

    /// Create aggregated node constraints
    ///
    /// One constraint is created per node to enforce any constraints (flow or storage)
    /// that it may define. Returns the row ids associated with each aggregated node constraint.
    /// Panics if the model contains aggregated nodes with broken references to nodes.
    fn create_aggregated_node_constraints(&mut self, network: &Network) -> Vec<usize> {
        let mut row_ids = Vec::with_capacity(network.aggregated_nodes().len());

        for agg_node in network.aggregated_nodes().deref() {
            // Create empty arrays to store the matrix data
            let mut row: RowBuilder<I> = RowBuilder::default();

            for node_index in agg_node.get_nodes() {
                // TODO error handling?
                let node = network.nodes().get(&node_index).expect("Node index not found!");
                self.add_node(node, 1.0, &mut row);
            }

            let row_id = self.builder.add_variable_row(row);
            row_ids.push(row_id.to_usize().unwrap())
        }
        row_ids
    }

    /// Create virtual storage node constraints
    ///
    fn create_virtual_storage_constraints(&mut self, network: &Network) -> Vec<usize> {
        let mut row_ids = Vec::with_capacity(network.virtual_storage_nodes().len());

        for virtual_storage in network.virtual_storage_nodes().deref() {
            // Create empty arrays to store the matrix data

            if let Some(nodes) = virtual_storage.get_nodes_with_factors() {
                let mut row: RowBuilder<I> = RowBuilder::default();
                for (node_index, factor) in nodes {
                    if !factor.is_finite() {
                        panic!(
                            "Virtual storage node {:?} contains a non-finite factor.",
                            virtual_storage.full_name()
                        );
                    }
                    let node = network.nodes().get(&node_index).expect("Node index not found!");
                    self.add_node(node, -factor, &mut row);
                }
                let row_id = self.builder.add_variable_row(row);
                row_ids.push(row_id.to_usize().unwrap());
            }
        }
        row_ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_builder_new() {
        let _builder: LpBuilder<i32> = LpBuilder::default();
    }

    #[test]
    fn builder_add_rows() {
        let mut builder: LpBuilder<i32> = LpBuilder::default();
        let mut row = RowBuilder::default();
        row.add_element(0, 1.0);
        row.add_element(1, 1.0);
        row.set_lower(0.0);
        row.set_upper(2.0);
        builder.add_variable_row(row);
    }

    #[test]
    fn builder_solve2() {
        let mut builder = LpBuilder::default();

        builder.add_column(-2.0, Bounds::Lower(0.0));
        builder.add_column(-3.0, Bounds::Lower(0.0));
        builder.add_column(-4.0, Bounds::Lower(0.0));

        // Row1
        let mut row = RowBuilder::default();
        row.add_element(0, 3.0);
        row.add_element(1, 2.0);
        row.add_element(2, 1.0);
        row.set_lower(f64::MIN);
        row.set_upper(10.0);
        builder.add_variable_row(row);

        // Row2
        let mut row = RowBuilder::default();
        row.add_element(0, 2.0);
        row.add_element(1, 5.0);
        row.add_element(2, 3.0);
        row.set_lower(f64::MIN);
        row.set_upper(15.0);
        builder.add_variable_row(row);

        let lp = builder.build();

        assert_eq!(lp.row_upper, vec![10.0, 15.0]);
        assert_eq!(lp.row_lower, vec![f64::MIN, f64::MIN]);
        assert_eq!(lp.col_lower, vec![0.0, 0.0, 0.0]);
        assert_eq!(lp.col_upper, vec![f64::MAX, f64::MAX, f64::MAX]);
        assert_eq!(lp.col_obj_coef, vec![-2.0, -3.0, -4.0]);
        assert_eq!(lp.row_starts, vec![0, 3, 6]);
        assert_eq!(lp.columns, vec![0, 1, 2, 0, 1, 2]);
        assert_eq!(lp.elements, vec![3.0, 2.0, 1.0, 2.0, 5.0, 3.0]);
    }
}
