use crate::data_tables::LoadedTableCollection;
use crate::error::{ConversionError, SchemaError};
use crate::parameters::{
    ConstantFloatVec, ConstantValue, DynamicFloatValueType, IntoV2Parameter, ParameterMeta, TryFromV1Parameter,
};
use pywr_core::parameters::ParameterIndex;
use pywr_v1_schema::parameters::{
    DailyProfileParameter as DailyProfileParameterV1, MonthInterpDay as MonthInterpDayV1,
    MonthlyProfileParameter as MonthlyProfileParameterV1,
    UniformDrawdownProfileParameter as UniformDrawdownProfileParameterV1,
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
            ConstantFloatVec::External(external.into())
        } else if let Some(table_ref) = v1.table_ref {
            ConstantFloatVec::Table(table_ref.into())
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
            ConstantFloatVec::External(external.into())
        } else if let Some(table_ref) = v1.table_ref {
            ConstantFloatVec::Table(table_ref.into())
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

/// Distance functions for radial basis function interpolation.
#[derive(serde::Deserialize, serde::Serialize, Debug, Copy, Clone)]
pub enum RadialBasisFunction {
    Linear,
    Cubic,
    ThinPlateSpline,
    Gaussian { epsilon: f64 },
    MultiQuadric { epsilon: f64 },
    InverseMultiQuadric { epsilon: f64 },
}

impl Into<pywr_core::parameters::RadialBasisFunction> for RadialBasisFunction {
    fn into(self) -> pywr_core::parameters::RadialBasisFunction {
        match self {
            Self::Linear => pywr_core::parameters::RadialBasisFunction::Linear,
            Self::Cubic => pywr_core::parameters::RadialBasisFunction::Cubic,
            Self::ThinPlateSpline => pywr_core::parameters::RadialBasisFunction::ThinPlateSpline,
            Self::Gaussian { epsilon } => pywr_core::parameters::RadialBasisFunction::Gaussian { epsilon },
            Self::MultiQuadric { epsilon } => pywr_core::parameters::RadialBasisFunction::MultiQuadric { epsilon },
            Self::InverseMultiQuadric { epsilon } => {
                pywr_core::parameters::RadialBasisFunction::InverseMultiQuadric { epsilon }
            }
        }
    }
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

        let p = pywr_core::parameters::RbfProfileParameter::new(
            &self.meta.name,
            self.points.clone(),
            self.function.into(),
            variable,
        );
        Ok(model.add_parameter(Box::new(p))?)
    }
}
