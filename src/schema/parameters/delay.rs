use crate::schema::data_tables::LoadedTableCollection;
use crate::schema::parameters::{DynamicFloatValue, DynamicFloatValueType, ParameterMeta};
use crate::{ParameterIndex, PywrError};

use std::collections::HashMap;
use std::path::Path;

/// A parameter that delays a value from the model by a number of time-steps.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct DelayParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub metric: DynamicFloatValue,
    pub delay: usize,
    pub initial_value: f64,
}

impl DelayParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }

    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        let mut attributes = HashMap::new();

        let metric = &self.metric;
        attributes.insert("metric", metric.into());

        attributes
    }

    pub fn add_to_model(
        &self,
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<ParameterIndex, PywrError> {
        let metric = self.metric.load(model, tables, data_path)?;
        let p = crate::parameters::DelayParameter::new(&self.meta.name, metric, self.delay, self.initial_value);
        model.add_parameter(Box::new(p))
    }
}
