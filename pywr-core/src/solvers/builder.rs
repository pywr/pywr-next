use crate::aggregated_node::AggregatedNodeIndex;
use crate::edge::EdgeIndex;
use crate::network::Network;
use crate::node::{Node, NodeBounds, NodeIndex, NodeType};
use crate::solvers::col_edge_map::{ColumnEdgeMap, ColumnEdgeMapBuilder};
use crate::solvers::{SolverSetupError, SolverSolveError, SolverTimings};
use crate::state::{ConstParameterValues, State};
use crate::timestep::Timestep;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Debug;
use std::ops::Deref;
use std::time::Instant;

enum Bounds {
    // Free,
    Lower(f64),
    // Upper(f64),
    Double(f64, f64),
    // Fixed(f64),
}

/// Column type
#[derive(Copy, Clone, Debug)]
pub enum ColType {
    Continuous,
    Integer,
}

/// Sparse form of a linear program.
///
/// This struct is intended to facilitate passing the LP data to a external library. Most
/// libraries accept LP construct in sparse form.
struct Lp<I> {
    /// The maximum value for a floating point number to be used as a bound.
    f64_max: f64,
    /// The minimum value for a floating point number to be used as a bound.
    f64_min: f64,
    col_lower: Vec<f64>,
    col_upper: Vec<f64>,
    col_obj_coef: Vec<f64>,
    col_type: Vec<ColType>,
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
                *lb = self.f64_min;
                *ub = self.f64_max;
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
    f64_max: f64,
    f64_min: f64,
    col_lower: Vec<f64>,
    col_upper: Vec<f64>,
    col_obj_coef: Vec<f64>,
    col_type: Vec<ColType>,
    rows: Vec<RowBuilder<I>>,
    fixed_rows: Vec<RowBuilder<I>>,
}

impl<I> LpBuilder<I>
where
    I: num::PrimInt,
{
    fn new(f64_max: f64, f64_min: f64) -> Self {
        Self {
            f64_max,
            f64_min,
            col_lower: Vec::new(),
            col_upper: Vec::new(),
            col_obj_coef: Vec::new(),
            col_type: Vec::new(),
            rows: Vec::new(),
            fixed_rows: Vec::new(),
        }
    }

    fn add_column(&mut self, obj_coef: f64, bounds: Bounds, col_type: ColType) -> I {
        let (lb, ub): (f64, f64) = match bounds {
            Bounds::Double(lb, ub) => (lb, ub),
            Bounds::Lower(lb) => (lb, self.f64_max),
            // Bounds::Fixed(b) => (b, b),
            // Bounds::Free => (f64::MIN, FMAX),
            // Bounds::Upper(ub) => (f64::MIN, ub),
        };

        self.col_lower.push(lb);
        self.col_upper.push(ub);
        self.col_obj_coef.push(obj_coef);
        self.col_type.push(col_type);
        I::from(self.col_lower.len() - 1).unwrap()
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
                row_upper.push(row.upper.unwrap_or(self.f64_max));
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
            f64_max: self.f64_max,
            f64_min: self.f64_min,
            col_lower: self.col_lower,
            col_upper: self.col_upper,
            col_obj_coef: self.col_obj_coef,
            col_type: self.col_type,
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
    upper: Option<f64>,
    columns: BTreeMap<I, f64>,
}

impl<I> Default for RowBuilder<I> {
    fn default() -> Self {
        Self {
            lower: 0.0,
            upper: None,
            columns: BTreeMap::new(),
        }
    }
}

impl<I> RowBuilder<I>
where
    I: num::PrimInt,
{
    pub fn set_upper(&mut self, upper: f64) {
        self.upper = Some(upper);
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

/// The row id types associated with a node's constraints.
#[derive(Copy, Clone)]
struct NodeRowId<I> {
    /// A regular node constraint bounded by lower and upper bounds.
    row_id: I,
    node_idx: NodeIndex,
    row_type: NodeRowType<I>,
}

/// The row id types associated with a node's constraints.
#[derive(Copy, Clone)]
enum NodeRowType<I> {
    /// A regular node constraint bounded by lower and upper bounds.
    Continuous,
    /// A binary node constraint where the upper bound is controlled by a binary variable.
    BinaryUpperBound { bin_col_id: I },
    /// A binary node constraint where the lower bound is controlled by a binary variable.
    BinaryLowerBound { bin_col_id: I },
}

struct AggNodeFactorRow<I> {
    agg_node_idx: AggregatedNodeIndex,
    // Row index for each node-pair. If `None` the row is fixed and does not need updating.
    row_indices: Vec<Option<I>>,
}

pub struct BuiltSolver<I> {
    builder: Lp<I>,
    col_edge_map: ColumnEdgeMap<I>,
    node_constraints_row_ids: Vec<NodeRowId<I>>,
    agg_node_constraint_row_ids: Vec<usize>,
    agg_node_factor_constraint_row_ids: Vec<AggNodeFactorRow<I>>,
    virtual_storage_constraint_row_ids: Vec<usize>,
}

impl<I> BuiltSolver<I>
where
    I: num::PrimInt + Default + Debug + Copy,
{
    #[allow(dead_code)]
    pub fn num_cols(&self) -> I {
        I::from(self.builder.col_upper.len()).unwrap()
    }

    #[allow(dead_code)]
    pub fn num_rows(&self) -> I {
        I::from(self.builder.row_upper.len()).unwrap()
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub fn col_type(&self) -> &[ColType] {
        &self.builder.col_type
    }

    pub fn row_lower(&self) -> &[f64] {
        &self.builder.row_lower
    }

    pub fn row_upper(&self) -> &[f64] {
        &self.builder.row_upper
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub fn coefficients_to_update(&self) -> &[(I, I, f64)] {
        &self.builder.coefficients_to_update
    }

    /// Apply the updated coefficients to the sparse matrix.
    #[allow(dead_code)]
    pub fn apply_updated_coefficients(&mut self) {
        for (row, col, value) in self.builder.coefficients_to_update.drain(..) {
            let row = row.to_usize().unwrap();
            let col = col.to_usize().unwrap();

            // Find the position of the column in the sparse matrix
            let start = self.builder.row_starts[row].to_usize().unwrap();
            let end = self.builder.row_starts[row + 1].to_usize().unwrap();
            if let Some(pos) = self.builder.columns[start..end]
                .iter()
                .position(|&c| c.to_usize().unwrap() == col)
            {
                self.builder.elements[start + pos] = value;
            } else {
                panic!("Column not found in row when applying updated coefficients.");
            }
        }
    }

    pub fn update(
        &mut self,
        network: &Network,
        timestep: &Timestep,
        state: &State,
        timings: &mut SolverTimings,
    ) -> Result<(), SolverSolveError> {
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
    fn update_edge_objectives(&mut self, network: &Network, state: &State) -> Result<(), SolverSolveError> {
        self.builder.zero_obj_coefficients();
        for edge in network.edges().deref() {
            let obj_coef: f64 = edge.cost(network.nodes(), network, state).map_err(|source| {
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
    ) -> Result<(), SolverSolveError> {
        let dt = timestep.days();

        for row in self.node_constraints_row_ids.iter() {
            let node = network
                .get_node(&row.node_idx)
                .ok_or(SolverSolveError::NodeIndexNotFound(row.node_idx))?;

            let (lb, ub): (f64, f64) =
                match node
                    .get_bounds(network, state)
                    .map_err(|source| SolverSolveError::NodeError {
                        name: node.name().to_string(),
                        sub_name: node.sub_name().map(|s| s.to_string()),
                        source,
                    })? {
                    NodeBounds::Flow(bounds) => (bounds.min_flow, bounds.max_flow),
                    NodeBounds::Volume(bounds) => (-bounds.available / dt, bounds.missing / dt),
                };

            match row.row_type {
                NodeRowType::Continuous => {
                    // Regular node constraint
                    self.builder.apply_row_bounds(row.row_id.to_usize().unwrap(), lb, ub);
                }
                NodeRowType::BinaryUpperBound { bin_col_id } => {
                    // Update the coefficients for the binary column to be the upper bound
                    // This row has the correct bounds already, so we just update the coefficients
                    self.builder.coefficients_to_update.push((row.row_id, bin_col_id, ub));
                }
                NodeRowType::BinaryLowerBound { bin_col_id } => {
                    // Update the coefficients for the binary column to be the lower bound
                    // This row has the correct bounds already, so we just update the coefficients
                    self.builder.coefficients_to_update.push((row.row_id, bin_col_id, -lb));
                }
            }
        }

        Ok(())
    }

    fn update_aggregated_node_factor_constraints(
        &mut self,
        network: &Network,
        state: &State,
    ) -> Result<(), SolverSolveError> {
        // Update the aggregated node factor constraints which are *not* constant
        for agg_node_row in self.agg_node_factor_constraint_row_ids.iter() {
            let agg_node = network
                .get_aggregated_node(&agg_node_row.agg_node_idx)
                .ok_or(SolverSolveError::AggregatedNodeIndexNotFound(agg_node_row.agg_node_idx))?;

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
                        for node0_idx in node_pair.node0_indices() {
                            let node0 = nodes.get(node0_idx).expect("Node index not found!");
                            self.builder.update_row_coefficients(
                                *row_idx,
                                node0,
                                node_pair.node0_factor(),
                                &self.col_edge_map,
                            );
                        }

                        for node1_idx in node_pair.node1_indices() {
                            let node1 = nodes.get(node1_idx).expect("Node index not found!");
                            self.builder.update_row_coefficients(
                                *row_idx,
                                node1,
                                node_pair.node1_factor(),
                                &self.col_edge_map,
                            );
                        }

                        // Apply the bounds to the row
                        self.builder
                            .apply_row_bounds(row_idx.to_usize().unwrap(), node_pair.rhs(), node_pair.rhs());
                    }
                }
            } else {
                panic!("No factor pairs found for an aggregated node that was setup with factors?!");
            }
        }

        Ok(())
    }

    /// Update aggregated node constraints
    fn update_aggregated_node_constraint_bounds(
        &mut self,
        network: &Network,
        state: &State,
    ) -> Result<(), SolverSolveError> {
        for (row_id, agg_node) in self
            .agg_node_constraint_row_ids
            .iter()
            .zip(network.aggregated_nodes().deref())
        {
            let (lb, ub): (f64, f64) = agg_node.get_current_flow_bounds(network, state).map_err(|e| {
                SolverSolveError::AggregatedNodeError {
                    name: agg_node.name().to_string(),
                    sub_name: agg_node.sub_name().map(|s| s.to_string()),
                    source: e,
                }
            })?;
            self.builder.apply_row_bounds(*row_id, lb, ub);
        }

        Ok(())
    }

    fn update_virtual_storage_node_constraint_bounds(
        &mut self,
        network: &Network,
        timestep: &Timestep,
        state: &State,
    ) -> Result<(), SolverSolveError> {
        let dt = timestep.days();

        for (row_id, node) in self
            .virtual_storage_constraint_row_ids
            .iter()
            .zip(network.virtual_storage_nodes().deref())
        {
            let (lb, ub) = if node.is_active(timestep) {
                let (avail, missing) = node.get_available_volume_bounds(state)?;
                (-avail / dt, missing / dt)
            } else {
                // Node is inactive, so set bounds to be unbounded
                (self.builder.f64_min, self.builder.f64_max)
            };

            self.builder.apply_row_bounds(*row_id, lb, ub);
        }

        Ok(())
    }
}

pub struct SolverBuilder<I> {
    builder: LpBuilder<I>,
    col_edge_map: ColumnEdgeMapBuilder<I>,
    node_bin_col_map: HashMap<NodeIndex, Vec<I>>,
    node_set_bin_col_map: HashMap<Vec<NodeIndex>, I>,
}

impl<I> SolverBuilder<I>
where
    I: num::PrimInt + Default + Debug,
{
    pub fn new(f64_max: f64, f64_min: f64) -> Self {
        Self {
            builder: LpBuilder::new(f64_max, f64_min),
            col_edge_map: ColumnEdgeMapBuilder::default(),
            node_bin_col_map: HashMap::new(),
            node_set_bin_col_map: HashMap::new(),
        }
    }

    pub fn col_for_edge(&self, edge_index: &EdgeIndex) -> I {
        self.col_edge_map.col_for_edge(edge_index)
    }

    pub fn create(
        mut self,
        network: &Network,
        values: &ConstParameterValues,
    ) -> Result<BuiltSolver<I>, SolverSetupError> {
        // Create the columns
        self.create_columns(network)?;

        // Create edge mass balance constraints
        self.create_mass_balance_constraints(network);
        // Create the nodal constraints
        let node_constraints_row_ids = self.create_node_constraints(network, values)?;
        // Create the aggregated node constraints
        let agg_node_constraint_row_ids = self.create_aggregated_node_constraints(network);
        // Create the aggregated node factor constraints
        let agg_node_factor_constraint_row_ids = self.create_aggregated_node_factor_constraints(network, values);
        // Create virtual storage constraints
        let virtual_storage_constraint_row_ids = self.create_virtual_storage_constraints(network);
        // Create mutual exclusivity constraints
        self.create_mutual_exclusivity_constraints(network);

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
            self.builder.add_column(0.0, Bounds::Lower(0.0), ColType::Continuous);
        }

        // Determine the set of nodes that are in one or more mutual exclusivity constraints
        // We only want to create one binary variable for each unique set of nodes. For the
        // majority of cases the nodes will only be in one set. However, if a node is in different
        // sets then we need to create separate binary variables and associated them with that node.
        let mut node_sets_in_a_mutual_exclusivity = HashSet::new();
        for agg_node in network.aggregated_nodes().deref() {
            if agg_node.has_exclusivity() {
                for node_set in agg_node.iter_nodes() {
                    node_sets_in_a_mutual_exclusivity.insert(node_set);
                }
            }
        }

        // Add any binary columns associated with each set of nodes
        for node_set in node_sets_in_a_mutual_exclusivity.into_iter() {
            let col_id = self.builder.add_column(0.0, Bounds::Double(0.0, 1.0), ColType::Integer);
            for node_idx in node_set.iter() {
                self.node_bin_col_map.entry(*node_idx).or_default().push(col_id);
            }

            self.node_set_bin_col_map.insert(node_set.to_vec(), col_id);
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

    fn add_node(&self, node: &Node, factor: f64, row: &mut RowBuilder<I>) {
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
    ///
    ///
    /// If the node has binary variables associated with it then two rows are created
    /// to enforce the upper and lower bounds of the binary variable. The lower bound row is only
    /// created if the `min_flow` is not zero or is a non-constant value. These rows enforce
    /// the following inequalities:
    /// - `binary_variable * max_flow >= flow`
    /// - `binary_variable * min_flow <= flow`
    ///
    fn create_node_constraints(
        &mut self,
        network: &Network,
        values: &ConstParameterValues,
    ) -> Result<Vec<NodeRowId<I>>, SolverSetupError> {
        let mut row_ids = Vec::with_capacity(network.nodes().len());

        for node in network.nodes().deref() {
            // Get the node's flow bounds if they are constants
            // Storage nodes cannot have constant bounds
            let bounds = match node.get_const_bounds(values)? {
                Some(NodeBounds::Flow(bounds)) => Some(bounds),
                _ => None,
            };

            // If there are binary variables associated with this node, then we need to add a row
            // that enforces each binary variable's constraints
            if let Some(cols) = self.node_bin_col_map.get(&node.index()) {
                for col in cols {
                    // Create separate rows for upper and lower bound constraints.
                    let mut row_ub: RowBuilder<I> = RowBuilder::default();
                    let mut row_lb: RowBuilder<I> = RowBuilder::default();

                    self.add_node(node, -1.0, &mut row_ub);
                    self.add_node(node, 1.0, &mut row_lb);

                    match bounds {
                        Some(bounds) => {
                            // If the bounds are constant then the binary variable is used to control the upper bound
                            row_ub.add_element(*col, bounds.max_flow.min(1e6));
                            row_ub.set_lower(0.0);
                            row_ub.set_upper(self.builder.f64_max);
                            self.builder.add_fixed_row(row_ub);

                            if bounds.min_flow != 0.0 {
                                row_lb.add_element(*col, -bounds.min_flow.max(1e-6));
                                row_lb.set_lower(0.0);
                                row_lb.set_upper(self.builder.f64_max);

                                self.builder.add_fixed_row(row_lb);
                            }
                        }
                        None => {
                            // If the bounds are not constant then the binary variable coefficient is updated later
                            // Use a placeholder of 1.0 and -1.0 for now
                            row_ub.add_element(*col, 1.0);
                            row_lb.add_element(*col, -1.0);

                            let row_id = self.builder.add_variable_row(row_ub);
                            let row_type = NodeRowType::BinaryUpperBound { bin_col_id: *col };

                            row_ids.push(NodeRowId {
                                row_id,
                                node_idx: node.index(),
                                row_type,
                            });

                            // We do not know the bounds yet, so we have to assume there is a possibility
                            // of a non-zero lower bound.
                            let row_id = self.builder.add_variable_row(row_lb);
                            let row_type = NodeRowType::BinaryLowerBound { bin_col_id: *col };

                            row_ids.push(NodeRowId {
                                row_id,
                                node_idx: node.index(),
                                row_type,
                            });
                        }
                    }
                }
            } else {
                let mut row: RowBuilder<I> = RowBuilder::default();
                self.add_node(node, 1.0, &mut row);
                let mut is_fixed = false;

                // Apply the bounds if they are constant; otherwise the bounds are updated later
                if let Some(bounds) = bounds {
                    row.set_lower(bounds.min_flow);
                    row.set_upper(bounds.max_flow);
                    is_fixed = true;
                }

                if is_fixed {
                    self.builder.add_fixed_row(row);
                } else {
                    let row_id = self.builder.add_variable_row(row);

                    row_ids.push(NodeRowId {
                        row_id,
                        node_idx: node.index(),
                        row_type: NodeRowType::Continuous,
                    });
                }
            }
        }
        Ok(row_ids)
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

                    let f0 = node_pair.node0_factor();

                    for node0_idx in node_pair.node0_indices() {
                        let node0 = nodes.get(node0_idx).expect("Node index not found!");
                        self.add_node(node0, f0.unwrap_or(1.0), &mut row);
                    }

                    let f1 = node_pair.node1_factor();

                    for node1_idx in node_pair.node1_indices() {
                        let node1 = nodes.get(node1_idx).expect("Node index not found!");
                        self.add_node(node1, f1.unwrap_or(1.0), &mut row);
                    }

                    // Make the row fixed at RHS
                    let rhs = node_pair.rhs();

                    row.set_lower(rhs);
                    row.set_upper(rhs);

                    // Row is fixed if we can compute the ratio now
                    if f0.is_some() && f1.is_some() {
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

            for node_indices in agg_node.iter_nodes() {
                // TODO error handling?
                for node_idx in node_indices {
                    let node = network.nodes().get(node_idx).expect("Node index not found!");
                    self.add_node(node, 1.0, &mut row);
                }
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

            let mut row: RowBuilder<I> = RowBuilder::default();
            for (node_index, factor) in virtual_storage.iter_nodes_with_factors() {
                if !factor.is_finite() {
                    panic!(
                        "Virtual storage node {:?} contains a non-finite factor.",
                        virtual_storage.full_name()
                    );
                }
                let node = network.nodes().get(node_index).expect("Node index not found!");
                self.add_node(node, -factor, &mut row);
            }
            let row_id = self.builder.add_variable_row(row);
            row_ids.push(row_id.to_usize().unwrap());
        }
        row_ids
    }

    /// Create mutual exclusivity constraints
    fn create_mutual_exclusivity_constraints(&mut self, network: &Network) {
        for agg_node in network.aggregated_nodes().iter() {
            if let Some(exclusivity) = agg_node.get_exclusivity() {
                let mut row = RowBuilder::default();
                for node_index in agg_node.iter_nodes() {
                    let bin_col = self
                        .node_set_bin_col_map
                        .get(node_index)
                        .expect("Binary column not found for Node in mutual exclusivity constraint!");

                    row.add_element(*bin_col, 1.0);
                }
                row.set_upper(exclusivity.max_active() as f64);
                row.set_lower(exclusivity.min_active() as f64);

                self.builder.add_fixed_row(row);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_builder_new() {
        let _builder: LpBuilder<i32> = LpBuilder::new(f64::INFINITY, f64::NEG_INFINITY);
    }

    #[test]
    fn builder_add_rows() {
        let mut builder: LpBuilder<i32> = LpBuilder::new(f64::INFINITY, f64::NEG_INFINITY);
        let mut row = RowBuilder::default();
        row.add_element(0, 1.0);
        row.add_element(1, 1.0);
        row.set_lower(0.0);
        row.set_upper(2.0);
        builder.add_variable_row(row);
    }

    #[test]
    fn builder_solve2() {
        let mut builder = LpBuilder::new(f64::MAX, f64::MIN);

        builder.add_column(-2.0, Bounds::Lower(0.0), ColType::Continuous);
        builder.add_column(-3.0, Bounds::Lower(0.0), ColType::Continuous);
        builder.add_column(-4.0, Bounds::Lower(0.0), ColType::Continuous);

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
