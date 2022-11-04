use crate::node::StorageInitialVolume;
use crate::schema::nodes::NodeMeta;
use crate::schema::parameters::{ConstantFloatValue, DynamicFloatValue, ParameterFloatValue};
use crate::{NodeIndex, PywrError};
use std::collections::HashMap;

#[derive(serde::Deserialize, serde::Serialize)]
pub struct InputNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub max_flow: Option<DynamicFloatValue>,
    pub min_flow: Option<DynamicFloatValue>,
    pub cost: Option<DynamicFloatValue>,
}

impl InputNode {
    pub fn parameters(&self) -> HashMap<&str, &DynamicFloatValue> {
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

    pub fn add_to_model(&self, model: &mut crate::model::Model) -> Result<(), PywrError> {
        model.add_input_node(self.meta.name.as_str(), None)?;
        Ok(())
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<&str>)> {
        vec![(self.meta.name.as_str(), None)]
    }
    pub fn output_connectors(&self) -> Vec<(&str, Option<&str>)> {
        vec![(self.meta.name.as_str(), None)]
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct LinkNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub max_flow: Option<DynamicFloatValue>,
    pub min_flow: Option<DynamicFloatValue>,
    pub cost: Option<DynamicFloatValue>,
}

impl LinkNode {
    pub fn parameters(&self) -> HashMap<&str, &DynamicFloatValue> {
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

    pub fn add_to_model(&self, model: &mut crate::model::Model) -> Result<(), PywrError> {
        model.add_link_node(self.meta.name.as_str(), None)?;
        Ok(())
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<&str>)> {
        vec![(self.meta.name.as_str(), None)]
    }
    pub fn output_connectors(&self) -> Vec<(&str, Option<&str>)> {
        vec![(self.meta.name.as_str(), None)]
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct OutputNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub max_flow: Option<DynamicFloatValue>,
    pub min_flow: Option<DynamicFloatValue>,
    pub cost: Option<DynamicFloatValue>,
}

impl OutputNode {
    pub fn parameters(&self) -> HashMap<&str, &DynamicFloatValue> {
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

    pub fn add_to_model(&self, model: &mut crate::model::Model) -> Result<(), PywrError> {
        model.add_output_node(self.meta.name.as_str(), None)?;
        Ok(())
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<&str>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<&str>)> {
        vec![(self.meta.name.as_str(), None)]
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct StorageNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub max_volume: Option<ConstantFloatValue>,
    pub min_volume: Option<ConstantFloatValue>,
    pub cost: Option<DynamicFloatValue>,
    pub initial_volume: Option<f64>,
    pub initial_volume_pc: Option<f64>,
}

impl StorageNode {
    pub fn parameters(&self) -> HashMap<&str, &DynamicFloatValue> {
        let mut attributes = HashMap::new();
        // if let Some(p) = &self.max_volume {
        //     attributes.insert("max_volume", p);
        // }
        // if let Some(p) = &self.min_volume {
        //     attributes.insert("min_volume", p);
        // }
        if let Some(p) = &self.cost {
            attributes.insert("cost", p);
        }

        attributes
    }

    pub fn add_to_model(&self, model: &mut crate::model::Model) -> Result<(), PywrError> {
        let initial_volume = if let Some(iv) = self.initial_volume {
            StorageInitialVolume::Absolute(iv)
        } else if let Some(pc) = self.initial_volume_pc {
            StorageInitialVolume::Proportional(pc)
        } else {
            return Err(PywrError::MissingInitialVolume(self.meta.name.to_string()));
        };

        model.add_storage_node(self.meta.name.as_str(), None, initial_volume)?;
        Ok(())
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<&str>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<&str>)> {
        vec![(self.meta.name.as_str(), None)]
    }
}

#[doc = svgbobdoc::transform!(
/// This is used to represent a catchment inflow.
///
/// Catchment nodes create a single [`crate::node::InputNode`] node in the model, but
/// ensure that the maximum and minimum flow are equal to [`Self::flow`].
///
/// ```svgbob
///  <node>     D
///     *----->*- - -
/// ```
///
)]
#[derive(serde::Deserialize, serde::Serialize)]
pub struct CatchmentNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub flow: Option<DynamicFloatValue>,
    pub cost: Option<DynamicFloatValue>,
}

impl CatchmentNode {
    pub fn add_to_model(&self, model: &mut crate::model::Model) -> Result<(), PywrError> {
        model.add_input_node(self.meta.name.as_str(), None)?;
        Ok(())
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<&str>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<&str>)> {
        vec![(self.meta.name.as_str(), None)]
    }
}

#[doc = svgbobdoc::transform!(
/// This is used to represent a minimum residual flow (MRF) at a gauging station.
///
///
/// ```svgbob
///                               <node>.xxx
///            <node>.xxx     .------->L ------.
///          .----->L -------|                 |----->  D
///    U  --|                '-------->L -----'
///         '------>O
///            <node>.xxx
/// ```
///
)]
#[derive(serde::Deserialize, serde::Serialize)]
pub struct WaterTreatmentWorks {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub mrf: Option<DynamicFloatValue>,
    pub mrf_cost: Option<DynamicFloatValue>,
}

#[cfg(test)]
mod tests {
    use crate::schema::nodes::InputNode;

    #[test]
    fn test_input() {
        let data = r#"
            {
                "name": "supply1",
                "type": "Input",
                "max_flow": 15.0
            }
            "#;

        let node: InputNode = serde_json::from_str(data).unwrap();

        assert_eq!(node.meta.name, "supply1");
    }
}
