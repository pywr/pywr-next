use crate::schema::data_tables::LoadedTableCollection;
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

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct ConstantParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
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
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
    ) -> Result<ParameterIndex, PywrError> {
        let p = crate::parameters::ConstantParameter::new(&self.meta.name, self.value.load(tables)?);
        model.add_parameter(Box::new(p))
    }
}

impl TryFromV1Parameter<ConstantParameterV1> for ConstantParameter {
    type Error = PywrError;

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

        let p = crate::parameters::MaxParameter::new(&self.meta.name, idx.into(), threshold);
        model.add_parameter(Box::new(p))
    }
}

impl TryFromV1Parameter<MaxParameterV1> for MaxParameter {
    type Error = PywrError;

    fn try_from_v1_parameter(
        v1: MaxParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let parameter = v1.parameter.try_into_v2_parameter(parent_node, unnamed_count)?;

        let p = Self {
            meta: v1.meta.into_v2_parameter(parent_node, unnamed_count),
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

        let p = crate::parameters::NegativeParameter::new(&self.meta.name, idx.into());
        model.add_parameter(Box::new(p))
    }
}

impl TryFromV1Parameter<NegativeParameterV1> for NegativeParameter {
    type Error = PywrError;

    fn try_from_v1_parameter(
        v1: NegativeParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let parameter = v1.parameter.try_into_v2_parameter(parent_node, unnamed_count)?;

        let p = Self {
            meta: v1.meta.into_v2_parameter(parent_node, unnamed_count),
            parameter,
        };
        Ok(p)
    }
}
