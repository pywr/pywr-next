use crate::schema::data_tables::LoadedTableCollection;
use crate::schema::error::ConversionError;
use crate::schema::parameters::{
    ConstantValue, DynamicFloatValue, DynamicFloatValueType, IntoV2Parameter, ParameterMeta, TryFromV1Parameter,
    TryIntoV2Parameter,
};
use crate::{ParameterIndex, PywrError};
use pywr_schema::parameters::{
    ConstantParameter as ConstantParameterV1, DivisionParameter as DivisionParameterV1, MaxParameter as MaxParameterV1,
    MinParameter as MinParameterV1, NegativeParameter as NegativeParameterV1,
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
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<ParameterIndex, PywrError> {
        let n = self.numerator.load(model, tables, data_path)?;
        let d = self.denominator.load(model, tables, data_path)?;

        let p = crate::parameters::DivisionParameter::new(&self.meta.name, n, d);
        model.add_parameter(Box::new(p))
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
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<ParameterIndex, PywrError> {
        let idx = self.parameter.load(model, tables, data_path)?;
        let threshold = self.threshold.unwrap_or(0.0);

        let p = crate::parameters::MinParameter::new(&self.meta.name, idx, threshold);
        model.add_parameter(Box::new(p))
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
