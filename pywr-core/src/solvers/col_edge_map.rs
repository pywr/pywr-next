use crate::edge::EdgeIndex;
use std::collections::BTreeMap;
use std::ops::Deref;

pub struct ColumnEdgeMap<I> {
    edge_to_col: Vec<I>,
}

impl<I> ColumnEdgeMap<I>
where
    I: Copy + num::PrimInt,
{
    pub fn col_for_edge(&self, edge_index: &EdgeIndex) -> I {
        *self
            .edge_to_col
            .get(*edge_index.deref())
            .unwrap_or_else(|| panic!("EdgeIndex {edge_index:?} not found in column-edge map."))
    }
}

/// A helper struct that contains a mapping from column to model `EdgeIndex`
///
/// A single column may represent one or more edges in the model due to trivial mass-balance
/// constraints making their flows equal. This struct helps with construction of the mapping.
pub struct ColumnEdgeMapBuilder<I> {
    col_to_edges: Vec<Vec<EdgeIndex>>,
    edge_to_col: BTreeMap<EdgeIndex, I>,
}

impl<I> Default for ColumnEdgeMapBuilder<I>
where
    I: num::PrimInt,
{
    fn default() -> Self {
        Self {
            col_to_edges: Vec::default(),
            edge_to_col: BTreeMap::default(),
        }
    }
}

impl<I> ColumnEdgeMapBuilder<I>
where
    I: Copy + num::PrimInt,
{
    pub fn build(self) -> ColumnEdgeMap<I> {
        // Convert the hashmap to vector
        // There should be an entry for every index
        assert_eq!(
            *self.edge_to_col.keys().last().unwrap().deref(),
            self.edge_to_col.len() - 1
        );

        ColumnEdgeMap {
            edge_to_col: self.edge_to_col.into_values().collect(),
        }
    }

    /// The number of columns in the map
    pub fn ncols(&self) -> usize {
        self.col_to_edges.len()
    }

    pub fn col_for_edge(&self, edge_index: &EdgeIndex) -> I {
        *self
            .edge_to_col
            .get(edge_index)
            .unwrap_or_else(|| panic!("EdgeIndex {edge_index:?} not found in column-edge map."))
    }

    /// Add a new column to the map
    pub fn add_simple_edge(&mut self, idx: EdgeIndex) {
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
    pub fn add_equal_edges(&mut self, idx1: EdgeIndex, idx2: EdgeIndex) {
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
