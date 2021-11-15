use crate::metric::Metric;
use crate::node::{Constraint, ConstraintValue, FlowConstraints, Node, NodeMeta};
use crate::state::ParameterState;
use crate::PywrError;
use std::cell::RefCell;
use std::rc::Rc;
use std::slice::Iter;

pub type AggregatedNodeIndex = usize;
pub type AggregatedNodeRef = Rc<RefCell<_AggregatedNode>>;

#[derive(Debug, PartialEq)]
pub struct _AggregatedNode {
    pub meta: NodeMeta<AggregatedNodeIndex>,
    pub flow_constraints: FlowConstraints,
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AggregatedNode(AggregatedNodeRef);

impl AggregatedNode {
    pub fn new(index: &AggregatedNodeIndex, name: &str, nodes: Vec<Node>) -> Self {
        let agg_node = _AggregatedNode {
            meta: NodeMeta::new(index, name),
            flow_constraints: FlowConstraints::new(),
            nodes,
        };
        AggregatedNode(Rc::new(RefCell::new(agg_node)))
    }

    pub fn name(&self) -> String {
        self.0.borrow().meta.name()
    }

    pub fn index(&self) -> AggregatedNodeIndex {
        self.0.borrow().meta.index
    }

    pub fn get_nodes(&self) -> Vec<Node> {
        self.0.borrow().nodes.to_vec()
    }

    pub fn set_min_flow_constraint(&self, value: ConstraintValue) {
        self.0.borrow_mut().flow_constraints.min_flow = value;
    }
    pub fn get_min_flow_constraint(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        self.0.borrow().flow_constraints.get_min_flow(parameter_states)
    }
    pub fn set_max_flow_constraint(&self, value: ConstraintValue) {
        self.0.borrow_mut().flow_constraints.max_flow = value;
    }
    pub fn get_max_flow_constraint(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        self.0.borrow().flow_constraints.get_max_flow(parameter_states)
    }

    /// Set a constraint on a node.
    pub fn set_constraint(&self, value: ConstraintValue, constraint: Constraint) -> Result<(), PywrError> {
        match constraint {
            Constraint::MinFlow => self.set_min_flow_constraint(value),
            Constraint::MaxFlow => self.set_max_flow_constraint(value),
            Constraint::MinAndMaxFlow => {
                self.set_min_flow_constraint(value.clone());
                self.set_max_flow_constraint(value);
            }
            Constraint::MinVolume => return Err(PywrError::StorageConstraintsUndefined),
            Constraint::MaxVolume => return Err(PywrError::StorageConstraintsUndefined),
        }
        Ok(())
    }

    pub fn get_current_min_flow(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        self.0.borrow().flow_constraints.get_min_flow(parameter_states)
    }

    pub fn get_current_max_flow(&self, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        self.0.borrow().flow_constraints.get_max_flow(parameter_states)
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
        self.0
            .borrow()
            .nodes
            .iter()
            .map(|n| Metric::NodeOutFlow(n.index()))
            .collect()
    }
}
