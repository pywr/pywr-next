use crate::data_tables::LoadedTableCollection;
use crate::error::{ConversionError, SchemaError};
use crate::model::PywrMultiNetworkTransfer;
use crate::parameters::{
    DynamicFloatValue, DynamicFloatValueType, DynamicIndexValue, IntoV2Parameter, ParameterMeta, TryFromV1Parameter,
    TryIntoV2Parameter,
};
use pywr_core::models::ModelDomain;
use pywr_core::parameters::ParameterIndex;
use pywr_v1_schema::parameters::IndexedArrayParameter as IndexedArrayParameterV1;
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
        network: &mut pywr_core::network::Network,
        schema: &crate::model::PywrNetwork,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
    ) -> Result<ParameterIndex, SchemaError> {
        let index_parameter =
            self.index_parameter
                .load(network, schema, domain, tables, data_path, inter_network_transfers)?;

        let metrics = self
            .metrics
            .iter()
            .map(|v| v.load(network, schema, domain, tables, data_path, inter_network_transfers))
            .collect::<Result<Vec<_>, _>>()?;

        let p = pywr_core::parameters::IndexedArrayParameter::new(&self.meta.name, index_parameter, &metrics);

        Ok(network.add_parameter(Box::new(p))?)
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
