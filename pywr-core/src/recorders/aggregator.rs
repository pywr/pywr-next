use time::{Date, Duration, Month};

#[derive(Clone, Debug)]
enum AggregationPeriod {
    Monthly,
    Annual,
}

impl AggregationPeriod {
    fn is_date_in_period(&self, period_start: &Date, date: &Date) -> bool {
        match self {
            Self::Monthly => (period_start.year() == date.year()) && (period_start.month() == date.month()),
            Self::Annual => period_start.year() == date.year(),
        }
    }

    fn start_of_next_period(&self, current_date: &Date) -> Date {
        match self {
            Self::Monthly => {
                // Increment the year if we're in December
                let year = if current_date.month() == Month::December {
                    current_date.year() + 1
                } else {
                    current_date.year()
                };
                // 1st of the next month
                Date::from_calendar_date(year, current_date.month().next(), 1).unwrap()
            }
            // 1st of January in the next year
            Self::Annual => Date::from_calendar_date(current_date.year() + 1, Month::January, 1).unwrap(),
        }
    }

    /// Split the value representing a period into multiple ['PeriodValue'] that do not cross the
    /// boundary of the given period.
    fn split_value_into_periods(&self, value: PeriodValue) -> Vec<PeriodValue> {
        let mut sub_values = Vec::new();

        let mut current_date = value.start;
        let end_date = value.start + value.duration;

        while current_date < end_date {
            // This should be safe to unwrap as it will always create a valid date unless
            // we are at the limit of dates that are representable.
            let start_of_next_month = current_date
                .replace_day(1)
                .unwrap()
                .replace_month(current_date.month().next())
                .unwrap();

            let current_duration = if start_of_next_month <= end_date {
                start_of_next_month - current_date
            } else {
                end_date - current_date
            };

            sub_values.push(PeriodValue {
                start: current_date,
                duration: current_duration,
                value: value.value,
            });

            current_date = start_of_next_month;
        }

        sub_values
    }
}

#[derive(Clone, Debug)]
enum AggregationFunction {
    Sum,
    Mean,
    Min,
    Max,
}

impl AggregationFunction {
    fn calc(&self, values: &[PeriodValue]) -> Option<f64> {
        match self {
            AggregationFunction::Sum => Some(values.iter().map(|v| v.value * v.duration.whole_days() as f64).sum()),
            AggregationFunction::Mean => {
                let ndays: i64 = values.iter().map(|v| v.duration.whole_days()).sum();
                if ndays == 0 {
                    None
                } else {
                    let sum: f64 = values.iter().map(|v| v.value * v.duration.whole_days() as f64).sum();

                    Some(sum / ndays as f64)
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
        }
    }
}

#[derive(Default)]
pub struct PeriodicAggregatorState {
    current_values: Option<Vec<PeriodValue>>,
}

impl PeriodicAggregatorState {
    fn process_value(
        &mut self,
        value: PeriodValue,
        agg_period: &AggregationPeriod,
        agg_func: &AggregationFunction,
    ) -> Option<PeriodValue> {
        if let Some(current_values) = self.current_values.as_mut() {
            let current_period_start = current_values
                .get(0)
                .expect("Aggregation state contains no values when at least one is expected.")
                .start;

            // Determine if the value is in the current period
            if agg_period.is_date_in_period(&current_period_start, &value.start) {
                // New value in the current aggregation period; just append it.
                current_values.push(value);

                None
            } else {
                // New value is part of a different period (assume the next one).

                // Calculate the aggregated value of the previous period.
                let agg_period = if let Some(agg_value) = agg_func.calc(&current_values) {
                    let agg_duration = value.start - current_period_start;
                    Some(PeriodValue::new(current_period_start, agg_duration, agg_value))
                } else {
                    None
                };

                // Reset the state for the next period
                current_values.clear();
                current_values.push(value);

                // Finally return the aggregated value from the previous period
                agg_period
            }
        } else {
            // No previous values defined; just append the value
            self.current_values = Some(vec![value]);

            None
        }
    }

    // fn calc_aggregation(&self, agg_func: &AggregationFunction) -> f64 {
    //     match agg_func
    // }
}

#[derive(Clone, Debug)]
pub struct PeriodicAggregator {
    period: AggregationPeriod,
    function: AggregationFunction,
}

#[derive(Debug, Copy, Clone)]
pub struct PeriodValue {
    start: Date,
    duration: Duration,
    value: f64,
}

impl PeriodValue {
    pub fn new(start: Date, duration: Duration, value: f64) -> Self {
        Self { start, duration, value }
    }
}

impl PeriodicAggregator {
    /// Append a new value to the aggregator.
    ///
    /// The new value should sequentially follow from the previously processed values. If the
    /// value completes a new aggregation period then a value representing that aggregation is
    /// returned.
    pub fn process_value(
        &self,
        current_state: &mut PeriodicAggregatorState,
        value: PeriodValue,
    ) -> Option<PeriodValue> {
        // Split the given period into separate periods that align with the aggregation period.
        let mut agg_value = None;

        for v in self.period.split_value_into_periods(value) {
            let av = current_state.process_value(v, &self.period, &self.function);
            if av.is_some() {
                if agg_value.is_some() {
                    panic!("Multiple aggregated values yielded from aggregator. This indicates that the given value spans multiple aggregation periods which is not supported.")
                }
                agg_value = av;
            }
        }

        agg_value
    }
}

#[cfg(test)]
mod tests {
    use super::{AggregationFunction, AggregationPeriod, PeriodicAggregator, PeriodicAggregatorState};
    use crate::recorders::aggregator::PeriodValue;
    use time::macros::date;
    use time::Duration;

    #[test]
    fn test_aggregator() {
        let agg = PeriodicAggregator {
            period: AggregationPeriod::Monthly,
            function: AggregationFunction::Sum,
        };

        let mut state = PeriodicAggregatorState::default();

        let agg_value = agg.process_value(
            &mut state,
            PeriodValue::new(date!(2023 - 01 - 30), Duration::days(1), 1.0),
        );
        assert!(agg_value.is_none());

        let agg_value = agg.process_value(
            &mut state,
            PeriodValue::new(date!(2023 - 01 - 31), Duration::days(1), 1.0),
        );
        assert!(agg_value.is_none());

        let agg_value = agg.process_value(
            &mut state,
            PeriodValue::new(date!(2023 - 02 - 01), Duration::days(1), 1.0),
        );
        assert!(agg_value.is_some());

        let agg_value = agg.process_value(
            &mut state,
            PeriodValue::new(date!(2023 - 02 - 02), Duration::days(1), 1.0),
        );
        assert!(agg_value.is_none());
    }
}
