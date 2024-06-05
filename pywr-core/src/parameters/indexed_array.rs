use crate::metric::{MetricF64, MetricUsize};
use crate::network::Network;
use crate::parameters::{GeneralParameter, Parameter, ParameterMeta, ParameterState};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;

pub struct IndexedArrayParameter {
    meta: ParameterMeta,
    index_parameter: MetricUsize,
    metrics: Vec<MetricF64>,
}

impl IndexedArrayParameter {
    pub fn new(name: &str, index_parameter: MetricUsize, metrics: &[MetricF64]) -> Self {
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
    ) -> Result<f64, PywrError> {
        let index = self.index_parameter.get_value(network, state)?;

        let metric = self.metrics.get(index).ok_or(PywrError::DataOutOfRange)?;

        metric.get_value(network, state)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}
