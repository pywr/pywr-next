mod annual_virtual_storage;
mod core;
mod delay;
mod loss_link;
mod monthly_virtual_storage;
mod piecewise_link;
mod piecewise_storage;
mod river;
mod river_gauge;
mod river_split_with_gauge;
mod rolling_virtual_storage;
mod turbine;
mod virtual_storage;
mod water_treatment_works;

use crate::error::ConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::model::PywrNetwork;
use crate::parameters::TimeseriesV1Data;
use crate::visit::{VisitMetrics, VisitPaths};
pub use annual_virtual_storage::{AnnualReset, AnnualVirtualStorageNode};
pub use core::{
    AggregatedNode, AggregatedStorageNode, CatchmentNode, InputNode, LinkNode, OutputNode, Relationship,
    StorageInitialVolume, StorageNode,
};
pub use delay::DelayNode;
pub use loss_link::{LossFactor, LossLinkNode};
pub use monthly_virtual_storage::MonthlyVirtualStorageNode;
pub use piecewise_link::{PiecewiseLinkNode, PiecewiseLinkStep};
pub use piecewise_storage::{PiecewiseStorageNode, PiecewiseStore};
#[cfg(feature = "core")]
use pywr_core::metric::MetricF64;
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::nodes::{
    CoreNode as CoreNodeV1, Node as NodeV1, NodeMeta as NodeMetaV1, NodePosition as NodePositionV1,
};
use pywr_v1_schema::parameters::{
    CoreParameter as CoreParameterV1, Parameter as ParameterV1, ParameterValue as ParameterValueV1, ParameterValueType,
};
pub use river::RiverNode;
pub use river_gauge::RiverGaugeNode;
pub use river_split_with_gauge::{RiverSplit, RiverSplitWithGaugeNode};
pub use rolling_virtual_storage::{RollingVirtualStorageNode, RollingWindow};
use schemars::JsonSchema;
use std::path::{Path, PathBuf};
use strum_macros::{Display, EnumDiscriminants, EnumString, IntoStaticStr, VariantNames};
pub use turbine::{TargetType, TurbineNode};
pub use virtual_storage::VirtualStorageNode;
pub use water_treatment_works::WaterTreatmentWorks;

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

/// All possible attributes that could be produced by a node.
///
///
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy, Display, JsonSchema, PywrVisitAll)]
pub enum NodeAttribute {
    Inflow,
    Outflow,
    Volume,
    ProportionalVolume,
    Loss,
    Deficit,
    Power,
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
    pub fn next_default_name_for_model(mut self, network: &PywrNetwork) -> Self {
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
            NodeType::WaterTreatmentWorks => Node::WaterTreatmentWorks(WaterTreatmentWorks {
                meta,
                ..Default::default()
            }),
            NodeType::Aggregated => Node::Aggregated(AggregatedNode {
                meta,
                ..Default::default()
            }),
            NodeType::AggregatedStorage => Node::AggregatedStorage(AggregatedStorageNode {
                meta,
                ..Default::default()
            }),
            NodeType::VirtualStorage => Node::VirtualStorage(VirtualStorageNode {
                meta,
                ..Default::default()
            }),
            NodeType::AnnualVirtualStorage => Node::AnnualVirtualStorage(AnnualVirtualStorageNode {
                meta,
                ..Default::default()
            }),
            NodeType::MonthlyVirtualStorage => Node::MonthlyVirtualStorage(MonthlyVirtualStorageNode {
                meta,
                ..Default::default()
            }),
            NodeType::RollingVirtualStorage => Node::RollingVirtualStorage(RollingVirtualStorageNode {
                meta,
                ..Default::default()
            }),
            NodeType::Turbine => Node::Turbine(TurbineNode {
                meta,
                ..Default::default()
            }),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, EnumDiscriminants, Debug, JsonSchema)]
#[serde(tag = "type")]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, VariantNames))]
// This creates a separate enum called `NodeType` that is available in this module.
#[strum_discriminants(name(NodeType))]
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
    WaterTreatmentWorks(WaterTreatmentWorks),
    Aggregated(AggregatedNode),
    AggregatedStorage(AggregatedStorageNode),
    VirtualStorage(VirtualStorageNode),
    AnnualVirtualStorage(AnnualVirtualStorageNode),
    MonthlyVirtualStorage(MonthlyVirtualStorageNode),
    RollingVirtualStorage(RollingVirtualStorageNode),
    Turbine(TurbineNode),
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
            Node::Aggregated(n) => &n.meta,
            Node::AggregatedStorage(n) => &n.meta,
            Node::VirtualStorage(n) => &n.meta,
            Node::AnnualVirtualStorage(n) => &n.meta,
            Node::PiecewiseLink(n) => &n.meta,
            Node::PiecewiseStorage(n) => &n.meta,
            Node::Delay(n) => &n.meta,
            Node::MonthlyVirtualStorage(n) => &n.meta,
            Node::RollingVirtualStorage(n) => &n.meta,
            Node::Turbine(n) => &n.meta,
        }
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        match self {
            Node::Input(n) => n.input_connectors(),
            Node::Link(n) => n.input_connectors(),
            Node::Output(n) => n.input_connectors(),
            Node::Storage(n) => n.input_connectors(),
            Node::Catchment(n) => n.input_connectors(),
            Node::RiverGauge(n) => n.input_connectors(),
            Node::LossLink(n) => n.input_connectors(),
            Node::River(n) => n.input_connectors(),
            Node::RiverSplitWithGauge(n) => n.input_connectors(),
            Node::WaterTreatmentWorks(n) => n.input_connectors(),
            // TODO input_connectors should not exist for these aggregated & virtual nodes
            Node::Aggregated(n) => n.input_connectors(),
            Node::AggregatedStorage(n) => n.input_connectors(),
            Node::VirtualStorage(n) => n.input_connectors(),
            Node::AnnualVirtualStorage(n) => n.input_connectors(),
            Node::MonthlyVirtualStorage(n) => n.input_connectors(),
            Node::PiecewiseLink(n) => n.input_connectors(),
            Node::PiecewiseStorage(n) => n.input_connectors(),
            Node::Delay(n) => n.input_connectors(),
            Node::RollingVirtualStorage(n) => n.input_connectors(),
            Node::Turbine(n) => n.input_connectors(),
        }
    }

    pub fn output_connectors(&self, slot: Option<&str>) -> Vec<(&str, Option<String>)> {
        match self {
            Node::Input(n) => n.output_connectors(),
            Node::Link(n) => n.output_connectors(),
            Node::Output(n) => n.output_connectors(),
            Node::Storage(n) => n.output_connectors(),
            Node::Catchment(n) => n.output_connectors(),
            Node::RiverGauge(n) => n.output_connectors(),
            Node::LossLink(n) => n.output_connectors(),
            Node::River(n) => n.output_connectors(),
            Node::RiverSplitWithGauge(n) => n.output_connectors(slot),
            Node::WaterTreatmentWorks(n) => n.output_connectors(),
            // TODO output_connectors should not exist for these aggregated & virtual nodes
            Node::Aggregated(n) => n.output_connectors(),
            Node::AggregatedStorage(n) => n.output_connectors(),
            Node::VirtualStorage(n) => n.output_connectors(),
            Node::AnnualVirtualStorage(n) => n.output_connectors(),
            Node::MonthlyVirtualStorage(n) => n.output_connectors(),
            Node::PiecewiseLink(n) => n.output_connectors(),
            Node::PiecewiseStorage(n) => n.output_connectors(),
            Node::Delay(n) => n.output_connectors(),
            Node::RollingVirtualStorage(n) => n.output_connectors(),
            Node::Turbine(n) => n.output_connectors(),
        }
    }

    pub fn default_metric(&self) -> NodeAttribute {
        match self {
            Node::Input(n) => n.default_metric(),
            Node::Link(n) => n.default_metric(),
            Node::Output(n) => n.default_metric(),
            Node::Storage(n) => n.default_metric(),
            Node::Catchment(n) => n.default_metric(),
            Node::RiverGauge(n) => n.default_metric(),
            Node::LossLink(n) => n.default_metric(),
            Node::River(n) => n.default_metric(),
            Node::RiverSplitWithGauge(n) => n.default_metric(),
            Node::WaterTreatmentWorks(n) => n.default_metric(),
            Node::Aggregated(n) => n.default_metric(),
            Node::AggregatedStorage(n) => n.default_metric(),
            Node::VirtualStorage(n) => n.default_metric(),
            Node::AnnualVirtualStorage(n) => n.default_metric(),
            Node::MonthlyVirtualStorage(n) => n.default_metric(),
            Node::PiecewiseLink(n) => n.default_metric(),
            Node::PiecewiseStorage(n) => n.default_metric(),
            Node::Delay(n) => n.default_metric(),
            Node::RollingVirtualStorage(n) => n.default_metric(),
            Node::Turbine(n) => n.default_metric(),
        }
    }
}

#[cfg(feature = "core")]
impl Node {
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network, args: &LoadArgs) -> Result<(), SchemaError> {
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
            Node::Aggregated(n) => n.add_to_model(network, args),
            Node::AggregatedStorage(n) => n.add_to_model(network),
            Node::VirtualStorage(n) => n.add_to_model(network, args),
            Node::AnnualVirtualStorage(n) => n.add_to_model(network, args),
            Node::PiecewiseLink(n) => n.add_to_model(network),
            Node::PiecewiseStorage(n) => n.add_to_model(network, args),
            Node::Delay(n) => n.add_to_model(network),
            Node::Turbine(n) => n.add_to_model(network, args),
            Node::MonthlyVirtualStorage(n) => n.add_to_model(network, args),
            Node::RollingVirtualStorage(n) => n.add_to_model(network, args),
        }
    }

    /// Get the node indices for the constraints that this node has added to the network.
    pub fn node_indices_for_constraints(
        &self,
        network: &pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        match self {
            Node::Input(n) => n.node_indices_for_constraints(network),
            Node::Link(n) => n.node_indices_for_constraints(network),
            Node::Output(n) => n.node_indices_for_constraints(network),
            Node::Storage(n) => n.node_indices_for_constraints(network),
            Node::Catchment(n) => n.node_indices_for_constraints(network),
            Node::RiverGauge(n) => n.node_indices_for_constraints(network),
            Node::LossLink(n) => n.node_indices_for_constraints(network),
            Node::River(n) => n.node_indices_for_constraints(network),
            Node::RiverSplitWithGauge(n) => n.node_indices_for_constraints(network),
            Node::WaterTreatmentWorks(n) => n.node_indices_for_constraints(network),
            Node::Aggregated(n) => n.node_indices_for_constraints(network, args),
            Node::AggregatedStorage(n) => n.node_indices_for_constraints(network, args),
            Node::VirtualStorage(n) => n.node_indices_for_constraints(network, args),
            Node::AnnualVirtualStorage(n) => n.node_indices_for_constraints(network, args),
            Node::PiecewiseLink(n) => n.node_indices_for_constraints(network),
            Node::PiecewiseStorage(n) => n.node_indices_for_constraints(network),
            Node::Delay(n) => n.node_indices_for_constraints(network),
            Node::Turbine(n) => n.node_indices_for_constraints(network),
            Node::MonthlyVirtualStorage(n) => n.node_indices_for_constraints(network, args),
            Node::RollingVirtualStorage(n) => n.node_indices_for_constraints(network, args),
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
            Node::Aggregated(n) => n.set_constraints(network, args),
            Node::AggregatedStorage(_) => Ok(()), // No constraints on aggregated storage nodes.
            Node::VirtualStorage(_) => Ok(()),    // TODO
            Node::AnnualVirtualStorage(_) => Ok(()), // TODO
            Node::PiecewiseLink(n) => n.set_constraints(network, args),
            Node::PiecewiseStorage(n) => n.set_constraints(network, args),
            Node::Delay(n) => n.set_constraints(network, args),
            Node::Turbine(n) => n.set_constraints(network, args),
            Node::MonthlyVirtualStorage(_) => Ok(()), // TODO
            Node::RollingVirtualStorage(_) => Ok(()), // TODO
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
            Node::Aggregated(n) => n.create_metric(network, attribute),
            Node::AggregatedStorage(n) => n.create_metric(network, attribute),
            Node::VirtualStorage(n) => n.create_metric(network, attribute),
            Node::AnnualVirtualStorage(n) => n.create_metric(network, attribute),
            Node::MonthlyVirtualStorage(n) => n.create_metric(network, attribute),
            Node::PiecewiseLink(n) => n.create_metric(network, attribute),
            Node::PiecewiseStorage(n) => n.create_metric(network, attribute),
            Node::Delay(n) => n.create_metric(network, attribute),
            Node::RollingVirtualStorage(n) => n.create_metric(network, attribute),
            Node::Turbine(n) => n.create_metric(network, attribute, args),
        }
    }
}

impl TryFrom<NodeV1> for Node {
    type Error = ConversionError;

    fn try_from(v1: NodeV1) -> Result<Self, Self::Error> {
        match v1 {
            NodeV1::Core(n) => {
                let nv2: Node = n.try_into()?;
                Ok(nv2)
            }
            NodeV1::Custom(n) => Err(ConversionError::CustomNodeNotSupported {
                name: n.meta.name,
                ty: n.ty,
            }),
        }
    }
}

impl TryFrom<Box<CoreNodeV1>> for Node {
    type Error = ConversionError;

    fn try_from(v1: Box<CoreNodeV1>) -> Result<Self, Self::Error> {
        let n = match *v1 {
            CoreNodeV1::Input(n) => Self::Input(n.try_into()?),
            CoreNodeV1::Link(n) => Self::Link(n.try_into()?),
            CoreNodeV1::Output(n) => Self::Output(n.try_into()?),
            CoreNodeV1::Storage(n) => Self::Storage(n.try_into()?),
            CoreNodeV1::Reservoir(n) => Self::Storage(n.try_into()?),
            CoreNodeV1::Catchment(n) => Self::Catchment(n.try_into()?),
            CoreNodeV1::RiverGauge(n) => Self::RiverGauge(n.try_into()?),
            CoreNodeV1::LossLink(n) => Self::LossLink(n.try_into()?),
            CoreNodeV1::River(n) => Self::River(n.try_into()?),
            CoreNodeV1::RiverSplitWithGauge(n) => Self::RiverSplitWithGauge(n.try_into()?),
            CoreNodeV1::Aggregated(n) => Self::Aggregated(n.try_into()?),
            CoreNodeV1::AggregatedStorage(n) => Self::AggregatedStorage(n.try_into()?),
            CoreNodeV1::VirtualStorage(n) => Self::VirtualStorage(n.try_into()?),
            CoreNodeV1::AnnualVirtualStorage(n) => Self::AnnualVirtualStorage(n.try_into()?),
            CoreNodeV1::PiecewiseLink(n) => Self::PiecewiseLink(n.try_into()?),
            CoreNodeV1::MultiSplitLink(_) => todo!(),
            CoreNodeV1::BreakLink(_) => todo!(),
            CoreNodeV1::Delay(n) => Self::Delay(n.try_into()?),
            CoreNodeV1::RiverSplit(_) => todo!("Conversion of RiverSplit nodes"),
            CoreNodeV1::MonthlyVirtualStorage(n) => Self::MonthlyVirtualStorage(n.try_into()?),
            CoreNodeV1::SeasonalVirtualStorage(_) => todo!("Conversion of SeasonalVirtualStorage nodes"),
            CoreNodeV1::RollingVirtualStorage(n) => Self::RollingVirtualStorage(n.try_into()?),
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
            Node::Aggregated(n) => n.visit_metrics(visitor),
            Node::AggregatedStorage(n) => n.visit_metrics(visitor),
            Node::VirtualStorage(n) => n.visit_metrics(visitor),
            Node::AnnualVirtualStorage(n) => n.visit_metrics(visitor),
            Node::PiecewiseLink(n) => n.visit_metrics(visitor),
            Node::PiecewiseStorage(n) => n.visit_metrics(visitor),
            Node::Delay(n) => n.visit_metrics(visitor),
            Node::MonthlyVirtualStorage(n) => n.visit_metrics(visitor),
            Node::RollingVirtualStorage(n) => n.visit_metrics(visitor),
            Node::Turbine(n) => n.visit_metrics(visitor),
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
            Node::Aggregated(n) => n.visit_metrics_mut(visitor),
            Node::AggregatedStorage(n) => n.visit_metrics_mut(visitor),
            Node::VirtualStorage(n) => n.visit_metrics_mut(visitor),
            Node::AnnualVirtualStorage(n) => n.visit_metrics_mut(visitor),
            Node::PiecewiseLink(n) => n.visit_metrics_mut(visitor),
            Node::PiecewiseStorage(n) => n.visit_metrics_mut(visitor),
            Node::Delay(n) => n.visit_metrics_mut(visitor),
            Node::MonthlyVirtualStorage(n) => n.visit_metrics_mut(visitor),
            Node::RollingVirtualStorage(n) => n.visit_metrics_mut(visitor),
            Node::Turbine(n) => n.visit_metrics_mut(visitor),
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
            Node::Aggregated(n) => n.visit_paths(visitor),
            Node::AggregatedStorage(n) => n.visit_paths(visitor),
            Node::VirtualStorage(n) => n.visit_paths(visitor),
            Node::AnnualVirtualStorage(n) => n.visit_paths(visitor),
            Node::PiecewiseLink(n) => n.visit_paths(visitor),
            Node::PiecewiseStorage(n) => n.visit_paths(visitor),
            Node::Delay(n) => n.visit_paths(visitor),
            Node::MonthlyVirtualStorage(n) => n.visit_paths(visitor),
            Node::RollingVirtualStorage(n) => n.visit_paths(visitor),
            Node::Turbine(n) => n.visit_paths(visitor),
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
            Node::Aggregated(n) => n.visit_paths_mut(visitor),
            Node::AggregatedStorage(n) => n.visit_paths_mut(visitor),
            Node::VirtualStorage(n) => n.visit_paths_mut(visitor),
            Node::AnnualVirtualStorage(n) => n.visit_paths_mut(visitor),
            Node::PiecewiseLink(n) => n.visit_paths_mut(visitor),
            Node::PiecewiseStorage(n) => n.visit_paths_mut(visitor),
            Node::Delay(n) => n.visit_paths_mut(visitor),
            Node::MonthlyVirtualStorage(n) => n.visit_paths_mut(visitor),
            Node::RollingVirtualStorage(n) => n.visit_paths_mut(visitor),
            Node::Turbine(n) => n.visit_paths_mut(visitor),
        }
    }
}

/// struct that acts as a container for a node and any associated timeseries data.
///
/// v1 nodes may contain inline DataFrame parameters from which data needs to be extract
/// to created timeseries entries in the schema.
#[derive(Debug)]
pub struct NodeAndTimeseries {
    pub node: Node,
    pub timeseries: Option<Vec<TimeseriesV1Data>>,
}

impl TryFrom<NodeV1> for NodeAndTimeseries {
    type Error = ConversionError;

    fn try_from(v1: NodeV1) -> Result<Self, Self::Error> {
        let mut ts_vec = Vec::new();
        let mut unnamed_count: usize = 0;

        // extract timeseries data for all inline DataFame parameters included in the node.
        for param_value in v1.parameters().values() {
            ts_vec.extend(extract_timeseries(param_value, v1.name(), &mut unnamed_count));
        }
        let timeseries = if ts_vec.is_empty() { None } else { Some(ts_vec) };

        // Now convert the node to the v2 schema representation
        let node = Node::try_from(v1)?;
        Ok(Self { node, timeseries })
    }
}

/// Extract timeseries data from a parameter value.
///
/// If the parameter value is a DataFrame, then convert it to timeseries data. If it is another type then recursively
/// call the function on any inline parameters this parameter may contain to check for other dataframe parameters.
fn extract_timeseries(
    param_value: &ParameterValueType,
    name: &str,
    unnamed_count: &mut usize,
) -> Vec<TimeseriesV1Data> {
    let mut ts_vec = Vec::new();
    match param_value {
        ParameterValueType::Single(param) => {
            if let ParameterValueV1::Inline(p) = param {
                if let ParameterV1::Core(CoreParameterV1::DataFrame(df_param)) = p.as_ref() {
                    let mut ts_data: TimeseriesV1Data = df_param.clone().into();
                    if ts_data.name.is_none() {
                        // Because the parameter could contain multiple inline DataFrame parameters use the unnamed_count
                        // to create a unique name.
                        let name = format!("{}-p{}.timeseries", name, unnamed_count);
                        *unnamed_count += 1;
                        ts_data.name = Some(name);
                    }
                    ts_vec.push(ts_data);
                } else {
                    // Not a dataframe parameter but the parameter might have child dataframe parameters.
                    // Update the name and call the function recursively on all child parameters.
                    let name = if p.name().is_none() {
                        let n = format!("{}-p{}", name, unnamed_count);
                        *unnamed_count += 1;
                        n
                    } else {
                        p.name().unwrap().to_string()
                    };
                    for nested_param in p.parameters().values() {
                        ts_vec.extend(extract_timeseries(nested_param, &name, unnamed_count));
                    }
                }
            }
        }
        ParameterValueType::List(params) => {
            for param in params.iter() {
                if let ParameterValueV1::Inline(p) = param {
                    if let ParameterV1::Core(CoreParameterV1::DataFrame(df_param)) = p.as_ref() {
                        let mut ts_data: TimeseriesV1Data = df_param.clone().into();
                        if ts_data.name.is_none() {
                            // Because the parameter could contain multiple inline DataFrame parameters use the unnamed_count
                            // to create a unique name.
                            let name = format!("{}-p{}.timeseries", name, unnamed_count);
                            *unnamed_count += 1;
                            ts_data.name = Some(name);
                        }
                        ts_vec.push(ts_data);
                    } else {
                        // Not a dataframe parameter but the parameter might have child dataframe parameters.
                        // Update the name and call the function recursively on all child parameters.
                        let name = if p.name().is_none() {
                            let n = format!("{}-p{}", name, unnamed_count);
                            *unnamed_count += 1;
                            n
                        } else {
                            p.name().unwrap().to_string()
                        };
                        for nested_param in p.parameters().values() {
                            ts_vec.extend(extract_timeseries(nested_param, &name, unnamed_count));
                        }
                    }
                }
            }
        }
    };
    ts_vec
}

#[cfg(test)]
mod tests {
    use pywr_v1_schema::nodes::Node as NodeV1;

    use crate::metric::Metric;
    use crate::{
        nodes::{Node, NodeAndTimeseries},
        parameters::Parameter,
    };

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

        let node_ts: NodeAndTimeseries = v1_node.try_into().unwrap();

        let input_node = match node_ts.node {
            Node::Input(n) => n,
            _ => panic!("Expected InputNode"),
        };

        let expected_name = String::from("catchment1-p0.timeseries");

        match input_node.max_flow {
            Some(Metric::Timeseries(ts)) => {
                assert_eq!(ts.name(), &expected_name)
            }
            _ => panic!("Expected Timeseries"),
        };

        match node_ts.timeseries {
            Some(ts) => {
                assert_eq!(ts.len(), 1);
                assert_eq!(ts.first().unwrap().name.as_ref().unwrap().as_str(), &expected_name);
            }
            None => panic!("Expected timeseries data"),
        };
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

        let node_ts: NodeAndTimeseries = v1_node.try_into().unwrap();

        let input_node = match node_ts.node {
            Node::Input(n) => n,
            _ => panic!("Expected InputNode"),
        };

        let expected_name1 = String::from("catchment1-p0-p2.timeseries");
        let expected_name2 = String::from("catchment1-p0-p4.timeseries");

        match input_node.max_flow {
            Some(Metric::InlineParameter { definition }) => match *definition {
                Parameter::Aggregated(param) => {
                    assert_eq!(param.metrics.len(), 4);
                    match &param.metrics[1] {
                        Metric::Timeseries(ts) => {
                            assert_eq!(ts.name(), &expected_name1)
                        }
                        _ => panic!("Expected Timeseries"),
                    }

                    match &param.metrics[3] {
                        Metric::Timeseries(ts) => {
                            assert_eq!(ts.name(), &expected_name2)
                        }
                        _ => panic!("Expected Timeseries"),
                    }
                }
                _ => panic!("Expected Aggregated parameter"),
            },
            _ => panic!("Expected Timeseries"),
        };

        match node_ts.timeseries {
            Some(ts) => {
                assert_eq!(ts.len(), 2);
                assert_eq!(ts[0].name.as_ref().unwrap().as_str(), &expected_name1);
                assert_eq!(ts[1].name.as_ref().unwrap().as_str(), &expected_name2);
            }
            None => panic!("Expected timeseries data"),
        };
    }
}
