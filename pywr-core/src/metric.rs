use crate::NodeIndex;
use crate::models::MultiNetworkTransferIndex;
use crate::network::{
    AggregatedNodeIndex, AggregatedStorageNodeIndex, EdgeIndex, Network, ResolutionMaps, UnresolvedEdge,
    VirtualStorageIndex,
};
use crate::node::{NodeError, UnresolvedNode};
use crate::parameters::{
    ConstParameterIndex, GeneralAfterValueIndex, GeneralBeforeValueIndex, ParameterIndex, ParameterName,
    ParameterReturnValue, SimpleParameterIndex,
};
use crate::state::{
    ConstParameterValues, ConstParameterValuesError, MultiValue, NetworkStateError, SimpleParameterValues,
    SimpleParameterValuesError, State, StateError,
};
use num::Zero;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConstantMetricF64Error {
    #[error("Simple parameter value error: {0}")]
    ConstParameterValuesError(#[from] ConstParameterValuesError),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConstantMetricF64 {
    ParameterValue(ConstParameterIndex<f64>),
    IndexParameterValue(ConstParameterIndex<u64>),
    MultiParameterValue {
        index: ConstParameterIndex<MultiValue>,
        key: String,
    },
    Constant(f64),
}

impl ConstantMetricF64 {
    pub fn get_value(&self, values: &ConstParameterValues) -> Result<f64, ConstantMetricF64Error> {
        match self {
            ConstantMetricF64::ParameterValue(idx) => Ok(values.get_f64(*idx)?),
            ConstantMetricF64::IndexParameterValue(idx) => Ok(values.get_u64(*idx)? as f64),
            ConstantMetricF64::MultiParameterValue { index, key } => Ok(values.get_multi_f64(*index, key)?),
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
    #[error("Simple parameter value error: {0}")]
    SimpleParameterValuesError(#[from] SimpleParameterValuesError),
    #[error("Constant metric error: {0}")]
    ConstantMetricError(#[from] ConstantMetricF64Error),
    #[error("Cannot simplify metric to a constant metric")]
    CannotSimplifyMetric,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SimpleMetricF64 {
    ParameterValue {
        index: SimpleParameterIndex<f64>,
    },
    IndexParameterValue {
        index: SimpleParameterIndex<u64>,
    },
    MultiParameterValue {
        index: SimpleParameterIndex<MultiValue>,
        key: String,
    },
    Constant(ConstantMetricF64),
}

impl SimpleMetricF64 {
    pub fn get_value(&self, values: &SimpleParameterValues) -> Result<f64, SimpleMetricF64Error> {
        match self {
            SimpleMetricF64::ParameterValue { index } => Ok(values.get_f64(*index)?),
            SimpleMetricF64::IndexParameterValue { index } => Ok(values.get_u64(*index)? as f64),
            SimpleMetricF64::MultiParameterValue { index, key } => Ok(values.get_multi_f64(*index, key)?),
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
    #[error("Node index not found: {0}")]
    NodeIndexNotFound(NodeIndex),
    #[error("Virtual storage node index not found: {0}")]
    VirtualStorageIndexNotFound(VirtualStorageIndex),
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
    #[error("General parameter with has no key: {key}")]
    GeneralMultiValueParameterKeyNotFound { key: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum MetricF64 {
    NodeInFlow(NodeIndex),
    NodeOutFlow(NodeIndex),
    NodeMaxFlow(NodeIndex),
    NodeVolume(NodeIndex),
    NodeProportionalVolume(NodeIndex),
    NodeMaxVolume(NodeIndex),
    AggregatedNodeInFlow(AggregatedNodeIndex),
    AggregatedNodeOutFlow(AggregatedNodeIndex),
    AggregatedStorageNodeVolume(AggregatedStorageNodeIndex),
    AggregatedStorageNodeProportionalVolume(AggregatedStorageNodeIndex),
    EdgeFlow(EdgeIndex),
    MultiEdgeFlow {
        indices: Vec<EdgeIndex>,
        name: String,
    },
    ParameterBeforeF64(GeneralBeforeValueIndex<f64>),
    ParameterAfterF64(GeneralAfterValueIndex<f64>),
    ParameterBeforeU64(GeneralBeforeValueIndex<u64>),
    ParameterAfterU64(GeneralAfterValueIndex<u64>),
    ParameterBeforeMulti {
        index: GeneralBeforeValueIndex<MultiValue>,
        key: String,
    },
    ParameterAfterMulti {
        index: GeneralAfterValueIndex<MultiValue>,
        key: String,
    },
    VirtualStorageVolume(VirtualStorageIndex),
    VirtualStorageProportionalVolume(VirtualStorageIndex),
    VirtualStorageMaxVolume(VirtualStorageIndex),
    MultiNodeInFlow {
        indices: Vec<NodeIndex>,
        name: String,
    },
    MultiNodeOutFlow {
        indices: Vec<NodeIndex>,
        name: String,
    },
    // TODO implement other MultiNodeXXX variants
    InterNetworkTransfer(MultiNetworkTransferIndex),
    Simple(SimpleMetricF64),
}

impl MetricF64 {
    pub fn get_value(&self, network: &Network, state: &State) -> Result<f64, MetricF64Error> {
        match self {
            MetricF64::NodeInFlow(idx) => Ok(state.get_network_state().get_node_in_flow(idx)?),
            MetricF64::NodeOutFlow(idx) => Ok(state.get_network_state().get_node_out_flow(idx)?),
            MetricF64::NodeMaxFlow(idx) => Ok(network
                .get_node(idx)
                .ok_or(MetricF64Error::NodeIndexNotFound(*idx))?
                .get_max_flow(network, state)
                .map_err(|e| MetricF64Error::NodeError(Box::new(e)))?),
            MetricF64::NodeVolume(idx) => Ok(state.get_network_state().get_node_volume(idx)?),
            MetricF64::NodeProportionalVolume(idx) => {
                Ok(state.get_network_state().get_node_proportional_volume(idx)?)
            }
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
            MetricF64::ParameterBeforeF64(idx) => Ok(state.get_general_parameter_f64_before(*idx)?),
            MetricF64::ParameterAfterF64(idx) => Ok(state.get_general_parameter_f64_after(*idx)?),
            MetricF64::ParameterBeforeU64(idx) => Ok(state.get_general_parameter_u64_before(*idx)? as f64),
            MetricF64::ParameterAfterU64(idx) => Ok(state.get_general_parameter_u64_after(*idx)? as f64),
            MetricF64::ParameterBeforeMulti { index, key } => {
                let mv = state.get_general_parameter_multi_before(*index)?;
                let value = mv
                    .get_value(key)
                    .ok_or_else(|| MetricF64Error::GeneralMultiValueParameterKeyNotFound { key: key.clone() })?;
                Ok(*value)
            }
            MetricF64::ParameterAfterMulti { index, key } => {
                let mv = state.get_general_parameter_multi_after(*index)?;
                let value = mv
                    .get_value(key)
                    .ok_or_else(|| MetricF64Error::GeneralMultiValueParameterKeyNotFound { key: key.clone() })?;
                Ok(*value)
            }
            MetricF64::VirtualStorageVolume(idx) => Ok(state.get_network_state().get_virtual_storage_volume(idx)?),
            MetricF64::VirtualStorageProportionalVolume(idx) => {
                Ok(state.get_network_state().get_virtual_storage_proportional_volume(idx)?)
            }
            MetricF64::VirtualStorageMaxVolume(idx) => Ok(network
                .get_virtual_storage_node(idx)
                .ok_or(MetricF64Error::VirtualStorageIndexNotFound(*idx))?
                .get_max_volume(state)?),
            MetricF64::AggregatedStorageNodeVolume(idx) => {
                let node = network
                    .get_aggregated_storage_node(*idx)
                    .ok_or(MetricF64Error::AggregatedStorageNodeIndexNotFound(*idx))?;

                let network_state = state.get_network_state();

                let volume = node
                    .iter_nodes()
                    .map(|idx| network_state.get_node_volume(idx))
                    .sum::<Result<_, _>>()?;

                Ok(volume)
            }
            MetricF64::AggregatedStorageNodeProportionalVolume(idx) => {
                let node = network
                    .get_aggregated_storage_node(*idx)
                    .ok_or(MetricF64Error::AggregatedStorageNodeIndexNotFound(*idx))?;

                let network_state = state.get_network_state();

                let (volumes, max_volumes): (Vec<_>, Vec<_>) = node
                    .iter_nodes()
                    .map(|idx| {
                        (
                            network_state.get_node_volume(idx),
                            network_state.get_node_max_volume(idx),
                        )
                    })
                    .unzip();

                let volume: f64 = volumes.into_iter().sum::<Result<_, _>>()?;
                let max_volume: f64 = max_volumes.into_iter().sum::<Result<_, _>>()?;

                if max_volume.is_zero() {
                    Ok(1.0)
                } else {
                    Ok(volume / max_volume)
                }
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

impl TryFrom<MetricF64> for ConstantMetricF64 {
    type Error = MetricF64Error;

    fn try_from(value: MetricF64) -> Result<Self, Self::Error> {
        let simple_metric: SimpleMetricF64 = value.try_into()?;
        let constant_metric: ConstantMetricF64 = simple_metric.try_into()?;
        Ok(constant_metric)
    }
}

/// Try to convert a slice of [`MetricF64`] into a vector of [`ConstantMetricF64`].
/// If any of the metrics cannot be converted, return `None`.
pub fn try_into_constant_metrics_f64(metrics: &[MetricF64]) -> Option<Vec<ConstantMetricF64>> {
    metrics
        .iter()
        .map(|m| m.clone().try_into())
        .collect::<Result<Vec<_>, _>>()
        .ok()
}

/// Try to convert a slice of [`MetricF64`] into a vector of [`SimpleMetricF64`].
/// If any of the metrics cannot be converted, return `None`.
pub fn try_into_simple_metrics_f64(metrics: &[MetricF64]) -> Option<Vec<SimpleMetricF64>> {
    metrics
        .iter()
        .map(|m| m.clone().try_into())
        .collect::<Result<Vec<_>, _>>()
        .ok()
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MetricConsumerPhase {
    #[default]
    Before,
    After,
    Both,
}

#[derive(Debug, Error)]
pub enum MetricF64ResolutionError {
    #[error("Node not found when resolving F64 metric: {node}")]
    NodeNotFound { node: UnresolvedNode },
    #[error("Parameter not found when resolving F64 metric: {parameter}")]
    ParameterNotFound { parameter: ParameterName },
    #[error("Aggregated node not found when resolving F64 metric: {aggregated_node}")]
    AggregatedNodeNotFound { aggregated_node: UnresolvedNode },
    #[error("Aggregated storage node not found when resolving F64 metric: {aggregated_node}")]
    AggregatedStorageNodeNotFound { aggregated_node: UnresolvedNode },
    #[error("Edge not found when resolving F64 metric: {edge}")]
    EdgeNotFound { edge: UnresolvedEdge },
    #[error("Virtual storage node not found when resolving F64 metric: {node}")]
    VirtualStorageNodeNotFound { node: UnresolvedNode },
    #[error("Inter-network transfer not found when resolving F64 metric: {transfer}")]
    InterNetworkTransferNotFound { transfer: String },
    #[error(
        "Parameter not registered in the correct phase when resolving F64 metric: {parameter}, consumer phase: {consumer_phase:?}, return value: {return_value:?}"
    )]
    ParameterNotRegisteredInCorrectPhase {
        parameter: ParameterName,
        consumer_phase: MetricConsumerPhase,
        return_value: ParameterReturnValue,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnresolvedMetricF64 {
    NodeInFlow(UnresolvedNode),
    NodeOutFlow(UnresolvedNode),
    NodeVolume(UnresolvedNode),
    NodeProportionalVolume(UnresolvedNode),
    NodeMaxVolume(UnresolvedNode),
    NodeMaxFlow(UnresolvedNode),
    AggregatedNodeInFlow(UnresolvedNode),
    AggregatedNodeOutFlow(UnresolvedNode),
    AggregatedStorageNodeVolume(UnresolvedNode),
    AggregatedStorageNodeProportionalVolume(UnresolvedNode),
    EdgeFlow(UnresolvedEdge),
    MultiEdgeFlow {
        edges: Vec<UnresolvedEdge>,
        name: String,
    },
    ParameterValue {
        name: ParameterName,
        return_value: ParameterReturnValue,
    },
    MultiParameterValue {
        name: ParameterName,
        key: String,
        return_value: ParameterReturnValue,
    },
    VirtualStorageVolume(UnresolvedNode),
    VirtualStorageProportionalVolume(UnresolvedNode),
    MultiNodeInFlow {
        nodes: Vec<UnresolvedNode>,
        name: String,
    },
    MultiNodeOutFlow {
        nodes: Vec<UnresolvedNode>,
        name: String,
    },

    InterNetworkTransfer(String),
    Constant(f64),
}

impl UnresolvedMetricF64 {
    /// Create a new [`Self::ParameterValue`] variant with the given parameter name and a return
    /// value of [`ParameterReturnValue::Before`].
    pub fn new_parameter_before<N: Into<ParameterName>>(name: N) -> Self {
        Self::ParameterValue {
            name: name.into(),
            return_value: ParameterReturnValue::Before,
        }
    }

    /// Create a new [`Self::MultiParameterValue`] variant with the given parameter name and a return
    /// value of [`ParameterReturnValue::Before`].
    pub fn new_parameter_before_key<N: Into<ParameterName>>(name: N, key: &str) -> Self {
        Self::MultiParameterValue {
            name: name.into(),
            key: key.to_string(),
            return_value: ParameterReturnValue::Before,
        }
    }

    /// Create a new [`Self::ParameterValue`] variant with the given parameter name and a return
    /// value of [`ParameterReturnValue::After`].
    pub fn new_parameter_after<N: Into<ParameterName>>(name: N) -> Self {
        Self::ParameterValue {
            name: name.into(),
            return_value: ParameterReturnValue::After,
        }
    }

    /// Create a new [`Self::MultiParameterValue`] variant with the given parameter name and a return
    /// value of [`ParameterReturnValue::After`].
    pub fn new_parameter_after_key<N: Into<ParameterName>>(name: N, key: &str) -> Self {
        Self::MultiParameterValue {
            name: name.into(),
            key: key.to_string(),
            return_value: ParameterReturnValue::After,
        }
    }

    /// Returns true if the constant value is a [`UnresolvedMetricF64::Constant`].
    pub fn is_constant(&self) -> bool {
        matches!(self, UnresolvedMetricF64::Constant(_))
    }

    /// Returns true if the constant value is a [`UnresolvedMetricF64::Constant`] with a value of zero.
    pub fn is_constant_zero(&self) -> bool {
        matches!(self, UnresolvedMetricF64::Constant(v) if *v == 0.0)
    }

    /// Resolve the [`UnresolvedMetricF64`] into a [`MetricF64`] using the provided [`ResolutionMaps`].
    ///
    /// The `consumer_phase` parameter is used to determine which phase the consumer will be using
    /// the metric in. This is important for resolving parameter metrics, as the phase determines whether
    /// the value of the parameter to use is available. If the consumer phase is
    /// [`MetricConsumerPhase::Before`], the metric will must be available in the "before" phase.
    /// If the consumer phase is [`MetricConsumerPhase::After`], the metric must be available in the
    /// "after" phase.
    /// An error will be returned if the consumer phase is not compatible with what the parameter
    /// supports.
    ///
    /// If any data required to resolve the metric is missing, an error will be returned.
    pub fn resolve(
        &self,
        maps: &ResolutionMaps,
        consumer_phase: MetricConsumerPhase,
    ) -> Result<MetricF64, MetricF64ResolutionError> {
        let m = match self {
            UnresolvedMetricF64::NodeInFlow(unresolved) => {
                let idx = maps
                    .nodes
                    .get(unresolved)
                    .ok_or_else(|| MetricF64ResolutionError::NodeNotFound {
                        node: unresolved.clone(),
                    })?;
                MetricF64::NodeInFlow(*idx)
            }
            UnresolvedMetricF64::NodeOutFlow(unresolved) => {
                let idx = maps
                    .nodes
                    .get(unresolved)
                    .ok_or_else(|| MetricF64ResolutionError::NodeNotFound {
                        node: unresolved.clone(),
                    })?;
                MetricF64::NodeOutFlow(*idx)
            }
            UnresolvedMetricF64::NodeVolume(unresolved) => {
                let idx = maps
                    .nodes
                    .get(unresolved)
                    .ok_or_else(|| MetricF64ResolutionError::NodeNotFound {
                        node: unresolved.clone(),
                    })?;
                MetricF64::NodeVolume(*idx)
            }
            UnresolvedMetricF64::NodeProportionalVolume(unresolved) => {
                let idx = maps
                    .nodes
                    .get(unresolved)
                    .ok_or_else(|| MetricF64ResolutionError::NodeNotFound {
                        node: unresolved.clone(),
                    })?;

                MetricF64::NodeProportionalVolume(*idx)
            }
            UnresolvedMetricF64::NodeMaxVolume(unresolved) => {
                let idx = maps
                    .nodes
                    .get(unresolved)
                    .ok_or_else(|| MetricF64ResolutionError::NodeNotFound {
                        node: unresolved.clone(),
                    })?;
                MetricF64::NodeMaxVolume(*idx)
            }
            UnresolvedMetricF64::NodeMaxFlow(unresolved) => {
                let idx = maps
                    .nodes
                    .get(unresolved)
                    .ok_or_else(|| MetricF64ResolutionError::NodeNotFound {
                        node: unresolved.clone(),
                    })?;
                MetricF64::NodeMaxFlow(*idx)
            }
            UnresolvedMetricF64::AggregatedNodeInFlow(unresolved) => {
                let idx = maps.aggregated_nodes.get(unresolved).ok_or_else(|| {
                    MetricF64ResolutionError::AggregatedNodeNotFound {
                        aggregated_node: unresolved.clone(),
                    }
                })?;
                MetricF64::AggregatedNodeInFlow(*idx)
            }
            UnresolvedMetricF64::AggregatedNodeOutFlow(unresolved) => {
                let idx = maps.aggregated_nodes.get(unresolved).ok_or_else(|| {
                    MetricF64ResolutionError::AggregatedNodeNotFound {
                        aggregated_node: unresolved.clone(),
                    }
                })?;
                MetricF64::AggregatedNodeOutFlow(*idx)
            }
            UnresolvedMetricF64::AggregatedStorageNodeVolume(unresolved) => {
                let idx = maps.aggregated_storage_nodes.get(unresolved).ok_or_else(|| {
                    MetricF64ResolutionError::AggregatedStorageNodeNotFound {
                        aggregated_node: unresolved.clone(),
                    }
                })?;
                MetricF64::AggregatedStorageNodeVolume(*idx)
            }
            UnresolvedMetricF64::AggregatedStorageNodeProportionalVolume(unresolved) => {
                let idx = maps.aggregated_storage_nodes.get(unresolved).ok_or_else(|| {
                    MetricF64ResolutionError::AggregatedStorageNodeNotFound {
                        aggregated_node: unresolved.clone(),
                    }
                })?;
                MetricF64::AggregatedStorageNodeProportionalVolume(*idx)
            }
            UnresolvedMetricF64::EdgeFlow(unresolved) => {
                let idx = maps
                    .edges
                    .get(unresolved)
                    .ok_or_else(|| MetricF64ResolutionError::EdgeNotFound {
                        edge: unresolved.clone(),
                    })?;
                MetricF64::EdgeFlow(*idx)
            }
            UnresolvedMetricF64::MultiEdgeFlow { edges, name } => {
                let resolved = edges
                    .iter()
                    .map(|unresolved| {
                        maps.edges
                            .get(unresolved)
                            .copied()
                            .ok_or_else(|| MetricF64ResolutionError::EdgeNotFound {
                                edge: unresolved.clone(),
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                MetricF64::MultiEdgeFlow {
                    indices: resolved,
                    name: name.clone(),
                }
            }
            UnresolvedMetricF64::ParameterValue { name, return_value } => {
                match maps.parameters_f64.get(name) {
                    Some(idx) => resolve_parameter_index_f64_to_metric_f64(name, *idx, *return_value, consumer_phase)?,
                    None => {
                        // Not found as a F64 parameter; try index parameter instead.
                        let idx = maps.parameters_u64.get(name).ok_or_else(|| {
                            MetricF64ResolutionError::ParameterNotFound {
                                parameter: name.clone(),
                            }
                        })?;

                        resolve_parameter_index_u64_to_metric_f64(name, *idx, *return_value, consumer_phase)?
                    }
                }
            }
            UnresolvedMetricF64::MultiParameterValue {
                name,
                key,
                return_value,
            } => {
                let idx =
                    maps.parameters_multi
                        .get(name)
                        .ok_or_else(|| MetricF64ResolutionError::ParameterNotFound {
                            parameter: name.clone(),
                        })?;

                resolve_parameter_index_multi_to_metric_f64(name, idx.clone(), key, *return_value, consumer_phase)?
            }
            UnresolvedMetricF64::VirtualStorageVolume(unresolved) => {
                let idx = maps.virtual_storage_node.get(unresolved).ok_or_else(|| {
                    MetricF64ResolutionError::VirtualStorageNodeNotFound {
                        node: unresolved.clone(),
                    }
                })?;

                MetricF64::VirtualStorageVolume(*idx)
            }
            UnresolvedMetricF64::VirtualStorageProportionalVolume(unresolved) => {
                let idx = maps.virtual_storage_node.get(unresolved).ok_or_else(|| {
                    MetricF64ResolutionError::VirtualStorageNodeNotFound {
                        node: unresolved.clone(),
                    }
                })?;

                MetricF64::VirtualStorageProportionalVolume(*idx)
            }
            UnresolvedMetricF64::MultiNodeInFlow { name, nodes: indices } => {
                let resolved = indices
                    .iter()
                    .map(|unresolved| {
                        maps.nodes
                            .get(unresolved)
                            .copied()
                            .ok_or_else(|| MetricF64ResolutionError::NodeNotFound {
                                node: unresolved.clone(),
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                MetricF64::MultiNodeInFlow {
                    indices: resolved,
                    name: name.clone(),
                }
            }
            UnresolvedMetricF64::MultiNodeOutFlow { name, nodes: indices } => {
                let resolved = indices
                    .iter()
                    .map(|unresolved| {
                        maps.nodes
                            .get(unresolved)
                            .copied()
                            .ok_or_else(|| MetricF64ResolutionError::NodeNotFound {
                                node: unresolved.clone(),
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                MetricF64::MultiNodeOutFlow {
                    indices: resolved,
                    name: name.clone(),
                }
            }
            UnresolvedMetricF64::InterNetworkTransfer(unresolved) => {
                let idx = maps.inter_network_transfers.get(unresolved).ok_or_else(|| {
                    MetricF64ResolutionError::InterNetworkTransferNotFound {
                        transfer: unresolved.clone(),
                    }
                })?;

                MetricF64::InterNetworkTransfer(*idx)
            }
            UnresolvedMetricF64::Constant(value) => (*value).into(),
        };

        Ok(m)
    }
}

impl From<f64> for UnresolvedMetricF64 {
    fn from(v: f64) -> Self {
        Self::Constant(v)
    }
}

/// Resolve a [`ParameterIndex<f64>`] to a [`MetricF64`] using the provided [`ParameterReturnValue`]
/// and [`MetricConsumerPhase`]. This function is used to determine if a parameter can be resolved to a metric
/// based on the phase in which the consumer is using the metric and the return value of the parameter.
///
/// If the parameter cannot be resolved to a metric, an error is returned.
fn resolve_parameter_index_f64_to_metric_f64(
    name: &ParameterName,
    idx: ParameterIndex<f64>,
    parameter_return_value: ParameterReturnValue,
    consumer_phase: MetricConsumerPhase,
) -> Result<MetricF64, MetricF64ResolutionError> {
    match idx {
        // Constant and simple can always be resolved to a metric, regardless of the consumer phase
        // as long as the parameter return value is "before".
        ParameterIndex::Const(idx) => match parameter_return_value {
            ParameterReturnValue::Before => Ok(ConstantMetricF64::ParameterValue(idx).into()),
            ParameterReturnValue::After | ParameterReturnValue::AfterOrElseInitial => {
                Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                    parameter: name.clone(),
                    consumer_phase,
                    return_value: parameter_return_value,
                })
            }
        },
        ParameterIndex::Simple(idx) => match parameter_return_value {
            ParameterReturnValue::Before => Ok(SimpleMetricF64::ParameterValue { index: idx }.into()),
            ParameterReturnValue::After | ParameterReturnValue::AfterOrElseInitial => {
                Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                    parameter: name.clone(),
                    consumer_phase,
                    return_value: parameter_return_value,
                })
            }
        },
        // General parameters must be validated against the consumer phase to determine if they can be resolved to a metric.
        ParameterIndex::General(idx) => {
            match (parameter_return_value, consumer_phase) {
                (ParameterReturnValue::Before, MetricConsumerPhase::Before) => {
                    // The consumer is using the metric in the "before" phase, and the parameter is
                    // providing a "before" value, so we can resolve it to a metric provided the
                    // parameter index contains a "before" index.
                    match idx.before {
                        Some(before_idx) => Ok(MetricF64::ParameterBeforeF64(before_idx)),
                        None => Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::Before, MetricConsumerPhase::After) => {
                    // The consumer is using the metric in the "after" phase, but the parameter is
                    // providing a "before" value. This is fine because the "before" value is still
                    // valid in the "after" phase, so we can resolve it to a metric provided the
                    // parameter index contains a "before" index.
                    match idx.before {
                        Some(before_idx) => Ok(MetricF64::ParameterBeforeF64(before_idx)),
                        None => Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::Before, MetricConsumerPhase::Both) => {
                    // The consumer is using the metric in both "before" and "after" phases, and the
                    // parameter is providing a "before" value. This is fine because the "before"
                    // value is valid in both phases, so we can resolve it to a metric provided the
                    // parameter index contains a "before" index.
                    match idx.before {
                        Some(before_idx) => Ok(MetricF64::ParameterBeforeF64(before_idx)),
                        None => Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::After, MetricConsumerPhase::Before)
                | (ParameterReturnValue::After, MetricConsumerPhase::Both) => {
                    // The consumer is using the metric in the "before" phase, but the parameter is
                    // providing an "after" value. This is not valid because the "after" value is not
                    // valid in the "before" phase, so we cannot resolve it to a metric.
                    Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                        parameter: name.clone(),
                        consumer_phase,
                        return_value: parameter_return_value,
                    })
                }
                (ParameterReturnValue::After, MetricConsumerPhase::After) => {
                    // The consumer is using the metric in the "after" phase, and the parameter is
                    // providing an "after" value, so we can resolve it to a metric provided the
                    // parameter index contains an "after" index.
                    match idx.after {
                        Some(after_idx) => Ok(MetricF64::ParameterAfterF64(after_idx)),
                        None => Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::AfterOrElseInitial, MetricConsumerPhase::Before) => {
                    // The consumer is using the metric in the "before" phase, but the parameter is
                    // providing an "after" value. However, they have specified that using any
                    // initial value is acceptable, so we can resolve it to a metric provided the
                    // parameter index contains an "after" index.
                    match idx.after {
                        Some(after_idx) => Ok(MetricF64::ParameterAfterF64(after_idx)),
                        None => Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::AfterOrElseInitial, MetricConsumerPhase::After) => {
                    // The consumer is using the metric in the "after" phase, and the parameter is
                    // providing an "after" value, so we can resolve it to a metric provided the
                    // parameter index contains an "after" index.
                    match idx.after {
                        Some(after_idx) => Ok(MetricF64::ParameterAfterF64(after_idx)),
                        None => Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::AfterOrElseInitial, MetricConsumerPhase::Both) => {
                    // The consumer is using the metric in both "before" and "after" phases, and the
                    // parameter is providing an "after" value. However, they have specified that using any
                    // initial value is acceptable in the "before" phase, so we can resolve it to a metric provided the
                    // parameter index contains an "after" index.
                    match idx.after {
                        Some(after_idx) => Ok(MetricF64::ParameterAfterF64(after_idx)),
                        None => Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
            }
        }
    }
}

/// Resolve a [`ParameterIndex<u64>`] to a [`MetricF64`] using the provided [`ParameterReturnValue`]
/// and [`MetricConsumerPhase`]. This function is used to determine if a parameter can be resolved to a metric
/// based on the phase in which the consumer is using the metric and the return value of the parameter.
///
/// If the parameter cannot be resolved to a metric, an error is returned.
fn resolve_parameter_index_u64_to_metric_f64(
    name: &ParameterName,
    idx: ParameterIndex<u64>,
    parameter_return_value: ParameterReturnValue,
    consumer_phase: MetricConsumerPhase,
) -> Result<MetricF64, MetricF64ResolutionError> {
    match idx {
        // Constant and simple can always be resolved to a metric, regardless of the consumer phase
        // as long as the parameter return value is "before".
        ParameterIndex::Const(idx) => match parameter_return_value {
            ParameterReturnValue::Before => Ok(ConstantMetricF64::IndexParameterValue(idx).into()),
            ParameterReturnValue::After | ParameterReturnValue::AfterOrElseInitial => {
                Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                    parameter: name.clone(),
                    consumer_phase,
                    return_value: parameter_return_value,
                })
            }
        },
        ParameterIndex::Simple(idx) => match parameter_return_value {
            ParameterReturnValue::Before => Ok(SimpleMetricF64::IndexParameterValue { index: idx }.into()),
            ParameterReturnValue::After | ParameterReturnValue::AfterOrElseInitial => {
                Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                    parameter: name.clone(),
                    consumer_phase,
                    return_value: parameter_return_value,
                })
            }
        },
        // General parameters must be validated against the consumer phase to determine if they can be resolved to a metric.
        ParameterIndex::General(idx) => {
            match (parameter_return_value, consumer_phase) {
                (ParameterReturnValue::Before, MetricConsumerPhase::Before) => {
                    // The consumer is using the metric in the "before" phase, and the parameter is
                    // providing a "before" value, so we can resolve it to a metric provided the
                    // parameter index contains a "before" index.
                    match idx.before {
                        Some(before_idx) => Ok(MetricF64::ParameterBeforeU64(before_idx)),
                        None => Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::Before, MetricConsumerPhase::After) => {
                    // The consumer is using the metric in the "after" phase, but the parameter is
                    // providing a "before" value. This is fine because the "before" value is still
                    // valid in the "after" phase, so we can resolve it to a metric provided the
                    // parameter index contains a "before" index.
                    match idx.before {
                        Some(before_idx) => Ok(MetricF64::ParameterBeforeU64(before_idx)),
                        None => Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::Before, MetricConsumerPhase::Both) => {
                    // The consumer is using the metric in both "before" and "after" phases, and the
                    // parameter is providing a "before" value. This is fine because the "before"
                    // value is valid in both phases, so we can resolve it to a metric provided the
                    // parameter index contains a "before" index.
                    match idx.before {
                        Some(before_idx) => Ok(MetricF64::ParameterBeforeU64(before_idx)),
                        None => Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::After, MetricConsumerPhase::Before)
                | (ParameterReturnValue::After, MetricConsumerPhase::Both) => {
                    // The consumer is using the metric in the "before" phase, but the parameter is
                    // providing an "after" value. This is not valid because the "after" value is not
                    // valid in the "before" phase, so we cannot resolve it to a metric.
                    Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                        parameter: name.clone(),
                        consumer_phase,
                        return_value: parameter_return_value,
                    })
                }
                (ParameterReturnValue::After, MetricConsumerPhase::After) => {
                    // The consumer is using the metric in the "after" phase, and the parameter is
                    // providing an "after" value, so we can resolve it to a metric provided the
                    // parameter index contains an "after" index.
                    match idx.after {
                        Some(after_idx) => Ok(MetricF64::ParameterAfterU64(after_idx)),
                        None => Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::AfterOrElseInitial, MetricConsumerPhase::Before) => {
                    // The consumer is using the metric in the "before" phase, but the parameter is
                    // providing an "after" value. However, they have specified that using any
                    // initial value is acceptable, so we can resolve it to a metric provided the
                    // parameter index contains an "after" index.
                    match idx.after {
                        Some(after_idx) => Ok(MetricF64::ParameterAfterU64(after_idx)),
                        None => Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::AfterOrElseInitial, MetricConsumerPhase::After) => {
                    // The consumer is using the metric in the "after" phase, and the parameter is
                    // providing an "after" value, so we can resolve it to a metric provided the
                    // parameter index contains an "after" index.
                    match idx.after {
                        Some(after_idx) => Ok(MetricF64::ParameterAfterU64(after_idx)),
                        None => Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::AfterOrElseInitial, MetricConsumerPhase::Both) => {
                    // The consumer is using the metric in both "before" and "after" phases, and the
                    // parameter is providing an "after" value. However, they have specified that using any
                    // initial value is acceptable in the "before" phase, so we can resolve it to a metric provided the
                    // parameter index contains an "after" index.
                    match idx.after {
                        Some(after_idx) => Ok(MetricF64::ParameterAfterU64(after_idx)),
                        None => Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
            }
        }
    }
}

/// Resolve a [`ParameterIndex<MultiValue>`] to a [`MetricF64`] using the provided [`ParameterReturnValue`]
/// and [`MetricConsumerPhase`]. This function is used to determine if a parameter can be resolved to a metric
/// based on the phase in which the consumer is using the metric and the return value of the parameter.
///
/// If the parameter cannot be resolved to a metric, an error is returned.
fn resolve_parameter_index_multi_to_metric_f64(
    name: &ParameterName,
    idx: ParameterIndex<MultiValue>,
    key: &str,
    parameter_return_value: ParameterReturnValue,
    consumer_phase: MetricConsumerPhase,
) -> Result<MetricF64, MetricF64ResolutionError> {
    match idx {
        // Constant and simple can always be resolved to a metric, regardless of the consumer phase
        // as long as the parameter return value is "before".
        ParameterIndex::Const(index) => match parameter_return_value {
            ParameterReturnValue::Before => Ok(ConstantMetricF64::MultiParameterValue {
                index,
                key: key.to_string(),
            }
            .into()),
            ParameterReturnValue::After | ParameterReturnValue::AfterOrElseInitial => {
                Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                    parameter: name.clone(),
                    consumer_phase,
                    return_value: parameter_return_value,
                })
            }
        },
        ParameterIndex::Simple(index) => match parameter_return_value {
            ParameterReturnValue::Before => Ok(SimpleMetricF64::MultiParameterValue {
                index,
                key: key.to_string(),
            }
            .into()),
            ParameterReturnValue::After | ParameterReturnValue::AfterOrElseInitial => {
                Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                    parameter: name.clone(),
                    consumer_phase,
                    return_value: parameter_return_value,
                })
            }
        },
        // General parameters must be validated against the consumer phase to determine if they can be resolved to a metric.
        ParameterIndex::General(idx) => {
            match (parameter_return_value, consumer_phase) {
                (ParameterReturnValue::Before, MetricConsumerPhase::Before) => {
                    // The consumer is using the metric in the "before" phase, and the parameter is
                    // providing a "before" value, so we can resolve it to a metric provided the
                    // parameter index contains a "before" index.
                    match idx.before {
                        Some(before_idx) => Ok(MetricF64::ParameterBeforeMulti {
                            index: before_idx,
                            key: key.to_string(),
                        }),
                        None => Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::Before, MetricConsumerPhase::After) => {
                    // The consumer is using the metric in the "after" phase, but the parameter is
                    // providing a "before" value. This is fine because the "before" value is still
                    // valid in the "after" phase, so we can resolve it to a metric provided the
                    // parameter index contains a "before" index.
                    match idx.before {
                        Some(before_idx) => Ok(MetricF64::ParameterBeforeMulti {
                            index: before_idx,
                            key: key.to_string(),
                        }),
                        None => Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::Before, MetricConsumerPhase::Both) => {
                    // The consumer is using the metric in both "before" and "after" phases, and the
                    // parameter is providing a "before" value. This is fine because the "before"
                    // value is valid in both phases, so we can resolve it to a metric provided the
                    // parameter index contains a "before" index.
                    match idx.before {
                        Some(before_idx) => Ok(MetricF64::ParameterBeforeMulti {
                            index: before_idx,
                            key: key.to_string(),
                        }),
                        None => Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::After, MetricConsumerPhase::Before)
                | (ParameterReturnValue::After, MetricConsumerPhase::Both) => {
                    // The consumer is using the metric in the "before" phase, but the parameter is
                    // providing an "after" value. This is not valid because the "after" value is not
                    // valid in the "before" phase, so we cannot resolve it to a metric.
                    Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                        parameter: name.clone(),
                        consumer_phase,
                        return_value: parameter_return_value,
                    })
                }
                (ParameterReturnValue::After, MetricConsumerPhase::After) => {
                    // The consumer is using the metric in the "after" phase, and the parameter is
                    // providing an "after" value, so we can resolve it to a metric provided the
                    // parameter index contains an "after" index.
                    match idx.after {
                        Some(after_idx) => Ok(MetricF64::ParameterAfterMulti {
                            index: after_idx,
                            key: key.to_string(),
                        }),
                        None => Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::AfterOrElseInitial, MetricConsumerPhase::Before) => {
                    // The consumer is using the metric in the "before" phase, but the parameter is
                    // providing an "after" value. However, they have specified that using any
                    // initial value is acceptable, so we can resolve it to a metric provided the
                    // parameter index contains an "after" index.
                    match idx.after {
                        Some(after_idx) => Ok(MetricF64::ParameterAfterMulti {
                            index: after_idx,
                            key: key.to_string(),
                        }),
                        None => Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::AfterOrElseInitial, MetricConsumerPhase::After) => {
                    // The consumer is using the metric in the "after" phase, and the parameter is
                    // providing an "after" value, so we can resolve it to a metric provided the
                    // parameter index contains an "after" index.
                    match idx.after {
                        Some(after_idx) => Ok(MetricF64::ParameterAfterMulti {
                            index: after_idx,
                            key: key.to_string(),
                        }),
                        None => Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::AfterOrElseInitial, MetricConsumerPhase::Both) => {
                    // The consumer is using the metric in both "before" and "after" phases, and the
                    // parameter is providing an "after" value. However, they have specified that using any
                    // initial value is acceptable in the "before" phase, so we can resolve it to a metric provided the
                    // parameter index contains an "after" index.
                    match idx.after {
                        Some(after_idx) => Ok(MetricF64::ParameterAfterMulti {
                            index: after_idx,
                            key: key.to_string(),
                        }),
                        None => Err(MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum ConstantMetricU64Error {
    #[error("Simple parameter value error: {0}")]
    ConstParameterValuesError(#[from] ConstParameterValuesError),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConstantMetricU64 {
    IndexParameterValue(ConstParameterIndex<u64>),
    MultiParameterValue {
        index: ConstParameterIndex<MultiValue>,
        key: String,
    },
    Constant(u64),
}

impl ConstantMetricU64 {
    pub fn get_value(&self, values: &ConstParameterValues) -> Result<u64, ConstantMetricU64Error> {
        match self {
            ConstantMetricU64::IndexParameterValue(idx) => Ok(values.get_u64(*idx)?),
            ConstantMetricU64::MultiParameterValue { index, key } => Ok(values.get_multi_u64(*index, key)?),
            ConstantMetricU64::Constant(v) => Ok(*v),
        }
    }
}

#[derive(Debug, Error)]
pub enum SimpleMetricU64Error {
    #[error("Simple parameter value error: {0}")]
    SimpleParameterValuesError(#[from] SimpleParameterValuesError),
    #[error("Constant metric error: {0}")]
    ConstantMetricError(#[from] ConstantMetricU64Error),
    #[error("Cannot simplify metric to a constant metric")]
    CannotSimplifyMetric,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SimpleMetricU64 {
    IndexParameterValue {
        index: SimpleParameterIndex<u64>,
    },
    MultiParameterValue {
        index: SimpleParameterIndex<MultiValue>,
        key: String,
    },
    Constant(ConstantMetricU64),
}

impl SimpleMetricU64 {
    pub fn get_value(&self, values: &SimpleParameterValues) -> Result<u64, SimpleMetricU64Error> {
        match self {
            SimpleMetricU64::IndexParameterValue { index } => Ok(values.get_u64(*index)?),
            SimpleMetricU64::MultiParameterValue { index, key } => Ok(values.get_multi_u64(*index, key)?),
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
    #[error("General parameter with has no key: {key}")]
    GeneralMultiValueParameterKeyNotFound { key: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum MetricU64 {
    ParameterBeforeU64(GeneralBeforeValueIndex<u64>),
    ParameterAfterU64(GeneralAfterValueIndex<u64>),
    ParameterBeforeMulti {
        index: GeneralBeforeValueIndex<MultiValue>,
        key: String,
    },
    ParameterAfterMulti {
        index: GeneralAfterValueIndex<MultiValue>,
        key: String,
    },
    Simple(SimpleMetricU64),
    InterNetworkTransfer(MultiNetworkTransferIndex),
}

impl MetricU64 {
    pub fn get_value(&self, _network: &Network, state: &State) -> Result<u64, MetricU64Error> {
        match self {
            Self::ParameterBeforeU64(idx) => Ok(state.get_general_parameter_u64_before(*idx)?),
            Self::ParameterAfterU64(idx) => Ok(state.get_general_parameter_u64_after(*idx)?),
            Self::ParameterBeforeMulti { index, key } => {
                let mv = state.get_general_parameter_multi_before(*index)?;
                let value = mv
                    .get_index(key)
                    .ok_or_else(|| MetricU64Error::GeneralMultiValueParameterKeyNotFound { key: key.clone() })?;
                Ok(*value)
            }
            Self::ParameterAfterMulti { index, key } => {
                let mv = state.get_general_parameter_multi_after(*index)?;
                let value = mv
                    .get_index(key)
                    .ok_or_else(|| MetricU64Error::GeneralMultiValueParameterKeyNotFound { key: key.clone() })?;
                Ok(*value)
            }
            Self::Simple(s) => Ok(s.get_value(&state.get_simple_parameter_values())?),
            Self::InterNetworkTransfer(_idx) => todo!("Support usize for inter-network transfers"),
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

impl TryFrom<MetricU64> for ConstantMetricU64 {
    type Error = MetricU64Error;

    fn try_from(value: MetricU64) -> Result<Self, Self::Error> {
        let simple_metric: SimpleMetricU64 = value.try_into()?;
        let constant_metric: ConstantMetricU64 = simple_metric.try_into()?;
        Ok(constant_metric)
    }
}

/// Try to convert a slice of [`MetricU64`] into a vector of [`ConstantMetricU64`].
/// If any of the metrics cannot be converted, return `None`.
pub fn try_into_constant_metrics_u64(metrics: &[MetricU64]) -> Option<Vec<ConstantMetricU64>> {
    metrics
        .iter()
        .map(|m| m.clone().try_into())
        .collect::<Result<Vec<_>, _>>()
        .ok()
}

/// Try to convert a slice of [`MetricU64`] into a vector of [`SimpleMetricU64`].
/// If any of the metrics cannot be converted, return `None`.
pub fn try_into_simple_metrics_u64(metrics: &[MetricU64]) -> Option<Vec<SimpleMetricU64>> {
    metrics
        .iter()
        .map(|m| m.clone().try_into())
        .collect::<Result<Vec<_>, _>>()
        .ok()
}

#[derive(Debug, Error)]
pub enum MetricU64ResolutionError {
    #[error("Node not found when resolving U64 metric: {node}")]
    NodeNotFound { node: UnresolvedNode },
    #[error("Parameter not found when resolving U64 metric: {parameter}")]
    ParameterNotFound { parameter: ParameterName },
    #[error("Inter-network transfer not found when resolving U64 metric: {transfer}")]
    InterNetworkTransferNotFound { transfer: String },
    #[error(
        "Parameter not registered in the correct phase when resolving U64 metric: {parameter}, consumer phase: {consumer_phase:?}, return value: {return_value:?}"
    )]
    ParameterNotRegisteredInCorrectPhase {
        parameter: ParameterName,
        consumer_phase: MetricConsumerPhase,
        return_value: ParameterReturnValue,
    },
}

#[derive(Debug)]
pub enum UnresolvedMetricU64 {
    ParameterValue {
        name: ParameterName,
        return_value: ParameterReturnValue,
    },
    MultiParameterValue {
        name: ParameterName,
        key: String,
        return_value: ParameterReturnValue,
    },
    InterNetworkTransfer(String),
    Constant(u64),
}

impl UnresolvedMetricU64 {
    pub fn new_parameter_before<N: Into<ParameterName>>(name: N) -> Self {
        Self::ParameterValue {
            name: name.into(),
            return_value: ParameterReturnValue::Before,
        }
    }
    pub fn resolve(
        &self,
        resolution_maps: &ResolutionMaps,
        consumer_phase: MetricConsumerPhase,
    ) -> Result<MetricU64, MetricU64ResolutionError> {
        let m = match self {
            UnresolvedMetricU64::ParameterValue { name, return_value } => {
                let idx = resolution_maps.parameters_u64.get(name).ok_or_else(|| {
                    MetricU64ResolutionError::ParameterNotFound {
                        parameter: name.clone(),
                    }
                })?;

                resolve_parameter_index_u64_to_metric_u64(name, *idx, *return_value, consumer_phase)?
            }
            UnresolvedMetricU64::MultiParameterValue {
                name,
                key,
                return_value,
            } => {
                let idx = resolution_maps.parameters_multi.get(name).ok_or_else(|| {
                    MetricU64ResolutionError::ParameterNotFound {
                        parameter: name.clone(),
                    }
                })?;

                resolve_parameter_index_multi_to_metric_u64(name, idx.clone(), key, *return_value, consumer_phase)?
            }
            UnresolvedMetricU64::InterNetworkTransfer(unresolved) => {
                let idx = resolution_maps.inter_network_transfers.get(unresolved).ok_or_else(|| {
                    MetricU64ResolutionError::InterNetworkTransferNotFound {
                        transfer: unresolved.clone(),
                    }
                })?;

                MetricU64::InterNetworkTransfer(*idx)
            }
            UnresolvedMetricU64::Constant(value) => (*value).into(),
        };

        Ok(m)
    }
}

impl From<u64> for UnresolvedMetricU64 {
    fn from(v: u64) -> Self {
        Self::Constant(v)
    }
}

/// Resolve a [`ParameterIndex<u64>`] to a [`MetricU64`] using the provided [`ParameterReturnValue`]
/// and [`MetricConsumerPhase`]. This function is used to determine if a parameter can be resolved to a metric
/// based on the phase in which the consumer is using the metric and the return value of the parameter.
///
/// If the parameter cannot be resolved to a metric, an error is returned.
fn resolve_parameter_index_u64_to_metric_u64(
    name: &ParameterName,
    idx: ParameterIndex<u64>,
    parameter_return_value: ParameterReturnValue,
    consumer_phase: MetricConsumerPhase,
) -> Result<MetricU64, MetricU64ResolutionError> {
    match idx {
        // Constant and simple can always be resolved to a metric.
        ParameterIndex::Const(idx) => Ok(ConstantMetricU64::IndexParameterValue(idx).into()),
        ParameterIndex::Simple(idx) => Ok(SimpleMetricU64::IndexParameterValue { index: idx }.into()),
        // General parameters must be validated against the consumer phase to determine if they can be resolved to a metric.
        ParameterIndex::General(idx) => {
            match (parameter_return_value, consumer_phase) {
                (ParameterReturnValue::Before, MetricConsumerPhase::Before) => {
                    // The consumer is using the metric in the "before" phase, and the parameter is
                    // providing a "before" value, so we can resolve it to a metric provided the
                    // parameter index contains a "before" index.
                    match idx.before {
                        Some(before_idx) => Ok(MetricU64::ParameterBeforeU64(before_idx)),
                        None => Err(MetricU64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::Before, MetricConsumerPhase::After) => {
                    // The consumer is using the metric in the "after" phase, but the parameter is
                    // providing a "before" value. This is fine because the "before" value is still
                    // valid in the "after" phase, so we can resolve it to a metric provided the
                    // parameter index contains a "before" index.
                    match idx.before {
                        Some(before_idx) => Ok(MetricU64::ParameterBeforeU64(before_idx)),
                        None => Err(MetricU64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::Before, MetricConsumerPhase::Both) => {
                    // The consumer is using the metric in both "before" and "after" phases, and the
                    // parameter is providing a "before" value. This is fine because the "before"
                    // value is valid in both phases, so we can resolve it to a metric provided the
                    // parameter index contains a "before" index.
                    match idx.before {
                        Some(before_idx) => Ok(MetricU64::ParameterBeforeU64(before_idx)),
                        None => Err(MetricU64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::After, MetricConsumerPhase::Before)
                | (ParameterReturnValue::After, MetricConsumerPhase::Both) => {
                    // The consumer is using the metric in the "before" phase, but the parameter is
                    // providing an "after" value. This is not valid because the "after" value is not
                    // valid in the "before" phase, so we cannot resolve it to a metric.
                    Err(MetricU64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                        parameter: name.clone(),
                        consumer_phase,
                        return_value: parameter_return_value,
                    })
                }
                (ParameterReturnValue::After, MetricConsumerPhase::After) => {
                    // The consumer is using the metric in the "after" phase, and the parameter is
                    // providing an "after" value, so we can resolve it to a metric provided the
                    // parameter index contains an "after" index.
                    match idx.after {
                        Some(after_idx) => Ok(MetricU64::ParameterAfterU64(after_idx)),
                        None => Err(MetricU64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::AfterOrElseInitial, MetricConsumerPhase::Before) => {
                    // The consumer is using the metric in the "before" phase, but the parameter is
                    // providing an "after" value. However, they have specified that using any
                    // initial value is acceptable, so we can resolve it to a metric provided the
                    // parameter index contains an "after" index.
                    match idx.after {
                        Some(after_idx) => Ok(MetricU64::ParameterAfterU64(after_idx)),
                        None => Err(MetricU64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::AfterOrElseInitial, MetricConsumerPhase::After) => {
                    // The consumer is using the metric in the "after" phase, and the parameter is
                    // providing an "after" value, so we can resolve it to a metric provided the
                    // parameter index contains an "after" index.
                    match idx.after {
                        Some(after_idx) => Ok(MetricU64::ParameterAfterU64(after_idx)),
                        None => Err(MetricU64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::AfterOrElseInitial, MetricConsumerPhase::Both) => {
                    // The consumer is using the metric in both "before" and "after" phases, and the
                    // parameter is providing an "after" value. However, they have specified that using any
                    // initial value is acceptable in the "before" phase, so we can resolve it to a metric provided the
                    // parameter index contains an "after" index.
                    match idx.after {
                        Some(after_idx) => Ok(MetricU64::ParameterAfterU64(after_idx)),
                        None => Err(MetricU64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
            }
        }
    }
}

/// Resolve a [`ParameterIndex<MultiValue>`] to a [`MetricU64`] using the provided [`ParameterReturnValue`]
/// and [`MetricConsumerPhase`]. This function is used to determine if a parameter can be resolved to a metric
/// based on the phase in which the consumer is using the metric and the return value of the parameter.
///
/// If the parameter cannot be resolved to a metric, an error is returned.
fn resolve_parameter_index_multi_to_metric_u64(
    name: &ParameterName,
    idx: ParameterIndex<MultiValue>,
    key: &str,
    parameter_return_value: ParameterReturnValue,
    consumer_phase: MetricConsumerPhase,
) -> Result<MetricU64, MetricU64ResolutionError> {
    match idx {
        // Constant and simple can always be resolved to a metric.
        ParameterIndex::Const(index) => Ok(ConstantMetricU64::MultiParameterValue {
            index,
            key: key.to_string(),
        }
        .into()),
        ParameterIndex::Simple(index) => Ok(SimpleMetricU64::MultiParameterValue {
            index,
            key: key.to_string(),
        }
        .into()),
        // General parameters must be validated against the consumer phase to determine if they can be resolved to a metric.
        ParameterIndex::General(idx) => {
            match (parameter_return_value, consumer_phase) {
                (ParameterReturnValue::Before, MetricConsumerPhase::Before) => {
                    // The consumer is using the metric in the "before" phase, and the parameter is
                    // providing a "before" value, so we can resolve it to a metric provided the
                    // parameter index contains a "before" index.
                    match idx.before {
                        Some(before_idx) => Ok(MetricU64::ParameterBeforeMulti {
                            index: before_idx,
                            key: key.to_string(),
                        }),
                        None => Err(MetricU64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::Before, MetricConsumerPhase::After) => {
                    // The consumer is using the metric in the "after" phase, but the parameter is
                    // providing a "before" value. This is fine because the "before" value is still
                    // valid in the "after" phase, so we can resolve it to a metric provided the
                    // parameter index contains a "before" index.
                    match idx.before {
                        Some(before_idx) => Ok(MetricU64::ParameterBeforeMulti {
                            index: before_idx,
                            key: key.to_string(),
                        }),
                        None => Err(MetricU64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::Before, MetricConsumerPhase::Both) => {
                    // The consumer is using the metric in both "before" and "after" phases, and the
                    // parameter is providing a "before" value. This is fine because the "before"
                    // value is valid in both phases, so we can resolve it to a metric provided the
                    // parameter index contains a "before" index.
                    match idx.before {
                        Some(before_idx) => Ok(MetricU64::ParameterBeforeMulti {
                            index: before_idx,
                            key: key.to_string(),
                        }),
                        None => Err(MetricU64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::After, MetricConsumerPhase::Before)
                | (ParameterReturnValue::After, MetricConsumerPhase::Both) => {
                    // The consumer is using the metric in the "before" phase, but the parameter is
                    // providing an "after" value. This is not valid because the "after" value is not
                    // valid in the "before" phase, so we cannot resolve it to a metric.
                    Err(MetricU64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                        parameter: name.clone(),
                        consumer_phase,
                        return_value: parameter_return_value,
                    })
                }
                (ParameterReturnValue::After, MetricConsumerPhase::After) => {
                    // The consumer is using the metric in the "after" phase, and the parameter is
                    // providing an "after" value, so we can resolve it to a metric provided the
                    // parameter index contains an "after" index.
                    match idx.after {
                        Some(after_idx) => Ok(MetricU64::ParameterAfterMulti {
                            index: after_idx,
                            key: key.to_string(),
                        }),
                        None => Err(MetricU64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::AfterOrElseInitial, MetricConsumerPhase::Before) => {
                    // The consumer is using the metric in the "before" phase, but the parameter is
                    // providing an "after" value. However, they have specified that using any
                    // initial value is acceptable, so we can resolve it to a metric provided the
                    // parameter index contains an "after" index.
                    match idx.after {
                        Some(after_idx) => Ok(MetricU64::ParameterAfterMulti {
                            index: after_idx,
                            key: key.to_string(),
                        }),
                        None => Err(MetricU64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::AfterOrElseInitial, MetricConsumerPhase::After) => {
                    // The consumer is using the metric in the "after" phase, and the parameter is
                    // providing an "after" value, so we can resolve it to a metric provided the
                    // parameter index contains an "after" index.
                    match idx.after {
                        Some(after_idx) => Ok(MetricU64::ParameterAfterMulti {
                            index: after_idx,
                            key: key.to_string(),
                        }),
                        None => Err(MetricU64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
                (ParameterReturnValue::AfterOrElseInitial, MetricConsumerPhase::Both) => {
                    // The consumer is using the metric in both "before" and "after" phases, and the
                    // parameter is providing an "after" value. However, they have specified that using any
                    // initial value is acceptable in the "before" phase, so we can resolve it to a metric provided the
                    // parameter index contains an "after" index.
                    match idx.after {
                        Some(after_idx) => Ok(MetricU64::ParameterAfterMulti {
                            index: after_idx,
                            key: key.to_string(),
                        }),
                        None => Err(MetricU64ResolutionError::ParameterNotRegisteredInCorrectPhase {
                            parameter: name.clone(),
                            consumer_phase,
                            return_value: parameter_return_value,
                        }),
                    }
                }
            }
        }
    }
}
