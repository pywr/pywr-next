use crate::error::ComponentConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::{IndexMetric, Metric};
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::parameters::{ConversionData, ParameterMeta, ParameterPhase};
use crate::v1::{TryFromV1, TryIntoV2, try_convert_parameter_attr};
#[cfg(feature = "core")]
use pywr_core::parameters::ParameterName;
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::parameters::IndexedArrayParameter as IndexedArrayParameterV1;
use schemars::JsonSchema;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct IndexedArrayParameter {
    pub meta: ParameterMeta,
    pub phase: ParameterPhase,
    #[serde(alias = "params")]
    pub metrics: Vec<Metric>,
    pub index_parameter: IndexMetric,
}

#[cfg(feature = "core")]
impl IndexedArrayParameter {
    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<(), SchemaError> {
        let index_parameter = self.index_parameter.load(network, args, None)?;

        let name = ParameterName::new(&self.meta.name, parent);

        let mut builder = match self.phase {
            ParameterPhase::Before => pywr_core::parameters::IndexedArrayParameterBuilder::before(name, index_parameter),
            ParameterPhase::After => pywr_core::parameters::IndexedArrayParameterBuilder::after(name, index_parameter),
            ParameterPhase::Both => pywr_core::parameters::IndexedArrayParameterBuilder::both(name, index_parameter),
        };

        for metric in &self.metrics {
            let m = metric.load(network, args, parent)?;
            builder.metric(m);
        }

        network.parameters().f64(Box::new(builder));

        Ok(())
    }
}

impl TryFromV1<IndexedArrayParameterV1> for IndexedArrayParameter {
    type Error = Box<ComponentConversionError>;

    fn try_from_v1(
        v1: IndexedArrayParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.try_into_v2(parent_node, conversion_data)?;

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
            phase: ParameterPhase::Before,
        };
        Ok(p)
    }
}
