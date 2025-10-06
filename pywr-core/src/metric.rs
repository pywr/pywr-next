use crate::aggregated_node::AggregatedNodeIndex;
use crate::aggregated_storage_node::AggregatedStorageNodeIndex;
use crate::derived_metric::DerivedMetricIndex;
use crate::edge::EdgeIndex;
use crate::models::MultiNetworkTransferIndex;
use crate::network::Network;
use crate::node::{NodeError, NodeIndex};
use crate::parameters::{ConstParameterIndex, GeneralParameterIndex, ParameterIndex, SimpleParameterIndex};
use crate::state::{ConstParameterValues, MultiValue, NetworkStateError, SimpleParameterValues, State, StateError};
use crate::virtual_storage::VirtualStorageIndex;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConstantMetricF64Error {
    #[error("Parameter not found: {index}")]
    ParameterNotFound { index: ConstParameterIndex<f64> },
    #[error("Index parameter not found: {index}")]
    IndexParameterNotFound { index: ConstParameterIndex<u64> },
    #[error("Multi-parameter not found: {index}, key: {key}")]
    MultiParameterNotFound {
        index: ConstParameterIndex<MultiValue>,
        key: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConstantMetricF64 {
    ParameterValue(ConstParameterIndex<f64>),
    IndexParameterValue(ConstParameterIndex<u64>),
    MultiParameterValue(ConstParameterIndex<MultiValue>, String),
    Constant(f64),
}

impl ConstantMetricF64 {
    pub fn get_value(&self, values: &ConstParameterValues) -> Result<f64, ConstantMetricF64Error> {
        match self {
            ConstantMetricF64::ParameterValue(idx) => values
                .get_const_parameter_f64(*idx)
                .ok_or(ConstantMetricF64Error::ParameterNotFound { index: *idx })
                .copied(),
            ConstantMetricF64::IndexParameterValue(idx) => values
                .get_const_parameter_u64(*idx)
                .ok_or(ConstantMetricF64Error::IndexParameterNotFound { index: *idx })
                .map(|&v| v as f64),
            ConstantMetricF64::MultiParameterValue(idx, key) => values
                .get_const_multi_parameter_f64(*idx, key)
                .ok_or_else(|| ConstantMetricF64Error::MultiParameterNotFound {
                    index: *idx,
                    key: key.clone(),
                })
                .copied(),
            ConstantMetricF64::Constant(v) => Ok(*v),
        }
    }

    /// Returns true if the constant value is a [`ConstantMetricF64::Constant`] with a value of zero.
    pub fn is_constant_zero(&self) -> bool {
        match self {
            ConstantMetricF64::Constant(v) => *v == 0.0,
            _ => false,
        }
    }
}

#[derive(Debug, Error)]
pub enum SimpleMetricF64Error {
    #[error("Parameter not found: {index}")]
    ParameterNotFound { index: SimpleParameterIndex<f64> },
    #[error("Index parameter not found: {index}")]
    IndexParameterNotFound { index: SimpleParameterIndex<u64> },
    #[error("Multi-parameter not found: {index}, key: {key}")]
    MultiParameterNotFound {
        index: SimpleParameterIndex<MultiValue>,
        key: String,
    },
    #[error("Constant metric error: {0}")]
    ConstantMetricError(#[from] ConstantMetricF64Error),
    #[error("Cannot simplify metric to a constant metric")]
    CannotSimplifyMetric,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SimpleMetricF64 {
    ParameterValue(SimpleParameterIndex<f64>),
    IndexParameterValue(SimpleParameterIndex<u64>),
    MultiParameterValue(SimpleParameterIndex<MultiValue>, String),
    Constant(ConstantMetricF64),
}

impl SimpleMetricF64 {
    pub fn get_value(&self, values: &SimpleParameterValues) -> Result<f64, SimpleMetricF64Error> {
        match self {
            SimpleMetricF64::ParameterValue(idx) => values
                .get_simple_parameter_f64(*idx)
                .ok_or(SimpleMetricF64Error::ParameterNotFound { index: *idx })
                .copied(),
            SimpleMetricF64::IndexParameterValue(idx) => values
                .get_simple_parameter_u64(*idx)
                .ok_or(SimpleMetricF64Error::IndexParameterNotFound { index: *idx })
                .map(|&v| v as f64),
            SimpleMetricF64::MultiParameterValue(idx, key) => values
                .get_simple_multi_parameter_f64(*idx, key)
                .ok_or_else(|| SimpleMetricF64Error::MultiParameterNotFound {
                    index: *idx,
                    key: key.clone(),
                })
                .copied(),
            SimpleMetricF64::Constant(m) => Ok(m.get_value(values.get_constant_values())?),
        }
    }

    /// Try to get the constant value of the metric, if it is a constant value.
    pub fn try_get_constant_value(&self, values: &ConstParameterValues) -> Result<Option<f64>, ConstantMetricF64Error> {
        match self {
            SimpleMetricF64::Constant(c) => c.get_value(values).map(Some),
            _ => Ok(None),
        }
    }

    /// Returns true if the metric is a constant value.
    pub fn is_constant(&self) -> bool {
        matches!(self, SimpleMetricF64::Constant(_))
    }

    /// Returns true if the constant value is a [`ConstantMetricF64::Constant`] with a value of zero.
    pub fn is_constant_zero(&self) -> bool {
        match self {
            SimpleMetricF64::Constant(c) => c.is_constant_zero(),
            _ => false,
        }
    }
}

#[derive(Debug, Error)]
pub enum MetricF64Error {
    #[error("Parameter not found: {index}")]
    ParameterNotFound { index: SimpleParameterIndex<f64> },
    #[error("Index parameter not found: {index}")]
    IndexParameterNotFound { index: SimpleParameterIndex<u64> },
    #[error("Multi-parameter not found: {index}, key: {key}")]
    MultiParameterNotFound {
        index: SimpleParameterIndex<MultiValue>,
        key: String,
    },
    #[error("Node index not found: {0}")]
    NodeIndexNotFound(NodeIndex),
    #[error("Node error: {0}")]
    NodeError(#[from] Box<NodeError>),
    #[error("Aggregated node index not found: {0}")]
    AggregatedNodeIndexNotFound(AggregatedNodeIndex),
    #[error("Aggregated storage node index not found: {0}")]
    AggregatedStorageNodeIndexNotFound(AggregatedStorageNodeIndex),
    #[error("Network state error: {0}")]
    NetworkStateError(#[from] NetworkStateError),
    #[error("State error: {0}")]
    StateError(#[from] StateError),
    #[error("Constant metric error: {0}")]
    SimpleMetricError(#[from] SimpleMetricF64Error),
    #[error("Cannot simplify metric to a simple metric")]
    CannotSimplifyMetric,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MetricF64 {
    NodeInFlow(NodeIndex),
    NodeOutFlow(NodeIndex),
    NodeVolume(NodeIndex),
    NodeMaxVolume(NodeIndex),
    AggregatedNodeInFlow(AggregatedNodeIndex),
    AggregatedNodeOutFlow(AggregatedNodeIndex),
    AggregatedNodeVolume(AggregatedStorageNodeIndex),
    EdgeFlow(EdgeIndex),
    MultiEdgeFlow { indices: Vec<EdgeIndex>, name: String },
    ParameterValue(GeneralParameterIndex<f64>),
    IndexParameterValue(GeneralParameterIndex<u64>),
    MultiParameterValue(GeneralParameterIndex<MultiValue>, String),
    VirtualStorageVolume(VirtualStorageIndex),
    MultiNodeInFlow { indices: Vec<NodeIndex>, name: String },
    MultiNodeOutFlow { indices: Vec<NodeIndex>, name: String },
    // TODO implement other MultiNodeXXX variants
    DerivedMetric(DerivedMetricIndex),
    InterNetworkTransfer(MultiNetworkTransferIndex),
    Simple(SimpleMetricF64),
}

impl MetricF64 {
    pub fn get_value(&self, network: &Network, state: &State) -> Result<f64, MetricF64Error> {
        match self {
            MetricF64::NodeInFlow(idx) => Ok(state.get_network_state().get_node_in_flow(idx)?),
            MetricF64::NodeOutFlow(idx) => Ok(state.get_network_state().get_node_out_flow(idx)?),
            MetricF64::NodeVolume(idx) => Ok(state.get_network_state().get_node_volume(idx)?),
            MetricF64::NodeMaxVolume(idx) => Ok(network
                .get_node(idx)
                .ok_or(MetricF64Error::NodeIndexNotFound(*idx))?
                .get_max_volume(state)
                .map_err(|e| MetricF64Error::NodeError(Box::new(e)))?),
            MetricF64::AggregatedNodeInFlow(idx) => {
                let node = network
                    .get_aggregated_node(idx)
                    .ok_or(MetricF64Error::AggregatedNodeIndexNotFound(*idx))?;

                let network_state = state.get_network_state();

                let flow = node
                    .iter_nodes()
                    .flat_map(|indices| indices.iter())
                    .map(|idx| network_state.get_node_in_flow(idx))
                    .sum::<Result<_, _>>()?;

                Ok(flow)
            }
            MetricF64::AggregatedNodeOutFlow(idx) => {
                let node = network
                    .get_aggregated_node(idx)
                    .ok_or(MetricF64Error::AggregatedNodeIndexNotFound(*idx))?;

                let network_state = state.get_network_state();

                let flow = node
                    .iter_nodes()
                    .flat_map(|indices| indices.iter())
                    .map(|idx| network_state.get_node_out_flow(idx))
                    .sum::<Result<_, _>>()?;

                Ok(flow)
            }

            MetricF64::EdgeFlow(idx) => Ok(state.get_network_state().get_edge_flow(idx)?),
            MetricF64::MultiEdgeFlow { indices, .. } => {
                let flow = indices
                    .iter()
                    .map(|idx| state.get_network_state().get_edge_flow(idx))
                    .sum::<Result<_, _>>()?;
                Ok(flow)
            }
            MetricF64::ParameterValue(idx) => Ok(state.get_parameter_value(*idx)?),
            MetricF64::IndexParameterValue(idx) => Ok(state.get_parameter_index(*idx)? as f64),
            MetricF64::MultiParameterValue(idx, key) => Ok(state.get_multi_parameter_value(*idx, key)?),
            MetricF64::VirtualStorageVolume(idx) => Ok(state.get_network_state().get_virtual_storage_volume(idx)?),
            MetricF64::DerivedMetric(idx) => Ok(state.get_derived_metric_value(*idx)?),

            MetricF64::AggregatedNodeVolume(idx) => {
                let node = network
                    .get_aggregated_storage_node(idx)
                    .ok_or(MetricF64Error::AggregatedStorageNodeIndexNotFound(*idx))?;

                let network_state = state.get_network_state();

                let volume = node
                    .iter_nodes()
                    .map(|idx| network_state.get_node_volume(idx))
                    .sum::<Result<_, _>>()?;

                Ok(volume)
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
            MetricF64::InterNetworkTransfer(idx) => Ok(state.get_inter_network_transfer_value(*idx)?),
            MetricF64::Simple(s) => Ok(s.get_value(&state.get_simple_parameter_values())?),
        }
    }

    /// Try to get the constant value of the metric, if it is a constant value.
    pub fn try_get_constant_value(&self, values: &ConstParameterValues) -> Result<Option<f64>, ConstantMetricF64Error> {
        match self {
            MetricF64::Simple(s) => s.try_get_constant_value(values),
            _ => Ok(None),
        }
    }

    pub fn is_constant(&self) -> bool {
        match self {
            MetricF64::Simple(s) => s.is_constant(),
            _ => false,
        }
    }

    /// Returns true if the constant value is a [`ConstantMetricF64::Constant`] with a value of zero.
    pub fn is_constant_zero(&self) -> bool {
        match self {
            MetricF64::Simple(s) => s.is_constant_zero(),
            _ => false,
        }
    }
}

impl TryFrom<MetricF64> for SimpleMetricF64 {
    type Error = MetricF64Error;

    fn try_from(value: MetricF64) -> Result<Self, Self::Error> {
        match value {
            MetricF64::Simple(s) => Ok(s),
            _ => Err(MetricF64Error::CannotSimplifyMetric),
        }
    }
}

impl TryFrom<SimpleMetricF64> for ConstantMetricF64 {
    type Error = SimpleMetricF64Error;

    fn try_from(value: SimpleMetricF64) -> Result<Self, Self::Error> {
        match value {
            SimpleMetricF64::Constant(c) => Ok(c),
            _ => Err(SimpleMetricF64Error::CannotSimplifyMetric),
        }
    }
}

impl From<f64> for ConstantMetricF64 {
    fn from(v: f64) -> Self {
        ConstantMetricF64::Constant(v)
    }
}

impl<T> From<T> for SimpleMetricF64
where
    T: Into<ConstantMetricF64>,
{
    fn from(v: T) -> Self {
        SimpleMetricF64::Constant(v.into())
    }
}
impl<T> From<T> for MetricF64
where
    T: Into<SimpleMetricF64>,
{
    fn from(v: T) -> Self {
        MetricF64::Simple(v.into())
    }
}

impl From<DerivedMetricIndex> for MetricF64 {
    fn from(idx: DerivedMetricIndex) -> Self {
        Self::DerivedMetric(idx)
    }
}

impl From<ParameterIndex<f64>> for MetricF64 {
    fn from(idx: ParameterIndex<f64>) -> Self {
        match idx {
            ParameterIndex::General(idx) => Self::ParameterValue(idx),
            ParameterIndex::Simple(idx) => Self::Simple(SimpleMetricF64::ParameterValue(idx)),
            ParameterIndex::Const(idx) => {
                Self::Simple(SimpleMetricF64::Constant(ConstantMetricF64::ParameterValue(idx)))
            }
        }
    }
}

impl From<ParameterIndex<u64>> for MetricF64 {
    fn from(idx: ParameterIndex<u64>) -> Self {
        match idx {
            ParameterIndex::General(idx) => Self::IndexParameterValue(idx),
            ParameterIndex::Simple(idx) => Self::Simple(SimpleMetricF64::IndexParameterValue(idx)),
            ParameterIndex::Const(idx) => {
                Self::Simple(SimpleMetricF64::Constant(ConstantMetricF64::IndexParameterValue(idx)))
            }
        }
    }
}

impl From<(ParameterIndex<MultiValue>, String)> for MetricF64 {
    fn from((idx, key): (ParameterIndex<MultiValue>, String)) -> Self {
        match idx {
            ParameterIndex::General(idx) => Self::MultiParameterValue(idx, key),
            ParameterIndex::Simple(idx) => Self::Simple(SimpleMetricF64::MultiParameterValue(idx, key)),
            ParameterIndex::Const(idx) => Self::Simple(SimpleMetricF64::Constant(
                ConstantMetricF64::MultiParameterValue(idx, key),
            )),
        }
    }
}

impl From<(ParameterIndex<MultiValue>, String)> for MetricU64 {
    fn from((idx, key): (ParameterIndex<MultiValue>, String)) -> Self {
        match idx {
            ParameterIndex::General(idx) => Self::MultiParameterValue((idx, key)),
            ParameterIndex::Simple(idx) => Self::Simple(SimpleMetricU64::MultiParameterValue((idx, key))),
            ParameterIndex::Const(idx) => Self::Simple(SimpleMetricU64::Constant(
                ConstantMetricU64::MultiParameterValue((idx, key)),
            )),
        }
    }
}

impl TryFrom<ParameterIndex<f64>> for SimpleMetricF64 {
    type Error = MetricF64Error;
    fn try_from(idx: ParameterIndex<f64>) -> Result<Self, Self::Error> {
        match idx {
            ParameterIndex::Simple(idx) => Ok(Self::ParameterValue(idx)),
            ParameterIndex::Const(idx) => Ok(Self::Constant(ConstantMetricF64::ParameterValue(idx))),
            ParameterIndex::General(_) => Err(MetricF64Error::CannotSimplifyMetric),
        }
    }
}

impl TryFrom<ParameterIndex<u64>> for SimpleMetricU64 {
    type Error = MetricU64Error;
    fn try_from(idx: ParameterIndex<u64>) -> Result<Self, Self::Error> {
        match idx {
            ParameterIndex::Simple(idx) => Ok(Self::IndexParameterValue(idx)),
            ParameterIndex::Const(idx) => Ok(Self::Constant(ConstantMetricU64::IndexParameterValue(idx))),
            ParameterIndex::General(_) => Err(MetricU64Error::CannotSimplifyMetric),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConstantMetricU64Error {
    #[error("Index parameter not found: {index}")]
    IndexParameterNotFound { index: ConstParameterIndex<u64> },
    #[error("Multi-parameter not found: {index}, key: {key}")]
    MultiParameterNotFound {
        index: ConstParameterIndex<MultiValue>,
        key: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConstantMetricU64 {
    IndexParameterValue(ConstParameterIndex<u64>),
    MultiParameterValue((ConstParameterIndex<MultiValue>, String)),
    Constant(u64),
}

impl ConstantMetricU64 {
    pub fn get_value(&self, values: &ConstParameterValues) -> Result<u64, ConstantMetricU64Error> {
        match self {
            ConstantMetricU64::IndexParameterValue(idx) => values
                .get_const_parameter_u64(*idx)
                .ok_or(ConstantMetricU64Error::IndexParameterNotFound { index: *idx })
                .copied(),
            ConstantMetricU64::MultiParameterValue((idx, key)) => values
                .get_const_multi_parameter_u64(*idx, key)
                .ok_or_else(|| ConstantMetricU64Error::MultiParameterNotFound {
                    index: *idx,
                    key: key.clone(),
                })
                .copied(),
            ConstantMetricU64::Constant(v) => Ok(*v),
        }
    }
}

#[derive(Debug, Error)]
pub enum SimpleMetricU64Error {
    #[error("Index parameter not found: {index}")]
    IndexParameterNotFound { index: SimpleParameterIndex<u64> },
    #[error("Multi-parameter not found: {index}, key: {key}")]
    MultiParameterNotFound {
        index: SimpleParameterIndex<MultiValue>,
        key: String,
    },
    #[error("Constant metric error: {0}")]
    ConstantMetricError(#[from] ConstantMetricU64Error),
    #[error("Cannot simplify metric to a constant metric")]
    CannotSimplifyMetric,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SimpleMetricU64 {
    IndexParameterValue(SimpleParameterIndex<u64>),
    MultiParameterValue((SimpleParameterIndex<MultiValue>, String)),
    Constant(ConstantMetricU64),
}

impl SimpleMetricU64 {
    pub fn get_value(&self, values: &SimpleParameterValues) -> Result<u64, SimpleMetricU64Error> {
        match self {
            SimpleMetricU64::IndexParameterValue(idx) => values
                .get_simple_parameter_u64(*idx)
                .ok_or(SimpleMetricU64Error::IndexParameterNotFound { index: *idx })
                .copied(),
            SimpleMetricU64::MultiParameterValue((idx, key)) => values
                .get_simple_multi_parameter_u64(*idx, key)
                .ok_or_else(|| SimpleMetricU64Error::MultiParameterNotFound {
                    index: *idx,
                    key: key.clone(),
                })
                .copied(),
            SimpleMetricU64::Constant(m) => Ok(m.get_value(values.get_constant_values())?),
        }
    }
}

#[derive(Debug, Error)]
pub enum MetricU64Error {
    #[error("Index parameter not found: {index}")]
    IndexParameterNotFound { index: SimpleParameterIndex<u64> },
    #[error("Multi-parameter not found: {index}, key: {key}")]
    MultiParameterNotFound {
        index: SimpleParameterIndex<MultiValue>,
        key: String,
    },
    #[error("State error: {0}")]
    StateError(#[from] StateError),
    #[error("Constant metric error: {0}")]
    SimpleMetricError(#[from] SimpleMetricU64Error),
    #[error("Cannot simplify metric to a simple metric")]
    CannotSimplifyMetric,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MetricU64 {
    IndexParameterValue(GeneralParameterIndex<u64>),
    Simple(SimpleMetricU64),
    MultiParameterValue((GeneralParameterIndex<MultiValue>, String)),
    InterNetworkTransfer(MultiNetworkTransferIndex),
}

impl MetricU64 {
    pub fn get_value(&self, _network: &Network, state: &State) -> Result<u64, MetricU64Error> {
        match self {
            Self::IndexParameterValue(idx) => Ok(state.get_parameter_index(*idx)?),
            Self::MultiParameterValue((idx, key)) => Ok(state.get_multi_parameter_index(*idx, key)?),
            Self::Simple(s) => Ok(s.get_value(&state.get_simple_parameter_values())?),
            Self::InterNetworkTransfer(_idx) => todo!("Support usize for inter-network transfers"),
        }
    }
}

impl From<ParameterIndex<u64>> for MetricU64 {
    fn from(idx: ParameterIndex<u64>) -> Self {
        match idx {
            ParameterIndex::General(idx) => Self::IndexParameterValue(idx),
            ParameterIndex::Simple(idx) => Self::Simple(SimpleMetricU64::IndexParameterValue(idx)),
            ParameterIndex::Const(idx) => {
                Self::Simple(SimpleMetricU64::Constant(ConstantMetricU64::IndexParameterValue(idx)))
            }
        }
    }
}
impl From<u64> for ConstantMetricU64 {
    fn from(v: u64) -> Self {
        ConstantMetricU64::Constant(v)
    }
}

impl<T> From<T> for SimpleMetricU64
where
    T: Into<ConstantMetricU64>,
{
    fn from(v: T) -> Self {
        SimpleMetricU64::Constant(v.into())
    }
}

impl<T> From<T> for MetricU64
where
    T: Into<SimpleMetricU64>,
{
    fn from(v: T) -> Self {
        MetricU64::Simple(v.into())
    }
}

impl TryFrom<MetricU64> for SimpleMetricU64 {
    type Error = MetricU64Error;

    fn try_from(value: MetricU64) -> Result<Self, Self::Error> {
        match value {
            MetricU64::Simple(s) => Ok(s),
            _ => Err(MetricU64Error::CannotSimplifyMetric),
        }
    }
}

impl TryFrom<SimpleMetricU64> for ConstantMetricU64 {
    type Error = SimpleMetricU64Error;

    fn try_from(value: SimpleMetricU64) -> Result<Self, Self::Error> {
        match value {
            SimpleMetricU64::Constant(c) => Ok(c),
            _ => Err(SimpleMetricU64Error::CannotSimplifyMetric),
        }
    }
}
