use crate::model::Model;
use crate::node::{Node, NodeIndex};
use crate::{ParameterState, PywrError};

pub(crate) type EdgeIndex = usize;

#[derive(Debug)]
pub struct Edge {
    pub(crate) index: EdgeIndex,
    pub(crate) from_node_index: NodeIndex,
    pub(crate) to_node_index: NodeIndex,
}

impl Edge {
    pub(crate) fn new(index: &EdgeIndex, from_node_index: &NodeIndex, to_node_index: &NodeIndex) -> Self {
        Self {
            index: index.clone(),
            from_node_index: from_node_index.clone(),
            to_node_index: to_node_index.clone(),
        }
    }

    pub(crate) fn cost(&self, model: &Model, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        let from_node = match model.nodes.get(self.from_node_index) {
            Some(n) => n,
            None => return Err(PywrError::NodeIndexNotFound),
        };

        let to_node = match model.nodes.get(self.to_node_index) {
            Some(n) => n,
            None => return Err(PywrError::NodeIndexNotFound),
        };

        let from_cost = match from_node {
            Node::Input(n) => match n.cost {
                Some(cost_idx) => match parameter_states.get(cost_idx) {
                    Some(s) => *s,
                    None => return Err(PywrError::ParameterIndexNotFound),
                },
                None => 0.0,
            },
            Node::Link(n) => match n.cost {
                Some(cost_idx) => match parameter_states.get(cost_idx) {
                    Some(s) => *s,
                    None => return Err(PywrError::ParameterIndexNotFound),
                },
                None => 0.0,
            },
            Node::Output(n) => match n.cost {
                Some(cost_idx) => match parameter_states.get(cost_idx) {
                    Some(s) => s / 2.0,
                    None => return Err(PywrError::ParameterIndexNotFound),
                },
                None => 0.0,
            },
            Node::Storage(n) => {
                match n.cost {
                    // Storage provides -ve cost for outgoing edges (i.e. if the storage node is
                    // the "from" node.
                    Some(cost_idx) => match parameter_states.get(cost_idx) {
                        Some(s) => -s,
                        None => return Err(PywrError::ParameterIndexNotFound),
                    },
                    None => 0.0,
                }
            }
        };

        let to_cost = match to_node {
            Node::Input(n) => match n.cost {
                Some(cost_idx) => match parameter_states.get(cost_idx) {
                    Some(s) => *s,
                    None => return Err(PywrError::ParameterIndexNotFound),
                },
                None => 0.0,
            },
            Node::Link(n) => match n.cost {
                Some(cost_idx) => match parameter_states.get(cost_idx) {
                    Some(s) => s / 2.0,
                    None => return Err(PywrError::ParameterIndexNotFound),
                },
                None => 0.0,
            },
            Node::Output(n) => match n.cost {
                Some(cost_idx) => match parameter_states.get(cost_idx) {
                    Some(s) => *s,
                    None => return Err(PywrError::ParameterIndexNotFound),
                },
                None => 0.0,
            },
            Node::Storage(n) => {
                match n.cost {
                    // Storage provides +ve cost for incoming edges (i.e. if the storage node is
                    // the "to" node.
                    Some(cost_idx) => match parameter_states.get(cost_idx) {
                        Some(s) => *s,
                        None => return Err(PywrError::ParameterIndexNotFound),
                    },
                    None => 0.0,
                }
            }
        };

        Ok(from_cost + to_cost)
    }
}
