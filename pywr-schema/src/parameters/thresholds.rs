use crate::error::ComponentConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::parameters::{ConversionData, ParameterMeta};
use crate::v1::{try_convert_parameter_attr, IntoV2, TryFromV1};
#[cfg(feature = "core")]
use pywr_core::parameters::ParameterIndex;
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::parameters::{
    ParameterThresholdParameter as ParameterThresholdParameterV1, Predicate as PredicateV1,
};
use schemars::JsonSchema;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy, JsonSchema, PywrVisitAll, strum_macros::Display)]
pub enum Predicate {
    #[serde(alias = "<")]
    LT,
    #[serde(alias = ">")]
    GT,
    #[serde(alias = "==")]
    EQ,
    #[serde(alias = "<=")]
    LE,
    #[serde(alias = ">=")]
    GE,
}

impl From<PredicateV1> for Predicate {
    fn from(v1: PredicateV1) -> Self {
        match v1 {
            PredicateV1::LT => Predicate::LT,
            PredicateV1::GT => Predicate::GT,
            PredicateV1::EQ => Predicate::EQ,
            PredicateV1::LE => Predicate::LE,
            PredicateV1::GE => Predicate::GE,
        }
    }
}

#[cfg(feature = "core")]
impl From<Predicate> for pywr_core::parameters::Predicate {
    fn from(p: Predicate) -> Self {
        match p {
            Predicate::LT => pywr_core::parameters::Predicate::LessThan,
            Predicate::GT => pywr_core::parameters::Predicate::GreaterThan,
            Predicate::EQ => pywr_core::parameters::Predicate::EqualTo,
            Predicate::LE => pywr_core::parameters::Predicate::LessThanOrEqualTo,
            Predicate::GE => pywr_core::parameters::Predicate::GreaterThanOrEqualTo,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct ThresholdParameter {
    pub meta: ParameterMeta,
    pub value: Metric,
    pub threshold: Metric,
    pub predicate: Predicate,
    #[serde(default)]
    pub ratchet: bool,
}

#[cfg(feature = "core")]
impl ThresholdParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<ParameterIndex<usize>, SchemaError> {
        let metric = self.value.load(network, args, None)?;
        let threshold = self.threshold.load(network, args, None)?;

        let p = pywr_core::parameters::ThresholdParameter::new(
            self.meta.name.as_str().into(),
            metric,
            threshold,
            self.predicate.into(),
            self.ratchet,
        );
        Ok(network.add_index_parameter(Box::new(p))?)
    }
}

impl TryFromV1<ParameterThresholdParameterV1> for ThresholdParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: ParameterThresholdParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let value = try_convert_parameter_attr(&meta.name, "parameter", v1.parameter, parent_node, conversion_data)?;
        let threshold =
            try_convert_parameter_attr(&meta.name, "threshold", v1.threshold, parent_node, conversion_data)?;

        // TODO warn or something about the lack of using the values here!!

        let p = Self {
            meta,
            value,
            threshold,
            predicate: v1.predicate.into(),
            ratchet: false,
        };
        Ok(p)
    }
}
