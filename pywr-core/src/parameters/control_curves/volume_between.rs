use crate::metric::SimpleMetricF64;
use crate::parameters::errors::SimpleCalculationError;
use crate::parameters::{Parameter, ParameterMeta, ParameterName, ParameterState, SimpleParameter};
use crate::scenario::ScenarioIndex;
use crate::state::SimpleParameterValues;
use crate::timestep::Timestep;

/// A parameter that returns the volume that is the proportion between two control curves
pub struct VolumeBetweenControlCurvesParameter<M> {
    meta: ParameterMeta,
    total: M,
    upper: Option<M>,
    lower: Option<M>,
}

impl<M> VolumeBetweenControlCurvesParameter<M> {
    pub fn new(name: ParameterName, total: M, upper: Option<M>, lower: Option<M>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            total,
            upper,
            lower,
        }
    }
}

impl<M> Parameter for VolumeBetweenControlCurvesParameter<M>
where
    M: Send + Sync,
{
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl SimpleParameter<f64> for VolumeBetweenControlCurvesParameter<SimpleMetricF64> {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        values: &SimpleParameterValues,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, SimpleCalculationError> {
        let total = self.total.get_value(values)?;

        let lower = self.lower.as_ref().map_or(Ok(0.0), |metric| metric.get_value(values))?;
        let upper = self.upper.as_ref().map_or(Ok(1.0), |metric| metric.get_value(values))?;

        Ok(total * (upper - lower))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}
