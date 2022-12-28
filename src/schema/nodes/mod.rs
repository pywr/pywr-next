mod annual_virtual_storage;
mod core;
mod loss_link;
mod river;
mod river_gauge;
mod river_split_with_gauge;
mod virtual_storage;
mod water_treatment_works;

use crate::schema::data_tables::LoadedTableCollection;
pub use crate::schema::nodes::core::{
    AggregatedNode, AggregatedStorageNode, CatchmentNode, InputNode, LinkNode, OutputNode, StorageNode,
};
use crate::schema::nodes::river::RiverNode;
use crate::schema::parameters::DynamicFloatValue;
use crate::PywrError;
pub use annual_virtual_storage::AnnualVirtualStorageNode;
pub use loss_link::LossLinkNode;
use pywr_schema::nodes::{
    CoreNode as CoreNodeV1, CustomNode as CustomNodeV1, Node as NodeV1, NodeMeta as NodeMetaV1,
    NodePosition as NodePositionV1,
};
pub use river_gauge::RiverGaugeNode;
pub use river_split_with_gauge::RiverSplitWithGaugeNode;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
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

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
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

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct CustomNode {
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(flatten)]
    pub meta: NodeMeta,
    #[serde(flatten)]
    pub attributes: HashMap<String, Value>,
}

impl From<CustomNodeV1> for CustomNode {
    fn from(v1: CustomNodeV1) -> Self {
        Self {
            ty: v1.ty,
            meta: v1.meta.into(),
            attributes: v1.attributes,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "type")]
pub enum CoreNode {
    Input(InputNode),
    Link(LinkNode),
    Output(OutputNode),
    Storage(StorageNode),
    Catchment(CatchmentNode),
    RiverGauge(RiverGaugeNode),
    LossLink(LossLinkNode),
    River(RiverNode),
    RiverSplitWithGauge(RiverSplitWithGaugeNode),
    WaterTreatmentWorks(WaterTreatmentWorks),
    Aggregated(AggregatedNode),
    AggregatedStorage(AggregatedStorageNode),
    VirtualStorage(VirtualStorageNode),
    AnnualVirtualStorage(AnnualVirtualStorageNode),
}

impl CoreNode {
    pub fn name(&self) -> &str {
        self.meta().name.as_str()
    }

    pub fn position(&self) -> Option<&NodePosition> {
        self.meta().position.as_ref()
    }

    pub fn node_type(&self) -> &str {
        match self {
            CoreNode::Input(_) => "Input",
            CoreNode::Link(_) => "Link",
            CoreNode::Output(_) => "Output",
            CoreNode::Storage(_) => "Storage",
            CoreNode::Catchment(_) => "Catchment",
            CoreNode::RiverGauge(_) => "RiverGauge",
            CoreNode::LossLink(_) => "LossLink",
            CoreNode::River(_) => "River",
            CoreNode::RiverSplitWithGauge(_) => "River",
            CoreNode::WaterTreatmentWorks(_) => "WaterTreatmentWorks",
            CoreNode::Aggregated(_) => "Aggregated",
            CoreNode::AggregatedStorage(_) => "AggregatedStorage",
            CoreNode::VirtualStorage(_) => "VirtualStorage",
            CoreNode::AnnualVirtualStorage(_) => "AnnualVirtualStorage",
        }
    }

    pub fn meta(&self) -> &NodeMeta {
        match self {
            CoreNode::Input(n) => &n.meta,
            CoreNode::Link(n) => &n.meta,
            CoreNode::Output(n) => &n.meta,
            CoreNode::Storage(n) => &n.meta,
            CoreNode::Catchment(n) => &n.meta,
            CoreNode::RiverGauge(n) => &n.meta,
            CoreNode::LossLink(n) => &n.meta,
            CoreNode::River(n) => &n.meta,
            CoreNode::RiverSplitWithGauge(n) => &n.meta,
            CoreNode::WaterTreatmentWorks(n) => &n.meta,
            CoreNode::Aggregated(n) => &n.meta,
            CoreNode::AggregatedStorage(n) => &n.meta,
            CoreNode::VirtualStorage(n) => &n.meta,
            CoreNode::AnnualVirtualStorage(n) => &n.meta,
        }
    }

    pub fn parameters(&self) -> HashMap<&str, &DynamicFloatValue> {
        match self {
            CoreNode::Input(n) => n.parameters(),
            CoreNode::Link(n) => n.parameters(),
            CoreNode::Output(n) => n.parameters(),
            CoreNode::Storage(n) => n.parameters(),
            _ => HashMap::new(), // TODO complete
        }
    }

    pub fn add_to_model(
        &self,
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
    ) -> Result<(), PywrError> {
        match self {
            CoreNode::Input(n) => n.add_to_model(model),
            CoreNode::Link(n) => n.add_to_model(model),
            CoreNode::Output(n) => n.add_to_model(model),
            CoreNode::Storage(n) => n.add_to_model(model, tables),
            CoreNode::Catchment(n) => n.add_to_model(model),
            CoreNode::RiverGauge(n) => n.add_to_model(model),
            CoreNode::LossLink(n) => n.add_to_model(model),
            CoreNode::River(n) => n.add_to_model(model),
            CoreNode::RiverSplitWithGauge(n) => n.add_to_model(model),
            CoreNode::WaterTreatmentWorks(n) => n.add_to_model(model),
            CoreNode::Aggregated(n) => n.add_to_model(model),
            CoreNode::AggregatedStorage(n) => n.add_to_model(model),
            CoreNode::VirtualStorage(n) => n.add_to_model(model),
            CoreNode::AnnualVirtualStorage(n) => n.add_to_model(model),
        }
    }

    pub fn set_constraints(
        &self,
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<(), PywrError> {
        match self {
            CoreNode::Input(n) => n.set_constraints(model, tables, data_path),
            CoreNode::Link(n) => n.set_constraints(model, tables, data_path),
            CoreNode::Output(n) => n.set_constraints(model, tables, data_path),
            CoreNode::Storage(n) => n.set_constraints(model, tables, data_path),
            CoreNode::Catchment(n) => n.set_constraints(model, tables, data_path),
            CoreNode::RiverGauge(n) => n.set_constraints(model, tables, data_path),
            CoreNode::LossLink(n) => n.set_constraints(model, tables, data_path),
            CoreNode::River(_) => Ok(()), // No constraints on river node
            CoreNode::RiverSplitWithGauge(n) => n.set_constraints(model, tables, data_path),
            CoreNode::WaterTreatmentWorks(n) => n.set_constraints(model, tables, data_path),
            CoreNode::Aggregated(n) => n.set_constraints(model, tables, data_path),
            CoreNode::AggregatedStorage(_) => Ok(()), // No constraints on aggregated storage nodes.
            CoreNode::VirtualStorage(_) => Ok(()),    // TODO
            CoreNode::AnnualVirtualStorage(_) => Ok(()), // TODO
        }
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<&str>)> {
        match self {
            CoreNode::Input(n) => n.input_connectors(),
            CoreNode::Link(n) => n.input_connectors(),
            CoreNode::Output(n) => n.input_connectors(),
            CoreNode::Storage(n) => n.input_connectors(),
            CoreNode::Catchment(n) => n.input_connectors(),
            CoreNode::RiverGauge(n) => n.input_connectors(),
            CoreNode::LossLink(n) => n.input_connectors(),
            CoreNode::River(n) => n.input_connectors(),
            CoreNode::RiverSplitWithGauge(n) => n.input_connectors(),
            CoreNode::WaterTreatmentWorks(n) => n.input_connectors(),
            // TODO input_connectors should not exist for these aggregated & virtual nodes
            CoreNode::Aggregated(n) => n.input_connectors(),
            CoreNode::AggregatedStorage(n) => n.input_connectors(),
            CoreNode::VirtualStorage(n) => n.input_connectors(),
            CoreNode::AnnualVirtualStorage(n) => n.input_connectors(),
        }
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<&str>)> {
        match self {
            CoreNode::Input(n) => n.output_connectors(),
            CoreNode::Link(n) => n.output_connectors(),
            CoreNode::Output(n) => n.output_connectors(),
            CoreNode::Storage(n) => n.output_connectors(),
            CoreNode::Catchment(n) => n.output_connectors(),
            CoreNode::RiverGauge(n) => n.output_connectors(),
            CoreNode::LossLink(n) => n.output_connectors(),
            CoreNode::River(n) => n.output_connectors(),
            CoreNode::RiverSplitWithGauge(n) => n.output_connectors(),
            CoreNode::WaterTreatmentWorks(n) => n.output_connectors(),
            // TODO output_connectors should not exist for these aggregated & virtual nodes
            CoreNode::Aggregated(n) => n.output_connectors(),
            CoreNode::AggregatedStorage(n) => n.output_connectors(),
            CoreNode::VirtualStorage(n) => n.output_connectors(),
            CoreNode::AnnualVirtualStorage(n) => n.output_connectors(),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
#[serde(untagged)]
pub enum Node {
    Core(CoreNode),
    Custom(CustomNode),
}

impl Node {
    pub fn name(&self) -> &str {
        match self {
            Node::Core(n) => n.name(),
            Node::Custom(n) => n.meta.name.as_str(),
        }
    }

    pub fn position(&self) -> Option<&NodePosition> {
        match self {
            Node::Core(n) => n.position(),
            Node::Custom(n) => n.meta.position.as_ref(),
        }
    }

    pub fn node_type(&self) -> &str {
        match self {
            Node::Core(n) => n.node_type(),
            Node::Custom(n) => n.ty.as_str(),
        }
    }

    pub fn parameters(&self) -> HashMap<&str, &DynamicFloatValue> {
        match self {
            Node::Core(n) => n.parameters(),
            Node::Custom(_) => HashMap::new(),
        }
    }

    pub fn add_to_model(
        &self,
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
    ) -> Result<(), PywrError> {
        match self {
            Node::Core(n) => n.add_to_model(model, tables),
            Node::Custom(n) => panic!("TODO custom nodes not yet supported: {}", n.meta.name.as_str()),
        }
    }

    pub fn set_constraints(
        &self,
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<(), PywrError> {
        match self {
            Node::Core(n) => n.set_constraints(model, tables, data_path),
            Node::Custom(n) => panic!("TODO custom nodes not yet supported: {}", n.meta.name.as_str()),
        }
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<&str>)> {
        match self {
            Node::Core(n) => n.input_connectors(),
            Node::Custom(n) => panic!("TODO custom nodes not yet supported: {}", n.meta.name.as_str()),
        }
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<&str>)> {
        match self {
            Node::Core(n) => n.output_connectors(),
            Node::Custom(n) => panic!("TODO custom nodes not yet supported: {}", n.meta.name.as_str()),
        }
    }
}

impl TryFrom<NodeV1> for Node {
    type Error = PywrError;

    fn try_from(v1: NodeV1) -> Result<Self, Self::Error> {
        match v1 {
            NodeV1::Core(n) => {
                let nv2: CoreNode = n.try_into()?;
                Ok(Self::Core(nv2))
            }
            NodeV1::Custom(n) => {
                // Custom nodes are left as is (and therefore may not work).
                let nv2 = CustomNode {
                    meta: n.meta.into(),
                    ty: n.ty,
                    attributes: n.attributes,
                };

                Ok(Self::Custom(nv2))
            }
        }
    }
}

impl TryFrom<Box<CoreNodeV1>> for CoreNode {
    type Error = PywrError;

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
        };

        Ok(n)
    }
}
