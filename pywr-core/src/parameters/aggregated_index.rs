/// AggregatedIndexParameter
///
use super::{
    BuiltParameter, ConstParameter, MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder,
    ParameterName, ParameterState, SimpleParameter,
};
use crate::agg_funcs::AggFuncU64;
use crate::metric::{ConstantMetricU64, MetricU64, SimpleMetricU64, UnresolvedMetricU64};
use crate::network::{Network, ResolutionMaps};
use crate::parameters::errors::{ConstCalculationError, GeneralCalculationError, SimpleCalculationError};
use crate::parameters::{GeneralParameter, ParameterMeta};
use crate::resolve_metric_u64_vec;
use crate::scenario::ScenarioIndex;
use crate::state::{ConstParameterValues, SimpleParameterValues, State};
use crate::timestep::Timestep;

pub struct AggregatedIndexParameter<M> {
    meta: ParameterMeta,
    metrics: Vec<M>,
    agg_func: AggFuncU64,
}

impl<M> Parameter for AggregatedIndexParameter<M>
where
    M: Send + Sync,
{
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter<u64> for AggregatedIndexParameter<MetricU64> {
    fn before(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<u64>, GeneralCalculationError> {
        let values = self
            .metrics
            .iter()
            .map(|p| p.get_value(network, state))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Some(self.agg_func.calc_iter_u64(&values)?))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }

    fn try_into_simple(&self) -> Option<Box<dyn SimpleParameter<u64>>> {
        // We can make a simple version if all metrics can be simplified
        let metrics: Vec<SimpleMetricU64> = self
            .metrics
            .clone()
            .into_iter()
            .map(|m| m.try_into().ok())
            .collect::<Option<Vec<_>>>()?;

        Some(Box::new(AggregatedIndexParameter::<SimpleMetricU64> {
            meta: self.meta.clone(),
            metrics,
            agg_func: self.agg_func.clone(),
        }))
    }
}

impl SimpleParameter<u64> for AggregatedIndexParameter<SimpleMetricU64> {
    fn before(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        values: &SimpleParameterValues,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<u64>, SimpleCalculationError> {
        let values = self
            .metrics
            .iter()
            .map(|p| p.get_value(values))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Some(self.agg_func.calc_iter_u64(&values)?))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }

    fn try_into_const(&self) -> Option<Box<dyn ConstParameter<u64>>> {
        // We can make a constant version if all metrics can be simplified to constants
        let metrics: Vec<ConstantMetricU64> = self
            .metrics
            .clone()
            .into_iter()
            .map(|m| m.try_into().ok())
            .collect::<Option<Vec<_>>>()?;

        Some(Box::new(AggregatedIndexParameter::<ConstantMetricU64> {
            meta: self.meta.clone(),
            metrics,
            agg_func: self.agg_func.clone(),
        }))
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

        let p = AggregatedIndexParameter {
            meta: self.meta,
            metrics,
            agg_func: self.agg_func,
        };

        let bp = BuiltParameter::General(Box::new(p));
        Ok(bp.into())
    }
}
