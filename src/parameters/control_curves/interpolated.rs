use super::interpolate;
use crate::metric::Metric;
use crate::parameters::{Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{NetworkState, ParameterState};
use crate::timestep::Timestep;
use crate::PywrError;

pub struct InterpolatedParameter {
    meta: ParameterMeta,
    metric: Metric,
    control_curves: Vec<Metric>,
    values: Vec<Metric>,
}

impl InterpolatedParameter {
    pub fn new(name: &str, metric: Metric, control_curves: Vec<Metric>, values: Vec<Metric>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves,
            values,
        }
    }
}

impl Parameter for InterpolatedParameter {
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

        let mut cc_prev = 1.0;
        for (idx, control_curve) in self.control_curves.iter().enumerate() {
            let cc_value = control_curve.get_value(state, parameter_state)?;

            if x >= cc_value {
                let lower_value = self.values[idx + 1].get_value(state, parameter_state)?;
                let upper_value = self.values[idx].get_value(state, parameter_state)?;

                return Ok(interpolate(x, cc_value, cc_prev, lower_value, upper_value));
            }

            cc_prev = cc_value
        }

        let cc_value = 0.0;
        let n = self.values.len();

        let lower_value = self.values[n - 1].get_value(state, parameter_state)?;
        let upper_value = self.values[n - 2].get_value(state, parameter_state)?;

        Ok(interpolate(x, cc_value, cc_prev, lower_value, upper_value))
    }
}
