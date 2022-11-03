mod core;
mod river_gauge;

pub use crate::schema::nodes::core::{
    CatchmentNode, InputNode, LinkNode, OutputNode, StorageNode, WaterTreatmentWorks,
};
use crate::schema::parameters::DynamicFloatValue;
use crate::{NodeIndex, PywrError};
pub use river_gauge::RiverGaugeNode;
use serde_json::Value;
use std::collections::HashMap;

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct NodePosition {
    pub schematic: Option<(f32, f32)>,
    pub geographic: Option<(f32, f32)>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct NodeMeta {
    pub name: String,
    pub comment: Option<String>,
    pub position: Option<NodePosition>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct CustomNode {
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(flatten)]
    pub meta: NodeMeta,
    #[serde(flatten)]
    pub attributes: HashMap<String, Value>,
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(tag = "type")]
pub enum CoreNode {
    Input(InputNode),
    Link(LinkNode),
    Output(OutputNode),
    Storage(StorageNode),
    Catchment(CatchmentNode),
    RiverGauge(RiverGaugeNode),
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

    pub fn add_to_model(&self, model: &mut crate::model::Model) -> Result<(), PywrError> {
        match self {
            CoreNode::Input(n) => n.add_to_model(model),
            CoreNode::Link(n) => n.add_to_model(model),
            CoreNode::Output(n) => n.add_to_model(model),
            CoreNode::Storage(n) => n.add_to_model(model),
            CoreNode::Catchment(n) => n.add_to_model(model),
            CoreNode::RiverGauge(n) => n.add_to_model(model),
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
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
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

    pub fn add_to_model(&self, model: &mut crate::model::Model) -> Result<(), PywrError> {
        match self {
            Node::Core(n) => n.add_to_model(model),
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
