use crate::error::ComponentConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::parameters::{ConversionData, ParameterMeta};
use crate::v1::{TryFromV1, TryIntoV2};
#[cfg(feature = "core")]
use pywr_core::parameters::ParameterName;
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::parameters::DiscountFactorParameter as DiscountFactorParameterV1;
use schemars::JsonSchema;

/// A parameter that returns the current discount factor for a given time-step.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct DiscountFactorParameter {
    pub meta: ParameterMeta,
    pub discount_rate: Metric,
    pub base_year: i32,
}

#[cfg(feature = "core")]
impl DiscountFactorParameter {
    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<(), SchemaError> {
        let discount_rate = self.discount_rate.load(network, args, None)?;
        let p = pywr_core::parameters::DiscountFactorParameterBuilder::new(
            ParameterName::new(&self.meta.name, parent),
            discount_rate,
            self.base_year,
        );

        network.parameters().f64(Box::new(p));

        Ok(())
    }
}

impl TryFromV1<DiscountFactorParameterV1> for DiscountFactorParameter {
    type Error = Box<ComponentConversionError>;

    fn try_from_v1(
        v1: DiscountFactorParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.try_into_v2(parent_node, conversion_data)?;
        let discount_rate = Metric::from(v1.rate);
        Ok(Self {
            meta,
            discount_rate,
            base_year: v1.base_year as i32,
        })
    }
}
