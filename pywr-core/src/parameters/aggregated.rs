use super::{
    BuiltParameter, ConstParameter, GeneralAfterParameter, GeneralBeforeParameter, GeneralParameterContext,
    GeneralParameterEntry, MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterName,
    ParameterState, SimpleAfterParameter, SimpleBeforeParameter, SimpleParameter, SimpleParameterContext,
    SimpleParameterEntry,
};
use crate::agg_funcs::AggFuncF64;
use crate::metric::{
    ConstantMetricF64, MetricF64, SimpleMetricF64, UnresolvedMetricF64, try_into_constant_metrics_f64,
    try_into_simple_metrics_f64,
};
use crate::network::ResolutionMaps;
use crate::parameters::errors::{ConstCalculationError, GeneralCalculationError, SimpleCalculationError};
use crate::parameters::{GeneralParameter, ParameterMeta};
use crate::resolve_metric_f64_vec;
use crate::scenario::ScenarioIndex;
use crate::state::ConstParameterValues;
use std::fmt::Debug;

#[derive(Debug)]
pub struct AggregatedParameter<M> {
    meta: ParameterMeta,
    metrics: Vec<M>,
    agg_func: AggFuncF64,
}

impl<M> Parameter for AggregatedParameter<M>
where
    M: Send + Sync + Debug,
{
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter for AggregatedParameter<MetricF64> {
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralBeforeParameter<f64> for AggregatedParameter<MetricF64> {
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, GeneralCalculationError> {
        let values = self
            .metrics
            .iter()
            .map(|p| p.get_value(ctx.network, ctx.state))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(self.agg_func.calc_iter_f64(&values)?)
    }
}

impl GeneralAfterParameter<f64> for AggregatedParameter<MetricF64> {
    fn after(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, GeneralCalculationError> {
        let values = self
            .metrics
            .iter()
            .map(|p| p.get_value(ctx.network, ctx.state))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(self.agg_func.calc_iter_f64(&values)?)
    }
}

impl<M> SimpleParameter for AggregatedParameter<M>
where
    M: Send + Sync + Debug,
{
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl SimpleBeforeParameter<f64> for AggregatedParameter<SimpleMetricF64> {
    fn before(
        &self,
        ctx: SimpleParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, SimpleCalculationError> {
        let values = self
            .metrics
            .iter()
            .map(|p| p.get_value(ctx.values))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(self.agg_func.calc_iter_f64(&values)?)
    }
}

impl SimpleAfterParameter<f64> for AggregatedParameter<SimpleMetricF64> {
    fn after(
        &self,
        ctx: SimpleParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, SimpleCalculationError> {
        let values = self
            .metrics
            .iter()
            .map(|p| p.get_value(ctx.values))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(self.agg_func.calc_iter_f64(&values)?)
    }
}

impl ConstParameter<f64> for AggregatedParameter<ConstantMetricF64> {
    fn compute(
        &self,
        _scenario_index: &ScenarioIndex,
        values: &ConstParameterValues,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, ConstCalculationError> {
        let values = self
            .metrics
            .iter()
            .map(|p| p.get_value(values))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(self.agg_func.calc_iter_f64(&values)?)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

#[derive(Debug)]
pub struct AggregatedParameterBuilder {
    meta: ParameterMeta,
    agg_func: AggFuncF64,
    metrics: Vec<UnresolvedMetricF64>,
}

impl AggregatedParameterBuilder {
    pub fn new(name: ParameterName, agg_func: AggFuncF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metrics: Vec::new(),
            agg_func,
        }
    }

    /// Add a new metric to the builder
    pub fn metric(&mut self, metric: UnresolvedMetricF64) -> &mut Self {
        self.metrics.push(metric);
        self
    }
}

impl ParameterBuilder<f64> for AggregatedParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }
    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        let metrics = resolve_metric_f64_vec!(self, &self.metrics, resolution_maps, "metrics");

        let meta = self.meta;
        let agg_func = self.agg_func;

        // Try the narrowest dependency class first.
        if let Some(metrics) = try_into_constant_metrics_f64(&metrics) {
            return Ok(
                BuiltParameter::Const(Box::new(AggregatedParameter::<ConstantMetricF64> {
                    meta,
                    metrics,
                    agg_func,
                }))
                .into(),
            );
        }

        if let Some(metrics) = try_into_simple_metrics_f64(&metrics) {
            return Ok(
                BuiltParameter::Simple(SimpleParameterEntry::both(AggregatedParameter::<SimpleMetricF64> {
                    meta,
                    metrics,
                    agg_func,
                }))
                .into(),
            );
        }

        Ok(
            BuiltParameter::General(GeneralParameterEntry::both(AggregatedParameter::<MetricF64> {
                meta,
                metrics,
                agg_func,
            }))
            .into(),
        )
    }
}
