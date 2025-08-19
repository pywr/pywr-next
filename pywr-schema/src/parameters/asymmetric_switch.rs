use crate::error::ComponentConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::IndexMetric;
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::parameters::{ConversionData, ParameterMeta};
use crate::v1::{IntoV2, TryFromV1, try_convert_parameter_attr};

#[cfg(feature = "core")]
use pywr_core::parameters::{ParameterIndex, ParameterName};
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::parameters::AsymmetricSwitchIndexParameter as AsymmetricSwitchIndexParameterV1;
use schemars::JsonSchema;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct AsymmetricSwitchIndexParameter {
    pub meta: ParameterMeta,
    pub on_index_parameter: IndexMetric,
    pub off_index_parameter: IndexMetric,
}

#[cfg(feature = "core")]
impl AsymmetricSwitchIndexParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<u64>, SchemaError> {
        let on_index_parameter = self.on_index_parameter.load(network, args, None)?;
        let off_index_parameter = self.off_index_parameter.load(network, args, None)?;

        let p = pywr_core::parameters::AsymmetricSwitchIndexParameter::new(
            ParameterName::new(&self.meta.name, parent),
            on_index_parameter,
            off_index_parameter,
        );

        Ok(network.add_index_parameter(Box::new(p))?)
    }
}

impl TryFromV1<AsymmetricSwitchIndexParameterV1> for AsymmetricSwitchIndexParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: AsymmetricSwitchIndexParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let on_index_parameter = try_convert_parameter_attr(
            &meta.name,
            "on_index_parameter",
            v1.on_index_parameter,
            parent_node,
            conversion_data,
        )?;
        let off_index_parameter = try_convert_parameter_attr(
            &meta.name,
            "off_index_parameter",
            v1.off_index_parameter,
            parent_node,
            conversion_data,
        )?;

        let p = Self {
            meta,
            on_index_parameter,
            off_index_parameter,
        };
        Ok(p)
    }
}
