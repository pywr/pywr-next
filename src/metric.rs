use crate::edge::EdgeIndex;
use crate::node::NodeIndex;
use crate::parameters::{FloatValue, ParameterIndex};
use crate::state::{NetworkState, ParameterState};
use crate::virtual_storage::VirtualStorageIndex;
use crate::PywrError;

#[derive(Clone, Debug)]
pub enum Metric {
    NodeInFlow(NodeIndex),
    NodeOutFlow(NodeIndex),
    NodeVolume(NodeIndex),
    NodeProportionalVolume(NodeIndex),
    AggregatedNodeVolume(Vec<NodeIndex>),
    AggregatedNodeProportionalVolume(Vec<NodeIndex>),
    EdgeFlow(EdgeIndex),
    ParameterValue(ParameterIndex),
    VirtualStorageVolume(VirtualStorageIndex),
    VirtualStorageProportionalVolume(VirtualStorageIndex),
    Constant(f64),
}

impl From<FloatValue> for Metric {
    fn from(v: FloatValue) -> Self {
        match v {
            FloatValue::Constant(v) => Self::Constant(v),
            FloatValue::Dynamic(idx) => Self::ParameterValue(idx),
        }
    }
}

impl Metric {
    pub fn get_value(&self, network_state: &NetworkState, parameter_state: &ParameterState) -> Result<f64, PywrError> {
        match self {
            Metric::NodeInFlow(idx) => Ok(network_state.get_node_in_flow(idx)?),
            Metric::NodeOutFlow(idx) => Ok(network_state.get_node_out_flow(idx)?),
            Metric::NodeVolume(idx) => Ok(network_state.get_node_volume(idx)?),
            Metric::NodeProportionalVolume(idx) => Ok(network_state.get_node_proportional_volume(idx)?),
            Metric::EdgeFlow(idx) => Ok(network_state.get_edge_flow(idx)?),
            Metric::ParameterValue(idx) => Ok(parameter_state.get_value(*idx)?),
            Metric::VirtualStorageVolume(_idx) => Ok(1.0), // TODO!!!
            Metric::VirtualStorageProportionalVolume(_idx) => Ok(1.0), // TODO!!!
            Metric::Constant(v) => Ok(*v),
            Metric::AggregatedNodeVolume(indices) => indices
                .iter()
                .map(|idx| network_state.get_node_volume(idx))
                .sum::<Result<_, _>>(),
            Metric::AggregatedNodeProportionalVolume(indices) => {
                let volume: f64 = indices
                    .iter()
                    .map(|idx| network_state.get_node_volume(idx))
                    .sum::<Result<_, _>>()?;

                let max_volume: f64 = indices
                    .iter()
                    .map(|idx| network_state.get_node_max_volume(idx))
                    .sum::<Result<_, _>>()?;
                // TODO handle divide by zero
                Ok(volume / max_volume)
            }
        }
    }
}
