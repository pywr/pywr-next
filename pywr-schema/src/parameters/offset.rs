#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::meta::NamedMeta;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::parameters::{ConstantValue, ParameterPhase};
#[cfg(feature = "core")]
use pywr_core::parameters::ParameterName;
use pywr_schema_macros::{PywrVisitAll, skip_serializing_none};
use schemars::JsonSchema;

/// A parameter that returns a fixed delta from another metric.
///
/// # JSON Examples
///
/// A simple example that returns 3.14 plus the value of the Parameter "my-other-parameter".
/// ```json
#[doc = include_str!("doc_examples/offset_simple.json")]
/// ```
///
/// The offset can be made variable by an external variable config (see
/// [`crate::ProblemConfig`]) using an activation function.
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct OffsetParameter {
    /// Meta-data.
    pub meta: NamedMeta,
    pub phase: ParameterPhase,
    /// The offset value applied to the metric.
    ///
    /// This is the offset used by the network, unless the parameter is made variable by an
    /// external variable config.
    pub offset: ConstantValue<f64>,
    /// The metric from which to apply the offset.
    pub metric: Metric,
}

#[cfg(feature = "core")]
impl OffsetParameter {
    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<(), SchemaError> {
        let metric = self.metric.load(network, args, parent)?;
        let name = ParameterName::new(&self.meta.name, parent);

        let p = match self.phase {
            ParameterPhase::Before => {
                pywr_core::parameters::OffsetParameterBuilder::before(name, metric, self.offset.load(args.tables)?)
            }
            ParameterPhase::After => {
                pywr_core::parameters::OffsetParameterBuilder::after(name, metric, self.offset.load(args.tables)?)
            }
            ParameterPhase::Both => {
                pywr_core::parameters::OffsetParameterBuilder::both(name, metric, self.offset.load(args.tables)?)
            }
        };

        network.parameters().f64(Box::new(p));

        Ok(())
    }
}
