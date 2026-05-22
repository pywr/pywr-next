use crate::metric::{
    ConstantMetricF64Error, MetricF64, MetricF64Error, MetricF64ResolutionError, SimpleMetricF64, SimpleMetricF64Error,
    UnresolvedMetricF64,
};
use crate::network::{EdgeIndex, Network, NodeIndex, ResolutionMaps, VirtualStorageIndex};
use crate::state::{ConstParameterValues, NetworkStateError, NodeState, SimpleParameterValues, State, StateError};
use crate::timestep::Timestep;
use std::fmt::{Display, Formatter};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NodeError {
    #[error("Flow constraints are undefined for this node type")]
    FlowConstraintsUndefined,
    #[error("Storage constraints are undefined for this node type")]
    StorageConstraintsUndefined,
    #[error("F64 metric error: {0}")]
    MetricF64Error(#[from] MetricF64Error),
    #[error("F64 simple metric error: {0}")]
    SimpleMetricF64Error(#[from] SimpleMetricF64Error),
    #[error("F64 constant metric error: {0}")]
    ConstantMetricF64Error(#[from] ConstantMetricF64Error),
    #[error("Invalid node connection to input node.")]
    InvalidNodeConnectionToInput,
    #[error("Input node has no incoming edges.")]
    InputNodeHasNoIncomingEdges,
    #[error("Invalid node connection from output node.")]
    InvalidNodeConnectionFromOutput,
    #[error("Output node has no outgoing edges.")]
    OutputNodeHasNoOutgoingEdges,
    #[error("No virtual storage on storage node")]
    NoVirtualStorageOnStorageNode,
    #[error("Network state error: {0}")]
    NetworkStateError(#[from] NetworkStateError),
    #[error("State error: {0}")]
    StateError(#[from] StateError),
    #[error("Virtual storage index not found: {0}")]
    VirtualStorageIndexNotFound(VirtualStorageIndex),
    #[error("Node index not found: {0}")]
    NodeIndexNotFound(NodeIndex),
}

#[derive(Debug, Error)]
pub enum NodeBuilderError {
    #[error("Index not found in resolution map.")]
    IndexNotFound,
    #[error("Could not resolve f64 metric for `{attr}` attribute: {source}")]
    ResolveMetricF64Error {
        attr: String,
        #[source]
        source: MetricF64ResolutionError,
    },
    #[error("Could not simplify f64 metric for `{attr}`: {source}")]
    CouldNotSimplifyMetricF64 {
        attr: String,
        #[source]
        source: MetricF64Error,
    },
    #[error("Initial volume not defined.")]
    InitialVolumeNotDefined,
    #[error("Node type `{node_type}` must have at-least one outgoing edge.")]
    NoOutgoingEdges { node_type: NodeType },
    #[error("Node type `{node_type}` must have at-least one incoming edge.")]
    NoIncomingEdges { node_type: NodeType },
    #[error("Node type `{node_type}` must not have any outgoing edges ({num} found).")]
    UnexpectedOutgoingEdges { node_type: NodeType, num: usize },
    #[error("Node type `{node_type}` must not have any incoming edges ({num} found).")]
    UnexpectedIncomingEdges { node_type: NodeType, num: usize },
}

pub struct NodeBuilder {
    name: UnresolvedNode,
    node_type: NodeType,
    cost: Option<UnresolvedMetricF64>,
    cost_agg_func: Option<CostAggFunc>,
    min_flow: Option<UnresolvedMetricF64>,
    max_flow: Option<UnresolvedMetricF64>,
    initial_volume: Option<UnresolvedStorageInitialVolume>,
    max_volume: Option<UnresolvedMetricF64>,
    min_volume: Option<UnresolvedMetricF64>,
}

impl NodeBuilder {
    /// The name of node that will be built.
    pub fn name(&self) -> &UnresolvedNode {
        &self.name
    }
    pub fn input(name: &str) -> Self {
        Self::new(name, NodeType::Input)
    }

    pub fn output(name: &str) -> Self {
        Self::new(name, NodeType::Output)
    }

    pub fn link(name: &str) -> Self {
        Self::new(name, NodeType::Link)
    }

    pub fn storage(name: &str) -> Self {
        Self::new(name, NodeType::Storage)
    }
    pub fn new(name: &str, node_type: NodeType) -> Self {
        let meta = UnresolvedNode {
            name: name.to_string(),
            sub_name: None,
        };
        Self {
            name: meta,
            node_type,
            cost: None,
            cost_agg_func: None,
            max_flow: None,
            min_flow: None,
            initial_volume: None,
            min_volume: None,
            max_volume: None,
        }
    }

    pub fn sub_name(&mut self, sub_name: &str) -> &mut Self {
        self.name.sub_name = Some(sub_name.to_string());
        self
    }

    pub fn cost(&mut self, cost: UnresolvedMetricF64) -> &mut Self {
        self.cost = Some(cost);
        self
    }

    pub fn cost_agg_func(&mut self, cost_agg_func: CostAggFunc) -> &mut Self {
        self.cost_agg_func = Some(cost_agg_func);
        self
    }

    pub fn min_flow(&mut self, min_flow: UnresolvedMetricF64) -> &mut Self {
        self.min_flow = Some(min_flow);
        self
    }

    pub fn max_flow(&mut self, max_flow: UnresolvedMetricF64) -> &mut Self {
        self.max_flow = Some(max_flow);
        self
    }

    pub fn initial_volume(&mut self, initial_volume: UnresolvedStorageInitialVolume) -> &mut Self {
        self.initial_volume = Some(initial_volume);
        self
    }

    pub fn max_volume(&mut self, max_volume: UnresolvedMetricF64) -> &mut Self {
        self.max_volume = Some(max_volume);
        self
    }

    pub fn min_volume(&mut self, min_volume: UnresolvedMetricF64) -> &mut Self {
        self.min_volume = Some(min_volume);
        self
    }

    fn build_cost(&self, resolution_maps: &ResolutionMaps) -> Result<NodeCost, NodeBuilderError> {
        let index = resolution_maps
            .nodes
            .get(&self.name)
            .ok_or(NodeBuilderError::IndexNotFound)?;

        let local = match &self.cost {
            Some(cost) => cost
                .resolve(resolution_maps)
                .map_err(|source| NodeBuilderError::ResolveMetricF64Error {
                    attr: "cost".to_string(),
                    source,
                })?,
            None => 0.0.into(),
        };

        let virtual_storage_nodes = resolution_maps
            .virtual_storage_associated_nodes
            .get(index)
            .cloned()
            .unwrap_or_default();

        let cost = NodeCost {
            local: Some(local),
            virtual_storage_nodes,
            agg_func: self.cost_agg_func,
        };

        Ok(cost)
    }

    /// Build a [`FlowConstraints`] from the builder.
    fn build_flow_constraints(&self, resolution_maps: &ResolutionMaps) -> Result<FlowConstraints, NodeBuilderError> {
        let min_flow = self
            .min_flow
            .as_ref()
            .map(|min_flow| {
                min_flow
                    .resolve(resolution_maps)
                    .map_err(|source| NodeBuilderError::ResolveMetricF64Error {
                        attr: "min_flow".to_string(),
                        source,
                    })
            })
            .transpose()?;

        let max_flow = self
            .max_flow
            .as_ref()
            .map(|max_flow| {
                max_flow
                    .resolve(resolution_maps)
                    .map_err(|source| NodeBuilderError::ResolveMetricF64Error {
                        attr: "max_flow".to_string(),
                        source,
                    })
            })
            .transpose()?;

        let flow_constraints = FlowConstraints::new(min_flow, max_flow);

        Ok(flow_constraints)
    }

    /// Build a [`StorageConstraints`] from the builder.
    fn build_storage_constraints(
        &self,
        resolution_maps: &ResolutionMaps,
    ) -> Result<StorageConstraints, NodeBuilderError> {
        let min_volume = self
            .min_volume
            .as_ref()
            .map(|min_volume| {
                min_volume
                    .resolve(resolution_maps)
                    .map_err(|source| NodeBuilderError::ResolveMetricF64Error {
                        attr: "min_volume".to_string(),
                        source,
                    })?
                    .try_into()
                    .map_err(|source| NodeBuilderError::CouldNotSimplifyMetricF64 {
                        attr: "max_volume".to_string(),
                        source,
                    })
            })
            .transpose()?;

        let max_volume = self
            .max_volume
            .as_ref()
            .map(|max_volume| {
                max_volume
                    .resolve(resolution_maps)
                    .map_err(|source| NodeBuilderError::ResolveMetricF64Error {
                        attr: "max_volume".to_string(),
                        source,
                    })?
                    .try_into()
                    .map_err(|source| NodeBuilderError::CouldNotSimplifyMetricF64 {
                        attr: "max_volume".to_string(),
                        source,
                    })
            })
            .transpose()?;

        let storage_constraints = StorageConstraints { min_volume, max_volume };

        Ok(storage_constraints)
    }

    fn build_storage_initial_volume(
        &self,
        resolution_maps: &ResolutionMaps,
    ) -> Result<StorageInitialVolume, NodeBuilderError> {
        match &self.initial_volume {
            Some(iv) => match iv {
                UnresolvedStorageInitialVolume::Absolute(iv) => Ok(StorageInitialVolume::Absolute(*iv)),
                UnresolvedStorageInitialVolume::Proportional(iv) => Ok(StorageInitialVolume::Proportional(*iv)),
                UnresolvedStorageInitialVolume::DistributedAbsolute {
                    absolute,
                    prior_max_volume,
                } => {
                    let prior_max_volume = prior_max_volume
                        .resolve(resolution_maps)
                        .map_err(|source| NodeBuilderError::ResolveMetricF64Error {
                            attr: "prior_max_volume".to_string(),
                            source,
                        })?
                        .try_into()
                        .map_err(|source| NodeBuilderError::CouldNotSimplifyMetricF64 {
                            attr: "prior_max_volume".to_string(),
                            source,
                        })?;
                    Ok(StorageInitialVolume::DistributedAbsolute {
                        absolute: *absolute,
                        prior_max_volume,
                    })
                }
                UnresolvedStorageInitialVolume::DistributedProportional {
                    proportion,
                    total_volume,
                    prior_max_volume,
                } => {
                    let total_volume = total_volume
                        .resolve(resolution_maps)
                        .map_err(|source| NodeBuilderError::ResolveMetricF64Error {
                            attr: "total_volume".to_string(),
                            source,
                        })?
                        .try_into()
                        .map_err(|source| NodeBuilderError::CouldNotSimplifyMetricF64 {
                            attr: "total_volume".to_string(),
                            source,
                        })?;
                    let prior_max_volume = prior_max_volume
                        .resolve(resolution_maps)
                        .map_err(|source| NodeBuilderError::ResolveMetricF64Error {
                            attr: "prior_max_volume".to_string(),
                            source,
                        })?
                        .try_into()
                        .map_err(|source| NodeBuilderError::CouldNotSimplifyMetricF64 {
                            attr: "prior_max_volume".to_string(),
                            source,
                        })?;

                    Ok(StorageInitialVolume::DistributedProportional {
                        total_volume,
                        proportion: *proportion,
                        prior_max_volume,
                    })
                }
            },
            None => Err(NodeBuilderError::InitialVolumeNotDefined),
        }
    }
    pub fn build(&self, resolution_maps: &ResolutionMaps) -> Result<Node, NodeBuilderError> {
        let index = resolution_maps
            .nodes
            .get(&self.name)
            .ok_or(NodeBuilderError::IndexNotFound)?;

        let meta = NodeMeta {
            index: *index,
            name: self.name.name.clone(),
            sub_name: self.name.sub_name.clone(),
        };

        let cost = self.build_cost(resolution_maps)?;

        let incoming_edges = resolution_maps.incoming_edges.get(index).cloned().unwrap_or_default();
        let outgoing_edges = resolution_maps.outgoing_edges.get(index).cloned().unwrap_or_default();

        let node = match self.node_type {
            NodeType::Input => {
                if outgoing_edges.is_empty() {
                    return Err(NodeBuilderError::NoOutgoingEdges {
                        node_type: self.node_type,
                    });
                }
                if !incoming_edges.is_empty() {
                    return Err(NodeBuilderError::UnexpectedIncomingEdges {
                        node_type: self.node_type,
                        num: incoming_edges.len(),
                    });
                }

                Node::Input(InputNode {
                    meta,
                    cost,
                    flow_constraints: self.build_flow_constraints(resolution_maps)?,
                    outgoing_edges,
                })
            }
            NodeType::Output => {
                if !outgoing_edges.is_empty() {
                    return Err(NodeBuilderError::UnexpectedOutgoingEdges {
                        node_type: self.node_type,
                        num: outgoing_edges.len(),
                    });
                }
                if incoming_edges.is_empty() {
                    return Err(NodeBuilderError::NoIncomingEdges {
                        node_type: self.node_type,
                    });
                }

                Node::Output(OutputNode {
                    meta,
                    cost,
                    flow_constraints: self.build_flow_constraints(resolution_maps)?,
                    incoming_edges,
                })
            }
            NodeType::Link => {
                if outgoing_edges.is_empty() {
                    return Err(NodeBuilderError::NoOutgoingEdges {
                        node_type: self.node_type,
                    });
                }
                if incoming_edges.is_empty() {
                    return Err(NodeBuilderError::NoIncomingEdges {
                        node_type: self.node_type,
                    });
                }

                Node::Link(LinkNode {
                    meta,
                    cost,
                    flow_constraints: Default::default(),
                    incoming_edges,
                    outgoing_edges,
                })
            }
            NodeType::Storage => Node::Storage(StorageNode {
                meta,
                cost,
                initial_volume: self.build_storage_initial_volume(resolution_maps)?,
                incoming_edges,
                outgoing_edges,
                storage_constraints: self.build_storage_constraints(resolution_maps)?,
            }),
        };

        Ok(node)
    }
}

#[derive(Debug, PartialEq)]
pub enum Node {
    Input(InputNode),
    Output(OutputNode),
    Link(LinkNode),
    Storage(StorageNode),
}

#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub enum NodeType {
    Input,
    Output,
    Link,
    Storage,
}

impl Display for NodeType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeType::Input => write!(f, "Input"),
            NodeType::Output => write!(f, "Output"),
            NodeType::Link => write!(f, "Link"),
            NodeType::Storage => write!(f, "Storage"),
        }
    }
}

/// Bounds for the flow of a node
#[derive(Debug, Clone, Copy)]
pub struct FlowBounds {
    /// Minimum flow
    pub min_flow: f64,
    /// Maximum flow
    pub max_flow: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct VolumeBounds {
    /// Available volume (i.e. max that can be removed)
    pub available: f64,
    /// Missing volume (i.e. max that can be added)
    pub missing: f64,
}

pub enum NodeBounds {
    Flow(FlowBounds),
    Volume(VolumeBounds),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostAggFunc {
    Sum,
    Max,
    Min,
}

impl Node {
    /// Get a node's name
    pub fn name(&self) -> &str {
        match self {
            Self::Input(n) => n.meta.name(),
            Self::Output(n) => n.meta.name(),
            Self::Link(n) => n.meta.name(),
            Self::Storage(n) => n.meta.name(),
        }
    }

    /// Get a node's sub_name
    pub fn sub_name(&self) -> Option<&str> {
        match self {
            Self::Input(n) => n.meta.sub_name(),
            Self::Output(n) => n.meta.sub_name(),
            Self::Link(n) => n.meta.sub_name(),
            Self::Storage(n) => n.meta.sub_name(),
        }
    }

    /// Get a node's full name
    pub fn full_name(&self) -> (&str, Option<&str>) {
        match self {
            Self::Input(n) => n.meta.full_name(),
            Self::Output(n) => n.meta.full_name(),
            Self::Link(n) => n.meta.full_name(),
            Self::Storage(n) => n.meta.full_name(),
        }
    }

    /// Get a node's index
    pub fn index(&self) -> NodeIndex {
        match self {
            Self::Input(n) => n.meta.index,
            Self::Output(n) => n.meta.index,
            Self::Link(n) => n.meta.index,
            Self::Storage(n) => n.meta.index,
        }
    }

    pub fn node_type(&self) -> NodeType {
        match self {
            Self::Input(_) => NodeType::Input,
            Self::Output(_) => NodeType::Output,
            Self::Link(_) => NodeType::Link,
            Self::Storage(_) => NodeType::Storage,
        }
    }

    pub fn apply<F>(&self, f: F)
    where
        F: Fn(&Node),
    {
        f(self);
    }

    pub fn default_state(&self) -> NodeState {
        match self {
            Self::Input(_n) => NodeState::new_flow_state(),
            Self::Output(_n) => NodeState::new_flow_state(),
            Self::Link(_n) => NodeState::new_flow_state(),
            Self::Storage(_n) => NodeState::new_storage_state(0.0, 0.0),
        }
    }

    pub fn default_metric(&self) -> MetricF64 {
        match self {
            Self::Input(_n) => MetricF64::NodeOutFlow(self.index()),
            Self::Output(_n) => MetricF64::NodeInFlow(self.index()),
            Self::Link(_n) => MetricF64::NodeOutFlow(self.index()),
            Self::Storage(_n) => MetricF64::NodeVolume(self.index()),
        }
    }

    pub fn get_incoming_edges(&self) -> Result<&Vec<EdgeIndex>, NodeError> {
        match self {
            Self::Input(_) => Err(NodeError::InputNodeHasNoIncomingEdges),
            Self::Output(n) => Ok(&n.incoming_edges),
            Self::Link(n) => Ok(&n.incoming_edges),
            Self::Storage(n) => Ok(&n.incoming_edges),
        }
    }

    pub fn get_outgoing_edges(&self) -> Result<&Vec<EdgeIndex>, NodeError> {
        match self {
            Self::Input(n) => Ok(&n.outgoing_edges),
            Self::Output(_) => Err(NodeError::OutputNodeHasNoOutgoingEdges),
            Self::Link(n) => Ok(&n.outgoing_edges),
            Self::Storage(n) => Ok(&n.outgoing_edges),
        }
    }

    pub fn before(&self, timestep: &Timestep, state: &mut State) -> Result<(), NodeError> {
        // Currently only storage nodes do something during before
        match self {
            Node::Input(_) => Ok(()),
            Node::Output(_) => Ok(()),
            Node::Link(_) => Ok(()),
            Node::Storage(n) => n.before(timestep, state),
        }
    }

    pub fn get_min_flow(&self, network: &Network, state: &State) -> Result<f64, NodeError> {
        match self {
            Self::Input(n) => Ok(n.get_min_flow(network, state)?),
            Self::Link(n) => Ok(n.get_min_flow(network, state)?),
            Self::Output(n) => Ok(n.get_min_flow(network, state)?),
            Self::Storage(_) => Err(NodeError::FlowConstraintsUndefined),
        }
    }

    pub fn get_const_min_flow(&self, values: &ConstParameterValues) -> Result<Option<f64>, NodeError> {
        match self {
            Self::Input(n) => Ok(n.get_const_min_flow(values)?),
            Self::Link(n) => Ok(n.get_const_min_flow(values)?),
            Self::Output(n) => Ok(n.get_const_min_flow(values)?),
            Self::Storage(_) => Err(NodeError::FlowConstraintsUndefined),
        }
    }

    pub fn get_max_flow(&self, network: &Network, state: &State) -> Result<f64, NodeError> {
        match self {
            Self::Input(n) => Ok(n.get_max_flow(network, state)?),
            Self::Link(n) => Ok(n.get_max_flow(network, state)?),
            Self::Output(n) => Ok(n.get_max_flow(network, state)?),
            Self::Storage(_) => Err(NodeError::FlowConstraintsUndefined),
        }
    }

    pub fn get_const_max_flow(&self, values: &ConstParameterValues) -> Result<Option<f64>, NodeError> {
        match self {
            Self::Input(n) => Ok(n.get_const_max_flow(values)?),
            Self::Link(n) => Ok(n.get_const_max_flow(values)?),
            Self::Output(n) => Ok(n.get_const_max_flow(values)?),
            Self::Storage(_) => Err(NodeError::FlowConstraintsUndefined),
        }
    }

    pub fn is_max_flow_unconstrained(&self) -> Result<bool, NodeError> {
        match self {
            Self::Input(n) => Ok(n.is_max_flow_unconstrained()),
            Self::Link(n) => Ok(n.is_max_flow_unconstrained()),
            Self::Output(n) => Ok(n.is_max_flow_unconstrained()),
            Self::Storage(_) => Err(NodeError::FlowConstraintsUndefined),
        }
    }

    pub fn get_min_volume(&self, state: &State) -> Result<f64, NodeError> {
        match self {
            Self::Input(_) => Err(NodeError::StorageConstraintsUndefined),
            Self::Link(_) => Err(NodeError::StorageConstraintsUndefined),
            Self::Output(_) => Err(NodeError::StorageConstraintsUndefined),
            Self::Storage(n) => Ok(n.get_min_volume(state)?),
        }
    }

    pub fn get_max_volume(&self, state: &State) -> Result<f64, NodeError> {
        match self {
            Self::Input(_) => Err(NodeError::StorageConstraintsUndefined),
            Self::Link(_) => Err(NodeError::StorageConstraintsUndefined),
            Self::Output(_) => Err(NodeError::StorageConstraintsUndefined),
            Self::Storage(n) => Ok(n.get_max_volume(state)?),
        }
    }

    /// Return the current min and max volumes as a tuple.
    pub fn get_volume_bounds(&self, state: &State) -> Result<(f64, f64), NodeError> {
        match (self.get_min_volume(state), self.get_max_volume(state)) {
            (Ok(min_vol), Ok(max_vol)) => Ok((min_vol, max_vol)),
            _ => Err(NodeError::StorageConstraintsUndefined),
        }
    }

    /// Get constant bounds for the node, if they exist, depending on its type.
    ///
    /// Note that [`Node::Storage`] nodes can never have constant bounds.
    pub fn get_const_bounds(&self, values: &ConstParameterValues) -> Result<Option<NodeBounds>, NodeError> {
        match self {
            Self::Input(n) => {
                let min_flow = n.get_const_min_flow(values)?;
                let max_flow = n.get_const_max_flow(values)?;

                match (min_flow, max_flow) {
                    (Some(min_flow), Some(max_flow)) => Ok(Some(NodeBounds::Flow(FlowBounds { min_flow, max_flow }))),
                    _ => Ok(None),
                }
            }
            Self::Output(n) => {
                let min_flow = n.get_const_min_flow(values)?;
                let max_flow = n.get_const_max_flow(values)?;

                match (min_flow, max_flow) {
                    (Some(min_flow), Some(max_flow)) => Ok(Some(NodeBounds::Flow(FlowBounds { min_flow, max_flow }))),
                    _ => Ok(None),
                }
            }
            Self::Link(n) => {
                let min_flow = n.get_const_min_flow(values)?;
                let max_flow = n.get_const_max_flow(values)?;

                match (min_flow, max_flow) {
                    (Some(min_flow), Some(max_flow)) => Ok(Some(NodeBounds::Flow(FlowBounds { min_flow, max_flow }))),
                    _ => Ok(None),
                }
            }
            Self::Storage(_) => Ok(None),
        }
    }

    /// Get bounds for the node depending on its type.
    pub fn get_bounds(&self, network: &Network, state: &State) -> Result<NodeBounds, NodeError> {
        match self {
            Self::Input(n) => Ok(NodeBounds::Flow(FlowBounds {
                min_flow: n.flow_constraints.get_min_flow(network, state)?,
                max_flow: n.flow_constraints.get_max_flow(network, state)?,
            })),
            Self::Output(n) => Ok(NodeBounds::Flow(FlowBounds {
                min_flow: n.flow_constraints.get_min_flow(network, state)?,
                max_flow: n.flow_constraints.get_max_flow(network, state)?,
            })),
            Self::Link(n) => Ok(NodeBounds::Flow(FlowBounds {
                min_flow: n.flow_constraints.get_min_flow(network, state)?,
                max_flow: n.flow_constraints.get_max_flow(network, state)?,
            })),
            Self::Storage(n) => {
                let current_volume = state.get_network_state().get_node_volume(&n.meta.index)?;

                let available = current_volume - n.get_min_volume(state)?;
                let missing = n.get_max_volume(state)? - current_volume;

                Ok(NodeBounds::Volume(VolumeBounds { available, missing }))
            }
        }
    }

    pub fn get_outgoing_cost(&self, network: &Network, state: &State) -> Result<f64, NodeError> {
        match self {
            Self::Input(n) => n.get_cost(network, state),
            Self::Link(n) => Ok(n.get_cost(network, state)? / 2.0),
            Self::Output(n) => n.get_cost(network, state),
            Self::Storage(n) => Ok(-n.get_cost(network, state)?),
        }
    }

    pub fn get_incoming_cost(&self, network: &Network, state: &State) -> Result<f64, NodeError> {
        match self {
            Self::Input(n) => n.get_cost(network, state),
            Self::Link(n) => Ok(n.get_cost(network, state)? / 2.0),
            Self::Output(n) => n.get_cost(network, state),
            Self::Storage(n) => Ok(n.get_cost(network, state)?),
        }
    }
}

/// Metadata for a node without its index.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UnresolvedNode {
    name: String,
    sub_name: Option<String>,
}

impl From<&str> for UnresolvedNode {
    fn from(name: &str) -> Self {
        Self {
            name: name.to_string(),
            sub_name: None,
        }
    }
}

impl UnresolvedNode {
    pub fn new(name: &str, sub_name: Option<&str>) -> Self {
        Self {
            name: name.to_string(),
            sub_name: sub_name.map(|s| s.to_string()),
        }
    }

    pub fn set_sub_name(&mut self, sub_name: Option<&str>) {
        self.sub_name = sub_name.map(|s| s.to_string());
    }
}

impl Display for UnresolvedNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.sub_name {
            Some(sub) => write!(f, "{}[{}]", self.name, sub),
            None => write!(f, "{}", self.name),
        }
    }
}

/// Meta data common to all nodes.
#[derive(Debug, PartialEq, Eq)]
pub struct NodeMeta<T> {
    index: T,
    name: String,
    sub_name: Option<String>,
}

impl<T> NodeMeta<T> {
    pub fn from_unresolved_name(name: UnresolvedNode, index: T) -> Self {
        Self {
            name: name.name,
            index,
            sub_name: name.sub_name,
        }
    }
}

impl<T> NodeMeta<T>
where
    T: Copy,
{
    pub fn index(&self) -> &T {
        &self.index
    }
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
    pub fn sub_name(&self) -> Option<&str> {
        self.sub_name.as_deref()
    }
    pub fn full_name(&self) -> (&str, Option<&str>) {
        (self.name(), self.sub_name())
    }
}

#[derive(Debug, PartialEq, Default)]
pub struct FlowConstraints {
    min_flow: Option<MetricF64>,
    max_flow: Option<MetricF64>,
}

impl FlowConstraints {
    pub fn new(min_flow: Option<MetricF64>, max_flow: Option<MetricF64>) -> Self {
        Self { min_flow, max_flow }
    }

    /// Return the current minimum flow from the parameter state
    ///
    /// Defaults to zero if no parameter is defined.
    pub fn get_min_flow(&self, network: &Network, state: &State) -> Result<f64, MetricF64Error> {
        match &self.min_flow {
            None => Ok(0.0),
            Some(m) => m.get_value(network, state),
        }
    }

    /// Return the constant minimum flow if it exists.
    ///
    /// Defaults to zero if no parameter is defined.
    pub fn get_const_min_flow(&self, values: &ConstParameterValues) -> Result<Option<f64>, ConstantMetricF64Error> {
        match &self.min_flow {
            None => Ok(Some(0.0)),
            Some(m) => m.try_get_constant_value(values),
        }
    }
    /// Return the current maximum flow from the parameter state
    ///
    /// Defaults to [`f64::MAX`] if no parameter is defined.
    pub fn get_max_flow(&self, network: &Network, state: &State) -> Result<f64, MetricF64Error> {
        match &self.max_flow {
            None => Ok(f64::MAX),
            Some(m) => m.get_value(network, state),
        }
    }

    /// Return the constant maximum flow if it exists.
    ///
    /// Defaults to [`f64::MAX`] if no parameter is defined.
    pub fn get_const_max_flow(&self, values: &ConstParameterValues) -> Result<Option<f64>, ConstantMetricF64Error> {
        match &self.max_flow {
            None => Ok(Some(f64::MAX)),
            Some(m) => m.try_get_constant_value(values),
        }
    }

    pub fn is_max_flow_unconstrained(&self) -> bool {
        self.max_flow.is_none()
    }
}

#[derive(Debug, PartialEq)]
pub struct StorageConstraints {
    min_volume: Option<SimpleMetricF64>,
    max_volume: Option<SimpleMetricF64>,
}

impl StorageConstraints {
    pub fn new(min_volume: Option<SimpleMetricF64>, max_volume: Option<SimpleMetricF64>) -> Self {
        Self { min_volume, max_volume }
    }

    /// Return the current minimum volume from the parameter state
    ///
    /// Defaults to zero if no parameter is defined.
    pub fn get_min_volume(&self, values: &SimpleParameterValues) -> Result<f64, SimpleMetricF64Error> {
        match &self.min_volume {
            None => Ok(0.0),
            Some(m) => m.get_value(values),
        }
    }
    /// Return the current maximum volume from the metric state
    ///
    /// Defaults to f64::MAX if no parameter is defined.
    pub fn get_max_volume(&self, values: &SimpleParameterValues) -> Result<f64, SimpleMetricF64Error> {
        match &self.max_volume {
            None => Ok(f64::MAX),
            Some(m) => m.get_value(values),
        }
    }
}

/// Generic cost data for a node.
#[derive(Debug, PartialEq, Default)]
pub struct NodeCost {
    local: Option<MetricF64>,
    virtual_storage_nodes: Vec<VirtualStorageIndex>,
    agg_func: Option<CostAggFunc>,
}

impl NodeCost {
    fn get_cost(&self, network: &Network, state: &State) -> Result<f64, NodeError> {
        // Initial local cost that has any virtual storage cost applied
        let mut cost = match &self.local {
            None => Ok(0.0),
            Some(m) => m.get_value(network, state),
        }?;

        if let Some(agg_func) = &self.agg_func {
            let vs_costs = self.virtual_storage_nodes.iter().map(|idx| {
                let vs = network
                    .get_virtual_storage_node(idx)
                    .ok_or(NodeError::VirtualStorageIndexNotFound(*idx))?;
                Ok::<_, NodeError>(vs.get_cost(network, state)?)
            });

            match agg_func {
                CostAggFunc::Sum => {
                    for vs_cost in vs_costs {
                        cost += vs_cost?;
                    }
                }
                CostAggFunc::Max => {
                    for vs_cost in vs_costs {
                        cost = cost.max(vs_cost?);
                    }
                }
                CostAggFunc::Min => {
                    for vs_cost in vs_costs {
                        cost = cost.min(vs_cost?);
                    }
                }
            };
        }

        Ok(cost)
    }
}

#[derive(Debug, PartialEq)]
pub struct InputNode {
    meta: NodeMeta<NodeIndex>,
    cost: NodeCost,
    flow_constraints: FlowConstraints,
    outgoing_edges: Vec<EdgeIndex>,
}

impl InputNode {
    fn get_cost(&self, network: &Network, state: &State) -> Result<f64, NodeError> {
        self.cost.get_cost(network, state)
    }

    fn get_min_flow(&self, network: &Network, state: &State) -> Result<f64, MetricF64Error> {
        self.flow_constraints.get_min_flow(network, state)
    }
    fn get_const_min_flow(&self, values: &ConstParameterValues) -> Result<Option<f64>, ConstantMetricF64Error> {
        self.flow_constraints.get_const_min_flow(values)
    }

    fn get_max_flow(&self, network: &Network, state: &State) -> Result<f64, MetricF64Error> {
        self.flow_constraints.get_max_flow(network, state)
    }
    fn get_const_max_flow(&self, values: &ConstParameterValues) -> Result<Option<f64>, ConstantMetricF64Error> {
        self.flow_constraints.get_const_max_flow(values)
    }
    fn is_max_flow_unconstrained(&self) -> bool {
        self.flow_constraints.is_max_flow_unconstrained()
    }
}

#[derive(Debug, PartialEq)]
pub struct OutputNode {
    meta: NodeMeta<NodeIndex>,
    cost: NodeCost,
    flow_constraints: FlowConstraints,
    incoming_edges: Vec<EdgeIndex>,
}

impl OutputNode {
    fn get_cost(&self, network: &Network, state: &State) -> Result<f64, NodeError> {
        self.cost.get_cost(network, state)
    }

    fn get_min_flow(&self, network: &Network, state: &State) -> Result<f64, MetricF64Error> {
        self.flow_constraints.get_min_flow(network, state)
    }
    fn get_const_min_flow(&self, values: &ConstParameterValues) -> Result<Option<f64>, ConstantMetricF64Error> {
        self.flow_constraints.get_const_min_flow(values)
    }

    fn get_max_flow(&self, network: &Network, state: &State) -> Result<f64, MetricF64Error> {
        self.flow_constraints.get_max_flow(network, state)
    }
    fn get_const_max_flow(&self, values: &ConstParameterValues) -> Result<Option<f64>, ConstantMetricF64Error> {
        self.flow_constraints.get_const_max_flow(values)
    }
    fn is_max_flow_unconstrained(&self) -> bool {
        self.flow_constraints.is_max_flow_unconstrained()
    }
}

#[derive(Debug, PartialEq)]
pub struct LinkNode {
    meta: NodeMeta<NodeIndex>,
    cost: NodeCost,
    flow_constraints: FlowConstraints,
    incoming_edges: Vec<EdgeIndex>,
    outgoing_edges: Vec<EdgeIndex>,
}

impl LinkNode {
    fn get_cost(&self, network: &Network, state: &State) -> Result<f64, NodeError> {
        self.cost.get_cost(network, state)
    }

    fn get_min_flow(&self, network: &Network, state: &State) -> Result<f64, MetricF64Error> {
        self.flow_constraints.get_min_flow(network, state)
    }
    fn get_const_min_flow(&self, values: &ConstParameterValues) -> Result<Option<f64>, ConstantMetricF64Error> {
        self.flow_constraints.get_const_min_flow(values)
    }

    fn get_max_flow(&self, network: &Network, state: &State) -> Result<f64, MetricF64Error> {
        self.flow_constraints.get_max_flow(network, state)
    }
    fn get_const_max_flow(&self, values: &ConstParameterValues) -> Result<Option<f64>, ConstantMetricF64Error> {
        self.flow_constraints.get_const_max_flow(values)
    }
    fn is_max_flow_unconstrained(&self) -> bool {
        self.flow_constraints.is_max_flow_unconstrained()
    }
}

/// Initial volume for a storage node.
#[derive(Debug, PartialEq, Clone)]
pub enum StorageInitialVolume {
    /// Absolute initial volume.
    Absolute(f64),
    /// Proportional initial volume, relative to the maximum volume.
    Proportional(f64),
    /// Absolute initial volume, but distributed progressively over other storage nodes.
    /// This is used for piecewise storage node configurations that comprise multiple
    /// nodes. The `absolute` field is the initial volume, and `prior_max_volume` contains
    /// the metrics that this volume is distributed over before this node.
    /// Only if there is any remaining volume after distributing
    /// `absolute` over `prior_max_volume`, this node will have a non-zero initial volume.
    DistributedAbsolute {
        /// The absolute initial volume.
        absolute: f64,
        /// The sum of the max volumes distributed prior to this node.
        prior_max_volume: SimpleMetricF64,
    },
    /// Similar to `DistributedAbsolute`, but the initial volume is proportional.
    DistributedProportional {
        /// The total max volume of the group of storage nodes.
        total_volume: SimpleMetricF64,
        /// The absolute initial volume.
        proportion: f64,
        /// The sum of the max volumes distributed prior to this node.
        prior_max_volume: SimpleMetricF64,
    },
}

impl StorageInitialVolume {
    /// Get the initial volume as an absolute value.
    pub fn get_absolute_initial_volume(&self, max_volume: f64, state: &State) -> Result<f64, SimpleMetricF64Error> {
        match self {
            StorageInitialVolume::Absolute(iv) => Ok(*iv),
            StorageInitialVolume::Proportional(ipc) => Ok(max_volume * ipc),
            StorageInitialVolume::DistributedAbsolute {
                absolute,
                prior_max_volume,
            } => {
                let prior_max_volume = prior_max_volume.get_value(&state.get_simple_parameter_values())?;

                // The initial volume is the absolute value minus the prior volumes,
                // but it cannot exceed the maximum volume.
                Ok((*absolute - prior_max_volume).max(0.0).min(max_volume))
            }
            StorageInitialVolume::DistributedProportional {
                total_volume,
                proportion,
                prior_max_volume,
            } => {
                let prior_max_volume = prior_max_volume.get_value(&state.get_simple_parameter_values())?;

                // Calculate the absolute initial volume based on the total volume and proportion.
                let absolute = total_volume.get_value(&state.get_simple_parameter_values())? * proportion;

                // The initial volume is the absolute value minus the prior volumes,
                // but it cannot exceed the maximum volume.
                Ok((absolute - prior_max_volume).max(0.0).min(max_volume))
            }
        }
    }
}

pub enum UnresolvedStorageInitialVolume {
    Absolute(f64),
    Proportional(f64),
    DistributedAbsolute {
        absolute: f64,
        prior_max_volume: UnresolvedMetricF64,
    },
    DistributedProportional {
        total_volume: UnresolvedMetricF64,
        proportion: f64,
        prior_max_volume: UnresolvedMetricF64,
    },
}

impl UnresolvedStorageInitialVolume {
    pub fn absolute(absolute: f64) -> Self {
        Self::Absolute(absolute)
    }

    pub fn proportional(proportion: f64) -> Self {
        Self::Proportional(proportion)
    }

    pub fn distributed_absolute(absolute: f64, prior_max_volume: UnresolvedMetricF64) -> Self {
        Self::DistributedAbsolute {
            absolute,
            prior_max_volume,
        }
    }

    pub fn distributed_proportional(
        total_volume: UnresolvedMetricF64,
        proportion: f64,
        prior_max_volume: UnresolvedMetricF64,
    ) -> Self {
        Self::DistributedProportional {
            total_volume,
            proportion,
            prior_max_volume,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct StorageNode {
    meta: NodeMeta<NodeIndex>,
    cost: NodeCost,
    initial_volume: StorageInitialVolume,
    storage_constraints: StorageConstraints,
    incoming_edges: Vec<EdgeIndex>,
    outgoing_edges: Vec<EdgeIndex>,
}

impl StorageNode {
    pub fn before(&self, timestep: &Timestep, state: &mut State) -> Result<(), NodeError> {
        // Set the initial volume if it is the first timestep.
        if timestep.is_first() {
            let max_volume = self.get_max_volume(state)?;
            let volume = self.initial_volume.get_absolute_initial_volume(max_volume, state)?;

            state.set_node_volume(&self.meta.index, volume, max_volume)?;
        }
        Ok(())
    }

    fn get_cost(&self, network: &Network, state: &State) -> Result<f64, NodeError> {
        self.cost.get_cost(network, state)
    }

    pub fn get_min_volume(&self, state: &State) -> Result<f64, SimpleMetricF64Error> {
        self.storage_constraints
            .get_min_volume(&state.get_simple_parameter_values())
    }

    pub fn get_max_volume(&self, state: &State) -> Result<f64, SimpleMetricF64Error> {
        self.storage_constraints
            .get_max_volume(&state.get_simple_parameter_values())
    }
}
