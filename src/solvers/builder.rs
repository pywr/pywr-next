use crate::edge::EdgeIndex;
use crate::model::Model;
use crate::node::{Node, NodeType};
use crate::parameters::FloatValue;
use crate::solvers::SolverTimings;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use num::Zero;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Debug;
use std::ops::Deref;
use std::time::Instant;

const FMAX: f64 = f64::MAX;

#[derive(Debug)]
enum Bounds {
    Free,
    Lower(f64),
    Upper(f64),
    Double(f64, f64),
    Fixed(f64),
}

struct LpBuilder<I> {
    col_lower: Vec<f64>,
    col_upper: Vec<f64>,
    col_obj_coef: Vec<f64>,
    row_lower: Vec<f64>,
    row_upper: Vec<f64>,
    row_mask: Vec<I>,
    row_starts: Vec<I>,
    columns: Vec<I>,
    elements: Vec<f64>,
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
            row_lower: Vec::new(),
            row_upper: Vec::new(),
            row_mask: Vec::new(),
            row_starts: vec![I::zero()],
            columns: Vec::new(),
            elements: Vec::new(),
        }
    }
}

impl<I> LpBuilder<I>
where
    I: num::PrimInt,
{
    fn add_column(&mut self, obj_coef: f64, bounds: Bounds) {
        let (lb, ub): (f64, f64) = match bounds {
            Bounds::Double(lb, ub) => (lb, ub),
            Bounds::Lower(lb) => (lb, FMAX),
            Bounds::Fixed(b) => (b, b),
            Bounds::Free => (f64::MIN, FMAX),
            Bounds::Upper(ub) => (f64::MIN, ub),
        };

        self.col_lower.push(lb);
        self.col_upper.push(ub);
        self.col_obj_coef.push(obj_coef);
    }

    fn zero_obj_coefficients(&mut self) {
        self.col_obj_coef.fill(0.0);
    }

    fn set_obj_coefficient(&mut self, col: usize, obj_coef: f64) {
        self.col_obj_coef[col] = obj_coef;
    }

    fn add_obj_coefficient(&mut self, col: usize, obj_coef: f64) {
        self.col_obj_coef[col] += obj_coef;
    }

    fn set_row_bounds(&mut self, row: usize, lb: f64, ub: f64) {
        self.row_lower[row] = lb;
        self.row_upper[row] = ub.min(FMAX);
    }

    fn add_row(&mut self, row: RowBuilder<I>) {
        self.row_lower.push(row.lower);
        self.row_upper.push(row.upper);
        self.row_mask.push(I::one());
        let prev_row_start = *self.row_starts.get(&self.row_starts.len() - 1).unwrap();
        self.row_starts
            .push(prev_row_start + I::from(row.columns.len()).unwrap());
        for (column, value) in row.columns {
            self.columns.push(column);
            self.elements.push(value);
        }
    }

    fn num_rows(&self) -> I {
        I::from(self.row_upper.len()).unwrap()
    }
}

#[derive(Debug)]
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

/// A helper struct that contains a mapping from column to model `EdgeIndex`
///
/// A single column may represent one or more edges in the model due to trivial mass-balance
/// constraints making their flows equal. This struct helps with construction of the mapping.
struct ColumnEdgeMap<I> {
    col_to_edges: Vec<Vec<EdgeIndex>>,
    edge_to_col: HashMap<EdgeIndex, I>,
}

impl<I> Default for ColumnEdgeMap<I>
where
    I: num::PrimInt,
{
    fn default() -> Self {
        Self {
            col_to_edges: Vec::default(),
            edge_to_col: HashMap::default(),
        }
    }
}

impl<I> ColumnEdgeMap<I>
where
    I: Copy + num::PrimInt,
{
    /// The number of columns in the map
    fn ncols(&self) -> usize {
        self.col_to_edges.len()
    }

    fn col_for_edge(&self, edge_index: &EdgeIndex) -> I {
        *self
            .edge_to_col
            .get(edge_index)
            .unwrap_or_else(|| panic!("EdgeIndex {:?} not found in column-edge map.", edge_index))
    }

    /// Add a new column to the map
    fn add_simple_edge(&mut self, idx: EdgeIndex) {
        if self.edge_to_col.contains_key(&idx) {
            // TODO maybe this should be an error?
            // panic!("Cannot add the same edge index twice.");
            return;
        }
        // Next column id;
        let col = I::from(self.col_to_edges.len()).unwrap();
        self.col_to_edges.push(vec![idx]);
        self.edge_to_col.insert(idx, col);
    }

    /// Add related columns
    ///
    /// `new_idx` should be
    fn add_equal_edges(&mut self, idx1: EdgeIndex, idx2: EdgeIndex) {
        let idx1_present = self.edge_to_col.contains_key(&idx1);
        let idx2_present = self.edge_to_col.contains_key(&idx2);

        match (idx1_present, idx2_present) {
            (true, true) => {
                // Both are already present; this should not happen?
            }
            (false, true) => {
                // idx1 is not present, but idx2 is
                // Therefore add idx1 to idx2's column;
                let col = self.col_for_edge(&idx2);
                self.col_to_edges[col.to_usize().unwrap()].push(idx1);
                self.edge_to_col.insert(idx1, col);
            }
            (true, false) => {
                // idx1 is present, but idx2 is not
                // Therefore add idx2 to idx1's column;
                let col = self.col_for_edge(&idx1);
                self.col_to_edges[col.to_usize().unwrap()].push(idx2);
                self.edge_to_col.insert(idx2, col);
            }
            (false, false) => {
                // Neither idx is present
                let col = I::from(self.col_to_edges.len()).unwrap();
                self.col_to_edges.push(vec![idx1, idx2]);
                self.edge_to_col.insert(idx1, col);
                self.edge_to_col.insert(idx2, col);
            }
        }
    }
}

pub struct SolverBuilder<I> {
    builder: LpBuilder<I>,
    col_edge_map: ColumnEdgeMap<I>,
    start_node_constraints: Option<I>,
    start_agg_node_constraints: Option<I>,
    start_agg_node_factor_constraints: Option<I>,
    start_virtual_storage_constraints: Option<I>,
}

impl<I> Default for SolverBuilder<I>
where
    I: num::PrimInt,
{
    fn default() -> Self {
        Self {
            builder: LpBuilder::default(),
            col_edge_map: ColumnEdgeMap::default(),
            start_node_constraints: None,
            start_agg_node_constraints: None,
            start_agg_node_factor_constraints: None,
            start_virtual_storage_constraints: None,
        }
    }
}

impl<I> SolverBuilder<I>
where
    I: num::PrimInt + Default + Debug,
{
    pub fn num_rows(&self) -> I {
        I::from(self.builder.row_upper.len()).unwrap()
    }

    pub fn num_cols(&self) -> I {
        I::from(self.builder.col_upper.len()).unwrap()
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

    pub fn create(model: &Model) -> Result<Self, PywrError> {
        let mut builder = Self::default();
        // Create the columns
        builder.create_columns(model)?;

        // Create edge mass balance constraints
        builder.create_mass_balance_constraints(model);
        // Create the nodal constraints
        builder.create_node_constraints(model);
        // Create the aggregated node constraints
        builder.create_aggregated_node_constraints(model);
        // Create the aggregated node factor constraints
        builder.create_aggregated_node_factor_constraints(model);
        // Create virtual storage constraints
        builder.create_virtual_storage_constraints(model);

        // println!("num_rows: {:?}", builder.num_rows());
        // println!("num_cols: {:?}", builder.num_cols());
        // println!("num_non_zero: {:?}", builder.num_non_zero());
        // println!("row_starts: {:?}", builder.row_starts());
        // println!("columns: {:?}", builder.columns());
        // println!("elements: {:?}", builder.elements());

        Ok(builder)
    }

    /// Create the columns in the linear program.
    ///
    /// Typically each edge will have its own column. However, we use the mass-balance information
    /// to collapse edges (and their columns) where they are trivially the same. I.e. if there
    /// is a single incoming edge and outgoing edge at a link node.
    fn create_columns(&mut self, model: &Model) -> Result<(), PywrError> {
        // One column per edge
        let ncols = model.edges.len();
        if ncols < 1 {
            return Err(PywrError::NoEdgesDefined);
        }

        for edge in model.edges.iter() {
            let edge_index = edge.index();
            let from_node = model.get_node(&edge.from_node_index)?;

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
    fn create_mass_balance_constraints(&mut self, model: &Model) {
        for node in model.nodes.deref() {
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

                    self.builder.add_row(row);
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
    /// that it may define.
    fn create_node_constraints(&mut self, model: &Model) {
        let start_row = self.builder.num_rows();

        for node in model.nodes.deref() {
            // Create empty arrays to store the matrix data
            let mut row: RowBuilder<I> = RowBuilder::default();

            self.add_node(node, 1.0, &mut row);

            self.builder.add_row(row);
        }
        self.start_node_constraints = Some(start_row);
    }

    /// Create aggregated node factor constraints
    ///
    /// One constraint is created per node to enforce any factor constraints
    fn create_aggregated_node_factor_constraints(&mut self, model: &Model) {
        let start_row = self.builder.num_rows();

        for agg_node in model.aggregated_nodes.deref() {
            // Only create row for nodes that have factors
            if let Some(factor_pairs) = agg_node.get_norm_factor_pairs() {
                for ((n0, f0), (n1, f1)) in factor_pairs {
                    // Create rows for each node in the aggregated node pair with the first one.

                    let mut row = RowBuilder::default();

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

                    if ff0.is_zero() || ff1.is_zero() {
                        panic!("Aggregated node {:?} contains a zero factor.", agg_node.full_name());
                    }

                    self.add_node(node0, 1.0, &mut row);
                    self.add_node(node1, -ff0 / ff1, &mut row);
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
        let start_row = self.builder.num_rows();

        for agg_node in model.aggregated_nodes.deref() {
            // Create empty arrays to store the matrix data
            let mut row: RowBuilder<I> = RowBuilder::default();

            for node_index in agg_node.get_nodes() {
                // TODO error handling?
                let node = model.nodes.get(&node_index).expect("Node index not found!");
                self.add_node(node, 1.0, &mut row);
            }

            self.builder.add_row(row);
        }
        self.start_agg_node_constraints = Some(start_row);
    }

    /// Create virtual storage node constraints
    ///
    fn create_virtual_storage_constraints(&mut self, model: &Model) {
        let start_row = self.builder.num_rows();

        for virtual_storage in model.virtual_storage_nodes.deref() {
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
                    let node = model.nodes.get(&node_index).expect("Node index not found!");
                    self.add_node(node, factor, &mut row);
                }
                self.builder.add_row(row);
            }
        }
        self.start_virtual_storage_constraints = Some(start_row);
    }

    pub fn update(
        &mut self,
        model: &Model,
        timestep: &Timestep,
        state: &State,
        timings: &mut SolverTimings,
    ) -> Result<(), PywrError> {
        let start_objective_update = Instant::now();
        self.update_edge_objectives(model, state)?;
        timings.update_objective += start_objective_update.elapsed();

        let start_constraint_update = Instant::now();
        self.update_node_constraint_bounds(model, timestep, state)?;
        self.update_aggregated_node_constraint_bounds(model, state)?;
        timings.update_constraints += start_constraint_update.elapsed();

        Ok(())
    }

    /// Update edge objective coefficients
    fn update_edge_objectives(&mut self, model: &Model, state: &State) -> Result<(), PywrError> {
        self.builder.zero_obj_coefficients();
        for edge in model.edges.deref() {
            let obj_coef: f64 = edge.cost(&model.nodes, state)?;
            let col = self.col_for_edge(&edge.index());

            self.builder.add_obj_coefficient(col.to_usize().unwrap(), obj_coef);
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

            self.builder
                .set_row_bounds(start_row.to_usize().unwrap() + *node.index(), lb, ub);
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
            self.builder
                .set_row_bounds(start_row.to_usize().unwrap() + *agg_node.index(), lb, ub);
        }

        Ok(())
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
        builder.add_row(row);
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
        builder.add_row(row);

        // Row2
        let mut row = RowBuilder::default();
        row.add_element(0, 2.0);
        row.add_element(1, 5.0);
        row.add_element(2, 3.0);
        row.set_lower(f64::MIN);
        row.set_upper(15.0);
        builder.add_row(row);

        assert_eq!(builder.row_upper, vec![10.0, 15.0]);
        assert_eq!(builder.row_lower, vec![f64::MIN, f64::MIN]);
        assert_eq!(builder.col_lower, vec![0.0, 0.0, 0.0]);
        assert_eq!(builder.col_upper, vec![f64::MAX, f64::MAX, f64::MAX]);
        assert_eq!(builder.col_obj_coef, vec![-2.0, -3.0, -4.0]);
        assert_eq!(builder.row_starts, vec![0, 3, 6]);
        assert_eq!(builder.columns, vec![0, 1, 2, 0, 1, 2]);
        assert_eq!(builder.elements, vec![3.0, 2.0, 1.0, 2.0, 5.0, 3.0]);
    }
}
