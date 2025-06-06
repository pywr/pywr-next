use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::parameters::Predicate as PredicateV1;
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
impl From<Predicate> for pywr_core::Predicate {
    fn from(p: Predicate) -> Self {
        match p {
            Predicate::LT => pywr_core::Predicate::LessThan,
            Predicate::GT => pywr_core::Predicate::GreaterThan,
            Predicate::EQ => pywr_core::Predicate::EqualTo,
            Predicate::LE => pywr_core::Predicate::LessThanOrEqualTo,
            Predicate::GE => pywr_core::Predicate::GreaterThanOrEqualTo,
        }
    }
}
