use super::interpolate;
use crate::metric::Metric;
use crate::model::Model;
use crate::parameters::{Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;

pub struct PiecewiseInterpolatedParameter {
    meta: ParameterMeta,
    metric: Metric,
    control_curves: Vec<Metric>,
    values: Vec<[f64; 2]>,
    maximum: f64,
    minimum: f64,
}

impl PiecewiseInterpolatedParameter {
    pub fn new(
        name: &str,
        metric: Metric,
        control_curves: Vec<Metric>,
        values: Vec<[f64; 2]>,
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

impl Parameter for PiecewiseInterpolatedParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Model,
        state: &State,
        _internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<f64, PywrError> {
        // Current value
        let x = self.metric.get_value(model, state)?;

        let mut cc_previous_value = self.maximum;
        for (idx, control_curve) in self.control_curves.iter().enumerate() {
            let cc_value = control_curve.get_value(model, state)?;
            if x > cc_value {
                let v = self.values.get(idx).ok_or(PywrError::DataOutOfRange)?;
                return Ok(interpolate(x, cc_value, cc_previous_value, v[1], v[0]));
            }
            cc_previous_value = cc_value;
        }
        let v = self.values.last().ok_or(PywrError::DataOutOfRange)?;
        Ok(interpolate(x, self.minimum, cc_previous_value, v[1], v[0]))
    }
}
