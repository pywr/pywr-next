use crate::error::ConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::{Metric, NodeReference};
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::nodes::NodeAttribute;
use crate::parameters::{IntoV2Parameter, ParameterMeta, TryFromV1Parameter};
#[cfg(feature = "core")]
use pywr_core::parameters::ParameterIndex;
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::parameters::Polynomial1DParameter as Polynomial1DParameterV1;
use schemars::JsonSchema;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct Polynomial1DParameter {
    pub meta: ParameterMeta,
    pub metric: Metric,
    pub coefficients: Vec<f64>,
    pub scale: Option<f64>,
    pub offset: Option<f64>,
}

#[cfg(feature = "core")]
impl Polynomial1DParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let metric = self.metric.load(network, args)?;

        let p = pywr_core::parameters::Polynomial1DParameter::new(
            self.meta.name.as_str().into(),
            metric,
            self.coefficients.clone(),
            self.scale.unwrap_or(1.0),
            self.offset.unwrap_or(0.0),
        );
        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1Parameter<Polynomial1DParameterV1> for Polynomial1DParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: Polynomial1DParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let attribute = match v1.use_proportional_volume.unwrap_or(true) {
            true => Some(NodeAttribute::ProportionalVolume),
            false => Some(NodeAttribute::Volume),
        };

        let metric = Metric::Node(NodeReference {
            name: v1.storage_node,
            attribute,
        });

        let p = Self {
            meta: v1.meta.into_v2_parameter(parent_node, unnamed_count),
            metric,
            coefficients: v1.coefficients,
            scale: v1.scale,
            offset: v1.offset,
        };
        Ok(p)
    }
}
