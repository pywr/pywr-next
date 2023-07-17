use crate::schema::data_tables::LoadedTableCollection;
use crate::schema::error::ConversionError;
use crate::schema::parameters::{
    ConstantValue, DynamicFloatValue, DynamicFloatValueType, IntoV2Parameter, ParameterMeta, TryFromV1Parameter,
    TryIntoV2Parameter,
};
use crate::{ParameterIndex, PywrError};
use pywr_schema::parameters::{
    ConstantParameter as ConstantParameterV1, MaxParameter as MaxParameterV1, NegativeParameter as NegativeParameterV1,
};
use std::collections::HashMap;
use std::path::Path;

/// Activation function or transformation to apply to variable value.
///
/// These different functions are used to specify how a variable value is transformed
/// before being used in a model. These transformations can be useful for optimisation
/// algorithms to represent a, for example, binary-like variable in a continuous domain. Each
/// activation function requires different data to parameterize the function's behaviour.
///
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy)]
#[serde(tag = "type")]
pub enum ActivationFunction {
    /// A unit or null transformation.
    ///
    /// ```rust
    /// # use pywr::schema::parameters::ActivationFunction;
    /// let data = r#"
    ///     {
    ///         "type": "Unit",
    ///         "min": 0.0,
    ///         "max": 10.0
    ///     }"#;
    /// let a: ActivationFunction = serde_json::from_str(data)?;
    /// # Ok::<(), serde_json::Error>(())
    /// ```
    Unit { min: f64, max: f64 },
    /// A linear rectifier function, or ramp function.
    ///
    /// ```rust
    /// # use pywr::schema::parameters::ActivationFunction;
    /// let data = r#"
    ///     {
    ///         "type": "Rectifier",
    ///         "min": 0.0,
    ///         "max": 10.0
    ///     }"#;
    /// let a: ActivationFunction = serde_json::from_str(data)?;
    /// # Ok::<(), serde_json::Error>(())
    /// ```
    Rectifier {
        /// Minimum output of the function (i.e. when x is 0.0)
        min: f64,
        /// Maximum output of the function (i.e. when x is 1.0).
        max: f64,
        /// Value to return in the negative part of the function (defaults to zero).
        off_value: Option<f64>,
    },
    /// A binary-step function.
    ///
    /// ```rust
    /// # use pywr::schema::parameters::ActivationFunction;
    /// let data = r#"
    ///     {
    ///         "type": "BinaryStep",
    ///         "min_output": 0.0,
    ///         "max_output": 10.0
    ///     }"#;
    /// let a: ActivationFunction = serde_json::from_str(data)?;
    /// # Ok::<(), serde_json::Error>(())
    /// ```
    BinaryStep {
        /// Value to return in the positive part of the function.
        on_value: f64,
        /// Value to return in the negative part of the function (defaults to zero).
        off_value: Option<f64>,
    },
    /// A logistic, or S, function.
    ///
    /// ```rust
    /// # use pywr::schema::parameters::ActivationFunction;
    /// let data = r#"
    ///     {
    ///         "type": "Logistic",
    ///         "growth_rate": 1.0,
    ///         "max_output": 10.0
    ///     }"#;
    /// let a: ActivationFunction = serde_json::from_str(data)?;
    /// # Ok::<(), serde_json::Error>(())
    /// ```
    Logistic { growth_rate: f64, max: f64 },
}

impl Into<crate::parameters::ActivationFunction> for ActivationFunction {
    fn into(self) -> crate::parameters::ActivationFunction {
        match self {
            Self::Unit { min, max } => crate::parameters::ActivationFunction::Unit { min, max },
            Self::Rectifier { min, max, off_value } => crate::parameters::ActivationFunction::Rectifier {
                min,
                max,
                neg_value: off_value.unwrap_or(0.0),
            },
            Self::BinaryStep { on_value, off_value } => crate::parameters::ActivationFunction::BinaryStep {
                pos_value: on_value,
                neg_value: off_value.unwrap_or(0.0),
            },
            Self::Logistic { growth_rate, max } => crate::parameters::ActivationFunction::Logistic { growth_rate, max },
        }
    }
}

/// Settings for a variable constant.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct ConstantVariableSettings {
    /// Is this parameter an active variable?
    pub is_active: bool,
    /// The activation function to use for the variable.
    pub activation: ActivationFunction,
}

/// A constant parameter.
///
/// This is the most basic type of parameter which represents a single constant value.
///
/// # JSON Examples
///
/// A simple example:
/// ```json
#[doc = include_str!("doc_examples/constant_simple.json")]
/// ```
///
/// An example specifying the parameter as a variable and defining the activation function:
/// ```json
#[doc = include_str!("doc_examples/constant_variable.json")]
/// ```
///
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct ConstantParameter {
    /// Meta-data.
    ///
    /// This field is flattened in the serialised format.
    #[serde(flatten)]
    pub meta: ParameterMeta,
    /// The value the parameter should return.
    ///
    /// In the simple case this will be the value used by the model. However, if an activation
    /// function is specified this value will be the `x` value for that activation function.
    pub value: ConstantValue<f64>,
    /// Definition of optional variable settings.
    pub variable: Option<ConstantVariableSettings>,
}

impl ConstantParameter {
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

        let p = crate::parameters::ConstantParameter::new(&self.meta.name, self.value.load(tables)?, variable);
        model.add_parameter(Box::new(p))
    }
}

impl TryFromV1Parameter<ConstantParameterV1> for ConstantParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: ConstantParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let value = if let Some(v) = v1.value {
            ConstantValue::Literal(v)
        } else if let Some(tbl) = v1.table {
            ConstantValue::Table(tbl.into())
        } else {
            ConstantValue::Literal(0.0)
        };

        let p = Self {
            meta: v1.meta.into_v2_parameter(parent_node, unnamed_count),
            value,
            variable: None, // TODO implement conversion of v1 variable definition
        };
        Ok(p)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct MaxParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub parameter: DynamicFloatValue,
    pub threshold: Option<f64>,
}

impl MaxParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }
    // pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
    //     let mut attributes = HashMap::new();
    //     attributes.insert("parameter", self.parameter.as_ref().into());
    //     attributes
    // }

    pub fn add_to_model(
        &self,
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<ParameterIndex, PywrError> {
        let idx = self.parameter.load(model, tables, data_path)?;
        let threshold = self.threshold.unwrap_or(0.0);

        let p = crate::parameters::MaxParameter::new(&self.meta.name, idx, threshold);
        model.add_parameter(Box::new(p))
    }
}

impl TryFromV1Parameter<MaxParameterV1> for MaxParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: MaxParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);

        let parameter = v1.parameter.try_into_v2_parameter(Some(&meta.name), unnamed_count)?;

        let p = Self {
            meta,
            parameter,
            threshold: v1.threshold,
        };
        Ok(p)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct NegativeParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub parameter: DynamicFloatValue,
}

impl NegativeParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }
    // pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
    //     let mut attributes = HashMap::new();
    //     attributes.insert("parameter", self.parameter.as_ref().into());
    //     attributes
    // }

    pub fn add_to_model(
        &self,
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<ParameterIndex, PywrError> {
        let idx = self.parameter.load(model, tables, data_path)?;

        let p = crate::parameters::NegativeParameter::new(&self.meta.name, idx);
        model.add_parameter(Box::new(p))
    }
}

impl TryFromV1Parameter<NegativeParameterV1> for NegativeParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: NegativeParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);

        let parameter = v1.parameter.try_into_v2_parameter(Some(&meta.name), unnamed_count)?;

        let p = Self { meta, parameter };
        Ok(p)
    }
}
