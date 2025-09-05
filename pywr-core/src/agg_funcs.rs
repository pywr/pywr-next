use crate::recorders::PeriodValue;

/// Aggregation functions that can be applied to a set of f64 values.
#[derive(Clone, Debug)]
pub enum AggFuncF64 {
    Sum,
    Mean,
    Product,
    Min,
    Max,
    CountNonZero,
    CountFunc { func: fn(f64) -> bool },
}

impl AggFuncF64 {
    /// Calculate the aggregation of the given values.
    pub fn calc_period_values(&self, values: &[PeriodValue<f64>]) -> Option<f64> {
        match self {
            Self::Sum => Some(values.iter().map(|v| v.value * v.duration.fractional_days()).sum()),
            Self::Mean => {
                let ndays: f64 = values.iter().map(|v| v.duration.fractional_days()).sum();
                if ndays == 0.0 {
                    None
                } else {
                    let sum: f64 = values.iter().map(|v| v.value * v.duration.fractional_days()).sum();

                    Some(sum / ndays)
                }
            }
            Self::Product => Some(values.iter().map(|v| v.value).product()),
            Self::Min => values.iter().map(|v| v.value).min_by(|a, b| {
                a.partial_cmp(b)
                    .expect("Failed to calculate minimum of values containing a NaN.")
            }),
            Self::Max => values.iter().map(|v| v.value).max_by(|a, b| {
                a.partial_cmp(b)
                    .expect("Failed to calculate maximum of values containing a NaN.")
            }),
            Self::CountNonZero => {
                let count = values.iter().filter(|v| v.value != 0.0).count();
                Some(count as f64)
            }
            Self::CountFunc { func } => {
                let count = values.iter().filter(|v| func(v.value)).count();
                Some(count as f64)
            }
        }
    }

    /// Calculate the aggregation of the given iterator of values.
    pub fn calc_iter_f64<'a, V>(&self, values: V) -> f64
    where
        V: IntoIterator<Item = &'a f64>,
    {
        match self {
            Self::Sum => values.into_iter().sum(),
            Self::Mean => {
                let iter = values.into_iter();
                let count = iter.size_hint().0;
                if count == 0 {
                    return 0.0;
                }
                let total: f64 = iter.sum();
                total / count as f64
            }
            Self::Max => {
                let mut total = f64::MIN;
                for v in values {
                    total = total.max(*v);
                }
                total
            }
            Self::Min => {
                let mut total = f64::MAX;
                for v in values {
                    total = total.min(*v);
                }
                total
            }
            Self::Product => values.into_iter().product(),
            Self::CountNonZero => {
                let count = values.into_iter().filter(|&v| *v != 0.0).count();
                count as f64
            }
            Self::CountFunc { func } => {
                let count = values.into_iter().filter(|&v| func(*v)).count();
                count as f64
            }
        }
    }

    /// Calculate the aggregation of the given iterator of values, which may return errors.
    pub fn calc_iter_result_f64<V, E>(&self, values: V) -> Result<f64, E>
    where
        V: IntoIterator<Item = Result<f64, E>>,
    {
        match self {
            Self::Sum => values.into_iter().sum(),
            Self::Mean => {
                let iter = values.into_iter();
                let count = iter.size_hint().0;
                if count == 0 {
                    return Ok(0.0);
                }
                let total = iter.sum::<Result<f64, _>>()?;
                Ok(total / count as f64)
            }
            Self::Max => {
                let mut total = f64::MIN;
                for v in values {
                    total = total.max(v?);
                }
                Ok(total)
            }
            Self::Min => {
                let mut total = f64::MAX;
                for v in values {
                    total = total.min(v?);
                }
                Ok(total)
            }
            Self::Product => values.into_iter().product(),
            Self::CountNonZero => {
                let mut count = 0;
                for v in values {
                    if v? != 0.0 {
                        count += 1;
                    }
                }
                Ok(count as f64)
            }
            Self::CountFunc { func } => {
                let mut count = 0;
                for v in values {
                    if func(v?) {
                        count += 1;
                    }
                }
                Ok(count as f64)
            }
        }
    }
}

/// Aggregation functions for aggregated index parameters.
#[derive(Debug, Clone, Copy)]
pub enum AggFuncU64 {
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

impl AggFuncU64 {
    /// Calculate the aggregation of the given iterator of values, which may return errors.
    pub fn calc_iter_result_u64<V, E>(&self, values: V) -> Result<u64, E>
    where
        V: IntoIterator<Item = Result<u64, E>>,
    {
        match self {
            AggFuncU64::Sum => values.into_iter().sum(),
            AggFuncU64::Product => values.into_iter().product(),
            AggFuncU64::Max => {
                let mut total = u64::MIN;
                for v in values {
                    total = total.max(v?);
                }
                Ok(total)
            }
            AggFuncU64::Min => {
                let mut total = u64::MAX;
                for v in values {
                    total = total.min(v?);
                }
                Ok(total)
            }
            AggFuncU64::Any => {
                for v in values {
                    if v? > 0 {
                        return Ok(1);
                    }
                }
                Ok(0)
            }
            AggFuncU64::All => {
                for v in values {
                    if v? == 0 {
                        return Ok(0);
                    }
                }
                Ok(1)
            }
        }
    }

    /// Calculate the aggregation of the given slice of values.
    pub fn calc_result_u64(&self, values: &[u64]) -> u64 {
        match self {
            AggFuncU64::Sum => values.iter().sum(),
            AggFuncU64::Product => values.iter().product(),
            AggFuncU64::Max => *values.iter().max().unwrap_or(&u64::MIN),
            AggFuncU64::Min => *values.iter().min().unwrap_or(&u64::MAX),
            AggFuncU64::Any => values.iter().any(|&b| b != 0) as u64,
            AggFuncU64::All => values.iter().all(|&b| b != 0) as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::agg_funcs::{AggFuncF64, AggFuncU64};
    use crate::metric::{ConstantMetricF64Error, ConstantMetricU64Error};
    use crate::parameters::ConstParameterIndex;
    use float_cmp::assert_approx_eq;

    #[test]
    fn test_f64_sum() {
        let values: Vec<Result<_, ConstantMetricF64Error>> = vec![Ok(1.0), Ok(2.0), Ok(3.0)];
        let result = AggFuncF64::Sum.calc_iter_result_f64(values).unwrap();
        assert_approx_eq!(f64, result, 6.0);

        let result = AggFuncF64::Sum.calc_iter_f64(&[1.0, 2.0, 3.0]);
        assert_approx_eq!(f64, result, 6.0);
    }

    #[test]
    fn test_f64_product() {
        let values: Vec<Result<_, ConstantMetricF64Error>> = vec![Ok(2.0), Ok(3.0), Ok(4.0)];
        let result = AggFuncF64::Product.calc_iter_result_f64(values).unwrap();
        assert_approx_eq!(f64, result, 24.0);

        let result = AggFuncF64::Product.calc_iter_f64(&[2.0, 3.0, 4.0]);
        assert_approx_eq!(f64, result, 24.0);
    }

    #[test]
    fn test_f64_mean() {
        let values: Vec<Result<_, ConstantMetricF64Error>> = vec![Ok(2.0), Ok(4.0), Ok(6.0)];
        let result = AggFuncF64::Mean.calc_iter_result_f64(values).unwrap();
        assert_approx_eq!(f64, result, 4.0);

        let result = AggFuncF64::Mean.calc_iter_f64(&[2.0, 4.0, 6.0]);
        assert_approx_eq!(f64, result, 4.0);
    }

    #[test]
    fn test_f64_min() {
        let values: Vec<Result<_, ConstantMetricF64Error>> = vec![Ok(5.0), Ok(2.0), Ok(8.0)];
        let result = AggFuncF64::Min.calc_iter_result_f64(values).unwrap();
        assert_approx_eq!(f64, result, 2.0);

        let result = AggFuncF64::Min.calc_iter_f64(&[5.0, 2.0, 8.0]);
        assert_approx_eq!(f64, result, 2.0);
    }

    #[test]
    fn test_f64_max() {
        let values: Vec<Result<_, ConstantMetricF64Error>> = vec![Ok(5.0), Ok(2.0), Ok(8.0)];
        let result = AggFuncF64::Max.calc_iter_result_f64(values).unwrap();
        assert_approx_eq!(f64, result, 8.0);

        let result = AggFuncF64::Max.calc_iter_f64(&[5.0, 2.0, 8.0]);
        assert_approx_eq!(f64, result, 8.0);
    }

    #[test]
    fn test_f64_empty_mean() {
        let values: Vec<Result<f64, ConstantMetricF64Error>> = vec![];
        let result = AggFuncF64::Mean.calc_iter_result_f64(values).unwrap();
        assert_approx_eq!(f64, result, 0.0);

        let result = AggFuncF64::Mean.calc_iter_f64(&[]);
        assert_approx_eq!(f64, result, 0.0);
    }

    #[test]
    fn test_f64_error_propagation() {
        let values = vec![
            Ok(1.0),
            Err(ConstantMetricF64Error::IndexParameterNotFound {
                index: ConstParameterIndex::new(10),
            }),
            Ok(3.0),
        ];
        let result = AggFuncF64::Sum.calc_iter_result_f64(values);
        assert!(result.is_err());
    }

    #[test]
    fn test_u64_sum() {
        let values: Vec<Result<_, ConstantMetricU64Error>> = vec![Ok(1u64), Ok(2u64), Ok(3u64)];
        let result = AggFuncU64::Sum.calc_iter_result_u64(values).unwrap();
        assert_eq!(result, 6u64);

        let result = AggFuncU64::Sum.calc_result_u64(&[1u64, 2u64, 3u64]);
        assert_eq!(result, 6u64);
    }

    #[test]
    fn test_u64_product() {
        let values: Vec<Result<_, ConstantMetricU64Error>> = vec![Ok(2u64), Ok(3u64), Ok(4u64)];
        let result = AggFuncU64::Product.calc_iter_result_u64(values).unwrap();
        assert_eq!(result, 24u64);

        let result = AggFuncU64::Product.calc_result_u64(&[2u64, 3u64, 4u64]);
        assert_eq!(result, 24u64);
    }

    #[test]
    fn test_u64_min() {
        let values: Vec<Result<_, ConstantMetricU64Error>> = vec![Ok(5u64), Ok(2u64), Ok(8u64)];
        let result = AggFuncU64::Min.calc_iter_result_u64(values).unwrap();
        assert_eq!(result, 2u64);

        let result = AggFuncU64::Min.calc_result_u64(&[5u64, 2u64, 8u64]);
        assert_eq!(result, 2u64);
    }

    #[test]
    fn test_u64_max() {
        let values: Vec<Result<_, ConstantMetricU64Error>> = vec![Ok(5u64), Ok(2u64), Ok(8u64)];
        let result = AggFuncU64::Max.calc_iter_result_u64(values).unwrap();
        assert_eq!(result, 8u64);

        let result = AggFuncU64::Max.calc_result_u64(&[5u64, 2u64, 8u64]);
        assert_eq!(result, 8u64);
    }

    #[test]
    fn test_u64_any_true() {
        let values: Vec<Result<_, ConstantMetricU64Error>> = vec![Ok(0u64), Ok(0u64), Ok(5u64)];
        let result = AggFuncU64::Any.calc_iter_result_u64(values).unwrap();
        assert_eq!(result, 1u64);

        let result = AggFuncU64::Any.calc_result_u64(&[0u64, 0u64, 5u64]);
        assert_eq!(result, 1u64);
    }

    #[test]
    fn test_u64_any_false() {
        let values: Vec<Result<_, ConstantMetricU64Error>> = vec![Ok(0u64), Ok(0u64), Ok(0u64)];
        let result = AggFuncU64::Any.calc_iter_result_u64(values).unwrap();
        assert_eq!(result, 0u64);

        let result = AggFuncU64::Any.calc_result_u64(&[0u64, 0u64, 0u64]);
        assert_eq!(result, 0u64);
    }

    #[test]
    fn test_u64_all_true() {
        let values: Vec<Result<_, ConstantMetricU64Error>> = vec![Ok(1u64), Ok(2u64), Ok(3u64)];
        let result = AggFuncU64::All.calc_iter_result_u64(values).unwrap();
        assert_eq!(result, 1u64);

        let result = AggFuncU64::All.calc_result_u64(&[1u64, 2u64, 3u64]);
        assert_eq!(result, 1u64);
    }

    #[test]
    fn test_u64_all_false() {
        let values: Vec<Result<_, ConstantMetricU64Error>> = vec![Ok(1u64), Ok(0u64), Ok(3u64)];
        let result = AggFuncU64::All.calc_iter_result_u64(values).unwrap();
        assert_eq!(result, 0u64);

        let result = AggFuncU64::All.calc_result_u64(&[1u64, 0u64, 3u64]);
        assert_eq!(result, 0u64);
    }

    #[test]
    fn test_u64_error_propagation() {
        let values = vec![
            Ok(1u64),
            Err(ConstantMetricU64Error::IndexParameterNotFound {
                index: ConstParameterIndex::new(10),
            }),
            Ok(3u64),
        ];
        let result = AggFuncU64::Sum.calc_iter_result_u64(values);
        assert!(result.is_err());
    }
}
