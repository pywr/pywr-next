mod core;

pub use crate::schema::nodes::core::{CatchmentNode, InputNode, LinkNode, OutputNode, ReservoirNode, StorageNode};
use crate::schema::parameters::ParameterFloatValue;
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
    #[serde(alias = "input")]
    Input(InputNode),
    #[serde(alias = "link")]
    Link(LinkNode),
    #[serde(alias = "output")]
    Output(OutputNode),
    #[serde(alias = "storage")]
    Storage(StorageNode),
    #[serde(alias = "reservoir")]
    Reservoir(ReservoirNode),
    #[serde(alias = "catchment")]
    Catchment(CatchmentNode),
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
            CoreNode::Input(_) => "input",
            CoreNode::Link(_) => "link",
            CoreNode::Output(_) => "output",
            CoreNode::Storage(_) => "storage",
            CoreNode::Reservoir(_) => "reservoir",
            CoreNode::Catchment(_) => "catchment",
        }
    }

    pub fn meta(&self) -> &NodeMeta {
        match self {
            CoreNode::Input(n) => &n.meta,
            CoreNode::Link(n) => &n.meta,
            CoreNode::Output(n) => &n.meta,
            CoreNode::Storage(n) => &n.meta,
            CoreNode::Reservoir(n) => &n.meta,
            CoreNode::Catchment(n) => &n.meta,
        }
    }

    pub fn parameters(&self) -> HashMap<&str, &ParameterFloatValue> {
        match self {
            CoreNode::Input(n) => n.parameters(),
            CoreNode::Link(n) => n.parameters(),
            CoreNode::Output(n) => n.parameters(),
            CoreNode::Storage(n) => n.parameters(),
            CoreNode::Reservoir(n) => n.parameters(),
            _ => HashMap::new(),
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

    pub fn parameters(&self) -> HashMap<&str, &ParameterFloatValue> {
        match self {
            Node::Core(n) => n.parameters(),
            Node::Custom(_) => HashMap::new(),
        }
    }
}
