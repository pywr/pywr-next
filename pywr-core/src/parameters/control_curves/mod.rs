mod apportion;
mod index;
mod interpolated;
mod piecewise;
mod simple;
mod volume_between;

pub use apportion::ApportionParameter;
pub use index::ControlCurveIndexParameter;
pub use interpolated::InterpolatedParameter;
pub use piecewise::PiecewiseInterpolatedParameter;
pub use simple::ControlCurveParameter;
pub use volume_between::VolumeBetweenControlCurvesParameter;

/// Interpolate
fn interpolate(value: f64, lower_bound: f64, upper_bound: f64, lower_value: f64, upper_value: f64) -> f64 {
    if value <= lower_bound {
        lower_value
    } else if value >= upper_bound {
        upper_value
    } else if (lower_bound - upper_bound).abs() < 1E-6 {
        lower_value
    } else {
        lower_value + (upper_value - lower_value) * (value - lower_bound) / (upper_bound - lower_bound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use float_cmp::assert_approx_eq;

    #[test]
    fn test_interpolate() {
        // Middle of the range
        assert_approx_eq!(f64, interpolate(0.5, 0.0, 1.0, 0.0, 1.0), 0.5);
        assert_approx_eq!(f64, interpolate(0.25, 0.0, 1.0, 0.0, 1.0), 0.25);
        assert_approx_eq!(f64, interpolate(0.75, 0.0, 1.0, 0.0, 1.0), 0.75);
        // Below bounds; returns lower value
        assert_approx_eq!(f64, interpolate(-1.0, 0.0, 1.0, 0.0, 1.0), 0.0);
        // Above bounds; returns upper value
        assert_approx_eq!(f64, interpolate(2.0, 0.0, 1.0, 0.0, 1.0), 1.0);
        // Equal bounds; returns lower value
        assert_approx_eq!(f64, interpolate(0.0, 0.0, 0.0, 0.0, 1.0), 0.0);
    }
}
