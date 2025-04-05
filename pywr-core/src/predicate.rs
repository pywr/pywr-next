use crate::PywrError;
use std::str::FromStr;

#[derive(Debug, Clone)]
pub enum Predicate {
    LessThan,
    GreaterThan,
    EqualTo,
    LessThanOrEqualTo,
    GreaterThanOrEqualTo,
}

impl Predicate {
    pub fn apply(&self, a: f64, b: f64) -> bool {
        match self {
            Predicate::LessThan => a < b,
            Predicate::GreaterThan => a > b,
            Predicate::EqualTo => (a - b).abs() < 1E-6, // TODO make this a global constant
            Predicate::LessThanOrEqualTo => a <= b,
            Predicate::GreaterThanOrEqualTo => a >= b,
        }
    }
}

impl FromStr for Predicate {
    type Err = PywrError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "<" => Ok(Self::LessThan),
            ">" => Ok(Self::GreaterThan),
            "=" => Ok(Self::EqualTo),
            "<=" => Ok(Self::LessThanOrEqualTo),
            ">=" => Ok(Self::GreaterThanOrEqualTo),
            _ => Err(PywrError::InvalidAggregationFunction(name.to_string())),
        }
    }
}
