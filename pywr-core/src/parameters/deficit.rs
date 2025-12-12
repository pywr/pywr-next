use crate::metric::MetricF64;
use crate::network::Network;
use crate::parameters::{
    GeneralParameter, Parameter, ParameterCalculationError, ParameterMeta, ParameterName, ParameterState,
};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;

/// A parameter representing the deficit between a flow metric and a max metric.
///
/// Typically used to represent the deficit between actual inflow and requested max flow at
/// a node.
pub struct DeficitParameter {
    meta: ParameterMeta,
    flow: MetricF64,
    max_flow: MetricF64,
}

impl DeficitParameter {
    pub fn new(name: ParameterName, flow: MetricF64, max_flow: MetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            flow,
            max_flow,
        }
    }
}

impl Parameter for DeficitParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter<f64> for DeficitParameter {
    fn after(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<f64>, ParameterCalculationError> {
        let actual_flow = self.flow.get_value(model, state)?;
        let max_flow = self.max_flow.get_value(model, state)?;

        let deficit = (max_flow - actual_flow).max(0.0);
        Ok(Some(deficit))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}
