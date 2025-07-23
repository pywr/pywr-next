use super::{Parameter, ParameterName, ParameterState, SimpleParameter};
use crate::metric::{MetricF64, SimpleMetricF64};
use crate::network::Network;
use crate::parameters::errors::{ParameterCalculationError, SimpleCalculationError};
use crate::parameters::{GeneralParameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{SimpleParameterValues, State};
use crate::timestep::Timestep;

#[derive(Debug, Clone, Copy)]
pub enum AggFunc {
    /// Sum of all values.
    Sum,
    /// Product of all values.
    Product,
    /// Mean of all values.
    Mean,
    /// Minimum value among all values.
    Min,
    /// Maximum value among all values.
    Max,
}

pub struct AggregatedParameter<M> {
    meta: ParameterMeta,
    metrics: Vec<M>,
    agg_func: AggFunc,
}

impl<M> AggregatedParameter<M>
where
    M: Send + Sync + Clone,
{
    pub fn new(name: ParameterName, metrics: &[M], agg_func: AggFunc) -> Self {
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
        let value: f64 = match self.agg_func {
            AggFunc::Sum => {
                let mut total = 0.0_f64;
                for p in &self.metrics {
                    total += p.get_value(model, state)?;
                }
                total
            }
            AggFunc::Mean => {
                let mut total = 0.0_f64;
                for p in &self.metrics {
                    total += p.get_value(model, state)?;
                }
                total / self.metrics.len() as f64
            }
            AggFunc::Max => {
                let mut total = f64::MIN;
                for p in &self.metrics {
                    total = total.max(p.get_value(model, state)?);
                }
                total
            }
            AggFunc::Min => {
                let mut total = f64::MAX;
                for p in &self.metrics {
                    total = total.min(p.get_value(model, state)?);
                }
                total
            }
            AggFunc::Product => {
                let mut total = 1.0_f64;
                for p in &self.metrics {
                    total *= p.get_value(model, state)?;
                }
                total
            }
        };

        Ok(value)
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
            agg_func: self.agg_func,
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
        let value: f64 = match self.agg_func {
            AggFunc::Sum => {
                let mut total = 0.0_f64;
                for p in &self.metrics {
                    total += p.get_value(values)?;
                }
                total
            }
            AggFunc::Mean => {
                let mut total = 0.0_f64;
                for p in &self.metrics {
                    total += p.get_value(values)?;
                }
                total / self.metrics.len() as f64
            }
            AggFunc::Max => {
                let mut total = f64::MIN;
                for p in &self.metrics {
                    total = total.max(p.get_value(values)?);
                }
                total
            }
            AggFunc::Min => {
                let mut total = f64::MAX;
                for p in &self.metrics {
                    total = total.min(p.get_value(values)?);
                }
                total
            }
            AggFunc::Product => {
                let mut total = 1.0_f64;
                for p in &self.metrics {
                    total *= p.get_value(values)?;
                }
                total
            }
        };

        Ok(value)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}
