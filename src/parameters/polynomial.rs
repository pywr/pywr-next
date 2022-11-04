use crate::metric::Metric;
use crate::model::Model;
use crate::parameters::{ParameterMeta, _Parameter};
use crate::scenario::ScenarioIndex;
use crate::state::ParameterState;
use crate::timestep::Timestep;
use crate::{NetworkState, PywrError};

pub struct Polynomial1DParameter {
    meta: ParameterMeta,
    metric: Metric,
    coefficients: Vec<f64>,
    scale: f64,
    offset: f64,
}

impl Polynomial1DParameter {
    pub fn new(name: &str, metric: Metric, coefficients: Vec<f64>, scale: f64, offset: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            coefficients,
            scale,
            offset,
        }
    }
}

impl _Parameter for Polynomial1DParameter {
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
        let x = x * self.scale + self.offset;
        // Calculate the polynomial value
        let y = self
            .coefficients
            .iter()
            .enumerate()
            .fold(0.0, |y, (i, c)| y + c * x.powi(i as i32));
        Ok(y)
    }
}
