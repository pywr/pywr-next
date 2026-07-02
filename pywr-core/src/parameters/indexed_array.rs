use crate::metric::{MetricF64, MetricU64, UnresolvedMetricF64, UnresolvedMetricU64};
use crate::network::{Network, ResolutionMaps};
use crate::parameters::errors::GeneralCalculationError;
use crate::parameters::{
    BuiltParameter, GeneralParameter, MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder,
    ParameterMeta, ParameterName, ParameterState,
};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::{resolve_metric_f64_vec, resolve_metric_u64};

pub struct IndexedArrayParameter {
    meta: ParameterMeta,
    index_parameter: MetricU64,
    metrics: Vec<MetricF64>,
}

impl Parameter for IndexedArrayParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter<f64> for IndexedArrayParameter {
    fn before(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<f64>, GeneralCalculationError> {
        let index = self.index_parameter.get_value(network, state)? as usize;

        let metric = self
            .metrics
            .get(index)
            .ok_or(GeneralCalculationError::OutOfBoundsError {
                index,
                length: self.metrics.len(),
                axis: 0,
            })?;

        Ok(Some(metric.get_value(network, state)?))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

pub struct IndexedArrayParameterBuilder {
    meta: ParameterMeta,
    index_parameter: UnresolvedMetricU64,
    metrics: Vec<UnresolvedMetricF64>,
}

impl IndexedArrayParameterBuilder {
    pub fn new(name: ParameterName, index_parameter: UnresolvedMetricU64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            index_parameter,
            metrics: Vec::new(),
        }
    }

    pub fn metric(&mut self, metric: UnresolvedMetricF64) -> &mut Self {
        self.metrics.push(metric);
        self
    }
}

impl ParameterBuilder<f64> for IndexedArrayParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        let index_parameter = resolve_metric_u64!(self, self.index_parameter, resolution_maps, "index_parameter");
        let metrics = resolve_metric_f64_vec!(self, &self.metrics, resolution_maps, "metrics");

        let p = IndexedArrayParameter {
            meta: self.meta,
            index_parameter,
            metrics,
        };

        Ok(MaybeBuiltParameter::Built(BuiltParameter::General(Box::new(p))))
    }
}
