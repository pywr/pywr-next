use crate::metric::MetricF64;
use crate::network::Network;
use crate::parameters::errors::ParameterCalculationError;
use crate::parameters::{GeneralParameter, Parameter, ParameterMeta, ParameterName, ParameterState};
use crate::scenario::ScenarioIndex;
use crate::state::{MultiValue, State};
use crate::timestep::Timestep;
use std::collections::HashMap;

/// A parameter which divides a apportions a metric to an upper and lower amount based
/// on the current value of a control curve.
///
/// The control curve is expected to produce values between 0.0 and 1.0. If the control curve
/// returns a value outside of this range it is "clamped" to it. The upper amount
/// is equal to `(1.0 - control_curve) * metric` and the lower amount is equal to
/// `control_curve * metric`.
///
pub struct ApportionParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    control_curve: MetricF64,
}

impl ApportionParameter {
    pub fn new(name: ParameterName, metric: MetricF64, control_curve: MetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curve,
        }
    }
}

impl Parameter for ApportionParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter<MultiValue> for ApportionParameter {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<MultiValue, ParameterCalculationError> {
        // Current value
        let x = self.metric.get_value(model, state)?;

        // Get the control curve value and force it
        let control_curve = self.control_curve.get_value(model, state)?.clamp(0.0, 1.0);

        let upper = (1.0 - control_curve) * x;
        let lower = control_curve * x;

        let values = HashMap::from([("upper".to_string(), upper), ("lower".to_string(), lower)]);

        let value = MultiValue::new(values, HashMap::new());
        Ok(value)
    }
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}
