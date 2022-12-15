use crate::metric::Metric;
use crate::parameters::{IndexParameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;

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

impl IndexParameter for ControlCurveIndexParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        state: &State,
        _internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<usize, PywrError> {
        // Current value
        let x = self.metric.get_value(state)?;

        for (idx, control_curve) in self.control_curves.iter().enumerate() {
            let cc_value = control_curve.get_value(state)?;
            if x >= cc_value {
                return Ok(idx);
            }
        }
        Ok(self.control_curves.len())
    }
}
