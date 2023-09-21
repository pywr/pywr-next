use crate::schema::data_tables::LoadedTableCollection;
use crate::schema::parameters::{
    ConstantValue, DynamicFloatValue, DynamicFloatValueType, ParameterMeta, VariableSettings,
};
use crate::{ParameterIndex, PywrError};

use std::collections::HashMap;
use std::path::Path;

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
    /// In the simple case this will be the value used by the model. However, if an activation
    /// function is specified this value will be the `x` value for that activation function.
    pub offset: ConstantValue<f64>,
    /// The metric from which to apply the offset.
    pub metric: DynamicFloatValue,
    /// Definition of optional variable settings.
    pub variable: Option<VariableSettings>,
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
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<ParameterIndex, PywrError> {
        let variable = match &self.variable {
            None => None,
            Some(v) => {
                // Only set the variable data if the user has indicated the variable is active.
                if v.is_active {
                    Some(v.activation.into())
                } else {
                    None
                }
            }
        };

        let idx = self.metric.load(model, tables, data_path)?;

        let p = crate::parameters::OffsetParameter::new(&self.meta.name, idx, self.offset.load(tables)?, variable);
        model.add_parameter(Box::new(p))
    }
}
