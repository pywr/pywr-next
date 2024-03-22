use crate::error::{ConversionError, SchemaError};
use crate::model::LoadArgs;
use crate::parameters::{
    ConstantValue, DynamicFloatValue, DynamicFloatValueType, IntoV2Parameter, ParameterMeta, TryFromV1Parameter,
    TryIntoV2Parameter,
};
use pywr_core::parameters::ParameterIndex;
use pywr_v1_schema::parameters::{
    ConstantParameter as ConstantParameterV1, DivisionParameter as DivisionParameterV1, MaxParameter as MaxParameterV1,
    MinParameter as MinParameterV1, NegativeParameter as NegativeParameterV1,
};
use std::collections::HashMap;

/// Activation function or transformation to apply to variable value.
///
/// These different functions are used to specify how a variable value is transformed
/// before being used in a network. These transformations can be useful for optimisation
/// algorithms to represent a, for example, binary-like variable in a continuous domain. Each
/// activation function requires different data to parameterize the function's behaviour.
///
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy)]
#[serde(tag = "type")]
pub enum ActivationFunction {
    /// A unit or null transformation.
    ///
    /// ```rust
    /// # use pywr_schema::parameters::ActivationFunction;
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
    /// # use pywr_schema::parameters::ActivationFunction;
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
    /// # use pywr_schema::parameters::ActivationFunction;
    /// let data = r#"
    ///     {
    ///         "type": "BinaryStep",
    ///         "on_value": 0.0,
    ///         "off_value": 10.0
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
    /// # use pywr_schema::parameters::ActivationFunction;
    /// let data = r#"
    ///     {
    ///         "type": "Logistic",
    ///         "growth_rate": 1.0,
    ///         "max": 10.0
    ///     }"#;
    /// let a: ActivationFunction = serde_json::from_str(data)?;
    /// # Ok::<(), serde_json::Error>(())
    /// ```
    Logistic { growth_rate: f64, max: f64 },
}

impl From<ActivationFunction> for pywr_core::parameters::ActivationFunction {
    fn from(a: ActivationFunction) -> Self {
        match a {
            ActivationFunction::Unit { min, max } => pywr_core::parameters::ActivationFunction::Unit { min, max },
            ActivationFunction::Rectifier { min, max, off_value } => {
                pywr_core::parameters::ActivationFunction::Rectifier {
                    min,
                    max,
                    neg_value: off_value.unwrap_or(0.0),
                }
            }
            ActivationFunction::BinaryStep { on_value, off_value } => {
                pywr_core::parameters::ActivationFunction::BinaryStep {
                    pos_value: on_value,
                    neg_value: off_value.unwrap_or(0.0),
                }
            }
            ActivationFunction::Logistic { growth_rate, max } => {
                pywr_core::parameters::ActivationFunction::Logistic { growth_rate, max }
            }
        }
    }
}

/// Settings for a variable value.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct VariableSettings {
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
    /// In the simple case this will be the value used by the network. However, if an activation
    /// function is specified this value will be the `x` value for that activation function.
    pub value: ConstantValue<f64>,
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
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let p = pywr_core::parameters::ConstantParameter::new(&self.meta.name, self.value.load(args.tables)?);
        Ok(network.add_parameter(Box::new(p))?)
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
            ConstantValue::Table(tbl.try_into()?)
        } else {
            ConstantValue::Literal(0.0)
        };

        let p = Self {
            meta: v1.meta.into_v2_parameter(parent_node, unnamed_count),
            value,
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
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let idx = self.parameter.load(network, args)?;
        let threshold = self.threshold.unwrap_or(0.0);

        let p = pywr_core::parameters::MaxParameter::new(&self.meta.name, idx, threshold);
        Ok(network.add_parameter(Box::new(p))?)
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

/// This parameter divides one Parameter by another.
///
/// # Arguments
///
/// * `numerator` - The parameter to use as the numerator (or dividend).
/// * `denominator` - The parameter to use as the denominator (or divisor).
///
/// # Examples
///
/// ```json
/// {
///     "type": "Division",
///     "numerator": {
///         "type": "MonthlyProfile",
///         "values": [1, 4, 5, 9, 1, 5, 10, 8, 11, 9, 11 ,12]
///     },
///     "denominator": {
///         "type": "Constant",
///         "value": 0.3
///     }
/// }
/// ```
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct DivisionParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub numerator: DynamicFloatValue,
    pub denominator: DynamicFloatValue,
}

impl DivisionParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }

    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let n = self.numerator.load(network, args)?;
        let d = self.denominator.load(network, args)?;

        let p = pywr_core::parameters::DivisionParameter::new(&self.meta.name, n, d);
        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1Parameter<DivisionParameterV1> for DivisionParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: DivisionParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);

        let numerator = v1.numerator.try_into_v2_parameter(Some(&meta.name), unnamed_count)?;
        let denominator = v1.denominator.try_into_v2_parameter(Some(&meta.name), unnamed_count)?;

        let p = Self {
            meta,
            numerator,
            denominator,
        };
        Ok(p)
    }
}

/// This parameter takes the minimum of another Parameter and a constant value (threshold).
///
/// # Arguments
///
/// * `parameter` - The parameter to compare with the float.
/// * `threshold` - The threshold value to compare with the given parameter.
///
/// # Examples
///
/// ```json
/// {
///     "type": "Min",
///     "parameter": {
///         "type": "MonthlyProfile",
///         "values": [1, 4, 5, 9, 1, 5, 10, 8, 11, 9, 11 ,12]
///     },
///     "threshold": 2
/// }
/// ```
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct MinParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub parameter: DynamicFloatValue,
    pub threshold: Option<f64>,
}

impl MinParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }

    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let idx = self.parameter.load(network, args)?;
        let threshold = self.threshold.unwrap_or(0.0);

        let p = pywr_core::parameters::MinParameter::new(&self.meta.name, idx, threshold);
        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1Parameter<MinParameterV1> for MinParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: MinParameterV1,
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
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let idx = self.parameter.load(network, args)?;

        let p = pywr_core::parameters::NegativeParameter::new(&self.meta.name, idx);
        Ok(network.add_parameter(Box::new(p))?)
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
