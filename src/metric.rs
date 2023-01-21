use crate::aggregated_storage_node::AggregatedStorageNodeIndex;
use crate::edge::EdgeIndex;
use crate::model::Model;
use crate::node::NodeIndex;
use crate::parameters::ParameterIndex;
use crate::state::State;
use crate::virtual_storage::VirtualStorageIndex;
use crate::PywrError;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Metric {
    NodeInFlow(NodeIndex),
    NodeOutFlow(NodeIndex),
    NodeVolume(NodeIndex),
    NodeProportionalVolume(NodeIndex),
    AggregatedNodeVolume(AggregatedStorageNodeIndex),
    AggregatedNodeProportionalVolume(AggregatedStorageNodeIndex),
    EdgeFlow(EdgeIndex),
    ParameterValue(ParameterIndex),
    VirtualStorageVolume(VirtualStorageIndex),
    VirtualStorageProportionalVolume(VirtualStorageIndex),
    Constant(f64),
}

impl Metric {
    pub fn get_value(&self, model: &Model, state: &State) -> Result<f64, PywrError> {
        match self {
            Metric::NodeInFlow(idx) => Ok(state.get_network_state().get_node_in_flow(idx)?),
            Metric::NodeOutFlow(idx) => Ok(state.get_network_state().get_node_out_flow(idx)?),
            Metric::NodeVolume(idx) => Ok(state.get_network_state().get_node_volume(idx)?),
            Metric::NodeProportionalVolume(idx) => {
                let max_volume = model.get_node(idx)?.get_current_max_volume(model, state)?;
                Ok(state
                    .get_network_state()
                    .get_node_proportional_volume(idx, max_volume)?)
            }
            Metric::EdgeFlow(idx) => Ok(state.get_network_state().get_edge_flow(idx)?),
            Metric::ParameterValue(idx) => Ok(state.get_parameter_value(*idx)?),
            Metric::VirtualStorageVolume(idx) => Ok(state.get_network_state().get_virtual_storage_volume(idx)?),
            Metric::VirtualStorageProportionalVolume(idx) => {
                let max_volume = model.get_virtual_storage_node(idx)?.get_max_volume(model, state)?;
                Ok(state
                    .get_network_state()
                    .get_virtual_storage_proportional_volume(idx, max_volume)?)
            }
            Metric::Constant(v) => Ok(*v),
            Metric::AggregatedNodeVolume(idx) => {
                let node = model.get_aggregated_storage_node(idx)?;
                node.nodes
                    .iter()
                    .map(|idx| state.get_network_state().get_node_volume(idx))
                    .sum::<Result<_, _>>()
            }
            Metric::AggregatedNodeProportionalVolume(idx) => {
                let node = model.get_aggregated_storage_node(idx)?;
                let volume: f64 = node
                    .nodes
                    .iter()
                    .map(|idx| state.get_network_state().get_node_volume(idx))
                    .sum::<Result<_, _>>()?;

                let max_volume: f64 = node
                    .nodes
                    .iter()
                    .map(|idx| model.get_node(idx)?.get_current_max_volume(model, state))
                    .sum::<Result<_, _>>()?;
                // TODO handle divide by zero
                Ok(volume / max_volume)
            }
        }
    }
}
