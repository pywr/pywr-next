use crate::error::ComponentConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::parameters::{ConstantFloatVec, ConstantValue, ConversionData, ParameterMeta};
use crate::v1::{try_convert_parameter_attr, try_convert_values, IntoV2, TryFromV1};
#[cfg(feature = "core")]
use pywr_core::parameters::{ParameterIndex, ParameterName};
use pywr_schema_macros::{PywrVisitAll, skip_serializing_none};
use pywr_v1_schema::parameters::{
    ConstantParameter as ConstantParameterV1, ConstantScenarioParameter as ConstantScenarioParameterV1, DivisionParameter as DivisionParameterV1, MaxParameter as MaxParameterV1,
    MinParameter as MinParameterV1, NegativeMaxParameter as NegativeMaxParameterV1,
    NegativeMinParameter as NegativeMinParameterV1, NegativeParameter as NegativeParameterV1,
};
use schemars::JsonSchema;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};

/// Activation function or transformation to apply to variable value.
///
/// These different functions are used to specify how a variable value is transformed
/// before being used in a network. These transformations can be useful for optimisation
/// algorithms to represent a, for example, binary-like variable in a continuous domain. Each
/// activation function requires different data to parameterize the function's behaviour.
///
#[derive(
    serde::Deserialize, serde::Serialize, Debug, Clone, Copy, JsonSchema, PywrVisitAll, Display, EnumDiscriminants,
)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(ActivationFunctionType))]
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

#[cfg(feature = "core")]
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
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
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
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct ConstantParameter {
    /// Meta-data.
    pub meta: ParameterMeta,
    /// The value the parameter should return.
    ///
    /// In the simple case this will be the value used by the network. However, if an activation
    /// function is specified this value will be the `x` value for that activation function.
    pub value: ConstantValue<f64>,
    /// Optional settings for configuring how the value of this parameter can be varied. This
    /// is used by, for example, external algorithms to optimise the value of the parameter.
    pub variable: Option<VariableSettings>,
}

#[cfg(feature = "core")]
impl ConstantParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let name = ParameterName::new(&self.meta.name, parent);
        let p = pywr_core::parameters::ConstantParameter::new(name, self.value.load(args.tables)?);
        Ok(network.add_const_parameter(Box::new(p))?)
    }
}

impl TryFromV1<ConstantParameterV1> for ConstantParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: ConstantParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let value = if let Some(v) = v1.value {
            v.into()
        } else if let Some(tbl) = v1.table {
            ConstantValue::Table(tbl.try_into().map_err(|error| ComponentConversionError::Parameter {
                name: meta.name.clone(),
                attr: "table".to_string(),
                error,
            })?)
        } else {
            0.0.into()
        };

        let p = Self {
            meta,
            value,
            variable: None, // TODO convert variable settings
        };
        Ok(p)
    }
}


/// A constant scenario parameter.
///
/// A parameter that provides a constant value for each scenario in a scenario group.
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct ConstantScenarioParameter {

    pub meta: ParameterMeta,
    /// The values the parameter should return.
    ///
    /// The length of this array must match the number of scenarios in the scenario group.
    pub values: ConstantFloatVec,
    /// The name of the scenario group
    pub scenario_group: String,
}

#[cfg(feature = "core")]
impl ConstantScenarioParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let name = ParameterName::new(&self.meta.name, parent);
        let scenario_group_index = args.domain.scenarios().group_index(&self.scenario_group)?;
        let values = self.values.load(args.tables)?;

        let scenario_group_size = args.domain.scenarios().group_size(&self.scenario_group)?;
        if values.len() != scenario_group_size {
            return Err(SchemaError::ScenarioValuesLengthMismatch {
                name: name.to_string(),
                values: values.len(),
                scenarios: scenario_group_size,
                group: self.scenario_group.clone(),
            });
        }

        let p = pywr_core::parameters::ConstantScenarioParameter::new(name, values, scenario_group_index);
        Ok(network.add_const_parameter(Box::new(p))?)
    }
}

impl TryFromV1<ConstantScenarioParameterV1> for ConstantScenarioParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: ConstantScenarioParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let values = try_convert_values(&meta.name, v1.values, v1.external, v1.table)?;

        let p = Self {
            meta,
            values,
            scenario_group: v1.scenario,
        };
        Ok(p)
    }
}

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct MaxParameter {
    pub meta: ParameterMeta,
    pub parameter: Metric,
    pub threshold: Option<f64>,
}

#[cfg(feature = "core")]
impl MaxParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let idx = self.parameter.load(network, args, None)?;
        let threshold = self.threshold.unwrap_or(0.0);

        let p = pywr_core::parameters::MaxParameter::new(ParameterName::new(&self.meta.name, parent), idx, threshold);
        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1<MaxParameterV1> for MaxParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: MaxParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let parameter =
            try_convert_parameter_attr(&meta.name, "parameter", v1.parameter, parent_node, conversion_data)?;

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
#[doc = include_str!("doc_examples/division.json")]
/// ```
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct DivisionParameter {
    pub meta: ParameterMeta,
    pub numerator: Metric,
    pub denominator: Metric,
}

#[cfg(feature = "core")]
impl DivisionParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let n = self.numerator.load(network, args, None)?;
        let d = self.denominator.load(network, args, None)?;

        let p = pywr_core::parameters::DivisionParameter::new(ParameterName::new(&self.meta.name, parent), n, d);
        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1<DivisionParameterV1> for DivisionParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: DivisionParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let numerator =
            try_convert_parameter_attr(&meta.name, "numerator", v1.numerator, parent_node, conversion_data)?;
        let denominator =
            try_convert_parameter_attr(&meta.name, "denominator", v1.denominator, parent_node, conversion_data)?;

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
#[doc = include_str!("doc_examples/min.json")]
/// ```
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct MinParameter {
    pub meta: ParameterMeta,
    pub parameter: Metric,
    pub threshold: Option<f64>,
}

#[cfg(feature = "core")]
impl MinParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let idx = self.parameter.load(network, args, None)?;
        let threshold = self.threshold.unwrap_or(0.0);

        let p = pywr_core::parameters::MinParameter::new(ParameterName::new(&self.meta.name, parent), idx, threshold);
        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1<MinParameterV1> for MinParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: MinParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let parameter =
            try_convert_parameter_attr(&meta.name, "parameter", v1.parameter, parent_node, conversion_data)?;

        let p = Self {
            meta,
            parameter,
            threshold: v1.threshold,
        };
        Ok(p)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct NegativeParameter {
    pub meta: ParameterMeta,
    pub parameter: Metric,
}

#[cfg(feature = "core")]
impl NegativeParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let idx = self.parameter.load(network, args, None)?;

        let p = pywr_core::parameters::NegativeParameter::new(ParameterName::new(&self.meta.name, parent), idx);
        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1<NegativeParameterV1> for NegativeParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: NegativeParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let parameter =
            try_convert_parameter_attr(&meta.name, "parameter", v1.parameter, parent_node, conversion_data)?;

        let p = Self { meta, parameter };
        Ok(p)
    }
}

/// This parameter takes the maximum of the negative of a metric and a constant value (threshold).
///
/// # Arguments
///
/// * `metric` - The metric value to compare with the float.
/// * `threshold` - The threshold value to compare against the given parameter. Default to 0.0.
///
/// # Examples
///
/// ```json
#[doc = include_str!("doc_examples/negative_max.json")]
/// ```
/// In January this parameter returns 2, in February 4.
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct NegativeMaxParameter {
    pub meta: ParameterMeta,
    pub metric: Metric,
    pub threshold: Option<f64>,
}

#[cfg(feature = "core")]
impl NegativeMaxParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let idx = self.metric.load(network, args, None)?;
        let threshold = self.threshold.unwrap_or(0.0);

        let p = pywr_core::parameters::NegativeMaxParameter::new(
            ParameterName::new(&self.meta.name, parent),
            idx,
            threshold,
        );
        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1<NegativeMaxParameterV1> for NegativeMaxParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: NegativeMaxParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let parameter =
            try_convert_parameter_attr(&meta.name, "parameter", v1.parameter, parent_node, conversion_data)?;

        let p = Self {
            meta,
            metric: parameter,
            threshold: v1.threshold,
        };
        Ok(p)
    }
}

/// This parameter takes the minimum of the negative of a metric and a constant value (threshold).
///
/// # Arguments
///
/// * `metric` - The metric value to compare with the float.
/// * `threshold` - The threshold value to compare against the given parameter. Default to 0.0.
///
/// # Examples
///
/// ```json
#[doc = include_str!("doc_examples/negative_min.json")]
/// ```
/// In January this parameter returns 1, in February 2.
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct NegativeMinParameter {
    pub meta: ParameterMeta,
    pub metric: Metric,
    pub threshold: Option<f64>,
}

#[cfg(feature = "core")]
impl NegativeMinParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let idx = self.metric.load(network, args, None)?;
        let threshold = self.threshold.unwrap_or(0.0);

        let p = pywr_core::parameters::NegativeMinParameter::new(
            ParameterName::new(&self.meta.name, parent),
            idx,
            threshold,
        );
        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1<NegativeMinParameterV1> for NegativeMinParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: NegativeMinParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);
        let parameter =
            try_convert_parameter_attr(&meta.name, "parameter", v1.parameter, parent_node, conversion_data)?;

        let p = Self {
            meta,
            metric: parameter,
            threshold: v1.threshold,
        };
        Ok(p)
    }
}
