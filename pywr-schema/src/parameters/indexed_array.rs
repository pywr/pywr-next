use crate::error::ConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::parameters::{DynamicIndexValue, IntoV2Parameter, ParameterMeta, TryFromV1Parameter, TryIntoV2Parameter};
#[cfg(feature = "core")]
use pywr_core::parameters::ParameterIndex;
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::parameters::IndexedArrayParameter as IndexedArrayParameterV1;
use schemars::JsonSchema;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct IndexedArrayParameter {
    pub meta: ParameterMeta,
    #[serde(alias = "params")]
    pub metrics: Vec<Metric>,
    pub index_parameter: DynamicIndexValue,
}

#[cfg(feature = "core")]
impl IndexedArrayParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let index_parameter = self.index_parameter.load(network, args)?;

        let metrics = self
            .metrics
            .iter()
            .map(|v| v.load(network, args))
            .collect::<Result<Vec<_>, _>>()?;

        let p = pywr_core::parameters::IndexedArrayParameter::new(
            self.meta.name.as_str().into(),
            index_parameter,
            &metrics,
        );

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
