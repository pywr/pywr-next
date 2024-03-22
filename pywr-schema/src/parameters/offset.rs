use crate::error::SchemaError;
use crate::model::LoadArgs;
use crate::parameters::{ConstantValue, DynamicFloatValue, DynamicFloatValueType, ParameterMeta};
use pywr_core::parameters::ParameterIndex;
use std::collections::HashMap;

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
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct OffsetParameter {
    /// Meta-data.
    ///
    /// This field is flattened in the serialised format.
    #[serde(flatten)]
    pub meta: ParameterMeta,
    /// The offset value applied to the metric.
    ///
    /// In the simple case this will be the value used by the network. However, if an activation
    /// function is specified this value will be the `x` value for that activation function.
    pub offset: ConstantValue<f64>,
    /// The metric from which to apply the offset.
    pub metric: DynamicFloatValue,
}

impl OffsetParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }

    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        HashMap::new()
    }

    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let idx = self.metric.load(network, args)?;

        let p = pywr_core::parameters::OffsetParameter::new(&self.meta.name, idx, self.offset.load(args.tables)?);
        Ok(network.add_parameter(Box::new(p))?)
    }
}
