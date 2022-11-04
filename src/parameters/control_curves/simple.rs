use crate::metric::Metric;
use crate::model::Model;
use crate::parameters::{ParameterMeta, _Parameter};
use crate::scenario::ScenarioIndex;
use crate::state::{NetworkState, ParameterState};
use crate::timestep::Timestep;
use crate::PywrError;

pub struct ControlCurveParameter {
    meta: ParameterMeta,
    metric: Metric,
    control_curves: Vec<Metric>,
    values: Vec<Metric>,
}

impl ControlCurveParameter {
    pub fn new(name: &str, metric: Metric, control_curves: Vec<Metric>, values: Vec<Metric>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves,
            values,
        }
    }
}

impl _Parameter for ControlCurveParameter {
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

        for (idx, control_curve) in self.control_curves.iter().enumerate() {
            let cc_value = control_curve.get_value(model, state, parameter_state)?;
            if x >= cc_value {
                let value = self.values.get(idx).ok_or_else(|| PywrError::DataOutOfRange)?;
                return Ok(value.get_value(model, state, parameter_state)?);
            }
        }

        let value = self.values.last().ok_or_else(|| PywrError::DataOutOfRange)?;
        return Ok(value.get_value(model, state, parameter_state)?);
    }
}
