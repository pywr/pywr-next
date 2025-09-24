use thiserror::Error;

/// Interpolate a value between two bounds.
pub fn interpolate(value: f64, lower_bound: f64, upper_bound: f64, lower_value: f64, upper_value: f64) -> f64 {
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

#[derive(Error, Debug, PartialEq, Eq)]
pub enum InterpolationError {
    #[error("At least 2 points are required for interpolation")]
    InsufficientPoints,
    #[error("The")]
    InconsistentPoints,
    #[error("Value below lower bounds")]
    BelowLowerBounds,
    #[error("Value above upper bounds")]
    AboveUpperBounds,
    #[error("Points are not strictly monotonic")]
    NotStrictlyMonotonic,
}

pub fn linear_interpolation(
    value: f64,
    points: &[(f64, f64)],
    error_on_bounds: bool,
) -> Result<f64, InterpolationError> {
    if points.len() < 2 {
        return Err(InterpolationError::InsufficientPoints);
    }

    // Handle lower bounds checking
    if value < points[0].0 {
        return if error_on_bounds {
            Err(InterpolationError::BelowLowerBounds)
        } else {
            Ok(points[0].1)
        };
    }

    for pts in points.windows(2) {
        let lp = pts[0];
        let up = pts[1];

        if lp.0 >= up.0 {
            return Err(InterpolationError::NotStrictlyMonotonic);
        }

        if value <= up.0 {
            return Ok(interpolate(value, lp.0, up.0, lp.1, up.1));
        }
    }

    if error_on_bounds {
        Err(InterpolationError::AboveUpperBounds)
    } else {
        Ok(points
            .last()
            .expect("This should be impossible because fp has been checked for a length of at least 2")
            .1)
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

    #[test]
    fn test_linear_interpolation() {
        let points = vec![(1.0, 3.0), (2.0, 4.5), (3.0, 6.0), (4.0, 7.5), (5.0, 9.0)];

        assert_approx_eq!(f64, linear_interpolation(1.0, &points, true).unwrap(), 3.0);
        assert_approx_eq!(f64, linear_interpolation(0.5, &points, false).unwrap(), 3.0);
        assert_approx_eq!(f64, linear_interpolation(2.5, &points, false).unwrap(), 5.25);
        assert_approx_eq!(f64, linear_interpolation(5.0, &points, false).unwrap(), 9.0);
        assert_approx_eq!(f64, linear_interpolation(5.5, &points, false).unwrap(), 9.0);

        // Check errors
        assert!(linear_interpolation(0.0, &points, true).is_err());
        assert!(linear_interpolation(6.0, &points, true).is_err());

        let not_enough_points = vec![(1.0, 3.0)];
        assert!(linear_interpolation(1.0, &not_enough_points, true).is_err());

        let non_monotonic_points = vec![(1.0, 3.0), (2.0, 4.5), (2.0, 6.0), (4.0, 7.5), (5.0, 9.0)];
        assert!(linear_interpolation(3.0, &non_monotonic_points, true).is_err());
    }
}
