use crate::data_tables::LoadedTableCollection;
use crate::error::{ConversionError, SchemaError};
use crate::parameters::{
    DynamicFloatValueType, DynamicIndexValue, IntoV2Parameter, ParameterMeta, TryFromV1Parameter, TryIntoV2Parameter,
};
use pywr_core::parameters::IndexParameterIndex;
use pywr_v1_schema::parameters::AsymmetricSwitchIndexParameter as AsymmetricSwitchIndexParameterV1;
use std::collections::HashMap;
use std::path::Path;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct AsymmetricSwitchIndexParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub on_index_parameter: DynamicIndexValue,
    pub off_index_parameter: DynamicIndexValue,
}

impl AsymmetricSwitchIndexParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }
    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        todo!()
    }

    pub fn add_to_model(
        &self,
        model: &mut pywr_core::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<IndexParameterIndex, SchemaError> {
        let on_index_parameter = self.on_index_parameter.load(model, tables, data_path)?;
        let off_index_parameter = self.off_index_parameter.load(model, tables, data_path)?;

        let p = pywr_core::parameters::AsymmetricSwitchIndexParameter::new(
            &self.meta.name,
            on_index_parameter,
            off_index_parameter,
        );

        Ok(model.add_index_parameter(Box::new(p))?)
    }
}

impl TryFromV1Parameter<AsymmetricSwitchIndexParameterV1> for AsymmetricSwitchIndexParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: AsymmetricSwitchIndexParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);

        let on_index_parameter = v1
            .on_index_parameter
            .try_into_v2_parameter(Some(&meta.name), unnamed_count)?;
        let off_index_parameter = v1
            .off_index_parameter
            .try_into_v2_parameter(Some(&meta.name), unnamed_count)?;

        let p = Self {
            meta,
            on_index_parameter,
            off_index_parameter,
        };
        Ok(p)
    }
}
