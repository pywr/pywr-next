use crate::metric::MetricF64;
use crate::network::Network;
use crate::parameters::{Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{ParameterState, State};
use crate::timestep::Timestep;
use crate::PywrError;

pub struct ControlCurveIndexParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    control_curves: Vec<MetricF64>,
}

impl ControlCurveIndexParameter {
    pub fn new(name: &str, metric: MetricF64, control_curves: Vec<MetricF64>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves,
        }
    }
}

impl Parameter<usize> for ControlCurveIndexParameter {
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
    ) -> Result<usize, PywrError> {
        // Current value
        let x = self.metric.get_value(model, state)?;

        for (idx, control_curve) in self.control_curves.iter().enumerate() {
            let cc_value = control_curve.get_value(model, state)?;
            if x >= cc_value {
                return Ok(idx);
            }
        }
        Ok(self.control_curves.len())
    }
}
