use crate::metric::Metric;
use crate::network::Network;
use crate::parameters::interpolate::interpolate;
use crate::parameters::{Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{ParameterState, State};
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;

pub struct ControlCurveInterpolatedParameter {
    meta: ParameterMeta,
    metric: Metric,
    control_curves: Vec<Metric>,
    values: Vec<Metric>,
}

impl ControlCurveInterpolatedParameter {
    pub fn new(name: &str, metric: Metric, control_curves: Vec<Metric>, values: Vec<Metric>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves,
            values,
        }
    }
}

impl Parameter<f64> for ControlCurveInterpolatedParameter {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, PywrError> {
        // Current value
        let x = self.metric.get_value(model, state)?;

        let mut cc_prev = 1.0;
        for (idx, control_curve) in self.control_curves.iter().enumerate() {
            let cc_value = control_curve.get_value(model, state)?;

            if x >= cc_value {
                let lower_value = self.values[idx + 1].get_value(model, state)?;
                let upper_value = self.values[idx].get_value(model, state)?;

                return Ok(interpolate(x, cc_value, cc_prev, lower_value, upper_value));
            }

            cc_prev = cc_value
        }

        let cc_value = 0.0;
        let n = self.values.len();

        let lower_value = self.values[n - 1].get_value(model, state)?;
        let upper_value = self.values[n - 2].get_value(model, state)?;

        Ok(interpolate(x, cc_value, cc_prev, lower_value, upper_value))
    }
}
