/// AggregatedIndexParameter
///
use super::{ConstParameter, Parameter, ParameterName, ParameterState, SimpleParameter};
use crate::metric::{ConstantMetricU64, MetricU64, SimpleMetricU64};
use crate::network::Network;
use crate::parameters::errors::{ConstCalculationError, ParameterCalculationError, SimpleCalculationError};
use crate::parameters::{GeneralParameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{ConstParameterValues, SimpleParameterValues, State};
use crate::timestep::Timestep;

/// Aggregation functions for aggregated index parameters.
#[derive(Debug, Clone, Copy)]
pub enum AggIndexFunc {
    /// Sum of all values.
    Sum,
    /// Product of all values.
    Product,
    /// Minimum value among all values.
    Min,
    /// Maximum value among all values.
    Max,
    /// Returns 1 if any value is non-zero, otherwise 0.
    Any,
    /// Returns 1 if all values are non-zero, otherwise 0.
    All,
}

pub struct AggregatedIndexParameter<M> {
    meta: ParameterMeta,
    metrics: Vec<M>,
    agg_func: AggIndexFunc,
}

impl<M> AggregatedIndexParameter<M>
where
    M: Send + Sync + Clone,
{
    pub fn new(name: ParameterName, metrics: &[M], agg_func: AggIndexFunc) -> Self {
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
        Ok(aggregate_values(
            self.metrics.iter().map(|p| p.get_value(network, state)),
            self.agg_func,
        )?)
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
            agg_func: self.agg_func,
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
        Ok(aggregate_values(
            self.metrics.iter().map(|p| p.get_value(values)),
            self.agg_func,
        )?)
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
            agg_func: self.agg_func,
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
        Ok(aggregate_values(
            self.metrics.iter().map(|p| p.get_value(values)),
            self.agg_func,
        )?)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

fn aggregate_values<V, E>(values: V, agg_func: AggIndexFunc) -> Result<u64, E>
where
    V: IntoIterator<Item = Result<u64, E>>,
{
    match agg_func {
        AggIndexFunc::Sum => values.into_iter().sum(),
        AggIndexFunc::Product => values.into_iter().product(),
        AggIndexFunc::Max => {
            let mut total = u64::MIN;
            for v in values {
                total = total.max(v?);
            }
            Ok(total)
        }
        AggIndexFunc::Min => {
            let mut total = u64::MAX;
            for v in values {
                total = total.min(v?);
            }
            Ok(total)
        }
        AggIndexFunc::Any => {
            for v in values {
                if v? > 0 {
                    return Ok(1);
                }
            }
            Ok(0)
        }
        AggIndexFunc::All => {
            for v in values {
                if v? == 0 {
                    return Ok(0);
                }
            }
            Ok(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metric::ConstantMetricU64Error;
    use crate::parameters::ConstParameterIndex;

    #[test]
    fn test_sum() {
        let values: Vec<Result<_, ConstantMetricU64Error>> = vec![Ok(1u64), Ok(2u64), Ok(3u64)];
        let result = aggregate_values(values, AggIndexFunc::Sum).unwrap();
        assert_eq!(result, 6u64);
    }

    #[test]
    fn test_product() {
        let values: Vec<Result<_, ConstantMetricU64Error>> = vec![Ok(2u64), Ok(3u64), Ok(4u64)];
        let result = aggregate_values(values, AggIndexFunc::Product).unwrap();
        assert_eq!(result, 24u64);
    }

    #[test]
    fn test_min() {
        let values: Vec<Result<_, ConstantMetricU64Error>> = vec![Ok(5u64), Ok(2u64), Ok(8u64)];
        let result = aggregate_values(values, AggIndexFunc::Min).unwrap();
        assert_eq!(result, 2u64);
    }

    #[test]
    fn test_max() {
        let values: Vec<Result<_, ConstantMetricU64Error>> = vec![Ok(5u64), Ok(2u64), Ok(8u64)];
        let result = aggregate_values(values, AggIndexFunc::Max).unwrap();
        assert_eq!(result, 8u64);
    }

    #[test]
    fn test_any_true() {
        let values: Vec<Result<_, ConstantMetricU64Error>> = vec![Ok(0u64), Ok(0u64), Ok(5u64)];
        let result = aggregate_values(values, AggIndexFunc::Any).unwrap();
        assert_eq!(result, 1u64);
    }

    #[test]
    fn test_any_false() {
        let values: Vec<Result<_, ConstantMetricU64Error>> = vec![Ok(0u64), Ok(0u64), Ok(0u64)];
        let result = aggregate_values(values, AggIndexFunc::Any).unwrap();
        assert_eq!(result, 0u64);
    }

    #[test]
    fn test_all_true() {
        let values: Vec<Result<_, ConstantMetricU64Error>> = vec![Ok(1u64), Ok(2u64), Ok(3u64)];
        let result = aggregate_values(values, AggIndexFunc::All).unwrap();
        assert_eq!(result, 1u64);
    }

    #[test]
    fn test_all_false() {
        let values: Vec<Result<_, ConstantMetricU64Error>> = vec![Ok(1u64), Ok(0u64), Ok(3u64)];
        let result = aggregate_values(values, AggIndexFunc::All).unwrap();
        assert_eq!(result, 0u64);
    }

    #[test]
    fn test_error_propagation() {
        let values = vec![
            Ok(1u64),
            Err(ConstantMetricU64Error::IndexParameterNotFound {
                index: ConstParameterIndex::new(10),
            }),
            Ok(3u64),
        ];
        let result = aggregate_values(values, AggIndexFunc::Sum);
        assert!(result.is_err());
    }
}
