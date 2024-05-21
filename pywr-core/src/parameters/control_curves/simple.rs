use crate::metric::MetricF64;
use crate::network::Network;
use crate::parameters::{GeneralParameter, Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{ParameterState, State};
use crate::timestep::Timestep;
use crate::PywrError;

pub struct ControlCurveParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    control_curves: Vec<MetricF64>,
    values: Vec<MetricF64>,
}

impl ControlCurveParameter {
    pub fn new(name: &str, metric: MetricF64, control_curves: Vec<MetricF64>, values: Vec<MetricF64>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves,
            values,
        }
    }
}

impl Parameter for ControlCurveParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter<f64> for ControlCurveParameter {
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

        for (idx, control_curve) in self.control_curves.iter().enumerate() {
            let cc_value = control_curve.get_value(model, state)?;
            if x >= cc_value {
                let value = self.values.get(idx).ok_or(PywrError::DataOutOfRange)?;
                return value.get_value(model, state);
            }
        }

        let value = self.values.last().ok_or(PywrError::DataOutOfRange)?;
        value.get_value(model, state)
    }
}
