use crate::schema::nodes::NodeMeta;
use crate::schema::parameters::ParameterFloatValue;
use std::collections::HashMap;

#[derive(serde::Deserialize, serde::Serialize)]
pub struct InputNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub max_flow: Option<ParameterFloatValue>,
    pub min_flow: Option<ParameterFloatValue>,
    pub cost: Option<ParameterFloatValue>,
}

impl InputNode {
    pub fn parameters(&self) -> HashMap<&str, &ParameterFloatValue> {
        let mut attributes = HashMap::new();
        if let Some(p) = &self.max_flow {
            attributes.insert("max_flow", p);
        }
        if let Some(p) = &self.min_flow {
            attributes.insert("min_flow", p);
        }
        if let Some(p) = &self.cost {
            attributes.insert("cost", p);
        }

        attributes
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct LinkNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub max_flow: Option<ParameterFloatValue>,
    pub min_flow: Option<ParameterFloatValue>,
    pub cost: Option<ParameterFloatValue>,
}

impl LinkNode {
    pub fn parameters(&self) -> HashMap<&str, &ParameterFloatValue> {
        let mut attributes = HashMap::new();
        if let Some(p) = &self.max_flow {
            attributes.insert("max_flow", p);
        }
        if let Some(p) = &self.min_flow {
            attributes.insert("min_flow", p);
        }
        if let Some(p) = &self.cost {
            attributes.insert("cost", p);
        }

        attributes
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct OutputNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub max_flow: Option<ParameterFloatValue>,
    pub min_flow: Option<ParameterFloatValue>,
    pub cost: Option<ParameterFloatValue>,
}

impl OutputNode {
    pub fn parameters(&self) -> HashMap<&str, &ParameterFloatValue> {
        let mut attributes = HashMap::new();
        if let Some(p) = &self.max_flow {
            attributes.insert("max_flow", p);
        }
        if let Some(p) = &self.min_flow {
            attributes.insert("min_flow", p);
        }
        if let Some(p) = &self.cost {
            attributes.insert("cost", p);
        }

        attributes
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct StorageNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub max_volume: Option<ParameterFloatValue>,
    pub min_volume: Option<ParameterFloatValue>,
    pub cost: Option<ParameterFloatValue>,
    pub initial_volume: Option<f64>,
    pub initial_volume_pc: Option<f64>,
}

impl StorageNode {
    pub fn parameters(&self) -> HashMap<&str, &ParameterFloatValue> {
        let mut attributes = HashMap::new();
        if let Some(p) = &self.max_volume {
            attributes.insert("max_volume", p);
        }
        if let Some(p) = &self.min_volume {
            attributes.insert("min_volume", p);
        }
        if let Some(p) = &self.cost {
            attributes.insert("cost", p);
        }

        attributes
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct ReservoirNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub max_volume: Option<ParameterFloatValue>,
    pub min_volume: Option<ParameterFloatValue>,
    pub cost: Option<ParameterFloatValue>,
    pub initial_volume: Option<f64>,
    pub initial_volume_pc: Option<f64>,
}

impl ReservoirNode {
    pub fn parameters(&self) -> HashMap<&str, &ParameterFloatValue> {
        let mut attributes = HashMap::new();
        if let Some(p) = &self.max_volume {
            attributes.insert("max_volume", p);
        }
        if let Some(p) = &self.min_volume {
            attributes.insert("min_volume", p);
        }
        if let Some(p) = &self.cost {
            attributes.insert("cost", p);
        }

        attributes
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct CatchmentNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub flow: Option<ParameterFloatValue>,
    pub cost: Option<ParameterFloatValue>,
}
