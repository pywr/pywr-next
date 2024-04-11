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
    IndexParameterValue(ParameterIndex<usize>),
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
            MetricF64::IndexParameterValue(idx) => Ok(state.get_parameter_index(*idx)? as f64),
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
    pub fn name<'a>(&'a self, network: &'a Network) -> Result<&'a str, PywrError> {
        match self {
            Self::NodeInFlow(idx) | Self::NodeOutFlow(idx) | Self::NodeVolume(idx) => {
                network.get_node(idx).map(|n| n.name())
            }
            Self::AggregatedNodeInFlow(idx) | Self::AggregatedNodeOutFlow(idx) => {
                network.get_aggregated_node(idx).map(|n| n.name())
            }
            Self::AggregatedNodeVolume(idx) => network.get_aggregated_storage_node(idx).map(|n| n.name()),
            Self::EdgeFlow(idx) => {
                let edge = network.get_edge(idx)?;
                network.get_node(&edge.from_node_index).map(|n| n.name())
            }
            Self::ParameterValue(idx) => network.get_parameter(idx).map(|p| p.name()),
            Self::IndexParameterValue(idx) => network.get_index_parameter(idx).map(|p| p.name()),
            Self::MultiParameterValue((idx, _)) => network.get_multi_valued_parameter(idx).map(|p| p.name()),
            Self::VirtualStorageVolume(idx) => network.get_virtual_storage_node(idx).map(|v| v.name()),
            Self::MultiNodeInFlow { name, .. } | Self::MultiNodeOutFlow { name, .. } => Ok(name),
            Self::Constant(_) => Ok(""),
            Self::DerivedMetric(idx) => network.get_derived_metric(idx)?.name(network),
            Self::InterNetworkTransfer(_) => todo!("InterNetworkTransfer name is not implemented"),
        }
    }

    pub fn sub_name<'a>(&'a self, network: &'a Network) -> Result<Option<&'a str>, PywrError> {
        match self {
            Self::NodeInFlow(idx) | Self::NodeOutFlow(idx) | Self::NodeVolume(idx) => {
                network.get_node(idx).map(|n| n.sub_name())
            }
            Self::AggregatedNodeInFlow(idx) | Self::AggregatedNodeOutFlow(idx) => {
                network.get_aggregated_node(idx).map(|n| n.sub_name())
            }
            Self::AggregatedNodeVolume(idx) => network.get_aggregated_storage_node(idx).map(|n| n.sub_name()),
            Self::EdgeFlow(idx) => {
                let edge = network.get_edge(idx)?;
                network.get_node(&edge.to_node_index).map(|n| Some(n.name()))
            }
            Self::ParameterValue(_) | Self::IndexParameterValue(_) | Self::MultiParameterValue(_) => Ok(None),
            Self::VirtualStorageVolume(idx) => network.get_virtual_storage_node(idx).map(|v| v.sub_name()),
            Self::MultiNodeInFlow { .. } | Self::MultiNodeOutFlow { .. } => Ok(None),
            Self::Constant(_) => Ok(None),
            Self::DerivedMetric(idx) => network.get_derived_metric(idx)?.sub_name(network),
            Self::InterNetworkTransfer(_) => todo!("InterNetworkTransfer sub_name is not implemented"),
        }
    }

    pub fn attribute(&self) -> &str {
        match self {
            Self::NodeInFlow(_) => "inflow",
            Self::NodeOutFlow(_) => "outflow",
            Self::NodeVolume(_) => "volume",
            Self::AggregatedNodeInFlow(_) => "inflow",
            Self::AggregatedNodeOutFlow(_) => "outflow",
            Self::AggregatedNodeVolume(_) => "volume",
            Self::EdgeFlow(_) => "edge_flow",
            Self::ParameterValue(_) => "value",
            Self::IndexParameterValue(_) => "value",
            Self::MultiParameterValue(_) => "value",
            Self::VirtualStorageVolume(_) => "volume",
            Self::MultiNodeInFlow { .. } => "inflow",
            Self::MultiNodeOutFlow { .. } => "outflow",
            Self::Constant(_) => "value",
            Self::DerivedMetric(_) => "value",
            Self::InterNetworkTransfer(_) => "value",
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

    pub fn name<'a>(&'a self, network: &'a Network) -> Result<&'a str, PywrError> {
        match self {
            Self::IndexParameterValue(idx) => network.get_index_parameter(idx).map(|p| p.name()),
            Self::Constant(_) => Ok(""),
        }
    }

    pub fn sub_name<'a>(&'a self, _network: &'a Network) -> Result<Option<&'a str>, PywrError> {
        match self {
            Self::IndexParameterValue(_) => Ok(None),
            Self::Constant(_) => Ok(None),
        }
    }

    pub fn attribute(&self) -> &str {
        match self {
            Self::IndexParameterValue(_) => "value",
            Self::Constant(_) => "value",
        }
    }
}
