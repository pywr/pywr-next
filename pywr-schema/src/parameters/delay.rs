#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::parameters::ParameterMeta;
#[cfg(feature = "core")]
use pywr_core::parameters::ParameterIndex;
use pywr_schema_macros::PywrVisitAll;
use schemars::JsonSchema;

/// A parameter that delays a value from the network by a number of time-steps.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct DelayParameter {
    pub meta: ParameterMeta,
    pub metric: Metric,
    pub delay: usize,
    pub initial_value: f64,
}

#[cfg(feature = "core")]
impl DelayParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let metric = self.metric.load(network, args, None)?;
        let p = pywr_core::parameters::DelayParameter::new(
            self.meta.name.as_str().into(),
            metric,
            self.delay,
            self.initial_value,
        );
        Ok(network.add_parameter(Box::new(p))?)
    }
}
