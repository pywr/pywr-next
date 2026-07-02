use crate::error::ComponentConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::{Metric, NodeAttrReference};
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::nodes::NodeAttribute;
use crate::parameters::{ConversionData, ParameterMeta};
use crate::v1::{TryFromV1, TryIntoV2};
#[cfg(feature = "core")]
use pywr_core::parameters::ParameterName;
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
    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<(), SchemaError> {
        let metric = self.metric.load(network, args, None)?;

        let mut builder = pywr_core::parameters::Polynomial1DParameterBuilder::new(
            ParameterName::new(&self.meta.name, parent),
            metric,
            self.coefficients.clone(),
        );

        builder
            .scale(self.scale.unwrap_or(1.0))
            .offset(self.offset.unwrap_or(0.0));

        network.parameters().f64(Box::new(builder));

        Ok(())
    }
}

impl TryFromV1<Polynomial1DParameterV1> for Polynomial1DParameter {
    type Error = Box<ComponentConversionError>;

    fn try_from_v1(
        v1: Polynomial1DParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let attribute = match v1.use_proportional_volume.unwrap_or(true) {
            true => Some(NodeAttribute::ProportionalVolume),
            false => Some(NodeAttribute::Volume),
        };

        let metric = Metric::Node(NodeAttrReference {
            name: v1.storage_node,
            attribute,
        });

        Ok(Self {
            meta: v1.meta.try_into_v2(parent_node, conversion_data)?,
            metric,
            coefficients: v1.coefficients,
            scale: v1.scale,
            offset: v1.offset,
        })
    }
}
