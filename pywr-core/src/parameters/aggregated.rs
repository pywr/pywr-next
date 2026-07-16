use super::{
    BuiltParameter, ConstParameter, GeneralParameterContext, MaybeBuiltParameter, Parameter, ParameterBuildError,
    ParameterBuilder, ParameterName, ParameterState, SimpleParameter, SimpleParameterContext,
};
use crate::agg_funcs::AggFuncF64;
use crate::metric::{ConstantMetricF64, MetricF64, SimpleMetricF64, UnresolvedMetricF64};
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

impl GeneralParameter<f64> for AggregatedParameter<MetricF64> {
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<f64>, GeneralCalculationError> {
        let values = self
            .metrics
            .iter()
            .map(|p| p.get_value(ctx.network, ctx.state))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Some(self.agg_func.calc_iter_f64(&values)?))
    }

    fn try_into_simple(&self) -> Option<Box<dyn SimpleParameter<f64>>> {
        // We can make a simple version if all metrics can be simplified
        let metrics: Vec<SimpleMetricF64> = self
            .metrics
            .clone()
            .into_iter()
            .map(|m| m.try_into().ok())
            .collect::<Option<Vec<_>>>()?;

        Some(Box::new(AggregatedParameter::<SimpleMetricF64> {
            meta: self.meta.clone(),
            metrics,
            agg_func: self.agg_func.clone(),
        }))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl SimpleParameter<f64> for AggregatedParameter<SimpleMetricF64> {
    fn before(
        &self,
        ctx: SimpleParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<f64>, SimpleCalculationError> {
        let values = self
            .metrics
            .iter()
            .map(|p| p.get_value(ctx.values))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Some(self.agg_func.calc_iter_f64(&values)?))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }

    fn try_into_const(&self) -> Option<Box<dyn ConstParameter<f64>>> {
        // We can make a constant version if all metrics can be simplified
        let metrics: Vec<ConstantMetricF64> = self
            .metrics
            .clone()
            .into_iter()
            .map(|m| m.try_into().ok())
            .collect::<Option<Vec<_>>>()?;

        Some(Box::new(AggregatedParameter::<ConstantMetricF64> {
            meta: self.meta.clone(),
            metrics,
            agg_func: self.agg_func.clone(),
        }))
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

        let p = AggregatedParameter {
            meta: self.meta,
            metrics,
            agg_func: self.agg_func,
        };

        let bp = BuiltParameter::General(Box::new(p));
        Ok(bp.into())
    }
}
