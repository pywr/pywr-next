use crate::data_tables::LoadedTableCollection;
use crate::error::{ConversionError, SchemaError};
use crate::parameters::{
    ConstantFloatVec, ConstantValue, DynamicFloatValueType, IntoV2Parameter, ParameterMeta, TryFromV1Parameter,
};
use pywr_core::parameters::ParameterIndex;
use pywr_v1_schema::parameters::{
    DailyProfileParameter as DailyProfileParameterV1,
    MonthInterpDay as MonthInterpDayV1,
    MonthlyProfileParameter as MonthlyProfileParameterV1,
    // WeeklyProfileParameter as WeeklyProfileParameterV1,
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
