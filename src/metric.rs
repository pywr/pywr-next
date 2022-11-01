use crate::edge::EdgeIndex;
use crate::model::Model;
use crate::node::NodeIndex;
use crate::parameters::ParameterIndex;
use crate::state::{NetworkState, ParameterState};
use crate::virtual_storage::VirtualStorageIndex;
use crate::PywrError;

#[derive(Clone, Debug)]
pub enum Metric {
    NodeInFlow(NodeIndex),
    NodeOutFlow(NodeIndex),
    NodeVolume(NodeIndex),
    NodeProportionalVolume(NodeIndex),
    EdgeFlow(EdgeIndex),
    ParameterValue(ParameterIndex),
    VirtualStorageProportionalVolume(VirtualStorageIndex),
    Constant(f64),
}

impl Metric {
    pub fn get_value(
        &self,
        model: &Model,
        network_state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<f64, PywrError> {
        match self {
            Metric::NodeInFlow(idx) => Ok(network_state.get_node_in_flow(idx)?),
            Metric::NodeOutFlow(idx) => Ok(network_state.get_node_out_flow(idx)?),
            Metric::NodeVolume(idx) => Ok(network_state.get_node_volume(idx)?),
            Metric::NodeProportionalVolume(idx) => {
                let volume = network_state.get_node_volume(idx)?;
                let node = model.nodes.get(idx).map_err(|_| PywrError::NodeIndexNotFound)?;
                let max_volume = node.get_current_max_volume()?;

                // TODO handle divide by zero (is it full or empty?)
                Ok(volume / max_volume)
            }
            Metric::EdgeFlow(idx) => Ok(network_state.get_edge_flow(idx)?),
            Metric::ParameterValue(idx) => Ok(parameter_state.get_value(*idx)?),
            Metric::VirtualStorageProportionalVolume(_idx) => Ok(1.0),
            Metric::Constant(v) => Ok(*v),
        }
    }
}
