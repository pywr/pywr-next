use crate::metric::Metric;
use crate::model::Model;
use crate::parameters::{ParameterMeta, _IndexParameter};
use crate::scenario::ScenarioIndex;
use crate::state::{NetworkState, ParameterState};
use crate::timestep::Timestep;
use crate::PywrError;

pub struct ControlCurveIndexParameter {
    meta: ParameterMeta,
    metric: Metric,
    control_curves: Vec<Metric>,
}

impl ControlCurveIndexParameter {
    pub fn new(name: &str, metric: Metric, control_curves: Vec<Metric>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves,
        }
    }
}

impl _IndexParameter for ControlCurveIndexParameter {
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
    ) -> Result<usize, PywrError> {
        // Current value
        let x = self.metric.get_value(model, state, parameter_state)?;

        for (idx, control_curve) in self.control_curves.iter().enumerate() {
            let cc_value = control_curve.get_value(model, state, parameter_state)?;
            if x >= cc_value {
                return Ok(idx);
            }
        }
        Ok(self.control_curves.len())
    }
}
