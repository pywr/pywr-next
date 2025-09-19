#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::parameters::{ConstantValue, ParameterMeta, VariableSettings};

#[cfg(feature = "core")]
use pywr_core::parameters::{ParameterIndex, ParameterName};
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
/// An example specifying the parameter as a variable and defining the activation function:
/// ```json
#[doc = include_str!("doc_examples/offset_variable.json")]
/// ```
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct OffsetParameter {
    /// Meta-data.
    pub meta: ParameterMeta,
    /// The offset value applied to the metric.
    ///
    /// In the simple case this will be the value used by the network. However, if an activation
    /// function is specified this value will be the `x` value for that activation function.
    pub offset: ConstantValue<f64>,
    /// The metric from which to apply the offset.
    pub metric: Metric,
    /// Optional settings for configuring how the value of this parameter can be varied. This
    /// is used by, for example, external algorithms to optimise the value of the parameter.
    pub variable: Option<VariableSettings>,
}

#[cfg(feature = "core")]
impl OffsetParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let idx = self.metric.load(network, args, None)?;

        let p = pywr_core::parameters::OffsetParameter::new(
            ParameterName::new(&self.meta.name, parent),
            idx,
            self.offset.load(args.tables)?,
        );
        Ok(network.add_parameter(Box::new(p))?)
    }
}
