use crate::data_tables::LoadedTableCollection;
use crate::error::SchemaError;
use crate::model::PywrMultiNetworkTransfer;
use crate::parameters::{DynamicFloatValue, DynamicFloatValueType, IntoV2Parameter, ParameterMeta, TryFromV1Parameter};
use crate::ConversionError;
use pywr_core::models::ModelDomain;
use pywr_core::parameters::ParameterIndex;
use pywr_v1_schema::parameters::DiscountFactorParameter as DiscountFactorParameterV1;
use std::collections::HashMap;
use std::path::Path;

/// A parameter that returns the current discount factor for a given time-step.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct DiscountFactorParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub discount_rate: DynamicFloatValue,
    pub base_year: i32,
}

impl DiscountFactorParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }

    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        let mut attributes = HashMap::new();

        let metric = &self.discount_rate;
        attributes.insert("discount_rate", metric.into());

        attributes
    }

    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
    ) -> Result<ParameterIndex, SchemaError> {
        let discount_rate = self
            .discount_rate
            .load(network, domain, tables, data_path, inter_network_transfers)?;
        let p = pywr_core::parameters::DiscountFactorParameter::new(&self.meta.name, discount_rate, self.base_year);
        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1Parameter<DiscountFactorParameterV1> for DiscountFactorParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: DiscountFactorParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);
        let discount_rate = DynamicFloatValue::from_f64(v1.rate);
        Ok(Self {
            meta,
            discount_rate,
            base_year: v1.base_year as i32,
        })
    }
}
