use super::interpolate;
use crate::metric::Metric;
use crate::parameters::{Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{NetworkState, ParameterState};
use crate::timestep::Timestep;
use crate::PywrError;

pub struct PiecewiseInterpolatedParameter {
    meta: ParameterMeta,
    metric: Metric,
    control_curves: Vec<Metric>,
    values: Vec<[f64; 2]>,
    maximum: f64,
    minimum: f64,
}

impl PiecewiseInterpolatedParameter {
    pub fn new(
        name: &str,
        metric: Metric,
        control_curves: Vec<Metric>,
        values: Vec<[f64; 2]>,
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

impl Parameter for PiecewiseInterpolatedParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &mut self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<f64, PywrError> {
        // Current value
        let x = self.metric.get_value(state, parameter_state)?;

        let mut cc_previous_value = self.maximum;
        for (idx, control_curve) in self.control_curves.iter().enumerate() {
            let cc_value = control_curve.get_value(state, parameter_state)?;
            if x > cc_value {
                let v = self.values.get(idx).ok_or(PywrError::DataOutOfRange)?;
                return Ok(interpolate(x, cc_value, cc_previous_value, v[1], v[0]));
            }
            cc_previous_value = cc_value;
        }
        let v = self.values.last().ok_or(PywrError::DataOutOfRange)?;
        Ok(interpolate(x, self.minimum, cc_previous_value, v[1], v[0]))
    }
}