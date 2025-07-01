use crate::metric::MetricF64;
use crate::network::Network;
use crate::parameters::errors::ParameterCalculationError;
use crate::parameters::{GeneralParameter, Parameter, ParameterMeta, ParameterName, ParameterState};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;

pub struct ControlCurveParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    control_curves: Vec<MetricF64>,
    values: Vec<MetricF64>,
}

impl ControlCurveParameter {
    pub fn new(name: ParameterName, metric: MetricF64, control_curves: Vec<MetricF64>, values: Vec<MetricF64>) -> Self {
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
    ) -> Result<f64, ParameterCalculationError> {
        // Current value
        let x = self.metric.get_value(model, state)?;

        for (idx, control_curve) in self.control_curves.iter().enumerate() {
            let cc_value = control_curve.get_value(model, state)?;
            if x >= cc_value {
                let value = self
                    .values
                    .get(idx)
                    .ok_or_else(|| ParameterCalculationError::OutOfBoundsError {
                        axis: 0,
                        index: idx,
                        length: self.values.len(),
                    })?;
                return Ok(value.get_value(model, state)?);
            }
        }

        let value = self
            .values
            .last()
            .ok_or_else(|| ParameterCalculationError::OutOfBoundsError {
                axis: 0,
                index: 0,
                length: self.values.len(),
            })?;

        Ok(value.get_value(model, state)?)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}
