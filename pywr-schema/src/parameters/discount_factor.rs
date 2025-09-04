#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::parameters::{ConversionData, ParameterMeta};
use crate::v1::{FromV1, IntoV2};

#[cfg(feature = "core")]
use pywr_core::parameters::{ParameterIndex, ParameterName};
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
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let discount_rate = self.discount_rate.load(network, args, None)?;
        let p = pywr_core::parameters::DiscountFactorParameter::new(
            ParameterName::new(&self.meta.name, parent),
            discount_rate,
            self.base_year,
        );
        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl FromV1<DiscountFactorParameterV1> for DiscountFactorParameter {
    fn from_v1(v1: DiscountFactorParameterV1, parent_node: Option<&str>, conversion_data: &mut ConversionData) -> Self {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);
        let discount_rate = Metric::from(v1.rate);
        Self {
            meta,
            discount_rate,
            base_year: v1.base_year as i32,
        }
    }
}
