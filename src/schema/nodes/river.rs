use crate::schema::nodes::NodeMeta;
use crate::schema::parameters::DynamicFloatValue;
use crate::PywrError;
use pywr_schema::nodes::LinkNode as LinkNodeV1;
use std::collections::HashMap;

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct RiverNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
}

impl RiverNode {
    pub fn parameters(&self) -> HashMap<&str, &DynamicFloatValue> {
        HashMap::new()
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

impl TryFrom<LinkNodeV1> for RiverNode {
    type Error = PywrError;

    fn try_from(v1: LinkNodeV1) -> Result<Self, Self::Error> {
        if v1.max_flow.is_some() {
            return Err(PywrError::V1SchemaConversion(
                "River node can not have a `max_flow`".to_string(),
            ));
        }
        if v1.min_flow.is_some() {
            return Err(PywrError::V1SchemaConversion(
                "River node can not have a `min_flow`".to_string(),
            ));
        }
        if v1.cost.is_some() {
            return Err(PywrError::V1SchemaConversion(
                "River node can not have a `cost`".to_string(),
            ));
        }

        let n = Self { meta: v1.meta.into() };
        Ok(n)
    }
}
