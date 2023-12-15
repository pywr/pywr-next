use crate::data_tables::LoadedTableCollection;
use crate::error::{ConversionError, SchemaError};
use crate::parameters::{
    ConstantFloatVec, ConstantValue, DynamicFloatValueType, IntoV2Parameter, ParameterMeta, TryFromV1Parameter,
};
use pywr_core::parameters::ParameterIndex;
use pywr_v1_schema::parameters::{
    DailyProfileParameter as DailyProfileParameterV1, MonthInterpDay as MonthInterpDayV1,
    MonthlyProfileParameter as MonthlyProfileParameterV1, RbfProfileParameter as RbfProfileParameterV1,
    UniformDrawdownProfileParameter as UniformDrawdownProfileParameterV1,
    WeeklyProfileParameter as WeeklyProfileParameterV1,
};
use std::collections::HashMap;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct DailyProfileParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub values: ConstantFloatVec,
}

impl DailyProfileParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }
    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        HashMap::new()
    }

    pub fn add_to_model(
        &self,
        model: &mut pywr_core::model::Model,
        tables: &LoadedTableCollection,
    ) -> Result<ParameterIndex, SchemaError> {
        let values = &self.values.load(tables)?[..366];
        let p = pywr_core::parameters::DailyProfileParameter::new(&self.meta.name, values.try_into().expect(""));
        Ok(model.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1Parameter<DailyProfileParameterV1> for DailyProfileParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: DailyProfileParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);

        let values: ConstantFloatVec = if let Some(values) = v1.values {
            ConstantFloatVec::Literal(values)
        } else if let Some(external) = v1.external {
            ConstantFloatVec::External(external.try_into()?)
        } else if let Some(table_ref) = v1.table_ref {
            ConstantFloatVec::Table(table_ref.try_into()?)
        } else {
            return Err(ConversionError::MissingAttribute {
                name: meta.name,
                attrs: vec!["values".to_string(), "table".to_string(), "url".to_string()],
            });
        };

        let p = Self { meta, values };
        Ok(p)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Copy, Clone)]
pub enum MonthlyInterpDay {
    First,
    Last,
}

impl From<MonthlyInterpDay> for pywr_core::parameters::MonthlyInterpDay {
    fn from(value: MonthlyInterpDay) -> Self {
        match value {
            MonthlyInterpDay::First => Self::First,
            MonthlyInterpDay::Last => Self::Last,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct MonthlyProfileParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub values: ConstantFloatVec,
    pub interp_day: Option<MonthlyInterpDay>,
}

impl MonthlyProfileParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }
    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        HashMap::new()
    }

    pub fn add_to_model(
        &self,
        model: &mut pywr_core::model::Model,
        tables: &LoadedTableCollection,
    ) -> Result<ParameterIndex, SchemaError> {
        let values = &self.values.load(tables)?[..12];
        let p = pywr_core::parameters::MonthlyProfileParameter::new(
            &self.meta.name,
            values.try_into().expect(""),
            self.interp_day.map(|id| id.into()),
        );
        Ok(model.add_parameter(Box::new(p))?)
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

impl TryFromV1Parameter<MonthlyProfileParameterV1> for MonthlyProfileParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: MonthlyProfileParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);
        let interp_day = v1.interp_day.map(|id| id.into());

        let values: ConstantFloatVec = if let Some(values) = v1.values {
            ConstantFloatVec::Literal(values.to_vec())
        } else if let Some(external) = v1.external {
            ConstantFloatVec::External(external.try_into()?)
        } else if let Some(table_ref) = v1.table_ref {
            ConstantFloatVec::Table(table_ref.try_into()?)
        } else {
            return Err(ConversionError::MissingAttribute {
                name: meta.name,
                attrs: vec!["values".to_string(), "table".to_string(), "url".to_string()],
            });
        };

        let p = Self {
            meta,
            values,
            interp_day,
        };
        Ok(p)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct UniformDrawdownProfileParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub reset_day: Option<ConstantValue<usize>>,
    pub reset_month: Option<ConstantValue<usize>>,
    pub residual_days: Option<ConstantValue<usize>>,
}

impl UniformDrawdownProfileParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }
    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        HashMap::new()
    }

    pub fn add_to_model(
        &self,
        model: &mut pywr_core::model::Model,
        tables: &LoadedTableCollection,
    ) -> Result<ParameterIndex, SchemaError> {
        let reset_day = match &self.reset_day {
            Some(v) => v.load(tables)? as u8,
            None => 1,
        };
        let reset_month = match &self.reset_month {
            Some(v) => time::Month::try_from(v.load(tables)? as u8)?,
            None => time::Month::January,
        };
        let residual_days = match &self.residual_days {
            Some(v) => v.load(tables)? as u8,
            None => 0,
        };

        let p = pywr_core::parameters::UniformDrawdownProfileParameter::new(
            &self.meta.name,
            reset_day,
            reset_month,
            residual_days,
        );
        Ok(model.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1Parameter<UniformDrawdownProfileParameterV1> for UniformDrawdownProfileParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: UniformDrawdownProfileParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);

        let p = Self {
            meta,
            reset_day: v1.reset_day.map(|v| ConstantValue::Literal(v as usize)),
            reset_month: v1.reset_day.map(|v| ConstantValue::Literal(v as usize)),
            residual_days: v1.reset_day.map(|v| ConstantValue::Literal(v as usize)),
        };

        Ok(p)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Copy, Clone)]
pub enum WeeklyInterpDay {
    First,
    Last,
}

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
///     the value for the 53<sup>rd</sup> week (day 364 - 29<sup>th</sup> Dec or 30<sup>th</sup>
///     Dec for a leap year) is copied from week 52<sup>nd</sup>.
/// * `interp_day` - This is an optional field to control the parameter interpolation. When this
///     is not provided, the profile is piecewise. When this equals "First" or "Last", the values
///     are linearly interpolated in each week and the string specifies whether the given values are
///     the first or last day of the week. See the examples below for more information.
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

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct WeeklyProfileParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub values: ConstantFloatVec,
    pub interp_day: Option<WeeklyInterpDay>,
}

impl WeeklyProfileParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }
    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        HashMap::new()
    }

    pub fn add_to_model(
        &self,
        model: &mut pywr_core::model::Model,
        tables: &LoadedTableCollection,
    ) -> Result<ParameterIndex, SchemaError> {
        let p = pywr_core::parameters::WeeklyProfileParameter::new(
            &self.meta.name,
            self.values.load(tables)?,
            self.interp_day.map(|id| id.into()),
        );
        Ok(model.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1Parameter<WeeklyProfileParameterV1> for WeeklyProfileParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: WeeklyProfileParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);

        let values: ConstantFloatVec = if let Some(values) = v1.values {
            // pywr v1 only accept a 52-week profile
            ConstantFloatVec::Literal(values)
        } else if let Some(external) = v1.external {
            ConstantFloatVec::External(external.try_into()?)
        } else if let Some(table_ref) = v1.table_ref {
            ConstantFloatVec::Table(table_ref.try_into()?)
        } else {
            return Err(ConversionError::MissingAttribute {
                name: meta.name,
                attrs: vec!["values".to_string(), "table".to_string(), "url".to_string()],
            });
        };

        // pywr 1 does not support interpolation
        let p = Self {
            meta,
            values,
            interp_day: None,
        };
        Ok(p)
    }
}

/// Distance functions for radial basis function interpolation.
#[derive(serde::Deserialize, serde::Serialize, Debug, Copy, Clone)]
pub enum RadialBasisFunction {
    Linear,
    Cubic,
    Quintic,
    ThinPlateSpline,
    Gaussian { epsilon: Option<f64> },
    MultiQuadric { epsilon: Option<f64> },
    InverseMultiQuadric { epsilon: Option<f64> },
}

impl RadialBasisFunction {
    /// Convert the schema representation of the RBF into `pywr_core` type.
    ///
    /// If required this will estimate values of from the provided points.
    fn into_core_rbf(self, points: &[(u32, f64)]) -> Result<pywr_core::parameters::RadialBasisFunction, SchemaError> {
        let rbf = match self {
            Self::Linear => pywr_core::parameters::RadialBasisFunction::Linear,
            Self::Cubic => pywr_core::parameters::RadialBasisFunction::Cubic,
            Self::Quintic => pywr_core::parameters::RadialBasisFunction::Cubic,
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
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy)]
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

impl Into<pywr_core::parameters::RbfProfileVariableConfig> for RbfProfileVariableSettings {
    fn into(self) -> pywr_core::parameters::RbfProfileVariableConfig {
        pywr_core::parameters::RbfProfileVariableConfig::new(
            self.days_of_year_range,
            self.value_upper_bounds.unwrap_or(f64::INFINITY),
            self.value_lower_bounds.unwrap_or(0.0),
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
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct RbfProfileParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    /// The points are the profile positions defined by an ordinal day of the year and a value.
    /// Radial basis function interpolation is used to create a daily profile from these points.
    pub points: Vec<(u32, f64)>,
    /// The distance function used for interpolation.
    pub function: RadialBasisFunction,
    /// Definition of optional variable settings.
    pub variable: Option<RbfProfileVariableSettings>,
}

impl RbfProfileParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }
    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        HashMap::new()
    }

    pub fn add_to_model(&self, model: &mut pywr_core::model::Model) -> Result<ParameterIndex, SchemaError> {
        let variable = match self.variable {
            None => None,
            Some(v) => {
                // Only set the variable data if the user has indicated the variable is active.
                if v.is_active {
                    Some(v.into())
                } else {
                    None
                }
            }
        };

        let function = self.function.into_core_rbf(&self.points)?;

        let p =
            pywr_core::parameters::RbfProfileParameter::new(&self.meta.name, self.points.clone(), function, variable);
        Ok(model.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1Parameter<RbfProfileParameterV1> for RbfProfileParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: RbfProfileParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);

        let points = v1
            .days_of_year
            .into_iter()
            .zip(v1.values.into_iter())
            .map(|(doy, v)| (doy, v))
            .collect();

        if v1.rbf_kwargs.contains_key("smooth") {
            return Err(ConversionError::UnsupportedFeature {
                feature: "The RBF `smooth` keyword argument is not supported.".to_string(),
                name: meta.name,
            });
        }

        if v1.rbf_kwargs.contains_key("norm") {
            return Err(ConversionError::UnsupportedFeature {
                feature: "The RBF `norm` keyword argument is not supported.".to_string(),
                name: meta.name,
            });
        }

        // Parse any epsilon value; we expect a float here.
        let epsilon = if let Some(epsilon_value) = v1.rbf_kwargs.get("epsilon") {
            if let Some(epsilon_f64) = epsilon_value.as_f64() {
                Some(epsilon_f64)
            } else {
                return Err(ConversionError::UnexpectedType {
                    attr: "epsilon".to_string(),
                    name: meta.name,
                    expected: "float".to_string(),
                    actual: format!("{}", epsilon_value),
                });
            }
        } else {
            None
        };

        let function = if let Some(function_value) = v1.rbf_kwargs.get("function") {
            if let Some(function_str) = function_value.as_str() {
                // Function kwarg is a string!
                let f = match function_str {
                    "multiquadric" => RadialBasisFunction::MultiQuadric { epsilon },
                    "inverse" => RadialBasisFunction::InverseMultiQuadric { epsilon },
                    "gaussian" => RadialBasisFunction::Gaussian { epsilon },
                    "linear" => RadialBasisFunction::Linear,
                    "cubic" => RadialBasisFunction::Cubic,
                    "thin_plate" => RadialBasisFunction::ThinPlateSpline,
                    _ => {
                        return Err(ConversionError::UnsupportedFeature {
                            feature: format!("Radial basis function `{}` not supported.", function_str),
                            name: meta.name.clone(),
                        })
                    }
                };
                f
            } else {
                return Err(ConversionError::UnexpectedType {
                    attr: "function".to_string(),
                    name: meta.name,
                    expected: "string".to_string(),
                    actual: format!("{}", function_value),
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
            variable: None,
        };

        Ok(p)
    }
}
