#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::error::{ComponentConversionError, ConversionError};
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::parameters::{ConstantFloatVec, ConstantValue, ConversionData, ParameterMeta};
use crate::v1::{FromV1, IntoV2, TryFromV1, try_convert_values};
#[cfg(feature = "core")]
use pywr_core::parameters::{ParameterIndex, ParameterName, WeeklyProfileError, WeeklyProfileValues};
use pywr_schema_macros::{PywrVisitAll, skip_serializing_none};
use pywr_v1_schema::parameters::{
    DailyProfileParameter as DailyProfileParameterV1, MonthInterpDay as MonthInterpDayV1,
    MonthlyProfileParameter as MonthlyProfileParameterV1, RbfProfileParameter as RbfProfileParameterV1,
    UniformDrawdownProfileParameter as UniformDrawdownProfileParameterV1,
    WeeklyProfileParameter as WeeklyProfileParameterV1,
};
use schemars::JsonSchema;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};

/// A parameter that defines a daily profile over a year.
///
/// The values array should contain 366 values, one for each day of the year. If the array contains
/// 365 values, then a value for the 29th February (day 59, zero-based) is inserted as a copy of the
/// 28th February (day 58, zero-based). Any other length will result in an error.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct DailyProfileParameter {
    pub meta: ParameterMeta,
    pub values: ConstantFloatVec,
}

#[cfg(feature = "core")]
impl DailyProfileParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let mut values = self.values.load(args.tables)?;

        match values.len() {
            366 => {} // already correct
            365 => {
                // Insert value for 29th Feb as copy of 28th Feb (day 59, zero-based)
                let feb_28 = values[58];
                values.insert(59, feb_28);
            }
            _ => {
                return Err(SchemaError::DataLengthMismatch {
                    expected: 366,
                    found: values.len(),
                });
            }
        }

        let p = pywr_core::parameters::DailyProfileParameter::new(
            ParameterName::new(&self.meta.name, parent),
            values.try_into().expect("Failed to convert values to [f64; 366]"),
        );
        Ok(network.add_simple_parameter(Box::new(p))?)
    }
}

impl TryFromV1<DailyProfileParameterV1> for DailyProfileParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: DailyProfileParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let values = try_convert_values(&meta.name, v1.values, v1.external, v1.table_ref)?;

        let p = Self { meta, values };
        Ok(p)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Copy, Clone, Display, JsonSchema, PywrVisitAll, EnumIter)]
pub enum MonthlyInterpDay {
    First,
    Last,
}

#[cfg(feature = "core")]
impl From<MonthlyInterpDay> for pywr_core::parameters::MonthlyInterpDay {
    fn from(value: MonthlyInterpDay) -> Self {
        match value {
            MonthlyInterpDay::First => Self::First,
            MonthlyInterpDay::Last => Self::Last,
        }
    }
}

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct MonthlyProfileParameter {
    pub meta: ParameterMeta,
    pub values: ConstantFloatVec,
    pub interp_day: Option<MonthlyInterpDay>,
}

#[cfg(feature = "core")]
impl MonthlyProfileParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let values = self.values.load(args.tables)?;

        let values: [f64; 12] = values.try_into().map_err(|v: Vec<_>| SchemaError::DataLengthMismatch {
            expected: 12,
            found: v.len(),
        })?;

        let p = pywr_core::parameters::MonthlyProfileParameter::new(
            ParameterName::new(&self.meta.name, parent),
            values,
            self.interp_day.map(|id| id.into()),
        );
        Ok(network.add_simple_parameter(Box::new(p))?)
    }
}

impl From<MonthInterpDayV1> for MonthlyInterpDay {
    fn from(value: MonthInterpDayV1) -> Self {
        match value {
            MonthInterpDayV1::First => Self::First,
            MonthInterpDayV1::Last => Self::Last,
        }
    }
}

impl TryFromV1<MonthlyProfileParameterV1> for MonthlyProfileParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: MonthlyProfileParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);
        let interp_day = v1.interp_day.map(|id| id.into());

        let values = try_convert_values(&meta.name, v1.values.map(|v| v.to_vec()), v1.external, v1.table_ref)?;

        let p = Self {
            meta,
            values,
            interp_day,
        };
        Ok(p)
    }
}

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct UniformDrawdownProfileParameter {
    pub meta: ParameterMeta,
    pub reset_day: Option<ConstantValue<u64>>,
    pub reset_month: Option<ConstantValue<u64>>,
    pub residual_days: Option<ConstantValue<u64>>,
}

#[cfg(feature = "core")]
impl UniformDrawdownProfileParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let reset_day = match &self.reset_day {
            Some(v) => v.load(args.tables)? as u32,
            None => 1,
        };
        let reset_month = match &self.reset_month {
            Some(v) => v.load(args.tables)? as u32,
            None => 1,
        };
        let residual_days = match &self.residual_days {
            Some(v) => v.load(args.tables)? as u8,
            None => 0,
        };

        let p = pywr_core::parameters::UniformDrawdownProfileParameter::new(
            ParameterName::new(&self.meta.name, parent),
            reset_day,
            reset_month,
            residual_days,
        );
        Ok(network.add_simple_parameter(Box::new(p))?)
    }
}

impl FromV1<UniformDrawdownProfileParameterV1> for UniformDrawdownProfileParameter {
    fn from_v1(
        v1: UniformDrawdownProfileParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Self {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        Self {
            meta,
            reset_day: v1.reset_day.map(|v| v.into()),
            reset_month: v1.reset_month.map(|v| v.into()),
            residual_days: v1.residual_days.map(|v| v.into()),
        }
    }
}

/// Distance functions for radial basis function interpolation.
#[derive(
    serde::Deserialize, serde::Serialize, Debug, Copy, Clone, JsonSchema, PywrVisitAll, Display, EnumDiscriminants,
)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(RadialBasisFunctionType))]
pub enum RadialBasisFunction {
    Linear,
    Cubic,
    Quintic,
    ThinPlateSpline,
    Gaussian { epsilon: Option<f64> },
    MultiQuadric { epsilon: Option<f64> },
    InverseMultiQuadric { epsilon: Option<f64> },
}

#[cfg(feature = "core")]
impl RadialBasisFunction {
    /// Convert the schema representation of the RBF into `pywr_core` type.
    ///
    /// If required this will estimate values of from the provided points.
    fn into_core_rbf(self, points: &[(u32, f64)]) -> Result<pywr_core::parameters::RadialBasisFunction, SchemaError> {
        let rbf = match self {
            Self::Linear => pywr_core::parameters::RadialBasisFunction::Linear,
            Self::Cubic => pywr_core::parameters::RadialBasisFunction::Cubic,
            Self::Quintic => pywr_core::parameters::RadialBasisFunction::Quintic,
            Self::ThinPlateSpline => pywr_core::parameters::RadialBasisFunction::ThinPlateSpline,
            Self::Gaussian { epsilon } => {
                let epsilon = match epsilon {
                    Some(e) => e,
                    None => estimate_epsilon(points).ok_or(SchemaError::RbfEpsilonEstimation)?,
                };

                pywr_core::parameters::RadialBasisFunction::Gaussian { epsilon }
            }
            Self::MultiQuadric { epsilon } => {
                let epsilon = match epsilon {
                    Some(e) => e,
                    None => estimate_epsilon(points).ok_or(SchemaError::RbfEpsilonEstimation)?,
                };

                pywr_core::parameters::RadialBasisFunction::MultiQuadric { epsilon }
            }
            Self::InverseMultiQuadric { epsilon } => {
                let epsilon = match epsilon {
                    Some(e) => e,
                    None => estimate_epsilon(points).ok_or(SchemaError::RbfEpsilonEstimation)?,
                };

                pywr_core::parameters::RadialBasisFunction::InverseMultiQuadric { epsilon }
            }
        };

        Ok(rbf)
    }
}

/// Compute an estimate for epsilon.
///
/// If there `points` is empty then `None` is returned.
#[cfg(feature = "core")]
fn estimate_epsilon(points: &[(u32, f64)]) -> Option<f64> {
    if points.is_empty() {
        return None;
    }

    // SAFETY: Above check that points is non-empty should make these unwraps safe.
    let x_min = points.iter().map(|(x, _)| *x).min().unwrap();
    let x_max = points.iter().map(|(x, _)| *x).max().unwrap();
    let y_min = points.iter().map(|(_, y)| *y).reduce(f64::min).unwrap();
    let y_max = points.iter().map(|(_, y)| *y).reduce(f64::max).unwrap();

    let mut x_range = x_max - x_min;
    if x_range == 0 {
        x_range = 1;
    }
    let mut y_range = y_max - y_min;
    if y_range == 0.0 {
        y_range = 1.0;
    }

    Some((x_range as f64 * y_range).powf(1.0 / points.len() as f64))
}

/// Settings for a variable RBF profile.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct RbfProfileVariableSettings {
    /// Is this parameter an active variable?
    pub is_active: bool,
    /// Optional maximum number of days that the interpolation points can be moved from their
    ///  original position. If this is `None` then the points can not be moved from their
    ///  original day of the year.
    pub days_of_year_range: Option<u32>,
    /// Optional upper bound for the value of each interpolation point. If this is `None` then
    ///  there is no upper bound.
    pub value_upper_bounds: Option<f64>,
    /// Optional lower bound for the value of each interpolation point. If this is `None` then
    ///  the lower bound is zero.
    pub value_lower_bounds: Option<f64>,
}

#[cfg(feature = "core")]
impl From<RbfProfileVariableSettings> for pywr_core::parameters::RbfProfileVariableConfig {
    fn from(settings: RbfProfileVariableSettings) -> Self {
        Self::new(
            settings.days_of_year_range,
            settings.value_upper_bounds.unwrap_or(f64::INFINITY),
            settings.value_lower_bounds.unwrap_or(0.0),
        )
    }
}

/// A parameter that interpolates between a set of points using a radial basis function to
/// create a daily profile.
///
/// # JSON Examples
///
/// The example below shows the definition of a [`RbfProfileParameter`] in JSON.
///
/// ```json
#[doc = include_str!("doc_examples/rbf_1.json")]
/// ```
///
///  The example below shows the definition of a [`RbfProfileParameter`] in JSON with variable
///  settings defined. This settings determine how the interpolation points be modified by
///  external algorithms. See [`RbfProfileVariableSettings`] for more information.
///
/// ```json
#[doc = include_str!("doc_examples/rbf_2.json")]
/// ```
///
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct RbfProfileParameter {
    pub meta: ParameterMeta,
    /// The points are the profile positions defined by an ordinal day of the year and a value.
    /// Radial basis function interpolation is used to create a daily profile from these points.
    pub points: Vec<(u32, f64)>,
    /// The distance function used for interpolation.
    pub function: RadialBasisFunction,
    /// Optional settings for configuring how the value of this parameter can be varied. This
    /// is used by, for example, external algorithms to optimise the value of the parameter.
    pub variable: Option<RbfProfileVariableSettings>,
}

#[cfg(feature = "core")]
impl RbfProfileParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let function = self.function.into_core_rbf(&self.points)?;

        let p = pywr_core::parameters::RbfProfileParameter::new(
            ParameterName::new(&self.meta.name, parent),
            self.points.clone(),
            function,
        );
        Ok(network.add_simple_parameter(Box::new(p))?)
    }
}

impl TryFromV1<RbfProfileParameterV1> for RbfProfileParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: RbfProfileParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let points = v1.days_of_year.into_iter().zip(v1.values).collect();

        if v1.rbf_kwargs.contains_key("smooth") {
            return Err(ComponentConversionError::Parameter {
                name: meta.name,
                attr: "smooth".to_string(),
                error: ConversionError::UnsupportedFeature {
                    feature: "The RBF `smooth` keyword argument is not supported.".to_string(),
                },
            });
        }

        if v1.rbf_kwargs.contains_key("norm") {
            return Err(ComponentConversionError::Parameter {
                name: meta.name,
                attr: "norm".to_string(),
                error: ConversionError::UnsupportedFeature {
                    feature: "The RBF `norm` keyword argument is not supported.".to_string(),
                },
            });
        }

        // Parse any epsilon value; we expect a float here.
        let epsilon = if let Some(epsilon_value) = v1.rbf_kwargs.get("epsilon") {
            if let Some(epsilon_f64) = epsilon_value.as_f64() {
                Some(epsilon_f64)
            } else {
                return Err(ComponentConversionError::Parameter {
                    name: meta.name,
                    attr: "epsilon".to_string(),
                    error: ConversionError::UnexpectedType {
                        expected: "float".to_string(),
                        actual: format!("{epsilon_value}"),
                    },
                });
            }
        } else {
            None
        };

        let function = if let Some(function_value) = v1.rbf_kwargs.get("function") {
            if let Some(function_str) = function_value.as_str() {
                // Function kwarg is a string!
                match function_str {
                    "multiquadric" => RadialBasisFunction::MultiQuadric { epsilon },
                    "inverse" => RadialBasisFunction::InverseMultiQuadric { epsilon },
                    "gaussian" => RadialBasisFunction::Gaussian { epsilon },
                    "linear" => RadialBasisFunction::Linear,
                    "cubic" => RadialBasisFunction::Cubic,
                    "thin_plate" => RadialBasisFunction::ThinPlateSpline,
                    _ => {
                        return Err(ComponentConversionError::Parameter {
                            name: meta.name,
                            attr: "rbf_kwargs".to_string(),
                            error: ConversionError::UnsupportedFeature {
                                feature: format!("Radial basis function `{function_str}` not supported."),
                            },
                        });
                    }
                }
            } else {
                return Err(ComponentConversionError::Parameter {
                    name: meta.name,
                    attr: "rbf_kwargs".to_string(),
                    error: ConversionError::UnexpectedType {
                        expected: "string".to_string(),
                        actual: format!("{function_value}"),
                    },
                });
            }
        } else {
            // Default to multi-quadratic
            RadialBasisFunction::MultiQuadric { epsilon }
        };

        let p = Self {
            meta,
            points,
            function,
            variable: None, // TODO convert variable settings
        };

        Ok(p)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Copy, Clone, JsonSchema, PywrVisitAll, Display, EnumIter)]
pub enum WeeklyInterpDay {
    First,
    Last,
}

#[cfg(feature = "core")]
impl From<WeeklyInterpDay> for pywr_core::parameters::WeeklyInterpDay {
    fn from(value: WeeklyInterpDay) -> Self {
        match value {
            WeeklyInterpDay::First => Self::First,
            WeeklyInterpDay::Last => Self::Last,
        }
    }
}

/// A parameter to handle a weekly profile of 52 or 53 weeks.
///
/// # Arguments
///
/// * `values` - The weekly values; this can be an array of 52 or 53 values. With 52 items,
///   the value for the 53<sup>rd</sup> week (day 364 - 29<sup>th</sup> Dec or 30<sup>th</sup>
///   Dec for a leap year) is copied from week 52<sup>nd</sup>.
/// * `interp_day` - This is an optional field to control the parameter interpolation. When this
///   is not provided, the profile is piecewise. When this equals "First" or "Last", the values
///   are linearly interpolated in each week and the string specifies whether the given values are
///   the first or last day of the week. See the examples below for more information.
///
/// ## Interpolation notes
/// When the profile is interpolated, the following assumptions are made for a 52-week profile due to the missing
/// values on the 53<sup>rd</sup> week:
///  - when `interp_day` is First, the upper boundary in the 52<sup>nd</sup> and 53<sup>rd</sup> week is the
///    same (i.e. the value on 1<sup>st</sup> January)
///  - when `interp_day` is Last the 1<sup>st</sup> and last week will share the same lower bound (i.e. the
///    value on the last week).
///
/// This does apply to a 53-week profile.
///
/// # Examples
/// ## Without interpolation
/// This defines a piece-wise weekly profile. Each day of the same week has the same value:
/// ```json
/// {
///     "type": "WeeklyProfile",
///     "values": [0.4, 4, ... , 12]
/// }
/// ```
/// In the example above, the parameter returns `0.4` from 1<sup>st</sup> to 6<sup>th</sup> January
/// for week 1, `4` for the week 2 (7<sup>st</sup> to 13<sup>th</sup>) and so on.
///
/// ## Interpolation
/// ### interp_day = "First"
/// ```json
/// {
///     "type": "WeeklyProfile",
///     "values": [0.4, 4, 9, ... , 10, 12],
///     "interp_day": "First"
/// }
/// ```
/// This defines an interpolated profile where the values in the 1<sup>st</sup> week are derived by
/// linearly interpolating between `0.4` and `4`, in the 2<sup>nd</sup> week between `4` and `9`.
/// The values in the last week are interpolated between `12` and `0.4` (i.e the value on 1<sup>st</sup>
/// January).
///
/// ### interp_day = "Last"
/// ```json
/// {
///     "type": "WeeklyProfile",
///     "values": [0.4, 4, 9, ... , 10, 12],
///     "interp_day": "Last"
/// }
/// ```
/// This defines an interpolated profile where the values in the 1<sup>st</sup> week are derived by
/// linearly interpolating between `12` and `0.4`, in the 2<sup>nd</sup> week between `0.4` and `4`.
/// The values in the last week are interpolated between `10` and `12` (i.e the value on 31<sup>st</sup>
/// December).
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct WeeklyProfileParameter {
    pub meta: ParameterMeta,
    pub values: ConstantFloatVec,
    pub interp_day: Option<WeeklyInterpDay>,
}

#[cfg(feature = "core")]
impl WeeklyProfileParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let p = pywr_core::parameters::WeeklyProfileParameter::new(
            ParameterName::new(&self.meta.name, parent),
            WeeklyProfileValues::try_from(self.values.load(args.tables)?.as_slice()).map_err(
                |err: WeeklyProfileError| SchemaError::LoadParameter {
                    name: self.meta.name.to_string(),
                    error: err.to_string(),
                },
            )?,
            self.interp_day.map(|id| id.into()),
        );
        Ok(network.add_simple_parameter(Box::new(p))?)
    }
}

impl TryFromV1<WeeklyProfileParameterV1> for WeeklyProfileParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: WeeklyProfileParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let values = try_convert_values(&meta.name, v1.values, v1.external, v1.table_ref)?;

        // pywr 1 does not support interpolation
        let p = Self {
            meta,
            values,
            interp_day: None,
        };
        Ok(p)
    }
}

/// A parameter that defines a profile over a 24-hour period.
///
/// The values array should contain 24 values, one for each hour of the day.
///
/// # JSON Example
///
/// ```json
#[doc = include_str!("doc_examples/dirunal_1.json")]
/// ```

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct DirunalProfileParameter {
    pub meta: ParameterMeta,
    pub values: ConstantFloatVec,
}

#[cfg(feature = "core")]
impl DirunalProfileParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let values = self.values.load(args.tables)?;

        let values: [f64; 24] = values.try_into().map_err(|v: Vec<_>| SchemaError::DataLengthMismatch {
            expected: 24,
            found: v.len(),
        })?;

        let p =
            pywr_core::parameters::DiurnalProfileParameter::new(ParameterName::new(&self.meta.name, parent), values);
        Ok(network.add_simple_parameter(Box::new(p))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::NetworkSchema;
    use crate::parameters::ParameterMeta;
    use crate::parameters::{ConstantFloatVec, Parameter};
    use pywr_core::models::ModelDomain;
    use pywr_core::test_utils::default_time_domain;

    #[test]
    fn add_to_model_with_366_values() {
        let meta = ParameterMeta {
            name: "test".to_string(),
            comment: None,
        };
        let values = ConstantFloatVec::Literal { values: vec![1.0; 366] };
        let param = DailyProfileParameter { meta, values };
        let domain: ModelDomain = default_time_domain().into();
        let network = NetworkSchema {
            parameters: Some(vec![Parameter::DailyProfile(param)]),
            ..Default::default()
        };

        let result = network.build_network(&domain, None, None, &[]);

        assert!(result.is_ok());
    }

    #[test]
    fn add_to_model_with_365_values_inserts_feb_29() {
        let meta = ParameterMeta {
            name: "test".to_string(),
            comment: None,
        };

        let values = vec![1.0; 365];
        let values = ConstantFloatVec::Literal { values };
        let param = DailyProfileParameter { meta, values };
        let domain: ModelDomain = default_time_domain().into();
        let network = NetworkSchema {
            parameters: Some(vec![Parameter::DailyProfile(param)]),
            ..Default::default()
        };

        let result = network.build_network(&domain, None, None, &[]);

        assert!(result.is_ok());
    }

    #[test]
    fn add_to_model_with_invalid_length_returns_error() {
        let meta = ParameterMeta {
            name: "test".to_string(),
            comment: None,
        };
        let values = ConstantFloatVec::Literal { values: vec![1.0; 364] };
        let param = DailyProfileParameter { meta, values };
        let domain: ModelDomain = default_time_domain().into();
        let network = NetworkSchema {
            parameters: Some(vec![Parameter::DailyProfile(param)]),
            ..Default::default()
        };

        let result = network.build_network(&domain, None, None, &[]);

        assert!(result.is_err());
    }
}
