#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::parameters::{DynamicFloatValueType, ParameterMeta};
#[cfg(feature = "core")]
use pywr_core::parameters::ParameterIndex;
use std::collections::HashMap;

/// A parameter that delays a value from the network by a number of time-steps.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct DelayParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub metric: Metric,
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
}

#[cfg(feature = "core")]
impl DelayParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let metric = self.metric.load(network, args)?;
        let p = pywr_core::parameters::DelayParameter::new(&self.meta.name, metric, self.delay, self.initial_value);
        Ok(network.add_parameter(Box::new(p))?)
    }
}
