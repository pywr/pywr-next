use crate::metric::Metric;
use crate::network::Network;
use crate::parameters::{Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{MultiValue, ParameterState, State};
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;
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
    metric: Metric,
    control_curve: Metric,
}

impl ApportionParameter {
    pub fn new(name: &str, metric: Metric, control_curve: Metric) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curve,
        }
    }
}

impl Parameter<MultiValue> for ApportionParameter {
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
    ) -> Result<MultiValue, PywrError> {
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
}
