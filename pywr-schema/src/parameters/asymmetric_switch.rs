use crate::error::ComponentConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::IndexMetric;
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::parameters::{ConversionData, ParameterMeta};
use crate::v1::{TryFromV1, TryIntoV2, try_convert_parameter_attr};

#[cfg(feature = "core")]
use pywr_core::parameters::ParameterName;
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
    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<(), SchemaError> {
        let on_index_parameter = self.on_index_parameter.load(network, args, None)?;
        let off_index_parameter = self.off_index_parameter.load(network, args, None)?;

        let p = pywr_core::parameters::AsymmetricSwitchIndexParameterBuilder::new(
            ParameterName::new(&self.meta.name, parent),
            on_index_parameter,
            off_index_parameter,
        );

        network.parameters().u64(Box::new(p));

        Ok(())
    }
}

impl TryFromV1<AsymmetricSwitchIndexParameterV1> for AsymmetricSwitchIndexParameter {
    type Error = Box<ComponentConversionError>;

    fn try_from_v1(
        v1: AsymmetricSwitchIndexParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.try_into_v2(parent_node, conversion_data)?;

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
