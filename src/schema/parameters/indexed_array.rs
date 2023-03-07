use crate::schema::data_tables::LoadedTableCollection;
use crate::schema::error::ConversionError;
use crate::schema::parameters::{
    DynamicFloatValue, DynamicFloatValueType, DynamicIndexValue, IntoV2Parameter, ParameterMeta, TryFromV1Parameter,
    TryIntoV2Parameter,
};
use crate::{ParameterIndex, PywrError};
use pywr_schema::parameters::IndexedArrayParameter as IndexedArrayParameterV1;
use std::collections::HashMap;
use std::path::Path;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct IndexedArrayParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    #[serde(alias = "params")]
    pub metrics: Vec<DynamicFloatValue>,
    pub index_parameter: DynamicIndexValue,
}

impl IndexedArrayParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }

    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        let mut attributes = HashMap::new();

        let metrics = &self.metrics;
        attributes.insert("metrics", metrics.into());

        attributes
    }

    pub fn add_to_model(
        &self,
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<ParameterIndex, PywrError> {
        let index_parameter = self.index_parameter.load(model, tables, data_path)?;

        let metrics = self
            .metrics
            .iter()
            .map(|v| v.load(model, tables, data_path))
            .collect::<Result<Vec<_>, _>>()?;

        let p = crate::parameters::IndexedArrayParameter::new(&self.meta.name, index_parameter, &metrics);

        model.add_parameter(Box::new(p))
    }
}

impl TryFromV1Parameter<IndexedArrayParameterV1> for IndexedArrayParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: IndexedArrayParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);

        let metrics = v1
            .parameters
            .into_iter()
            .map(|p| p.try_into_v2_parameter(Some(&meta.name), unnamed_count))
            .collect::<Result<Vec<_>, _>>()?;

        let index_parameter = v1
            .index_parameter
            .try_into_v2_parameter(Some(&meta.name), unnamed_count)?;

        let p = Self {
            meta,
            index_parameter,
            metrics,
        };
        Ok(p)
    }
}
