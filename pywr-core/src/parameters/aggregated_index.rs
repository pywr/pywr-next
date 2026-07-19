/// AggregatedIndexParameter
///
use super::{
    BuiltParameter, ConstParameter, GeneralAfterParameter, GeneralBeforeParameter, GeneralParameterContext,
    GeneralParameterEntry, MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterName,
    ParameterState, SimpleParameter, SimpleParameterContext,
};
use crate::agg_funcs::AggFuncU64;
use crate::metric::{
    ConstantMetricU64, MetricU64, SimpleMetricU64, UnresolvedMetricU64, try_into_constant_metrics_u64,
    try_into_simple_metrics_u64,
};
use crate::network::ResolutionMaps;
use crate::parameters::errors::{ConstCalculationError, GeneralCalculationError, SimpleCalculationError};
use crate::parameters::{GeneralParameter, ParameterMeta};
use crate::resolve_metric_u64_vec;
use crate::scenario::ScenarioIndex;
use crate::state::ConstParameterValues;
use std::fmt::Debug;

#[derive(Debug)]
pub struct AggregatedIndexParameter<M> {
    meta: ParameterMeta,
    metrics: Vec<M>,
    agg_func: AggFuncU64,
}

impl<M> Parameter for AggregatedIndexParameter<M>
where
    M: Send + Sync + Debug,
{
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter for AggregatedIndexParameter<MetricU64> {
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralBeforeParameter<u64> for AggregatedIndexParameter<MetricU64> {
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, GeneralCalculationError> {
        let values = self
            .metrics
            .iter()
            .map(|p| p.get_value(ctx.network, ctx.state))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(self.agg_func.calc_iter_u64(&values)?)
    }
}

impl GeneralAfterParameter<u64> for AggregatedIndexParameter<MetricU64> {
    fn after(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, GeneralCalculationError> {
        let values = self
            .metrics
            .iter()
            .map(|p| p.get_value(ctx.network, ctx.state))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(self.agg_func.calc_iter_u64(&values)?)
    }
}

impl SimpleParameter<u64> for AggregatedIndexParameter<SimpleMetricU64> {
    fn compute(
        &self,
        ctx: SimpleParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, SimpleCalculationError> {
        let values = self
            .metrics
            .iter()
            .map(|p| p.get_value(ctx.values))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(self.agg_func.calc_iter_u64(&values)?)
    }
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl ConstParameter<u64> for AggregatedIndexParameter<ConstantMetricU64> {
    fn compute(
        &self,
        _scenario_index: &ScenarioIndex,
        values: &ConstParameterValues,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, ConstCalculationError> {
        let values = self
            .metrics
            .iter()
            .map(|p| p.get_value(values))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(self.agg_func.calc_iter_u64(&values)?)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

#[derive(Debug)]
pub struct AggregatedIndexParameterBuilder {
    meta: ParameterMeta,
    metrics: Vec<UnresolvedMetricU64>,
    agg_func: AggFuncU64,
}

impl AggregatedIndexParameterBuilder {
    pub fn new(name: ParameterName, agg_func: AggFuncU64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metrics: Vec::new(),
            agg_func,
        }
    }

    /// Add a new metric to the builder
    pub fn metric(&mut self, metric: UnresolvedMetricU64) -> &mut Self {
        self.metrics.push(metric);
        self
    }
}

impl ParameterBuilder<u64> for AggregatedIndexParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }
    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<u64>, ParameterBuildError> {
        let metrics = resolve_metric_u64_vec!(self, &self.metrics, resolution_maps, "metrics");

        let meta = self.meta;
        let agg_func = self.agg_func;

        // Try the narrowest dependency class first.
        if let Some(metrics) = try_into_constant_metrics_u64(&metrics) {
            return Ok(
                BuiltParameter::Const(Box::new(AggregatedIndexParameter::<ConstantMetricU64> {
                    meta,
                    metrics,
                    agg_func,
                }))
                .into(),
            );
        }

        if let Some(metrics) = try_into_simple_metrics_u64(&metrics) {
            return Ok(
                BuiltParameter::Simple(Box::new(AggregatedIndexParameter::<SimpleMetricU64> {
                    meta,
                    metrics,
                    agg_func,
                }))
                .into(),
            );
        }

        Ok(
            BuiltParameter::General(GeneralParameterEntry::both(AggregatedIndexParameter::<MetricU64> {
                meta,
                metrics,
                agg_func,
            }))
            .into(),
        )
    }
}
