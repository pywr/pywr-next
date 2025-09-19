/// AggregatedIndexParameter
///
use super::{ConstParameter, Parameter, ParameterName, ParameterState, SimpleParameter};
use crate::agg_funcs::AggFuncU64;
use crate::metric::{ConstantMetricU64, MetricU64, SimpleMetricU64};
use crate::network::Network;
use crate::parameters::errors::{ConstCalculationError, ParameterCalculationError, SimpleCalculationError};
use crate::parameters::{GeneralParameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{ConstParameterValues, SimpleParameterValues, State};
use crate::timestep::Timestep;

pub struct AggregatedIndexParameter<M> {
    meta: ParameterMeta,
    metrics: Vec<M>,
    agg_func: AggFuncU64,
}

impl<M> AggregatedIndexParameter<M>
where
    M: Send + Sync + Clone,
{
    pub fn new(name: ParameterName, metrics: &[M], agg_func: AggFuncU64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metrics: metrics.to_vec(),
            agg_func,
        }
    }
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
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, ParameterCalculationError> {
        let values = self
            .metrics
            .iter()
            .map(|p| p.get_value(network, state))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(self.agg_func.calc_iter_u64(&values)?)
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
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        values: &SimpleParameterValues,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, SimpleCalculationError> {
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
