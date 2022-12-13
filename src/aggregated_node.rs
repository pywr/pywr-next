use crate::metric::Metric;
use crate::node::{Constraint, ConstraintValue, FlowConstraints, NodeMeta};
use crate::state::ParameterState;
use crate::{NodeIndex, PywrError};
use std::ops::{Deref, DerefMut};

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct AggregatedNodeIndex(usize);

impl Deref for AggregatedNodeIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Default)]
pub struct AggregatedNodeVec {
    nodes: Vec<AggregatedNode>,
}

impl Deref for AggregatedNodeVec {
    type Target = Vec<AggregatedNode>;

    fn deref(&self) -> &Self::Target {
        &self.nodes
    }
}

impl DerefMut for AggregatedNodeVec {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.nodes
    }
}

impl AggregatedNodeVec {
    pub fn get(&self, index: &AggregatedNodeIndex) -> Result<&AggregatedNode, PywrError> {
        self.nodes.get(index.0).ok_or(PywrError::NodeIndexNotFound)
    }

    pub fn get_mut(&mut self, index: &AggregatedNodeIndex) -> Result<&mut AggregatedNode, PywrError> {
        self.nodes.get_mut(index.0).ok_or(PywrError::NodeIndexNotFound)
    }

    pub fn push_new(&mut self, name: &str, sub_name: Option<&str>, nodes: Vec<NodeIndex>) -> AggregatedNodeIndex {
        let node_index = AggregatedNodeIndex(self.nodes.len());
        let node = AggregatedNode::new(&node_index, name, sub_name, nodes);
        self.nodes.push(node);
        node_index
    }
}

#[derive(Debug, PartialEq)]
pub struct AggregatedNode {
    pub meta: NodeMeta<AggregatedNodeIndex>,
    pub flow_constraints: FlowConstraints,
    pub nodes: Vec<NodeIndex>,
}

impl AggregatedNode {
    pub fn new(index: &AggregatedNodeIndex, name: &str, sub_name: Option<&str>, nodes: Vec<NodeIndex>) -> Self {
        Self {
            meta: NodeMeta::new(index, name, sub_name),
            flow_constraints: FlowConstraints::new(),
            nodes,
        }
    }

    pub fn name(&self) -> &str {
        self.meta.name()
    }

    /// Get a node's sub_name
    pub fn sub_name(&self) -> Option<&str> {
        self.meta.sub_name()
    }

    /// Get a node's full name
    pub fn full_name(&self) -> (&str, Option<&str>) {
        self.meta.full_name()
    }

    pub fn index(&self) -> AggregatedNodeIndex {
        *self.meta.index()
    }

    pub fn get_nodes(&self) -> Vec<NodeIndex> {
        self.nodes.to_vec()
    }

    pub fn set_min_flow_constraint(&mut self, value: ConstraintValue) {
        self.flow_constraints.min_flow = value;
    }
    pub fn get_min_flow_constraint(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        self.flow_constraints.get_min_flow(parameter_states)
    }
    pub fn set_max_flow_constraint(&mut self, value: ConstraintValue) {
        self.flow_constraints.max_flow = value;
    }
    pub fn get_max_flow_constraint(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        self.flow_constraints.get_max_flow(parameter_states)
    }

    /// Set a constraint on a node.
    pub fn set_constraint(&mut self, value: ConstraintValue, constraint: Constraint) -> Result<(), PywrError> {
        match constraint {
            Constraint::MinFlow => self.set_min_flow_constraint(value),
            Constraint::MaxFlow => self.set_max_flow_constraint(value),
            Constraint::MinAndMaxFlow => {
                self.set_min_flow_constraint(value);
                self.set_max_flow_constraint(value);
            }
            Constraint::MinVolume => return Err(PywrError::StorageConstraintsUndefined),
            Constraint::MaxVolume => return Err(PywrError::StorageConstraintsUndefined),
        }
        Ok(())
    }

    pub fn get_current_min_flow(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        self.flow_constraints.get_min_flow(parameter_states)
    }

    pub fn get_current_max_flow(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        self.flow_constraints.get_max_flow(parameter_states)
    }

    pub fn get_current_flow_bounds(&self, parameter_states: &ParameterState) -> Result<(f64, f64), PywrError> {
        match (
            self.get_current_min_flow(parameter_states),
            self.get_current_max_flow(parameter_states),
        ) {
            (Ok(min_flow), Ok(max_flow)) => Ok((min_flow, max_flow)),
            _ => Err(PywrError::FlowConstraintsUndefined),
        }
    }

    pub fn default_metric(&self) -> Vec<Metric> {
        self.nodes.iter().map(|n| Metric::NodeOutFlow(*n)).collect()
    }
}
