use crate::recorders::aggregator::{Event, PeriodValue};

#[derive(Clone, Debug)]
pub enum AggregationFunction {
    Sum,
    Mean,
    Min,
    Max,
    CountNonZero,
    CountFunc { func: fn(f64) -> bool },
}

impl AggregationFunction {
    /// Calculate the aggregation of the given `PeriodValue`.
    ///
    /// This function takes a slice of `PeriodValue<f64>` and applies the aggregation function to the values.
    /// It returns an `Option<f64>`, which will be `None` if the aggregation cannot be computed (e.g., for `Mean` with no values).
    ///
    pub fn calc_period_values(&self, values: &[PeriodValue<f64>]) -> Option<f64> {
        match self {
            AggregationFunction::Sum => Some(values.iter().map(|v| v.value * v.duration.fractional_days()).sum()),
            AggregationFunction::Mean => {
                let ndays: f64 = values.iter().map(|v| v.duration.fractional_days()).sum();
                if ndays == 0.0 {
                    None
                } else {
                    let sum: f64 = values.iter().map(|v| v.value * v.duration.fractional_days()).sum();

                    Some(sum / ndays)
                }
            }
            AggregationFunction::Min => values.iter().map(|v| v.value).min_by(|a, b| {
                a.partial_cmp(b)
                    .expect("Failed to calculate minimum of values containing a NaN.")
            }),
            AggregationFunction::Max => values.iter().map(|v| v.value).max_by(|a, b| {
                a.partial_cmp(b)
                    .expect("Failed to calculate maximum of values containing a NaN.")
            }),
            AggregationFunction::CountNonZero => {
                let count = values.iter().filter(|v| v.value != 0.0).count();
                Some(count as f64)
            }
            AggregationFunction::CountFunc { func } => {
                let count = values.iter().filter(|v| func(v.value)).count();
                Some(count as f64)
            }
        }
    }

    pub fn calc_f64(&self, values: &[f64]) -> Option<f64> {
        match self {
            AggregationFunction::Sum => Some(values.iter().sum()),
            AggregationFunction::Mean => {
                let ndays: i64 = values.len() as i64;
                if ndays == 0 {
                    None
                } else {
                    let sum: f64 = values.iter().sum();
                    Some(sum / ndays as f64)
                }
            }
            AggregationFunction::Min => values
                .iter()
                .min_by(|a, b| {
                    a.partial_cmp(b)
                        .expect("Failed to calculate minimum of values containing a NaN.")
                })
                .copied(),
            AggregationFunction::Max => values
                .iter()
                .max_by(|a, b| {
                    a.partial_cmp(b)
                        .expect("Failed to calculate maximum of values containing a NaN.")
                })
                .copied(),
            AggregationFunction::CountNonZero => {
                let count = values.iter().filter(|v| **v != 0.0).count();
                Some(count as f64)
            }
            AggregationFunction::CountFunc { func } => {
                let count = values.iter().filter(|v| func(**v)).count();
                Some(count as f64)
            }
        }
    }
}
