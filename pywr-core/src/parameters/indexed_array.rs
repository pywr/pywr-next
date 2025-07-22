use crate::metric::{MetricF64, MetricU64};
use crate::network::Network;
use crate::parameters::errors::ParameterCalculationError;
use crate::parameters::{GeneralParameter, Parameter, ParameterMeta, ParameterName, ParameterState};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;

pub struct IndexedArrayParameter {
    meta: ParameterMeta,
    index_parameter: MetricU64,
    metrics: Vec<MetricF64>,
}

impl IndexedArrayParameter {
    pub fn new(name: ParameterName, index_parameter: MetricU64, metrics: &[MetricF64]) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            index_parameter,
            metrics: metrics.to_vec(),
        }
    }
}

impl Parameter for IndexedArrayParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter<f64> for IndexedArrayParameter {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, ParameterCalculationError> {
        let index = self.index_parameter.get_value(network, state)? as usize;

        let metric = self
            .metrics
            .get(index)
            .ok_or(ParameterCalculationError::OutOfBoundsError {
                index,
                length: self.metrics.len(),
                axis: 0,
            })?;

        Ok(metric.get_value(network, state)?)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}
