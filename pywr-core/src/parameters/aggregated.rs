use super::{ConstParameter, Parameter, ParameterName, ParameterState, SimpleParameter};
use crate::agg_funcs::AggFuncF64;
use crate::metric::{ConstantMetricF64, MetricF64, SimpleMetricF64};
use crate::network::Network;
use crate::parameters::errors::{ConstCalculationError, ParameterCalculationError, SimpleCalculationError};
use crate::parameters::{GeneralParameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{ConstParameterValues, SimpleParameterValues, State};
use crate::timestep::Timestep;

pub struct AggregatedParameter<M> {
    meta: ParameterMeta,
    metrics: Vec<M>,
    agg_func: AggFuncF64,
}

impl<M> AggregatedParameter<M>
where
    M: Send + Sync + Clone,
{
    pub fn new(name: ParameterName, metrics: &[M], agg_func: AggFuncF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metrics: metrics.to_vec(),
            agg_func,
        }
    }
}

impl<M> Parameter for AggregatedParameter<M>
where
    M: Send + Sync,
{
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter<f64> for AggregatedParameter<MetricF64> {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, ParameterCalculationError> {
        let values = self
            .metrics
            .iter()
            .map(|p| p.get_value(model, state))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(self.agg_func.calc_iter_f64(&values)?)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
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
}

impl SimpleParameter<f64> for AggregatedParameter<SimpleMetricF64> {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        values: &SimpleParameterValues,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, SimpleCalculationError> {
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
