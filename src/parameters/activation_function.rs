#[derive(Copy, Clone)]
pub enum ActivationFunction {
    Unit { min: f64, max: f64 },
    Rectifier { min: f64, max: f64, neg_value: f64 },
    BinaryStep { pos_value: f64, neg_value: f64 },
    Logistic { growth_rate: f64, max: f64 },
}

impl ActivationFunction {
    /// Apply the activation function to a given value.
    ///
    /// The function applied depends on the current variant. In all cases the value
    /// is clamped to the lower and upper bounds before application in the function.
    pub fn apply(&self, value: f64) -> f64 {
        let value = value.clamp(self.lower_bound(), self.upper_bound());
        match self {
            Self::Unit { .. } => value,
            Self::Rectifier { max, min, neg_value } => {
                if value <= 0.0 {
                    *neg_value
                } else {
                    min + value * (max - min)
                }
            }
            Self::BinaryStep { pos_value, neg_value } => {
                if value <= 0.0 {
                    *neg_value
                } else {
                    *pos_value
                }
            }
            Self::Logistic { growth_rate, max } => max / (1.0 + (-growth_rate * value).exp()),
        }
    }

    pub fn lower_bound(&self) -> f64 {
        match self {
            Self::Unit { min, .. } => *min,
            Self::Rectifier { .. } => -1.0,
            Self::BinaryStep { .. } => -1.0,
            Self::Logistic { .. } => -6.0,
        }
    }
    pub fn upper_bound(&self) -> f64 {
        match self {
            Self::Unit { max, .. } => *max,
            Self::Rectifier { .. } => 1.0,
            Self::BinaryStep { .. } => 1.0,
            Self::Logistic { .. } => 6.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::parameters::ActivationFunction;
    use float_cmp::assert_approx_eq;

    #[test]
    fn test_unit() {
        let af = ActivationFunction::Unit { min: -10.0, max: 10.0 };

        assert_approx_eq!(f64, af.lower_bound(), -10.0);
        assert_approx_eq!(f64, af.upper_bound(), 10.0);
        assert_approx_eq!(f64, af.apply(0.0), 0.0);
        // Out of range value is clamped
        assert_approx_eq!(f64, af.apply(-20.0), -10.0);
        assert_approx_eq!(f64, af.apply(20.0), 10.0);
    }

    #[test]
    fn test_rectifier() {
        let af = ActivationFunction::Rectifier {
            min: -10.0,
            max: 10.0,
            neg_value: 3.0,
        };

        assert_approx_eq!(f64, af.lower_bound(), -1.0);
        assert_approx_eq!(f64, af.upper_bound(), 1.0);
        assert_approx_eq!(f64, af.apply(0.0), 3.0);
        assert_approx_eq!(f64, af.apply(-0.01), 3.0);
        assert_approx_eq!(f64, af.apply(0.01), -10.0 + 0.01 * 20.0);
        assert_approx_eq!(f64, af.apply(1.0), 10.0);
        assert_approx_eq!(f64, af.apply(0.5), 0.0);
        // Out of range value is clamped
        assert_approx_eq!(f64, af.apply(-20.0), 3.0);
        assert_approx_eq!(f64, af.apply(2.0), 10.0);
    }

    #[test]
    fn test_binary_step() {
        let af = ActivationFunction::BinaryStep {
            neg_value: -10.0,
            pos_value: 10.0,
        };

        assert_approx_eq!(f64, af.lower_bound(), -1.0);
        assert_approx_eq!(f64, af.upper_bound(), 1.0);
        assert_approx_eq!(f64, af.apply(0.0), -10.0);
        assert_approx_eq!(f64, af.apply(-0.01), -10.0);
        assert_approx_eq!(f64, af.apply(0.01), 10.0);
        assert_approx_eq!(f64, af.apply(1.0), 10.0);
        assert_approx_eq!(f64, af.apply(0.5), 10.0);
        // Out of range value is clamped
        assert_approx_eq!(f64, af.apply(-20.0), -10.0);
        assert_approx_eq!(f64, af.apply(2.0), 10.0);
    }
}
