use crate::edge::EdgeIndex;
use crate::node::NodeIndex;
use crate::parameters::ParameterIndex;
use crate::state::NetworkState;
use crate::PywrError;

#[derive(Clone, Debug)]
pub enum Metric {
    NodeInFlow(NodeIndex),
    NodeOutFlow(NodeIndex),
    NodeVolume(NodeIndex),
    EdgeFlow(EdgeIndex),
    ParameterValue(ParameterIndex),
}

impl Metric {
    pub fn get_value(&self, network_state: &NetworkState, parameter_state: &[f64]) -> Result<f64, PywrError> {
        match self {
            Metric::NodeInFlow(idx) => Ok(network_state.get_node_in_flow(*idx)?),
            Metric::NodeOutFlow(idx) => Ok(network_state.get_node_out_flow(*idx)?),
            Metric::NodeVolume(idx) => Ok(network_state.get_node_volume(*idx)?),
            Metric::EdgeFlow(idx) => Ok(network_state.get_edge_flow(*idx)?),
            Metric::ParameterValue(idx) => match parameter_state.get(*idx) {
                Some(v) => Ok(*v),
                None => Err(PywrError::ParameterIndexNotFound),
            },
        }
    }
}
