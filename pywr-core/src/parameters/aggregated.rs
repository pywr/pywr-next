use super::{ConstParameter, Parameter, ParameterName, ParameterState, SimpleParameter};
use crate::metric::{ConstantMetricF64, MetricF64, SimpleMetricF64};
use crate::network::Network;
use crate::parameters::errors::{ConstCalculationError, ParameterCalculationError, SimpleCalculationError};
use crate::parameters::{GeneralParameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::{ConstParameterValues, SimpleParameterValues, State};
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
        Ok(aggregate_values(
            self.metrics.iter().map(|p| p.get_value(model, state)),
            self.agg_func,
        )?)
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
            agg_func: self.agg_func,
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

fn aggregate_values<V, E>(values: V, agg_func: AggFunc) -> Result<f64, E>
where
    V: IntoIterator<Item = Result<f64, E>>,
{
    match agg_func {
        AggFunc::Sum => values.into_iter().sum(),
        AggFunc::Mean => {
            let iter = values.into_iter();
            let count = iter.size_hint().0;
            if count == 0 {
                return Ok(0.0);
            }
            let total = iter.sum::<Result<f64, _>>()?;
            Ok(total / count as f64)
        }
        AggFunc::Max => {
            let mut total = f64::MIN;
            for v in values {
                total = total.max(v?);
            }
            Ok(total)
        }
        AggFunc::Min => {
            let mut total = f64::MAX;
            for v in values {
                total = total.min(v?);
            }
            Ok(total)
        }
        AggFunc::Product => values.into_iter().product(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metric::ConstantMetricF64Error;
    use crate::parameters::ConstParameterIndex;

    #[test]
    fn test_sum() {
        let values: Vec<Result<_, ConstantMetricF64Error>> = vec![Ok(1.0), Ok(2.0), Ok(3.0)];
        let result = aggregate_values(values, AggFunc::Sum).unwrap();
        assert_eq!(result, 6.0);
    }

    #[test]
    fn test_product() {
        let values: Vec<Result<_, ConstantMetricF64Error>> = vec![Ok(2.0), Ok(3.0), Ok(4.0)];
        let result = aggregate_values(values, AggFunc::Product).unwrap();
        assert_eq!(result, 24.0);
    }

    #[test]
    fn test_mean() {
        let values: Vec<Result<_, ConstantMetricF64Error>> = vec![Ok(2.0), Ok(4.0), Ok(6.0)];
        let result = aggregate_values(values, AggFunc::Mean).unwrap();
        assert_eq!(result, 4.0);
    }

    #[test]
    fn test_min() {
        let values: Vec<Result<_, ConstantMetricF64Error>> = vec![Ok(5.0), Ok(2.0), Ok(8.0)];
        let result = aggregate_values(values, AggFunc::Min).unwrap();
        assert_eq!(result, 2.0);
    }

    #[test]
    fn test_max() {
        let values: Vec<Result<_, ConstantMetricF64Error>> = vec![Ok(5.0), Ok(2.0), Ok(8.0)];
        let result = aggregate_values(values, AggFunc::Max).unwrap();
        assert_eq!(result, 8.0);
    }

    #[test]
    fn test_empty_mean() {
        let values: Vec<Result<f64, ConstantMetricF64Error>> = vec![];
        let result = aggregate_values(values, AggFunc::Mean).unwrap();
        assert_eq!(result, 0.0);
    }

    #[test]
    fn test_error_propagation() {
        let values = vec![
            Ok(1.0),
            Err(ConstantMetricF64Error::IndexParameterNotFound {
                index: ConstParameterIndex::new(10),
            }),
            Ok(3.0),
        ];
        let result = aggregate_values(values, AggFunc::Sum);
        assert!(result.is_err());
    }
}
