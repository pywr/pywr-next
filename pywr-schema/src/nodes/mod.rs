//! This module contains the definition of all nodes in the model.
//!
//! Nodes are the main components of a Pywr network. They are connected to each other by [`crate::edge::Edge`]s
//! and can have various constraints and parameters. Each node type has its own specific
//! implementation, that defines its behaviour in the overall model.
//!
//! The valid nodes are defined in the [`Node`] enum, which is a tagged union of all the
//! node types. For more information on the individual nodes, see their individual modules.
//!
//! # Attributes
//!
//! Node attributes are properties that nodes can have, such as volume, cost, or flow. These attributes
//! are defined in the [`NodeAttribute`] enum, which is a tagged union of all the possible attributes.
//! Not all nodes have all attributes, and an error will be raised if an attribute is requested that
//! is not supported by the node.
//!
//! Each node can convert a subset of the attributes into a [`pywr_core::metric::MetricF64`], which can then be used
//! in calculations in the model. For example, by other parameters or in output [`crate::metric_sets::MetricSet`]s.
//!
//! # Components
//!
//! Similarly to attributes, nodes can have components that refer to particular sub-components of the node.
//! This is useful for nodes that have multiple components, such as a [`ReservoirNode`]. The purpose
//! of the component is to give a more fine-grained control over the node's behaviour when used
//! in constraints.
//!
//! Certain nodes, such as [`VirtualStorageNode`] or [`ReservoirNode`], refer to other nodes in the model
//! and have a special representation in the model. In this case the components are used to determine
//! how the referred to node behaves. For example, [`VirtualStorageNode`] representing a licence may
//! wish to use the inflow (gross) or outflow (net) from a [`WaterTreatmentWorksNode`]. The component
//! is used to determine which of these values is used in the constraint.
//!
//! # Slots
//!
//! Some nodes have multiple input or output connectors, which are referred to as "slots". These
//! are used to connect to different parts of the node. For example, a [`ReservoirNode`] has
//! output slots for connecting either the spill, compensation or storage itself.
//!
mod abstraction;
mod attributes;
mod components;
mod core;
mod delay;
mod loss_link;
mod piecewise_link;
mod piecewise_storage;
mod placeholder;
mod reservoir;
mod river;
mod river_gauge;
mod river_split_with_gauge;
mod slots;
mod turbine;
mod water_treatment_works;
// `virtual` is a reserved keyword in Rust, so we use `virtual_nodes` as the module name
mod virtual_nodes;

use crate::error::SchemaError;
use crate::error::{ComponentConversionError, ConversionError};
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::network::NetworkSchema;
use crate::parameters::Parameter;
use crate::v1::{ConversionData, TryFromV1, TryIntoV2};
use crate::visit::{VisitMetrics, VisitPaths};
pub use abstraction::AbstractionNode;
pub use attributes::NodeAttribute;
pub use components::NodeComponent;
pub use core::{
    CatchmentNode, CatchmentNodeAttribute, CatchmentNodeComponent, InputNode, InputNodeAttribute, InputNodeComponent,
    LinkNode, LinkNodeAttribute, LinkNodeComponent, OutputNode, OutputNodeAttribute, OutputNodeComponent,
    SoftConstraint, StorageInitialVolume, StorageNode, StorageNodeAttribute,
};
pub use delay::{DelayNode, DelayNodeAttribute, DelayNodeComponent};
pub use loss_link::{LossFactor, LossLinkNode, LossLinkNodeAttribute, LossLinkNodeComponent};
pub use piecewise_link::{
    PiecewiseLinkNode, PiecewiseLinkNodeAttribute, PiecewiseLinkNodeComponent, PiecewiseLinkStep,
};
pub use piecewise_storage::{PiecewiseStorageNode, PiecewiseStorageNodeAttribute, PiecewiseStore};
pub use placeholder::PlaceholderNode;
#[cfg(feature = "core")]
use pywr_core::metric::MetricF64;
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::nodes::{
    CoreNode as CoreNodeV1, Node as NodeV1, NodeMeta as NodeMetaV1, NodePosition as NodePositionV1,
};
pub use reservoir::{
    Bathymetry, BathymetryType, Evaporation, Leakage, Rainfall, ReservoirNode, ReservoirNodeAttribute,
    ReservoirNodeComponent, SpillNodeType,
};
pub use river::{MuskingumInitialCondition, RiverNode, RiverNodeAttribute, RiverNodeComponent, RoutingMethod};
pub use river_gauge::{RiverGaugeNode, RiverGaugeNodeAttribute, RiverGaugeNodeComponent};
pub use river_split_with_gauge::{
    RiverSplit, RiverSplitWithGaugeNode, RiverSplitWithGaugeNodeAttribute, RiverSplitWithGaugeNodeComponent,
};
use schemars::JsonSchema;
pub use slots::NodeSlot;
use std::path::{Path, PathBuf};
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};
pub use turbine::{TargetType, TurbineNode, TurbineNodeAttribute, TurbineNodeComponent};
pub use virtual_nodes::{
    AggregatedNode, AggregatedNodeAttribute, AggregatedStorageNode, AggregatedStorageNodeAttribute, AnnualReset,
    Relationship, RollingWindow, VirtualNode, VirtualNodeType, VirtualStorageNode, VirtualStorageNodeAttribute,
    VirtualStorageReset, VirtualStorageResetVolume,
};
pub use water_treatment_works::{
    WaterTreatmentWorksNode, WaterTreatmentWorksNodeAttribute, WaterTreatmentWorksNodeComponent,
};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy, JsonSchema, PywrVisitAll)]
pub struct NodePosition {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schematic: Option<(f32, f32)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geographic: Option<(f32, f32)>,
}

impl From<NodePositionV1> for NodePosition {
    fn from(v1: NodePositionV1) -> Self {
        Self {
            schematic: v1.schematic,
            geographic: v1.geographic,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Default, JsonSchema, PywrVisitAll)]
pub struct NodeMeta {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<NodePosition>,
}

impl From<NodeMetaV1> for NodeMeta {
    fn from(v1: NodeMetaV1) -> Self {
        Self {
            name: v1.name,
            comment: v1.comment,
            position: v1.position.map(|p| p.into()),
        }
    }
}

pub struct NodeBuilder {
    ty: NodeType,
    position: Option<NodePosition>,
    name: Option<String>,
}

/// A builder for creating a new node.
impl NodeBuilder {
    pub fn new(ty: NodeType) -> Self {
        Self {
            ty,
            position: None,
            name: None,
        }
    }

    /// Define the position of the node.
    pub fn position(mut self, position: NodePosition) -> Self {
        self.position = Some(position);
        self
    }

    /// Define the name of the node.
    pub fn name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }

    /// Create the next default name without duplicating an existing name in the model.
    pub fn next_default_name_for_model(mut self, network: &NetworkSchema) -> Self {
        let mut num = 1;
        loop {
            let name = format!("{}-{}", self.ty, num);
            if network.get_node_by_name(&name).is_none() {
                // No node with this name found!
                self.name = Some(name);
                break;
            } else {
                num += 1;
            }
        }
        self
    }

    /// Build the [`Node`].
    pub fn build(self) -> Node {
        let name = self.name.unwrap_or_else(|| self.ty.to_string());
        let meta = NodeMeta {
            name,
            position: self.position,
            ..Default::default()
        };

        match self.ty {
            NodeType::Input => Node::Input(InputNode {
                meta,
                ..Default::default()
            }),
            NodeType::Link => Node::Link(LinkNode {
                meta,
                ..Default::default()
            }),
            NodeType::Output => Node::Output(OutputNode {
                meta,
                ..Default::default()
            }),
            NodeType::Storage => Node::Storage(StorageNode {
                meta,
                ..Default::default()
            }),
            NodeType::Catchment => Node::Catchment(CatchmentNode {
                meta,
                ..Default::default()
            }),
            NodeType::RiverGauge => Node::RiverGauge(RiverGaugeNode {
                meta,
                ..Default::default()
            }),
            NodeType::LossLink => Node::LossLink(LossLinkNode {
                meta,
                ..Default::default()
            }),
            NodeType::Delay => Node::Delay(DelayNode {
                meta,
                ..Default::default()
            }),
            NodeType::PiecewiseLink => Node::PiecewiseLink(PiecewiseLinkNode {
                meta,
                ..Default::default()
            }),
            NodeType::PiecewiseStorage => Node::PiecewiseStorage(PiecewiseStorageNode {
                meta,
                ..Default::default()
            }),
            NodeType::River => Node::River(RiverNode {
                meta,
                ..Default::default()
            }),
            NodeType::RiverSplitWithGauge => Node::RiverSplitWithGauge(RiverSplitWithGaugeNode {
                meta,
                ..Default::default()
            }),
            NodeType::WaterTreatmentWorks => Node::WaterTreatmentWorks(WaterTreatmentWorksNode {
                meta,
                ..Default::default()
            }),
            NodeType::Turbine => Node::Turbine(TurbineNode {
                meta,
                ..Default::default()
            }),
            NodeType::Reservoir => Node::Reservoir(ReservoirNode {
                storage: StorageNode {
                    meta,
                    ..Default::default()
                },
                ..Default::default()
            }),
            NodeType::Placeholder => Node::Placeholder(PlaceholderNode { meta }),
            NodeType::Abstraction => Node::Abstraction(AbstractionNode {
                meta,
                ..Default::default()
            }),
        }
    }
}

/// The main enum for all nodes in the model.
#[derive(serde::Deserialize, serde::Serialize, Clone, EnumDiscriminants, Debug, JsonSchema, Display)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
// This creates a separate enum called `NodeType` that is available in this module.
#[strum_discriminants(name(NodeType))]
// This is currently required by the `Reservoir` node. Rather than box it
#[allow(clippy::large_enum_variant)]
pub enum Node {
    Input(InputNode),
    Link(LinkNode),
    Output(OutputNode),
    Storage(StorageNode),
    Catchment(CatchmentNode),
    RiverGauge(RiverGaugeNode),
    LossLink(LossLinkNode),
    Delay(DelayNode),
    PiecewiseLink(PiecewiseLinkNode),
    PiecewiseStorage(PiecewiseStorageNode),
    River(RiverNode),
    RiverSplitWithGauge(RiverSplitWithGaugeNode),
    WaterTreatmentWorks(WaterTreatmentWorksNode),
    Turbine(TurbineNode),
    Reservoir(ReservoirNode),
    Placeholder(PlaceholderNode),
    Abstraction(AbstractionNode),
}

impl Node {
    pub fn name(&self) -> &str {
        self.meta().name.as_str()
    }

    pub fn position(&self) -> Option<&NodePosition> {
        self.meta().position.as_ref()
    }

    pub fn node_type(&self) -> NodeType {
        // Implementation provided by the `EnumDiscriminants` derive macro.
        self.into()
    }

    pub fn meta(&self) -> &NodeMeta {
        match self {
            Node::Input(n) => &n.meta,
            Node::Link(n) => &n.meta,
            Node::Output(n) => &n.meta,
            Node::Storage(n) => &n.meta,
            Node::Catchment(n) => &n.meta,
            Node::RiverGauge(n) => &n.meta,
            Node::LossLink(n) => &n.meta,
            Node::River(n) => &n.meta,
            Node::RiverSplitWithGauge(n) => &n.meta,
            Node::WaterTreatmentWorks(n) => &n.meta,
            Node::PiecewiseLink(n) => &n.meta,
            Node::PiecewiseStorage(n) => &n.meta,
            Node::Delay(n) => &n.meta,
            Node::Turbine(n) => &n.meta,
            Node::Reservoir(n) => n.meta(),
            Node::Placeholder(n) => &n.meta,
            Node::Abstraction(n) => &n.meta,
        }
    }

    /// Get the input connectors for this node.
    ///
    /// The `slot` argument is used for nodes that have multiple input connectors. If the node
    /// does not have multiple input connectors, then a [`SchemaError`] is returned.
    ///
    /// The input connectors are returned as a vector of tuples, where the first element is the name of the
    /// connector, and the second element is an optional sub-name. The sub-name is used for nodes
    /// that have multiple internal nodes, such as a [`ReservoirNode`].
    ///
    pub fn input_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        match self {
            Node::Input(n) => n.input_connectors(slot),
            Node::Link(n) => n.input_connectors(slot),
            Node::Output(n) => n.input_connectors(slot),
            Node::Storage(n) => n.input_connectors(slot),
            Node::Catchment(n) => n.input_connectors(slot),
            Node::RiverGauge(n) => n.input_connectors(slot),
            Node::LossLink(n) => n.input_connectors(slot),
            Node::River(n) => n.input_connectors(slot),
            Node::RiverSplitWithGauge(n) => n.input_connectors(slot),
            Node::WaterTreatmentWorks(n) => n.input_connectors(slot),
            Node::PiecewiseLink(n) => n.input_connectors(slot),
            Node::PiecewiseStorage(n) => n.input_connectors(slot),
            Node::Delay(n) => n.input_connectors(slot),
            Node::Turbine(n) => n.input_connectors(slot),
            Node::Reservoir(n) => n.input_connectors(slot),
            // Deliberately do not take a slot for Placeholder nodes so they can be used with any slot
            Node::Placeholder(n) => n.input_connectors(),
            Node::Abstraction(n) => n.input_connectors(slot),
        }
    }

    /// Get any input (or "to") slots that this node has.
    pub fn iter_input_slots(&self) -> Option<Box<dyn Iterator<Item = NodeSlot> + '_>> {
        match self {
            Node::Input(_) => None,
            Node::Link(_) => None,
            Node::Output(_) => None,
            Node::Storage(_) => None,
            Node::Catchment(_) => None,
            Node::RiverGauge(_) => None,
            Node::LossLink(_) => None,
            Node::Delay(_) => None,
            Node::PiecewiseLink(_) => None,
            Node::PiecewiseStorage(_) => None,
            Node::River(_) => None,
            Node::RiverSplitWithGauge(_) => None,
            Node::WaterTreatmentWorks(_) => None,
            Node::Turbine(_) => None,
            Node::Reservoir(_) => None,
            Node::Placeholder(_) => None,
            Node::Abstraction(_) => None,
        }
    }

    /// Get the output connectors for this node.
    ///
    /// The `slot` argument is used for nodes that have multiple output connectors. If the node
    /// does not have multiple output connectors, then a [`SchemaError`] is returned.
    ///
    /// The output connectors are returned as a vector of tuples, where the first element is the name of the
    /// connector, and the second element is an optional sub-name. The sub-name is used for nodes
    /// that have multiple internal nodes, such as a [`ReservoirNode`].
    ///
    pub fn output_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        match self {
            Node::Input(n) => n.output_connectors(slot),
            Node::Link(n) => n.output_connectors(slot),
            Node::Output(n) => n.output_connectors(slot),
            Node::Storage(n) => n.output_connectors(slot),
            Node::Catchment(n) => n.output_connectors(slot),
            Node::RiverGauge(n) => n.output_connectors(slot),
            Node::LossLink(n) => n.output_connectors(slot),
            Node::River(n) => n.output_connectors(slot),
            Node::RiverSplitWithGauge(n) => n.output_connectors(slot),
            Node::WaterTreatmentWorks(n) => n.output_connectors(slot),
            Node::PiecewiseLink(n) => n.output_connectors(slot),
            Node::PiecewiseStorage(n) => n.output_connectors(slot),
            Node::Delay(n) => n.output_connectors(slot),
            Node::Turbine(n) => n.output_connectors(slot),
            Node::Reservoir(n) => n.output_connectors(slot),
            // Deliberately do not take a slot for Placeholder nodes so they can be used with any slot
            Node::Placeholder(n) => n.output_connectors(),
            Node::Abstraction(n) => n.output_connectors(slot),
        }
    }

    /// Get any output (or "from") slots that this node has.
    pub fn iter_output_slots(&self) -> Option<Box<dyn Iterator<Item = NodeSlot> + '_>> {
        match self {
            Node::Input(_) => None,
            Node::Link(_) => None,
            Node::Output(_) => None,
            Node::Storage(_) => None,
            Node::Catchment(_) => None,
            Node::RiverGauge(_) => None,
            Node::LossLink(_) => None,
            Node::Delay(_) => None,
            Node::PiecewiseLink(_) => None,
            Node::PiecewiseStorage(_) => None,
            Node::River(_) => None,
            Node::RiverSplitWithGauge(n) => Some(Box::new(n.iter_output_slots())),
            Node::WaterTreatmentWorks(_) => None,
            Node::Turbine(_) => None,
            Node::Reservoir(n) => Some(Box::new(n.iter_output_slots())),
            Node::Placeholder(_) => None,
            Node::Abstraction(n) => Some(Box::new(n.iter_output_slots())),
        }
    }

    pub fn default_attribute(&self) -> NodeAttribute {
        match self {
            Node::Input(n) => n.default_attribute().into(),
            Node::Link(n) => n.default_attribute().into(),
            Node::Output(n) => n.default_attribute().into(),
            Node::Storage(n) => n.default_attribute().into(),
            Node::Catchment(n) => n.default_attribute().into(),
            Node::RiverGauge(n) => n.default_attribute().into(),
            Node::LossLink(n) => n.default_attribute().into(),
            Node::River(n) => n.default_attribute().into(),
            Node::RiverSplitWithGauge(n) => n.default_attribute().into(),
            Node::WaterTreatmentWorks(n) => n.default_attribute().into(),
            Node::PiecewiseLink(n) => n.default_attribute().into(),
            Node::PiecewiseStorage(n) => n.default_attribute().into(),
            Node::Delay(n) => n.default_attribute().into(),
            Node::Turbine(n) => n.default_attribute().into(),
            Node::Reservoir(n) => n.default_attribute().into(),
            Node::Placeholder(n) => n.default_attribute(),
            Node::Abstraction(n) => n.default_attribute().into(),
        }
    }

    /// Returns the default component for the node, if defined.
    pub fn default_component(&self) -> Option<NodeComponent> {
        match self {
            Node::Input(n) => Some(n.default_component().into()),
            Node::Link(n) => Some(n.default_component().into()),
            Node::Output(n) => Some(n.default_component().into()),
            Node::Catchment(n) => Some(n.default_component().into()),
            Node::Storage(_) => None,
            Node::RiverGauge(n) => Some(n.default_component().into()),
            Node::LossLink(n) => Some(n.default_component().into()),
            Node::Delay(n) => Some(n.default_component().into()),
            Node::PiecewiseLink(n) => Some(n.default_component().into()),
            Node::PiecewiseStorage(_) => None,
            Node::River(n) => Some(n.default_component().into()),
            Node::RiverSplitWithGauge(n) => Some(n.default_component().into()),
            Node::WaterTreatmentWorks(n) => Some(n.default_component().into()),
            Node::Turbine(n) => Some(n.default_component().into()),
            Node::Reservoir(n) => Some(n.default_component().into()),
            Node::Placeholder(_) => None,
            Node::Abstraction(n) => Some(n.default_component().into()),
        }
    }

    /// Get the locally defined parameters for this node.
    ///
    /// This does **not** return which parameters this node might reference, but rather
    /// the parameters that are defined on this node itself.
    pub fn local_parameters(&self) -> Option<&[Parameter]> {
        match self {
            Node::Input(n) => n.parameters.as_deref(),
            Node::Link(n) => n.parameters.as_deref(),
            Node::Output(n) => n.parameters.as_deref(),
            Node::Storage(n) => n.parameters.as_deref(),
            Node::Catchment(n) => n.parameters.as_deref(),
            Node::RiverGauge(n) => n.parameters.as_deref(),
            Node::LossLink(n) => n.parameters.as_deref(),
            Node::River(n) => n.parameters.as_deref(),
            Node::RiverSplitWithGauge(n) => n.parameters.as_deref(),
            Node::WaterTreatmentWorks(n) => n.parameters.as_deref(),
            Node::PiecewiseLink(n) => n.parameters.as_deref(),
            Node::PiecewiseStorage(n) => n.parameters.as_deref(),
            Node::Delay(n) => n.parameters.as_deref(),
            Node::Turbine(n) => n.parameters.as_deref(),
            Node::Reservoir(n) => n.storage.parameters.as_deref(),
            Node::Placeholder(_) => None,
            Node::Abstraction(n) => n.parameters.as_deref(),
        }
    }
}

#[cfg(feature = "core")]
impl Node {
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        match self {
            Node::Input(n) => n.add_to_model(network),
            Node::Link(n) => n.add_to_model(network),
            Node::Output(n) => n.add_to_model(network),
            Node::Storage(n) => n.add_to_model(network),
            Node::Catchment(n) => n.add_to_model(network),
            Node::RiverGauge(n) => n.add_to_model(network),
            Node::LossLink(n) => n.add_to_model(network),
            Node::River(n) => n.add_to_model(network),
            Node::RiverSplitWithGauge(n) => n.add_to_model(network),
            Node::WaterTreatmentWorks(n) => n.add_to_model(network),
            Node::PiecewiseLink(n) => n.add_to_model(network),
            Node::PiecewiseStorage(n) => n.add_to_model(network),
            Node::Delay(n) => n.add_to_model(network),
            Node::Turbine(n) => n.add_to_model(network),
            Node::Reservoir(n) => n.add_to_model(network),
            Node::Placeholder(n) => n.add_to_model(),
            Node::Abstraction(n) => n.add_to_model(network),
        }
    }

    /// Get the node indices for flow constraints that this node has added to the network.
    ///
    /// This is used to determine which core nodes should be used when this node is used
    /// in a flow constraint. Depending on the node type, this may return multiple
    /// node indices, for example if node contains multiple internal components. The node
    /// indices may also be different depending on the `component` argument, which is used
    /// to determine which component of the node is used in the flow constraint.
    ///
    /// If the node is not allowed in flow constraints it should return [`SchemaError::NodeNotAllowedInFlowConstraint`].
    pub fn node_indices_for_flow_constraints(
        &self,
        network: &pywr_core::network::Network,
        component: Option<NodeComponent>,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        match self {
            Node::Input(n) => n.node_indices_for_flow_constraints(network, component),
            Node::Link(n) => n.node_indices_for_flow_constraints(network, component),
            Node::Output(n) => n.node_indices_for_flow_constraints(network, component),
            Node::Storage(_) => Err(SchemaError::NodeNotAllowedInFlowConstraint),
            Node::Catchment(n) => n.node_indices_for_flow_constraints(network, component),
            Node::RiverGauge(n) => n.node_indices_for_flow_constraints(network, component),
            Node::LossLink(n) => n.node_indices_for_flow_constraints(network, component),
            Node::River(n) => n.node_indices_for_flow_constraints(network, component),
            Node::RiverSplitWithGauge(n) => n.node_indices_for_flow_constraints(network, component),
            Node::WaterTreatmentWorks(n) => n.node_indices_for_flow_constraints(network, component),
            Node::PiecewiseLink(n) => n.node_indices_for_flow_constraints(network, component),
            Node::PiecewiseStorage(_) => Err(SchemaError::NodeNotAllowedInFlowConstraint),
            Node::Delay(n) => n.node_indices_for_flow_constraints(network, component),
            Node::Turbine(n) => n.node_indices_for_flow_constraints(network, component),
            Node::Reservoir(n) => n.node_indices_for_flow_constraints(network, component),
            Node::Placeholder(n) => n.node_indices_for_flow_constraints(),
            Node::Abstraction(n) => n.node_indices_for_flow_constraints(network, component),
        }
    }

    /// Get the node indices for storage nodes for this node.
    ///
    /// This is used to determine which core nodes should be used when this node is used
    /// in an [`AggregatedStorageNode`].
    ///
    /// If the node is not allowed in storage constraints it should return [`SchemaError::NodeNotAllowedInStorageConstraint`].
    pub fn node_indices_for_storage_constraints(
        &self,
        network: &pywr_core::network::Network,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        match self {
            Node::Input(_) => Err(SchemaError::NodeNotAllowedInStorageConstraint),
            Node::Link(_) => Err(SchemaError::NodeNotAllowedInStorageConstraint),
            Node::Output(_) => Err(SchemaError::NodeNotAllowedInStorageConstraint),
            Node::Storage(n) => n.node_indices_for_storage_constraints(network),
            Node::Catchment(_) => Err(SchemaError::NodeNotAllowedInStorageConstraint),
            Node::RiverGauge(_) => Err(SchemaError::NodeNotAllowedInStorageConstraint),
            Node::LossLink(_) => Err(SchemaError::NodeNotAllowedInStorageConstraint),
            Node::River(_) => Err(SchemaError::NodeNotAllowedInStorageConstraint),
            Node::RiverSplitWithGauge(_) => Err(SchemaError::NodeNotAllowedInStorageConstraint),
            Node::WaterTreatmentWorks(_) => Err(SchemaError::NodeNotAllowedInStorageConstraint),
            Node::PiecewiseLink(_) => Err(SchemaError::NodeNotAllowedInStorageConstraint),
            Node::PiecewiseStorage(n) => n.node_indices_for_storage_constraints(network),
            Node::Delay(_) => Err(SchemaError::NodeNotAllowedInStorageConstraint),
            Node::Turbine(_) => Err(SchemaError::NodeNotAllowedInStorageConstraint),
            Node::Reservoir(n) => n.node_indices_for_storage_constraints(network),
            Node::Placeholder(n) => n.node_indices_for_storage_constraints(),
            Node::Abstraction(_) => Err(SchemaError::NodeNotAllowedInStorageConstraint),
        }
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        match self {
            Node::Input(n) => n.set_constraints(network, args),
            Node::Link(n) => n.set_constraints(network, args),
            Node::Output(n) => n.set_constraints(network, args),
            Node::Storage(n) => n.set_constraints(network, args),
            Node::Catchment(n) => n.set_constraints(network, args),
            Node::RiverGauge(n) => n.set_constraints(network, args),
            Node::LossLink(n) => n.set_constraints(network, args),
            Node::River(n) => n.set_constraints(network, args),
            Node::RiverSplitWithGauge(n) => n.set_constraints(network, args),
            Node::WaterTreatmentWorks(n) => n.set_constraints(network, args),
            Node::PiecewiseLink(n) => n.set_constraints(network, args),
            Node::PiecewiseStorage(n) => n.set_constraints(network, args),
            Node::Delay(n) => n.set_constraints(network, args),
            Node::Turbine(n) => n.set_constraints(network, args),
            Node::Reservoir(n) => n.set_constraints(network, args),
            Node::Placeholder(n) => n.set_constraints(),
            Node::Abstraction(n) => n.set_constraints(network, args),
        }
    }

    /// Create a metric for the given attribute on this node.
    pub fn create_metric(
        &self,
        network: &mut pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
        args: &LoadArgs,
    ) -> Result<MetricF64, SchemaError> {
        match self {
            Node::Input(n) => n.create_metric(network, attribute),
            Node::Link(n) => n.create_metric(network, attribute),
            Node::Output(n) => n.create_metric(network, attribute),
            Node::Storage(n) => n.create_metric(network, attribute),
            Node::Catchment(n) => n.create_metric(network, attribute),
            Node::RiverGauge(n) => n.create_metric(network, attribute),
            Node::LossLink(n) => n.create_metric(network, attribute),
            Node::River(n) => n.create_metric(network, attribute),
            Node::RiverSplitWithGauge(n) => n.create_metric(network, attribute),
            Node::WaterTreatmentWorks(n) => n.create_metric(network, attribute),
            Node::PiecewiseLink(n) => n.create_metric(network, attribute),
            Node::PiecewiseStorage(n) => n.create_metric(network, attribute),
            Node::Delay(n) => n.create_metric(network, attribute),
            Node::Turbine(n) => n.create_metric(network, attribute, args),
            Node::Reservoir(n) => n.create_metric(network, attribute),
            Node::Placeholder(n) => n.create_metric(),
            Node::Abstraction(n) => n.create_metric(network, attribute),
        }
    }
}

impl TryFromV1<NodeV1> for NodeOrVirtualNode {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: NodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        match v1 {
            NodeV1::Core(n) => {
                let nv2: Self = n.try_into_v2(parent_node, conversion_data)?;
                Ok(nv2)
            }
            NodeV1::Custom(n) => Err(ComponentConversionError::Node {
                name: n.meta.name,
                attr: "".to_string(),
                error: ConversionError::CustomTypeNotSupported { ty: n.ty },
            }),
        }
    }
}

pub enum NodeOrVirtualNode {
    Node(Box<Node>),
    Virtual(Box<VirtualNode>),
}

#[cfg(feature = "core")]
impl NodeOrVirtualNode {
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network, args: &LoadArgs) -> Result<(), SchemaError> {
        match self {
            NodeOrVirtualNode::Node(n) => n.add_to_model(network),
            NodeOrVirtualNode::Virtual(n) => n.add_to_model(network, args),
        }
    }
}

impl From<Node> for NodeOrVirtualNode {
    fn from(n: Node) -> Self {
        Self::Node(Box::new(n))
    }
}

impl From<VirtualNode> for NodeOrVirtualNode {
    fn from(n: VirtualNode) -> Self {
        Self::Virtual(Box::new(n))
    }
}

impl TryFromV1<Box<CoreNodeV1>> for NodeOrVirtualNode {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: Box<CoreNodeV1>,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let n = match *v1 {
            CoreNodeV1::Input(n) => Node::Input(n.try_into_v2(parent_node, conversion_data)?).into(),
            CoreNodeV1::Link(n) => Node::Link(n.try_into_v2(parent_node, conversion_data)?).into(),
            CoreNodeV1::Output(n) => Node::Output(n.try_into_v2(parent_node, conversion_data)?).into(),
            CoreNodeV1::Storage(n) => Node::Storage(n.try_into_v2(parent_node, conversion_data)?).into(),
            CoreNodeV1::Reservoir(n) => Node::Storage(n.try_into_v2(parent_node, conversion_data)?).into(),
            CoreNodeV1::Catchment(n) => Node::Catchment(n.try_into_v2(parent_node, conversion_data)?).into(),
            CoreNodeV1::RiverGauge(n) => Node::RiverGauge(n.try_into_v2(parent_node, conversion_data)?).into(),
            CoreNodeV1::LossLink(n) => Node::LossLink(n.try_into_v2(parent_node, conversion_data)?).into(),
            CoreNodeV1::River(n) => Node::River(n.try_into()?).into(),
            CoreNodeV1::RiverSplitWithGauge(n) => {
                Node::RiverSplitWithGauge(n.try_into_v2(parent_node, conversion_data)?).into()
            }
            CoreNodeV1::Aggregated(n) => VirtualNode::Aggregated(n.try_into_v2(parent_node, conversion_data)?).into(),
            CoreNodeV1::AggregatedStorage(n) => VirtualNode::AggregatedStorage(n.into()).into(),
            CoreNodeV1::VirtualStorage(n) => {
                VirtualNode::VirtualStorage(n.try_into_v2(parent_node, conversion_data)?).into()
            }
            CoreNodeV1::AnnualVirtualStorage(n) => {
                VirtualNode::VirtualStorage(n.try_into_v2(parent_node, conversion_data)?).into()
            }
            CoreNodeV1::PiecewiseLink(n) => Node::PiecewiseLink(n.try_into_v2(parent_node, conversion_data)?).into(),
            CoreNodeV1::MultiSplitLink(_) => todo!(),
            CoreNodeV1::BreakLink(n) => Node::Link(n.try_into_v2(parent_node, conversion_data)?).into(),
            CoreNodeV1::Delay(n) => Node::Delay(n.try_into()?).into(),
            CoreNodeV1::RiverSplit(_) => todo!("Conversion of RiverSplit nodes"),
            CoreNodeV1::MonthlyVirtualStorage(n) => {
                VirtualNode::VirtualStorage(n.try_into_v2(parent_node, conversion_data)?).into()
            }
            CoreNodeV1::SeasonalVirtualStorage(n) => {
                VirtualNode::VirtualStorage(n.try_into_v2(parent_node, conversion_data)?).into()
            }
            CoreNodeV1::RollingVirtualStorage(n) => {
                VirtualNode::VirtualStorage(n.try_into_v2(parent_node, conversion_data)?).into()
            }
        };

        Ok(n)
    }
}

impl VisitMetrics for Node {
    fn visit_metrics<F: FnMut(&Metric)>(&self, visitor: &mut F) {
        match self {
            Node::Input(n) => n.visit_metrics(visitor),
            Node::Link(n) => n.visit_metrics(visitor),
            Node::Output(n) => n.visit_metrics(visitor),
            Node::Storage(n) => n.visit_metrics(visitor),
            Node::Catchment(n) => n.visit_metrics(visitor),
            Node::RiverGauge(n) => n.visit_metrics(visitor),
            Node::LossLink(n) => n.visit_metrics(visitor),
            Node::River(n) => n.visit_metrics(visitor),
            Node::RiverSplitWithGauge(n) => n.visit_metrics(visitor),
            Node::WaterTreatmentWorks(n) => n.visit_metrics(visitor),
            Node::PiecewiseLink(n) => n.visit_metrics(visitor),
            Node::PiecewiseStorage(n) => n.visit_metrics(visitor),
            Node::Delay(n) => n.visit_metrics(visitor),
            Node::Turbine(n) => n.visit_metrics(visitor),
            Node::Reservoir(n) => n.visit_metrics(visitor),
            Node::Placeholder(n) => n.visit_metrics(visitor),
            Node::Abstraction(n) => n.visit_metrics(visitor),
        }
    }

    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, visitor: &mut F) {
        match self {
            Node::Input(n) => n.visit_metrics_mut(visitor),
            Node::Link(n) => n.visit_metrics_mut(visitor),
            Node::Output(n) => n.visit_metrics_mut(visitor),
            Node::Storage(n) => n.visit_metrics_mut(visitor),
            Node::Catchment(n) => n.visit_metrics_mut(visitor),
            Node::RiverGauge(n) => n.visit_metrics_mut(visitor),
            Node::LossLink(n) => n.visit_metrics_mut(visitor),
            Node::River(n) => n.visit_metrics_mut(visitor),
            Node::RiverSplitWithGauge(n) => n.visit_metrics_mut(visitor),
            Node::WaterTreatmentWorks(n) => n.visit_metrics_mut(visitor),
            Node::PiecewiseLink(n) => n.visit_metrics_mut(visitor),
            Node::PiecewiseStorage(n) => n.visit_metrics_mut(visitor),
            Node::Delay(n) => n.visit_metrics_mut(visitor),
            Node::Turbine(n) => n.visit_metrics_mut(visitor),
            Node::Reservoir(n) => n.visit_metrics_mut(visitor),
            Node::Placeholder(n) => n.visit_metrics_mut(visitor),
            Node::Abstraction(n) => n.visit_metrics_mut(visitor),
        }
    }
}

impl VisitPaths for Node {
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        match self {
            Node::Input(n) => n.visit_paths(visitor),
            Node::Link(n) => n.visit_paths(visitor),
            Node::Output(n) => n.visit_paths(visitor),
            Node::Storage(n) => n.visit_paths(visitor),
            Node::Catchment(n) => n.visit_paths(visitor),
            Node::RiverGauge(n) => n.visit_paths(visitor),
            Node::LossLink(n) => n.visit_paths(visitor),
            Node::River(n) => n.visit_paths(visitor),
            Node::RiverSplitWithGauge(n) => n.visit_paths(visitor),
            Node::WaterTreatmentWorks(n) => n.visit_paths(visitor),
            Node::PiecewiseLink(n) => n.visit_paths(visitor),
            Node::PiecewiseStorage(n) => n.visit_paths(visitor),
            Node::Delay(n) => n.visit_paths(visitor),
            Node::Turbine(n) => n.visit_paths(visitor),
            Node::Reservoir(n) => n.visit_paths(visitor),
            Node::Placeholder(n) => n.visit_paths(visitor),
            Node::Abstraction(n) => n.visit_paths(visitor),
        }
    }

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        match self {
            Node::Input(n) => n.visit_paths_mut(visitor),
            Node::Link(n) => n.visit_paths_mut(visitor),
            Node::Output(n) => n.visit_paths_mut(visitor),
            Node::Storage(n) => n.visit_paths_mut(visitor),
            Node::Catchment(n) => n.visit_paths_mut(visitor),
            Node::RiverGauge(n) => n.visit_paths_mut(visitor),
            Node::LossLink(n) => n.visit_paths_mut(visitor),
            Node::River(n) => n.visit_paths_mut(visitor),
            Node::RiverSplitWithGauge(n) => n.visit_paths_mut(visitor),
            Node::WaterTreatmentWorks(n) => n.visit_paths_mut(visitor),
            Node::PiecewiseLink(n) => n.visit_paths_mut(visitor),
            Node::PiecewiseStorage(n) => n.visit_paths_mut(visitor),
            Node::Delay(n) => n.visit_paths_mut(visitor),
            Node::Turbine(n) => n.visit_paths_mut(visitor),
            Node::Reservoir(n) => n.visit_paths_mut(visitor),
            Node::Placeholder(n) => n.visit_paths_mut(visitor),
            Node::Abstraction(n) => n.visit_paths_mut(visitor),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::metric::Metric;
    use crate::nodes::{Node, NodeOrVirtualNode};
    use crate::v1::{ConversionData, TryIntoV2};
    use pywr_v1_schema::nodes::Node as NodeV1;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn test_ts_inline() {
        let node_data = r#"
        {
            "name": "catchment1",
            "type": "Input",
            "max_flow": {
                "type": "dataframe",
                "url" : "timeseries1.csv",
                "parse_dates": true,
                "dayfirst": true,
                "index_col": 0,
                "column": "Data"
            }
        }
        "#;

        let v1_node: NodeV1 = serde_json::from_str(node_data).unwrap();

        let mut conversion_data = ConversionData::default();

        let node_ts: NodeOrVirtualNode = v1_node.try_into_v2(None, &mut conversion_data).unwrap();

        let node_ts = match node_ts {
            NodeOrVirtualNode::Node(n) => n,
            _ => panic!("Expected Node"),
        };

        let input_node = match *node_ts {
            Node::Input(n) => n,
            _ => panic!("Expected InputNode"),
        };

        let expected_name = String::from("catchment1-p0");

        match input_node.max_flow {
            Some(Metric::Timeseries(ts)) => {
                assert_eq!(ts.name(), &expected_name)
            }
            _ => panic!("Expected Timeseries"),
        };

        assert_eq!(conversion_data.timeseries.len(), 1);
        assert_eq!(conversion_data.timeseries[0].name(), &expected_name);
    }

    #[test]
    fn test_ts_inline_nested() {
        let node_data = r#"
        {
            "name": "catchment1",
            "type": "Input",
            "max_flow": {
                "type": "aggregated",
                "agg_func": "product",
                "parameters": [
                    {
                        "type": "constant",
                        "value": 0.9
                    },
                    {
                        "type": "dataframe",
                        "url" : "timeseries1.csv",
                        "parse_dates": true,
                        "dayfirst": true,
                        "index_col": 0,
                        "column": "Data"
                    },
                    {
                        "type": "constant",
                        "value": 0.9
                    },
                    {
                        "type": "dataframe",
                        "url" : "timeseries2.csv",
                        "parse_dates": true,
                        "dayfirst": true,
                        "index_col": 0,
                        "column": "Data"
                    }
                ]
            }
        }
        "#;

        let v1_node: NodeV1 = serde_json::from_str(node_data).unwrap();

        let mut conversion_data = ConversionData::default();
        let node_ts: NodeOrVirtualNode = v1_node.try_into_v2(None, &mut conversion_data).unwrap();

        let node_ts = match node_ts {
            NodeOrVirtualNode::Node(n) => n,
            _ => panic!("Expected Node"),
        };

        let input_node = match *node_ts {
            Node::Input(n) => n,
            _ => panic!("Expected InputNode"),
        };

        let expected_name1 = "catchment1-p2";
        let expected_name2 = "catchment1-p4";

        match input_node.max_flow {
            Some(Metric::Parameter(parameter_ref)) => assert_eq!(&parameter_ref.name, "catchment1-p0"),
            _ => panic!("Expected Timeseries"),
        };

        assert_eq!(conversion_data.parameters.len(), 3);

        assert_eq!(conversion_data.timeseries.len(), 2);
        assert_eq!(conversion_data.timeseries[0].name(), expected_name1);
        assert_eq!(conversion_data.timeseries[1].name(), expected_name2);
    }

    #[test]
    fn test_doc_examples() {
        let mut doc_examples = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        doc_examples.push("src/nodes/doc_examples");

        for entry in fs::read_dir(doc_examples).unwrap() {
            let p = entry.unwrap().path();
            if p.is_file() {
                let data = fs::read_to_string(&p).unwrap_or_else(|_| panic!("Failed to read file: {p:?}",));

                let value: serde_json::Value =
                    serde_json::from_str(&data).unwrap_or_else(|_| panic!("Failed to deserialize: {p:?}",));

                match value {
                    serde_json::Value::Object(_) => {
                        let _ = serde_json::from_value::<Node>(value)
                            .unwrap_or_else(|_| panic!("Failed to deserialize: {p:?}",));
                    }
                    serde_json::Value::Array(_) => {
                        let _ = serde_json::from_value::<Vec<Node>>(value)
                            .unwrap_or_else(|_| panic!("Failed to deserialize: {p:?}",));
                    }
                    _ => panic!("Expected JSON object or array: {p:?}",),
                }
            }
        }
    }
}
