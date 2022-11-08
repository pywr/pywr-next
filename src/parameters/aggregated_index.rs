/// AggregatedIndexParameter
///
use super::{NetworkState, PywrError};
use crate::parameters::{IndexParameter, IndexParameterIndex, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::ParameterState;
use crate::timestep::Timestep;
use std::str::FromStr;

pub enum AggIndexFunc {
    Sum,
    Product,
    Min,
    Max,
    Any,
    All,
}

impl FromStr for AggIndexFunc {
    type Err = PywrError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "sum" => Ok(Self::Sum),
            "product" => Ok(Self::Product),
            "min" => Ok(Self::Min),
            "max" => Ok(Self::Max),
            "any" => Ok(Self::All),
            "all" => Ok(Self::Any),
            _ => Err(PywrError::InvalidAggregationFunction(name.to_string())),
        }
    }
}

pub struct AggregatedIndexParameter {
    meta: ParameterMeta,
    index_parameters: Vec<IndexParameterIndex>,
    agg_func: AggIndexFunc,
}

impl AggregatedIndexParameter {
    pub fn new(name: &str, index_parameters: Vec<IndexParameterIndex>, agg_func: AggIndexFunc) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            index_parameters,
            agg_func,
        }
    }
}

impl IndexParameter for AggregatedIndexParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &mut self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<usize, PywrError> {
        // TODO scenarios!

        let value: usize = match self.agg_func {
            AggIndexFunc::Sum => {
                let mut total = 0;
                for p in &self.index_parameters {
                    total += parameter_state.get_index(*p)?;
                }
                total
            }
            AggIndexFunc::Max => {
                let mut total = usize::MIN;
                for p in &self.index_parameters {
                    total = total.max(parameter_state.get_index(*p)?);
                }
                total
            }
            AggIndexFunc::Min => {
                let mut total = usize::MAX;
                for p in &self.index_parameters {
                    total = total.min(parameter_state.get_index(*p)?);
                }
                total
            }
            AggIndexFunc::Product => {
                let mut total = 1;
                for p in &self.index_parameters {
                    total *= parameter_state.get_index(*p)?;
                }
                total
            }
            AggIndexFunc::Any => {
                let mut any = 0;
                for p in &self.index_parameters {
                    if parameter_state.get_index(*p)? > 0 {
                        any = 1;
                        break;
                    };
                }
                any
            }
            AggIndexFunc::All => {
                let mut all = 1;
                for p in &self.index_parameters {
                    if parameter_state.get_index(*p)? == 0 {
                        all = 0;
                        break;
                    };
                }
                all
            }
        };

        Ok(value)
    }
}
