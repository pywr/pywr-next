use crate::metric::Metric;
use crate::model::Model;
use crate::parameters::interpolate::linear_interpolation;
use crate::parameters::{Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;

/// A parameter that interpolates a value to a function with given discrete data points.
pub struct InterpolatedParameter {
    meta: ParameterMeta,
    x: Metric,
    points: Vec<(Metric, Metric)>,
    error_on_bounds: bool,
}

impl InterpolatedParameter {
    pub fn new(name: &str, x: Metric, points: Vec<(Metric, Metric)>, error_on_bounds: bool) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            x,
            points,
            error_on_bounds,
        }
    }
}

impl Parameter for InterpolatedParameter {
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
        model: &Model,
        state: &State,
        _internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<f64, PywrError> {
        // Current value
        let x = self.x.get_value(model, state)?;

        let points = self
            .points
            .iter()
            .map(|(x, f)| {
                let xp = x.get_value(model, state)?;
                let fp = f.get_value(model, state)?;

                Ok::<(f64, f64), PywrError>((xp, fp))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let f = linear_interpolation(x, &points, self.error_on_bounds)?;

        Ok(f)
    }
}
