use crate::edge::EdgeIndex;
use crate::model::Model;
use crate::node::NodeIndex;
use crate::parameters::{FloatValue, ParameterIndex};
use crate::state::State;
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
    pub fn get_value(&self, model: &Model, state: &State) -> Result<f64, PywrError> {
        match self {
            Metric::NodeInFlow(idx) => Ok(state.get_network_state().get_node_in_flow(idx)?),
            Metric::NodeOutFlow(idx) => Ok(state.get_network_state().get_node_out_flow(idx)?),
            Metric::NodeVolume(idx) => Ok(state.get_network_state().get_node_volume(idx)?),
            Metric::NodeProportionalVolume(idx) => {
                let max_volume = model.get_node(idx)?.get_current_max_volume(state)?;
                Ok(state
                    .get_network_state()
                    .get_node_proportional_volume(idx, max_volume)?)
            }
            Metric::EdgeFlow(idx) => Ok(state.get_network_state().get_edge_flow(idx)?),
            Metric::ParameterValue(idx) => Ok(state.get_parameter_value(*idx)?),
            Metric::VirtualStorageVolume(_idx) => Ok(1.0), // TODO!!!
            Metric::VirtualStorageProportionalVolume(_idx) => Ok(1.0), // TODO!!!
            Metric::Constant(v) => Ok(*v),
            Metric::AggregatedNodeVolume(indices) => indices
                .iter()
                .map(|idx| state.get_network_state().get_node_volume(idx))
                .sum::<Result<_, _>>(),
            Metric::AggregatedNodeProportionalVolume(indices) => {
                let volume: f64 = indices
                    .iter()
                    .map(|idx| state.get_network_state().get_node_volume(idx))
                    .sum::<Result<_, _>>()?;

                let max_volume: f64 = indices
                    .iter()
                    .map(|idx| model.get_node(idx)?.get_current_max_volume(state))
                    .sum::<Result<_, _>>()?;
                // TODO handle divide by zero
                Ok(volume / max_volume)
            }
        }
    }
}
