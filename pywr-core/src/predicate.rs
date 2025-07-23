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
