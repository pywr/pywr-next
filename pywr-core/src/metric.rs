use crate::aggregated_node::AggregatedNodeIndex;
use crate::aggregated_storage_node::AggregatedStorageNodeIndex;
use crate::derived_metric::DerivedMetricIndex;
use crate::edge::EdgeIndex;
use crate::models::MultiNetworkTransferIndex;
use crate::network::Network;
use crate::node::NodeIndex;
use crate::parameters::ParameterIndex;
use crate::state::{MultiValue, State};
use crate::virtual_storage::VirtualStorageIndex;
use crate::PywrError;
#[derive(Clone, Debug, PartialEq)]
pub enum MetricF64 {
    NodeInFlow(NodeIndex),
    NodeOutFlow(NodeIndex),
    NodeVolume(NodeIndex),
    AggregatedNodeInFlow(AggregatedNodeIndex),
    AggregatedNodeOutFlow(AggregatedNodeIndex),
    AggregatedNodeVolume(AggregatedStorageNodeIndex),
    EdgeFlow(EdgeIndex),
    ParameterValue(ParameterIndex<f64>),
    MultiParameterValue((ParameterIndex<MultiValue>, String)),
    VirtualStorageVolume(VirtualStorageIndex),
    MultiNodeInFlow { indices: Vec<NodeIndex>, name: String },
    MultiNodeOutFlow { indices: Vec<NodeIndex>, name: String },
    // TODO implement other MultiNodeXXX variants
    Constant(f64),
    DerivedMetric(DerivedMetricIndex),
    InterNetworkTransfer(MultiNetworkTransferIndex),
}

impl MetricF64 {
    pub fn get_value(&self, model: &Network, state: &State) -> Result<f64, PywrError> {
        match self {
            MetricF64::NodeInFlow(idx) => Ok(state.get_network_state().get_node_in_flow(idx)?),
            MetricF64::NodeOutFlow(idx) => Ok(state.get_network_state().get_node_out_flow(idx)?),
            MetricF64::NodeVolume(idx) => Ok(state.get_network_state().get_node_volume(idx)?),
            MetricF64::AggregatedNodeInFlow(idx) => {
                let node = model.get_aggregated_node(idx)?;
                // TODO this could be more efficient with an iterator method? I.e. avoid the `Vec<_>` allocation
                node.get_nodes()
                    .iter()
                    .map(|idx| state.get_network_state().get_node_in_flow(idx))
                    .sum::<Result<_, _>>()
            }
            MetricF64::AggregatedNodeOutFlow(idx) => {
                let node = model.get_aggregated_node(idx)?;
                // TODO this could be more efficient with an iterator method? I.e. avoid the `Vec<_>` allocation
                node.get_nodes()
                    .iter()
                    .map(|idx| state.get_network_state().get_node_out_flow(idx))
                    .sum::<Result<_, _>>()
            }

            MetricF64::EdgeFlow(idx) => Ok(state.get_network_state().get_edge_flow(idx)?),
            MetricF64::ParameterValue(idx) => Ok(state.get_parameter_value(*idx)?),
            MetricF64::MultiParameterValue((idx, key)) => Ok(state.get_multi_parameter_value(*idx, key)?),
            MetricF64::VirtualStorageVolume(idx) => Ok(state.get_network_state().get_virtual_storage_volume(idx)?),
            MetricF64::DerivedMetric(idx) => state.get_derived_metric_value(*idx),
            MetricF64::Constant(v) => Ok(*v),
            MetricF64::AggregatedNodeVolume(idx) => {
                let node = model.get_aggregated_storage_node(idx)?;
                node.nodes
                    .iter()
                    .map(|idx| state.get_network_state().get_node_volume(idx))
                    .sum::<Result<_, _>>()
            }

            MetricF64::MultiNodeInFlow { indices, .. } => {
                let flow = indices
                    .iter()
                    .map(|idx| state.get_network_state().get_node_in_flow(idx))
                    .sum::<Result<_, _>>()?;
                Ok(flow)
            }
            MetricF64::MultiNodeOutFlow { indices, .. } => {
                let flow = indices
                    .iter()
                    .map(|idx| state.get_network_state().get_node_out_flow(idx))
                    .sum::<Result<_, _>>()?;
                Ok(flow)
            }
            MetricF64::InterNetworkTransfer(idx) => state.get_inter_network_transfer_value(*idx),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MetricUsize {
    IndexParameterValue(ParameterIndex<usize>),
    Constant(usize),
}

impl MetricUsize {
    pub fn get_value(&self, _network: &Network, state: &State) -> Result<usize, PywrError> {
        match self {
            Self::IndexParameterValue(idx) => state.get_parameter_index(*idx),
            Self::Constant(i) => Ok(*i),
        }
    }
}
