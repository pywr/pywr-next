use crate::schema::data_tables::LoadedTableCollection;
use crate::schema::parameters::{
    DynamicFloatValue, DynamicFloatValueType, IntoV2Parameter, ParameterMeta, TryFromV1Parameter, TryIntoV2Parameter,
};
use crate::{IndexParameterIndex, PywrError};
use pywr_schema::parameters::{ParameterThresholdParameter as ParameterThresholdParameterV1, Predicate as PredicateV1};
use std::collections::HashMap;
use std::path::Path;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy)]
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

impl From<Predicate> for crate::parameters::Predicate {
    fn from(p: Predicate) -> Self {
        match p {
            Predicate::LT => crate::parameters::Predicate::LessThan,
            Predicate::GT => crate::parameters::Predicate::GreaterThan,
            Predicate::EQ => crate::parameters::Predicate::EqualTo,
            Predicate::LE => crate::parameters::Predicate::LessThanOrEqualTo,
            Predicate::GE => crate::parameters::Predicate::GreaterThanOrEqualTo,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct ParameterThresholdParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub parameter: DynamicFloatValue,
    pub threshold: DynamicFloatValue,
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

    pub fn add_to_model(
        &self,
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<IndexParameterIndex, PywrError> {
        let metric = self.parameter.load(model, tables, data_path)?;
        let threshold = self.threshold.load(model, tables, data_path)?;

        let p = crate::parameters::ThresholdParameter::new(
            &self.meta.name,
            metric,
            threshold,
            self.predicate.into(),
            self.ratchet,
        );
        model.add_index_parameter(Box::new(p))
    }
}

impl TryFromV1Parameter<ParameterThresholdParameterV1> for ParameterThresholdParameter {
    type Error = PywrError;

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
