use crate::metric::MetricF64;
use crate::network::Network;
use crate::parameters::errors::ParameterCalculationError;
use crate::parameters::interpolate::linear_interpolation;
use crate::parameters::{GeneralParameter, Parameter, ParameterMeta, ParameterName, ParameterState};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;

/// A parameter that interpolates a value to a function with given discrete data points.
pub struct InterpolatedParameter {
    meta: ParameterMeta,
    x: MetricF64,
    points: Vec<(MetricF64, MetricF64)>,
    error_on_bounds: bool,
}

impl InterpolatedParameter {
    pub fn new(name: ParameterName, x: MetricF64, points: Vec<(MetricF64, MetricF64)>, error_on_bounds: bool) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            x,
            points,
            error_on_bounds,
        }
    }
}
impl Parameter for InterpolatedParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}
impl GeneralParameter<f64> for InterpolatedParameter {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, ParameterCalculationError> {
        // Current value
        let x = self.x.get_value(network, state)?;

        let points = self
            .points
            .iter()
            .map(|(x, f)| {
                let xp = x.get_value(network, state)?;
                let fp = f.get_value(network, state)?;

                Ok::<(f64, f64), ParameterCalculationError>((xp, fp))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let f = linear_interpolation(x, &points, self.error_on_bounds)?;

        Ok(f)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}
