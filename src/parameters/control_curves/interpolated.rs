use super::interpolate;
use crate::metric::Metric;
use crate::model::Model;
use crate::parameters::{ParameterMeta, _Parameter};
use crate::scenario::ScenarioIndex;
use crate::state::{NetworkState, ParameterState};
use crate::timestep::Timestep;
use crate::PywrError;

pub struct InterpolatedParameter {
    meta: ParameterMeta,
    metric: Metric,
    control_curves: Vec<Metric>,
    values: Vec<f64>,
}

impl InterpolatedParameter {
    pub fn new(name: &str, metric: Metric, control_curves: Vec<Metric>, values: Vec<f64>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves,
            values,
        }
    }
}

impl _Parameter for InterpolatedParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &mut self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Model,
        state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<f64, PywrError> {
        // Current value
        let x = self.metric.get_value(model, state, parameter_state)?;

        let mut cc_prev = 1.0;
        for (idx, control_curve) in self.control_curves.iter().enumerate() {
            let cc_value = control_curve.get_value(model, state, parameter_state)?;

            if x >= cc_value {
                return Ok(interpolate(
                    x,
                    cc_value,
                    cc_prev,
                    self.values[idx + 1],
                    self.values[idx],
                ));
            }

            cc_prev = cc_value
        }

        let cc_value = 0.0;
        let n = self.values.len();
        Ok(interpolate(
            x,
            cc_value,
            cc_prev,
            self.values[n - 1],
            self.values[n - 2],
        ))
    }
}
