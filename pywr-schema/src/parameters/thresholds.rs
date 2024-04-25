use crate::error::ConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::parameters::{
    DynamicFloatValueType, IntoV2Parameter, ParameterMeta, TryFromV1Parameter, TryIntoV2Parameter,
};
#[cfg(feature = "core")]
use pywr_core::parameters::ParameterIndex;
use pywr_v1_schema::parameters::{
    ParameterThresholdParameter as ParameterThresholdParameterV1, Predicate as PredicateV1,
};
use schemars::JsonSchema;
use std::collections::HashMap;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy, JsonSchema)]
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

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema)]
pub struct ParameterThresholdParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub parameter: Metric,
    pub threshold: Metric,
    pub predicate: Predicate,
    #[serde(default)]
    pub ratchet: bool,
}

impl ParameterThresholdParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }
    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        todo!()
    }
}

#[cfg(feature = "core")]
impl ParameterThresholdParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<ParameterIndex<usize>, SchemaError> {
        let metric = self.parameter.load(network, args)?;
        let threshold = self.threshold.load(network, args)?;

        let p = pywr_core::parameters::ThresholdParameter::new(
            &self.meta.name,
            metric,
            threshold,
            self.predicate.into(),
            self.ratchet,
        );
        Ok(network.add_index_parameter(Box::new(p))?)
    }
}

impl TryFromV1Parameter<ParameterThresholdParameterV1> for ParameterThresholdParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: ParameterThresholdParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);

        let parameter = v1.parameter.try_into_v2_parameter(Some(&meta.name), unnamed_count)?;
        let threshold = v1.threshold.try_into_v2_parameter(Some(&meta.name), unnamed_count)?;

        // TODO warn or something about the lack of using the values here!!

        let p = Self {
            meta,
            parameter,
            threshold,
            predicate: v1.predicate.into(),
            ratchet: false,
        };
        Ok(p)
    }
}
