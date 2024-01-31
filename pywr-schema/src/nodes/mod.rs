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
mod virtual_storage;
mod water_treatment_works;

use crate::data_tables::LoadedTableCollection;
use crate::error::{ConversionError, SchemaError};
use crate::model::{PywrMultiNetworkTransfer, PywrNetwork};
pub use crate::nodes::core::{
    AggregatedNode, AggregatedStorageNode, CatchmentNode, InputNode, LinkNode, OutputNode, StorageNode,
};
pub use crate::nodes::delay::DelayNode;
pub use crate::nodes::river::RiverNode;
use crate::parameters::DynamicFloatValue;
pub use annual_virtual_storage::AnnualVirtualStorageNode;
pub use loss_link::LossLinkNode;
pub use monthly_virtual_storage::MonthlyVirtualStorageNode;
pub use piecewise_link::{PiecewiseLinkNode, PiecewiseLinkStep};
pub use piecewise_storage::PiecewiseStorageNode;
use pywr_core::metric::Metric;
use pywr_core::models::ModelDomain;
use pywr_v1_schema::nodes::{
    CoreNode as CoreNodeV1, Node as NodeV1, NodeMeta as NodeMetaV1, NodePosition as NodePositionV1,
};
pub use river_gauge::RiverGaugeNode;
pub use river_split_with_gauge::RiverSplitWithGaugeNode;
use std::collections::HashMap;
use std::path::Path;
use strum_macros::{Display, EnumDiscriminants, EnumString, IntoStaticStr, VariantNames};
pub use virtual_storage::VirtualStorageNode;
pub use water_treatment_works::WaterTreatmentWorks;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy)]
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

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Default)]
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
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy, Display)]
pub enum NodeAttribute {
    Inflow,
    Outflow,
    Volume,
    ProportionalVolume,
    Loss,
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
            let name = format!("{}-{}", self.ty.to_string(), num);
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
            NodeType::River => Node::River(RiverNode { meta }),
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
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, EnumDiscriminants)]
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
        }
    }

    pub fn parameters(&self) -> HashMap<&str, &DynamicFloatValue> {
        match self {
            Node::Input(n) => n.parameters(),
            Node::Link(n) => n.parameters(),
            Node::Output(n) => n.parameters(),
            Node::Storage(n) => n.parameters(),
            _ => HashMap::new(), // TODO complete
        }
    }

    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &PywrNetwork,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
    ) -> Result<(), SchemaError> {
        match self {
            Node::Input(n) => n.add_to_model(network),
            Node::Link(n) => n.add_to_model(network),
            Node::Output(n) => n.add_to_model(network),
            Node::Storage(n) => n.add_to_model(network, schema, domain, tables, data_path, inter_network_transfers),
            Node::Catchment(n) => n.add_to_model(network),
            Node::RiverGauge(n) => n.add_to_model(network),
            Node::LossLink(n) => n.add_to_model(network),
            Node::River(n) => n.add_to_model(network),
            Node::RiverSplitWithGauge(n) => n.add_to_model(network),
            Node::WaterTreatmentWorks(n) => n.add_to_model(network),
            Node::Aggregated(n) => n.add_to_model(network),
            Node::AggregatedStorage(n) => n.add_to_model(network),
            Node::VirtualStorage(n) => {
                n.add_to_model(network, schema, domain, tables, data_path, inter_network_transfers)
            }
            Node::AnnualVirtualStorage(n) => {
                n.add_to_model(network, schema, domain, tables, data_path, inter_network_transfers)
            }
            Node::PiecewiseLink(n) => n.add_to_model(network),
            Node::PiecewiseStorage(n) => {
                n.add_to_model(network, schema, domain, tables, data_path, inter_network_transfers)
            }
            Node::Delay(n) => n.add_to_model(network),
            Node::MonthlyVirtualStorage(n) => {
                n.add_to_model(network, schema, domain, tables, data_path, inter_network_transfers)
            }
        }
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &PywrNetwork,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
    ) -> Result<(), SchemaError> {
        match self {
            Node::Input(n) => n.set_constraints(network, schema, domain, tables, data_path, inter_network_transfers),
            Node::Link(n) => n.set_constraints(network, schema, domain, tables, data_path, inter_network_transfers),
            Node::Output(n) => n.set_constraints(network, schema, domain, tables, data_path, inter_network_transfers),
            Node::Storage(n) => n.set_constraints(network, schema, domain, tables, data_path, inter_network_transfers),
            Node::Catchment(n) => {
                n.set_constraints(network, schema, domain, tables, data_path, inter_network_transfers)
            }
            Node::RiverGauge(n) => {
                n.set_constraints(network, schema, domain, tables, data_path, inter_network_transfers)
            }
            Node::LossLink(n) => n.set_constraints(network, schema, domain, tables, data_path, inter_network_transfers),
            Node::River(_) => Ok(()), // No constraints on river node
            Node::RiverSplitWithGauge(n) => {
                n.set_constraints(network, schema, domain, tables, data_path, inter_network_transfers)
            }
            Node::WaterTreatmentWorks(n) => {
                n.set_constraints(network, schema, domain, tables, data_path, inter_network_transfers)
            }
            Node::Aggregated(n) => {
                n.set_constraints(network, schema, domain, tables, data_path, inter_network_transfers)
            }
            Node::AggregatedStorage(_) => Ok(()), // No constraints on aggregated storage nodes.
            Node::VirtualStorage(_) => Ok(()),    // TODO
            Node::AnnualVirtualStorage(_) => Ok(()), // TODO
            Node::PiecewiseLink(n) => {
                n.set_constraints(network, schema, domain, tables, data_path, inter_network_transfers)
            }
            Node::PiecewiseStorage(n) => {
                n.set_constraints(network, schema, domain, tables, data_path, inter_network_transfers)
            }
            Node::Delay(n) => n.set_constraints(network, tables),
            Node::MonthlyVirtualStorage(_) => Ok(()), // TODO
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
        }
    }

    /// Create a metric for the given attribute on this node.
    pub fn create_metric(
        &self,
        network: &mut pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<Metric, SchemaError> {
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
            CoreNodeV1::RollingVirtualStorage(_) => todo!("Conversion of RollingVirtualStorage nodes"),
        };

        Ok(n)
    }
}
