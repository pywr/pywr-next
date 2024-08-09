#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::parameters::{IntoV2Parameter, ParameterMeta, TryFromV1Parameter};
use crate::ConversionError;
#[cfg(feature = "core")]
use pywr_core::parameters::ParameterIndex;
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::parameters::DiscountFactorParameter as DiscountFactorParameterV1;
use schemars::JsonSchema;

/// A parameter that returns the current discount factor for a given time-step.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
pub struct DiscountFactorParameter {
    #[serde(flatten)]
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
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let discount_rate = self.discount_rate.load(network, args)?;
        let p = pywr_core::parameters::DiscountFactorParameter::new(
            self.meta.name.as_str().into(),
            discount_rate,
            self.base_year,
        );
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
        let discount_rate = Metric::from(v1.rate);
        Ok(Self {
            meta,
            discount_rate,
            base_year: v1.base_year as i32,
        })
    }
}
