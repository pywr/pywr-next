/// AggregatedIndexParameter
///
use super::PywrError;
use crate::model::Model;
use crate::parameters::{IndexParameter, IndexValue, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use std::any::Any;
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
    values: Vec<IndexValue>,
    agg_func: AggIndexFunc,
}

impl AggregatedIndexParameter {
    pub fn new(name: &str, values: Vec<IndexValue>, agg_func: AggIndexFunc) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            values,
            agg_func,
        }
    }
}

impl IndexParameter for AggregatedIndexParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _model: &Model,
        state: &State,
        _internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<usize, PywrError> {
        // TODO scenarios!

        let value: usize = match self.agg_func {
            AggIndexFunc::Sum => {
                let mut total = 0;
                for p in &self.values {
                    total += match p {
                        IndexValue::Constant(v) => *v,
                        IndexValue::Dynamic(p) => state.get_parameter_index(*p)?,
                    };
                }
                total
            }
            AggIndexFunc::Max => {
                let mut total = usize::MIN;
                for p in &self.values {
                    total = total.max(match p {
                        IndexValue::Constant(v) => *v,
                        IndexValue::Dynamic(p) => state.get_parameter_index(*p)?,
                    });
                }
                total
            }
            AggIndexFunc::Min => {
                let mut total = usize::MAX;
                for p in &self.values {
                    total = total.min(match p {
                        IndexValue::Constant(v) => *v,
                        IndexValue::Dynamic(p) => state.get_parameter_index(*p)?,
                    });
                }
                total
            }
            AggIndexFunc::Product => {
                let mut total = 1;
                for p in &self.values {
                    total *= match p {
                        IndexValue::Constant(v) => *v,
                        IndexValue::Dynamic(p) => state.get_parameter_index(*p)?,
                    };
                }
                total
            }
            AggIndexFunc::Any => {
                let mut any = 0;
                for p in &self.values {
                    let value = match p {
                        IndexValue::Constant(v) => *v,
                        IndexValue::Dynamic(p) => state.get_parameter_index(*p)?,
                    };

                    if value > 0 {
                        any = 1;
                        break;
                    };
                }
                any
            }
            AggIndexFunc::All => {
                let mut all = 1;
                for p in &self.values {
                    let value = match p {
                        IndexValue::Constant(v) => *v,
                        IndexValue::Dynamic(p) => state.get_parameter_index(*p)?,
                    };

                    if value == 0 {
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
