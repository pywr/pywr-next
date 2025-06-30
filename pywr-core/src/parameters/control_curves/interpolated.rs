use crate::metric::MetricF64;
use crate::network::Network;
use crate::parameters::errors::ParameterCalculationError;
use crate::parameters::interpolate::interpolate;
use crate::parameters::{GeneralParameter, Parameter, ParameterMeta, ParameterName, ParameterState};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;

pub struct ControlCurveInterpolatedParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    control_curves: Vec<MetricF64>,
    values: Vec<MetricF64>,
}

impl ControlCurveInterpolatedParameter {
    pub fn new(name: ParameterName, metric: MetricF64, control_curves: Vec<MetricF64>, values: Vec<MetricF64>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves,
            values,
        }
    }
}

impl Parameter for ControlCurveInterpolatedParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter<f64> for ControlCurveInterpolatedParameter {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, ParameterCalculationError> {
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

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}
