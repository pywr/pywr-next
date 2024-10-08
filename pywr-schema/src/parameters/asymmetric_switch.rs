use crate::error::ConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::parameters::{DynamicIndexValue, IntoV2Parameter, ParameterMeta, TryFromV1Parameter, TryIntoV2Parameter};
#[cfg(feature = "core")]
use pywr_core::parameters::ParameterIndex;
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::parameters::AsymmetricSwitchIndexParameter as AsymmetricSwitchIndexParameterV1;
use schemars::JsonSchema;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct AsymmetricSwitchIndexParameter {
    pub meta: ParameterMeta,
    pub on_index_parameter: DynamicIndexValue,
    pub off_index_parameter: DynamicIndexValue,
}

#[cfg(feature = "core")]
impl AsymmetricSwitchIndexParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<ParameterIndex<usize>, SchemaError> {
        let on_index_parameter = self.on_index_parameter.load(network, args)?;
        let off_index_parameter = self.off_index_parameter.load(network, args)?;

        let p = pywr_core::parameters::AsymmetricSwitchIndexParameter::new(
            self.meta.name.as_str().into(),
            on_index_parameter,
            off_index_parameter,
        );

        Ok(network.add_index_parameter(Box::new(p))?)
    }
}

impl TryFromV1Parameter<AsymmetricSwitchIndexParameterV1> for AsymmetricSwitchIndexParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: AsymmetricSwitchIndexParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);

        let on_index_parameter = v1
            .on_index_parameter
            .try_into_v2_parameter(Some(&meta.name), unnamed_count)?;
        let off_index_parameter = v1
            .off_index_parameter
            .try_into_v2_parameter(Some(&meta.name), unnamed_count)?;

        let p = Self {
            meta,
            on_index_parameter,
            off_index_parameter,
        };
        Ok(p)
    }
}
