use super::interpolate;
use crate::metric::Metric;
use crate::model::Model;
use crate::parameters::{Parameter, ParameterMeta, _Parameter};
use crate::scenario::ScenarioIndex;
use crate::state::{NetworkState, ParameterState};
use crate::timestep::Timestep;
use crate::PywrError;

pub struct InterpolatedParameter {
    meta: ParameterMeta,
    metric: Metric,
    control_curves: Vec<Metric>,
    values: Vec<f64>,
    maximum: f64,
    minimum: f64,
}

impl InterpolatedParameter {
    pub fn new(
        name: &str,
        metric: Metric,
        control_curves: Vec<Metric>,
        values: Vec<f64>,
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

        let mut cc_previous_value = self.maximum;
        for (idx, control_curve) in self.control_curves.iter().enumerate() {
            let cc_value = control_curve.get_value(model, state, parameter_state)?;
            if x > cc_value {
                let upper_value = self.values.get(idx).ok_or(PywrError::DataOutOfRange)?;
                let lower_value = self.values.get(idx + 1).ok_or(PywrError::DataOutOfRange)?;
                return Ok(interpolate(x, cc_value, cc_previous_value, *lower_value, *upper_value));
            }
            cc_previous_value = cc_value;
        }
        let upper_value = self
            .values
            .get(self.values.len() - 2)
            .ok_or(PywrError::DataOutOfRange)?;
        let lower_value = self.values.last().ok_or(PywrError::DataOutOfRange)?;

        Ok(interpolate(x, 0.0, cc_previous_value, *lower_value, *upper_value))
    }
}
