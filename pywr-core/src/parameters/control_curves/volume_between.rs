use crate::metric::Metric;
use crate::network::Network;
use crate::parameters::{Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;

/// A parameter that returns the volume that is the proportion between two control curves
pub struct VolumeBetweenControlCurvesParameter {
    meta: ParameterMeta,
    total: Metric,
    upper: Option<Metric>,
    lower: Option<Metric>,
}

impl VolumeBetweenControlCurvesParameter {
    pub fn new(name: &str, total: Metric, upper: Option<Metric>, lower: Option<Metric>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            total,
            upper,
            lower,
        }
    }
}

impl Parameter for VolumeBetweenControlCurvesParameter {
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
        network: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<f64, PywrError> {
        let total = self.total.get_value(network, state)?;

        let lower = self
            .lower
            .as_ref()
            .map_or(Ok(0.0), |metric| metric.get_value(network, state))?;
        let upper = self
            .upper
            .as_ref()
            .map_or(Ok(1.0), |metric| metric.get_value(network, state))?;

        Ok(total * (upper - lower))
    }
}
