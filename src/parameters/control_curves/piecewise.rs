use crate::metric::Metric;
use crate::parameters::{Parameter, ParameterMeta, _Parameter};
use crate::scenario::ScenarioIndex;
use crate::state::NetworkState;
use crate::timestep::Timestep;
use crate::PywrError;

pub struct PiecewiseInterpolatedParameter {
    meta: ParameterMeta,
    metric: Metric,
    control_curves: Vec<Metric>,
    values: Vec<(f64, f64)>,
    maximum: f64,
    minimum: f64,
}

impl PiecewiseInterpolatedParameter {
    pub fn new(
        name: &str,
        metric: Metric,
        control_curves: Vec<Metric>,
        values: Vec<(f64, f64)>,
        maximum: f64,
        minimum: f64,
    ) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves,
            values,
            maximum,
            minimum,
        }
    }
}

impl _Parameter for PiecewiseInterpolatedParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        state: &NetworkState,
        parameter_state: &[f64],
    ) -> Result<f64, PywrError> {
        // Current value
        let x = self.metric.get_value(state, parameter_state)?;

        let mut cc_previous_value = self.maximum;
        for (idx, control_curve) in self.control_curves.iter().enumerate() {
            let cc_value = control_curve.get_value(state, parameter_state)?;
            if x > cc_value {
                let (upper_value, lower_value) = self.values.get(idx).ok_or(PywrError::DataOutOfRange)?;
                return Ok(interpolate(x, cc_value, cc_previous_value, *lower_value, *upper_value));
            }
            cc_previous_value = cc_value;
        }
        let (upper_value, lower_value) = self.values.last().ok_or(PywrError::DataOutOfRange)?;
        Ok(interpolate(
            x,
            self.minimum,
            cc_previous_value,
            *lower_value,
            *upper_value,
        ))
    }
}

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
    use crate::assert_almost_eq;

    #[test]
    fn test_interpolate() {
        // Middle of the range
        assert_almost_eq!(interpolate(0.5, 0.0, 1.0, 0.0, 1.0), 0.5);
        assert_almost_eq!(interpolate(0.25, 0.0, 1.0, 0.0, 1.0), 0.25);
        assert_almost_eq!(interpolate(0.75, 0.0, 1.0, 0.0, 1.0), 0.75);
        // Below bounds; returns lower value
        assert_almost_eq!(interpolate(-1.0, 0.0, 1.0, 0.0, 1.0), 0.0);
        // Above bounds; returns upper value
        assert_almost_eq!(interpolate(2.0, 0.0, 1.0, 0.0, 1.0), 1.0);
        // Equal bounds; returns lower value
        assert_almost_eq!(interpolate(0.0, 0.0, 0.0, 0.0, 1.0), 0.0);
    }
}
