use crate::metric::Metric;
use crate::network::Network;
use crate::parameters::{IndexValue, Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{ParameterState, State};
use crate::timestep::Timestep;
use crate::PywrError;

pub struct IndexedArrayParameter {
    meta: ParameterMeta,
    index_parameter: IndexValue,
    metrics: Vec<Metric>,
}

impl IndexedArrayParameter {
    pub fn new(name: &str, index_parameter: IndexValue, metrics: &[Metric]) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            index_parameter,
            metrics: metrics.to_vec(),
        }
    }
}

impl Parameter<f64> for IndexedArrayParameter {
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
    ) -> Result<f64, PywrError> {
        let index = match self.index_parameter {
            IndexValue::Constant(idx) => idx,
            IndexValue::Dynamic(idx) => state.get_parameter_index(idx)?,
        };

        let metric = self.metrics.get(index).ok_or(PywrError::DataOutOfRange)?;

        metric.get_value(model, state)
    }
}
