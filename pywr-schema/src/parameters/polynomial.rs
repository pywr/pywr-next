use crate::error::ConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::parameters::{IntoV2Parameter, ParameterMeta, TryFromV1Parameter};
#[cfg(feature = "core")]
use pywr_core::parameters::ParameterIndex;
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::parameters::Polynomial1DParameter as Polynomial1DParameterV1;
use schemars::JsonSchema;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
pub struct Polynomial1DParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub storage_node: String,
    pub coefficients: Vec<f64>,
    pub use_proportional_volume: Option<bool>,
    pub scale: Option<f64>,
    pub offset: Option<f64>,
}

#[cfg(feature = "core")]
impl Polynomial1DParameter {
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<ParameterIndex<f64>, SchemaError> {
        let metric =
            network.get_storage_node_metric(&self.storage_node, None, self.use_proportional_volume.unwrap_or(true))?;

        let p = pywr_core::parameters::Polynomial1DParameter::new(
            &self.meta.name,
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
        let p = Self {
            meta: v1.meta.into_v2_parameter(parent_node, unnamed_count),
            storage_node: v1.storage_node,
            coefficients: v1.coefficients,
            use_proportional_volume: v1.use_proportional_volume,
            scale: v1.scale,
            offset: v1.offset,
        };
        Ok(p)
    }
}
