use crate::aggregated_node::AggregatedNodeIndex;
use crate::aggregated_storage_node::AggregatedStorageNodeIndex;
use crate::derived_metric::DerivedMetricIndex;
use crate::edge::EdgeIndex;
use crate::network::Network;
use crate::node::NodeIndex;
use crate::parameters::{IndexParameterIndex, MultiValueParameterIndex, ParameterIndex};
use crate::state::State;
use crate::virtual_storage::VirtualStorageIndex;
use crate::PywrError;
#[derive(Clone, Debug, PartialEq)]
pub enum Metric {
    NodeInFlow(NodeIndex),
    NodeOutFlow(NodeIndex),
    NodeVolume(NodeIndex),
    AggregatedNodeInFlow(AggregatedNodeIndex),
    AggregatedNodeOutFlow(AggregatedNodeIndex),
    AggregatedNodeVolume(AggregatedStorageNodeIndex),
    EdgeFlow(EdgeIndex),
    ParameterValue(ParameterIndex),
    MultiParameterValue((MultiValueParameterIndex, String)),
    VirtualStorageVolume(VirtualStorageIndex),
    MultiNodeInFlow {
        indices: Vec<NodeIndex>,
        name: String,
        sub_name: Option<String>,
    },
    // TODO implement other MultiNodeXXX variants
    Constant(f64),
    DerivedMetric(DerivedMetricIndex),
}

impl Metric {
    pub fn get_value(&self, model: &Network, state: &State) -> Result<f64, PywrError> {
        match self {
            Metric::NodeInFlow(idx) => Ok(state.get_network_state().get_node_in_flow(idx)?),
            Metric::NodeOutFlow(idx) => Ok(state.get_network_state().get_node_out_flow(idx)?),
            Metric::NodeVolume(idx) => Ok(state.get_network_state().get_node_volume(idx)?),
            Metric::AggregatedNodeInFlow(idx) => {
                let node = model.get_aggregated_node(idx)?;
                // TODO this could be more efficient with an iterator method? I.e. avoid the `Vec<_>` allocation
                node.get_nodes()
                    .iter()
                    .map(|idx| state.get_network_state().get_node_in_flow(idx))
                    .sum::<Result<_, _>>()
            }
            Metric::AggregatedNodeOutFlow(idx) => {
                let node = model.get_aggregated_node(idx)?;
                // TODO this could be more efficient with an iterator method? I.e. avoid the `Vec<_>` allocation
                node.get_nodes()
                    .iter()
                    .map(|idx| state.get_network_state().get_node_out_flow(idx))
                    .sum::<Result<_, _>>()
            }

            Metric::EdgeFlow(idx) => Ok(state.get_network_state().get_edge_flow(idx)?),
            Metric::ParameterValue(idx) => Ok(state.get_parameter_value(*idx)?),
            Metric::MultiParameterValue((idx, key)) => Ok(state.get_multi_parameter_value(*idx, key)?),
            Metric::VirtualStorageVolume(idx) => Ok(state.get_network_state().get_virtual_storage_volume(idx)?),
            Metric::DerivedMetric(idx) => state.get_derived_metric_value(*idx),
            Metric::Constant(v) => Ok(*v),
            Metric::AggregatedNodeVolume(idx) => {
                let node = model.get_aggregated_storage_node(idx)?;
                node.nodes
                    .iter()
                    .map(|idx| state.get_network_state().get_node_volume(idx))
                    .sum::<Result<_, _>>()
            }

            Metric::MultiNodeInFlow { indices, .. } => {
                let flow = indices
                    .iter()
                    .map(|idx| state.get_network_state().get_node_in_flow(idx))
                    .sum::<Result<_, _>>()?;
                Ok(flow)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum IndexMetric {
    IndexParameterValue(IndexParameterIndex),
    Constant(usize),
}

impl IndexMetric {
    pub fn get_value(&self, _network: &Network, state: &State) -> Result<usize, PywrError> {
        match self {
            Self::IndexParameterValue(idx) => state.get_parameter_index(*idx),
            Self::Constant(i) => Ok(*i),
        }
    }
}
