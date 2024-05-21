use super::{Parameter, PywrError};
use crate::metric::MetricF64;
use crate::network::Network;
use crate::parameters::{GeneralParameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{ParameterState, State};
use crate::timestep::Timestep;
use std::str::FromStr;

pub enum AggFunc {
    Sum,
    Product,
    Mean,
    Min,
    Max,
}

impl FromStr for AggFunc {
    type Err = PywrError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "sum" => Ok(Self::Sum),
            "product" => Ok(Self::Product),
            "mean" => Ok(Self::Mean),
            "min" => Ok(Self::Min),
            "max" => Ok(Self::Max),
            _ => Err(PywrError::InvalidAggregationFunction(name.to_string())),
        }
    }
}

pub struct AggregatedParameter {
    meta: ParameterMeta,
    metrics: Vec<MetricF64>,
    agg_func: AggFunc,
}

impl AggregatedParameter {
    pub fn new(name: &str, metrics: &[MetricF64], agg_func: AggFunc) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metrics: metrics.to_vec(),
            agg_func,
        }
    }
}

impl Parameter for AggregatedParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter<f64> for AggregatedParameter {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, PywrError> {
        // TODO scenarios!

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
}
