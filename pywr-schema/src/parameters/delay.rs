#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::meta::NamedMeta;
use crate::metric::{IndexMetric, Metric};
#[cfg(feature = "core")]
use crate::network::LoadArgs;
#[cfg(feature = "core")]
use pywr_core::parameters::ParameterName;
use pywr_schema_macros::PywrVisitAll;
use schemars::JsonSchema;
use std::num::NonZeroU64;

/// The default number of time-steps to delay by.
pub const DEFAULT_DELAY: NonZeroU64 = NonZeroU64::new(1).unwrap();

/// A parameter that delays a value from the network by a number of time-steps.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct DelayParameter {
    pub meta: NamedMeta,
    pub metric: Metric,
    pub delay: NonZeroU64,
    pub initial_value: f64,
}

#[cfg(feature = "core")]
impl DelayParameter {
    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<(), SchemaError> {
        let metric = self.metric.load(network, args, parent)?;
        let p = pywr_core::parameters::DelayParameterBuilder::new(
            ParameterName::new(&self.meta.name, parent),
            metric,
            self.delay,
            self.initial_value,
        );

        network.parameters().f64(Box::new(p));

        Ok(())
    }
}

/// A parameter that delays a value from the network by a number of time-steps.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct DelayIndexParameter {
    pub meta: NamedMeta,
    pub metric: IndexMetric,
    pub delay: NonZeroU64,
    pub initial_value: u64,
}

#[cfg(feature = "core")]
impl DelayIndexParameter {
    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<(), SchemaError> {
        let metric = self.metric.load(network, args, parent)?;
        let p = pywr_core::parameters::DelayParameterBuilder::new(
            ParameterName::new(&self.meta.name, parent),
            metric,
            self.delay,
            self.initial_value,
        );

        network.parameters().u64(Box::new(p));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::nodes::DelayNode;
    use crate::parameters::DelayParameter;

    /// A delay of zero would leave the parameter's queue empty, so it must fail to load. The
    /// delay is a plain field on the parameter and a `ConstantValue` on the node, so both
    /// shapes are checked.
    #[test]
    fn zero_delay_is_refused() {
        let parameter = r#"
            {
                "meta": {
                    "name": "my-delay-param"
                },
                "metric": {
                    "type": "Parameter",
                    "name": "a-parameter"
                },
                "delay": DELAY,
                "initial_value": 0.0
            }
            "#;

        let node = r#"
            {
                "meta": {
                    "name": "my-delay-node"
                },
                "delay": {
                    "type": "Literal",
                    "value": DELAY
                },
                "initial_value": {
                    "type": "Literal",
                    "value": 0.0
                }
            }
            "#;

        serde_json::from_str::<DelayParameter>(&parameter.replace("DELAY", "0"))
            .expect_err("A parameter with a delay of zero should not load.");
        serde_json::from_str::<DelayNode>(&node.replace("DELAY", "0"))
            .expect_err("A node with a delay of zero should not load.");

        serde_json::from_str::<DelayParameter>(&parameter.replace("DELAY", "1"))
            .expect("A parameter with a delay of one should load.");
        serde_json::from_str::<DelayNode>(&node.replace("DELAY", "1"))
            .expect("A node with a delay of one should load.");
    }
}
