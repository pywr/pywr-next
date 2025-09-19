#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::{Metric, NodeAttrReference};
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::nodes::NodeAttribute;
use crate::parameters::{ConversionData, ParameterMeta};
use crate::v1::{FromV1, IntoV2};

#[cfg(feature = "core")]
use pywr_core::parameters::{ParameterIndex, ParameterName};
use pywr_schema_macros::{PywrVisitAll, skip_serializing_none};
use pywr_v1_schema::parameters::Polynomial1DParameter as Polynomial1DParameterV1;
use schemars::JsonSchema;

#[skip_serializing_none]
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
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let metric = self.metric.load(network, args, None)?;

        let p = pywr_core::parameters::Polynomial1DParameter::new(
            ParameterName::new(&self.meta.name, parent),
            metric,
            self.coefficients.clone(),
            self.scale.unwrap_or(1.0),
            self.offset.unwrap_or(0.0),
        );
        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl FromV1<Polynomial1DParameterV1> for Polynomial1DParameter {
    fn from_v1(v1: Polynomial1DParameterV1, parent_node: Option<&str>, conversion_data: &mut ConversionData) -> Self {
        let attribute = match v1.use_proportional_volume.unwrap_or(true) {
            true => Some(NodeAttribute::ProportionalVolume),
            false => Some(NodeAttribute::Volume),
        };

        let metric = Metric::Node(NodeAttrReference {
            name: v1.storage_node,
            attribute,
        });

        Self {
            meta: v1.meta.into_v2(parent_node, conversion_data),
            metric,
            coefficients: v1.coefficients,
            scale: v1.scale,
            offset: v1.offset,
        }
    }
}
