use crate::error::ComponentConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::{IndexMetric, Metric};
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::parameters::{ConversionData, ParameterMeta};
use crate::v1::{IntoV2, TryFromV1, try_convert_parameter_attr};

#[cfg(feature = "core")]
use pywr_core::parameters::{ParameterIndex, ParameterName};
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::parameters::IndexedArrayParameter as IndexedArrayParameterV1;
use schemars::JsonSchema;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct IndexedArrayParameter {
    pub meta: ParameterMeta,
    #[serde(alias = "params")]
    pub metrics: Vec<Metric>,
    pub index_parameter: IndexMetric,
}

#[cfg(feature = "core")]
impl IndexedArrayParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let index_parameter = self.index_parameter.load(network, args, None)?;

        let metrics = self
            .metrics
            .iter()
            .map(|v| v.load(network, args, None))
            .collect::<Result<Vec<_>, _>>()?;

        let p = pywr_core::parameters::IndexedArrayParameter::new(
            ParameterName::new(&self.meta.name, parent),
            index_parameter,
            &metrics,
        );

        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1<IndexedArrayParameterV1> for IndexedArrayParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: IndexedArrayParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let metrics = v1
            .parameters
            .into_iter()
            .map(|p| try_convert_parameter_attr(&meta.name, "parameters", p, parent_node, conversion_data))
            .collect::<Result<Vec<_>, _>>()?;

        let index_parameter = try_convert_parameter_attr(
            &meta.name,
            "index_parameter",
            v1.index_parameter,
            parent_node,
            conversion_data,
        )?;

        let p = Self {
            meta,
            index_parameter,
            metrics,
        };
        Ok(p)
    }
}
