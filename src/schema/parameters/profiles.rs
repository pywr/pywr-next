use crate::schema::data_tables::LoadedTableCollection;
use crate::schema::parameters::{
    ConstantFloatVec, ConstantValue, DynamicFloatValueType, IntoV2Parameter, ParameterMeta, TryFromV1Parameter,
};
use crate::{ParameterIndex, PywrError};
use pywr_schema::parameters::{
    DailyProfileParameter as DailyProfileParameterV1, MonthlyProfileParameter as MonthlyProfileParameterV1,
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
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
    ) -> Result<ParameterIndex, PywrError> {
        let values = &self.values.load(tables)?[..366];
        let p = crate::parameters::DailyProfileParameter::new(&self.meta.name, values.try_into().expect(""));
        model.add_parameter(Box::new(p))
    }
}

impl TryFromV1Parameter<DailyProfileParameterV1> for DailyProfileParameter {
    type Error = PywrError;

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
            return Err(PywrError::V1SchemaConversion(format!(
                "DailyProfileParameter '{}' has no valid values defined.",
                &meta.name
            )));
        };

        let p = Self { meta, values };
        Ok(p)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct MonthlyProfileParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub values: ConstantFloatVec,
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
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
    ) -> Result<ParameterIndex, PywrError> {
        let values = &self.values.load(tables)?[..12];
        let p = crate::parameters::MonthlyProfileParameter::new(&self.meta.name, values.try_into().expect(""));
        model.add_parameter(Box::new(p))
    }
}

impl TryFromV1Parameter<MonthlyProfileParameterV1> for MonthlyProfileParameter {
    type Error = PywrError;

    fn try_from_v1_parameter(
        v1: MonthlyProfileParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);

        let values: ConstantFloatVec = if let Some(values) = v1.values {
            ConstantFloatVec::Literal(values.to_vec())
        } else if let Some(external) = v1.external {
            ConstantFloatVec::External(external.into())
        } else if let Some(table_ref) = v1.table_ref {
            ConstantFloatVec::Table(table_ref.into())
        } else {
            return Err(PywrError::V1SchemaConversion(format!(
                "MonthlyProfileParameter '{}' has no valid values defined.",
                &meta.name
            )));
        };

        let p = Self { meta, values };
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
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
    ) -> Result<ParameterIndex, PywrError> {
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

        let p = crate::parameters::UniformDrawdownProfileParameter::new(
            &self.meta.name,
            reset_day,
            reset_month,
            residual_days,
        );
        model.add_parameter(Box::new(p))
    }
}

impl TryFromV1Parameter<UniformDrawdownProfileParameterV1> for UniformDrawdownProfileParameter {
    type Error = PywrError;

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
