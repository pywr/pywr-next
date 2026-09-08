use crate::metric::{MetricConsumerPhase, MetricF64, MetricU64, UnresolvedMetricF64, UnresolvedMetricU64};
use crate::network::ResolutionMaps;
use crate::parameters::errors::GeneralCalculationError;
use crate::parameters::{
    BuiltParameter, GeneralBeforeParameter, GeneralAfterParameter, GeneralParameter, GeneralParameterContext, GeneralParameterEntry,
    MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterMeta, ParameterName,
    ParameterState,
};
use crate::{resolve_metric_f64_vec, resolve_metric_u64};

#[derive(Debug)]
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

impl GeneralParameter for IndexedArrayParameter {
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralBeforeParameter<f64> for IndexedArrayParameter {
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, GeneralCalculationError> {
        let index = self.index_parameter.get_value(ctx.network, ctx.state)? as usize;

        let metric = self
            .metrics
            .get(index)
            .ok_or(GeneralCalculationError::OutOfBoundsError {
                index,
                length: self.metrics.len(),
                axis: 0,
            })?;

        Ok(metric.get_value(ctx.network, ctx.state)?)
    }
}

impl GeneralAfterParameter<f64> for IndexedArrayParameter {
    fn after(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, GeneralCalculationError> {
        let index = self.index_parameter.get_value(ctx.network, ctx.state)? as usize;
        let metric = self
            .metrics
            .get(index)
            .ok_or(GeneralCalculationError::OutOfBoundsError {
                index,
                length: self.metrics.len(),
                axis: 0,
            })?;
        Ok(metric.get_value(ctx.network, ctx.state)?)
    }
}


/// Builder for creating an [`IndexedArrayParameter`].
#[derive(Debug)]
pub struct IndexedArrayParameterBuilder {
    meta: ParameterMeta,
    index_parameter: UnresolvedMetricU64,
    metrics: Vec<UnresolvedMetricF64>,
    phase: MetricConsumerPhase,
}

impl IndexedArrayParameterBuilder {
    /// Create a new builder for [`IndexedArrayParameter`] that is evaluated in "before" phase.
    pub fn before(name: ParameterName, index_parameter: UnresolvedMetricU64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            index_parameter,
            metrics: Vec::new(),
            phase: MetricConsumerPhase::Before,
        }
    }

    /// Create a new builder for [`IndexedArrayParameter`] that is evaluated in "after" phase.
    pub fn after(name: ParameterName, index_parameter: UnresolvedMetricU64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            index_parameter,
            metrics: Vec::new(),
            phase: MetricConsumerPhase::After,
        }
    }

    /// Create a new builder for [`IndexedArrayParameter`] that is evaluated in both "before" and "after" phases.
    pub fn both(name: ParameterName, index_parameter: UnresolvedMetricU64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            index_parameter,
            metrics: Vec::new(),
            phase: MetricConsumerPhase::Both,
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

        let index_parameter =
            resolve_metric_u64!(self, self.index_parameter, resolution_maps, self.phase, "index_parameter");
        let metrics = resolve_metric_f64_vec!(self, &self.metrics, resolution_maps, self.phase, "metrics");

        let p = IndexedArrayParameter {
            meta: self.meta,
            index_parameter,
            metrics,
        };

        let built = match self.phase {
            MetricConsumerPhase::Before => {
                BuiltParameter::General(GeneralParameterEntry::before(p))
            },
            MetricConsumerPhase::After => {
                BuiltParameter::General(GeneralParameterEntry::after(p))
            },
            MetricConsumerPhase::Both => {
                BuiltParameter::General(GeneralParameterEntry::both(p))
            }
        };

        Ok(built.into())
    }
}
