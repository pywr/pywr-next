use super::{Parameter, ParameterName, SimpleParameter};
use crate::metric::{MetricF64, SimpleMetricF64};
use crate::network::Network;
use crate::parameters::errors::{ParameterCalculationError, SimpleCalculationError};
use crate::parameters::{GeneralParameter, ParameterMeta, ParameterState};
use crate::scenario::ScenarioIndex;
use crate::state::{SimpleParameterValues, State};
use crate::timestep::Timestep;

/// A parameter that computes the difference between two metrics, with optional minimum and maximum bounds.
///
/// The calculation is defined as:
/// `result = a - b`, where `a` and `b` are the values of the two metrics.
///
/// If `min` is provided, the result is clamped to be at least `min`.
/// If `max` is provided, the result is clamped to be at most `max`.
pub struct DifferenceParameter<M> {
    meta: ParameterMeta,
    a: M,
    b: M,
    min: Option<M>,
    max: Option<M>,
}

impl<M> DifferenceParameter<M>
where
    M: Send + Sync + Clone,
{
    pub fn new(name: ParameterName, a: M, b: M, min: Option<M>, max: Option<M>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            a,
            b,
            min,
            max,
        }
    }
}
impl<M> Parameter for DifferenceParameter<M>
where
    M: Send + Sync,
{
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}
impl GeneralParameter<f64> for DifferenceParameter<MetricF64> {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, ParameterCalculationError> {
        let a = self.a.get_value(model, state)?;
        let b = self.b.get_value(model, state)?;
        let min = self.min.as_ref().map(|m| m.get_value(model, state)).transpose()?;
        let max = self.max.as_ref().map(|m| m.get_value(model, state)).transpose()?;

        Ok(difference(a, b, min, max))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }

    fn try_into_simple(&self) -> Option<Box<dyn SimpleParameter<f64>>> {
        // We can make a simple version if all metrics can be simplified
        let a: SimpleMetricF64 = self.a.clone().try_into().ok()?;
        let b: SimpleMetricF64 = self.b.clone().try_into().ok()?;
        let min: Option<SimpleMetricF64> = self.min.as_ref().map(|m| m.clone().try_into().ok())?;
        let max: Option<SimpleMetricF64> = self.max.as_ref().map(|m| m.clone().try_into().ok())?;

        Some(Box::new(DifferenceParameter::<SimpleMetricF64> {
            meta: self.meta.clone(),
            a,
            b,
            min,
            max,
        }))
    }
}

impl SimpleParameter<f64> for DifferenceParameter<SimpleMetricF64> {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        values: &SimpleParameterValues,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, SimpleCalculationError> {
        let a = self.a.get_value(values)?;
        let b = self.b.get_value(values)?;
        let min = self.min.as_ref().map(|m| m.get_value(values)).transpose()?;
        let max = self.max.as_ref().map(|m| m.get_value(values)).transpose()?;

        Ok(difference(a, b, min, max))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

/// This function computes the difference between two floating-point numbers,
/// optionally clamping the result to a specified minimum and maximum value.
fn difference(a: f64, b: f64, min: Option<f64>, max: Option<f64>) -> f64 {
    let result = a - b;
    if let Some(min_val) = min {
        if result < min_val {
            return min_val;
        }
    }
    if let Some(max_val) = max {
        if result > max_val {
            return max_val;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::difference;
    use float_cmp::assert_approx_eq;

    #[test]
    fn computes_difference_without_bounds() {
        let result = difference(10.0, 3.0, None, None);
        assert_approx_eq!(f64, result, 7.0);
    }

    #[test]
    fn clamps_to_min_when_result_below_min() {
        let result = difference(2.0, 5.0, Some(-1.0), None);
        assert_approx_eq!(f64, result, -1.0);
    }

    #[test]
    fn clamps_to_max_when_result_above_max() {
        let result = difference(10.0, 3.0, None, Some(5.0));
        assert_approx_eq!(f64, result, 5.0);
    }

    #[test]
    fn clamps_to_min_and_max_when_result_outside_bounds() {
        let result = difference(1.0, 10.0, Some(-5.0), Some(-2.0));
        assert_approx_eq!(f64, result, -5.0);

        let result = difference(10.0, 1.0, Some(2.0), Some(5.0));
        assert_approx_eq!(f64, result, 5.0);
    }

    #[test]
    fn returns_result_when_within_bounds() {
        let result = difference(8.0, 3.0, Some(2.0), Some(10.0));
        assert_approx_eq!(f64, result, 5.0);
    }

    #[test]
    fn handles_equal_a_and_b() {
        let result = difference(5.0, 5.0, None, None);
        assert_approx_eq!(f64, result, 0.0);
    }
}
