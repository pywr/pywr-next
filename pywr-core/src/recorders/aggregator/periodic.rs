use crate::recorders::AggregationFunction;
use crate::timestep::{PywrDuration, TimeDomain};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime};
use std::num::NonZeroUsize;

#[derive(Clone, Debug)]
pub enum AggregationFrequency {
    Monthly,
    Annual,
    Days(NonZeroUsize),
}

impl AggregationFrequency {
    /// Number of periods in the given time domain.
    fn number_of_periods(&self, time_domain: &TimeDomain) -> usize {
        match self {
            Self::Monthly => {
                let start = time_domain.first().date;
                let end = time_domain.last().date;
                let n_years = (end.year() - start.year()) as u32;
                (n_years * 12 + end.month() - start.month()) as usize
            }
            Self::Annual => {
                let start = time_domain.first().date;
                let end = time_domain.last().date;
                (end.year() - start.year()) as usize
            }
            Self::Days(days) => {
                let start = time_domain.first().date;
                let end = time_domain.last().date;
                let n_days = end.signed_duration_since(start).num_days();
                (n_days / days.get() as i64) as usize
            }
        }
    }

    fn is_date_in_period(&self, period_start: &NaiveDateTime, date: &NaiveDateTime) -> bool {
        match self {
            Self::Monthly => (period_start.year() == date.year()) && (period_start.month() == date.month()),
            Self::Annual => period_start.year() == date.year(),
            Self::Days(days) => {
                let period_end = *period_start + Duration::days(days.get() as i64);
                (period_start <= date) && (date < &period_end)
            }
        }
    }

    fn start_of_next_period(&self, current_date: &NaiveDateTime) -> NaiveDateTime {
        match self {
            Self::Monthly => {
                let current_month = current_date.month();
                // Increment the year if we're in December
                let year = if current_month == 12 {
                    current_date.year() + 1
                } else {
                    current_date.year()
                };
                let next_month = (current_month % 12) + 1;
                // 1st of the next month
                // SAFETY: This should be safe to unwrap as it will always create a valid date unless
                // we are at the limit of dates that are representable.
                let date = NaiveDate::from_ymd_opt(year, next_month, 1).unwrap();
                NaiveDateTime::new(date, NaiveTime::default())
            }
            Self::Annual => {
                // 1st of January in the next year
                // SAFETY: This should be safe to unwrap as it will always create a valid date unless
                // we are at the limit of dates that are representable.
                let date = NaiveDate::from_ymd_opt(current_date.year() + 1, 1, 1).unwrap();
                NaiveDateTime::new(date, NaiveTime::default())
            }
            Self::Days(days) => *current_date + Duration::days(days.get() as i64),
        }
    }

    /// Split the value representing a period into multiple ['PeriodValue'] that do not cross the
    /// boundary of the given period.
    fn split_value_into_periods(&self, value: PeriodValue<f64>) -> Vec<PeriodValue<f64>> {
        let mut sub_values = Vec::new();

        let mut current_date = value.start;
        let end_date = value.duration + value.start;

        while current_date < end_date {
            let start_of_next_period = self.start_of_next_period(&current_date);

            let current_duration = if start_of_next_period <= end_date {
                start_of_next_period - current_date
            } else {
                end_date - current_date
            };

            sub_values.push(PeriodValue {
                start: current_date,
                duration: current_duration.into(),
                value: value.value,
            });

            current_date = start_of_next_period;
        }

        sub_values
    }
}

/// State of the periodic aggregator.
///
/// This state stores the current values, if any, that are yielded from the aggregation on the
/// given time-step. Periodic output is consistent for each metric, and therefore is stored
/// as a vec of [`PeriodValue`]s that represents the aggregated value over a period of time for all
/// metrics.
#[derive(Default, Debug, Clone)]
pub struct PeriodicAggregatorState {
    current_values: Option<Vec<PeriodValue<f64>>>,
}

impl PeriodicAggregatorState {
    fn process_value(
        &mut self,
        value: PeriodValue<f64>,
        agg_freq: &AggregationFrequency,
        agg_func: &AggregationFunction,
    ) -> Option<PeriodValue<f64>> {
        if let Some(current_values) = self.current_values.as_mut() {
            // SAFETY: The current_values vector is guaranteed to contain at least one value.
            let current_period_start = current_values
                .first()
                .expect("Aggregation state contains no values when at least one is expected.")
                .start;

            // Determine if the value is in the current period
            if agg_freq.is_date_in_period(&current_period_start, &value.start) {
                // New value in the current aggregation period; just append it.
                current_values.push(value);

                None
            } else {
                // New value is part of a different period (assume the next one).

                // Calculate the aggregated value of the previous period.
                let agg_period = if let Some(agg_value) = agg_func.calc_period_values(current_values) {
                    let agg_duration = value.start - current_period_start;
                    Some(PeriodValue::new(current_period_start, agg_duration.into(), agg_value))
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

    fn process_value_no_period(&mut self, value: PeriodValue<f64>) {
        if let Some(current_values) = self.current_values.as_mut() {
            current_values.push(value);
        } else {
            self.current_values = Some(vec![value]);
        }
    }

    fn calc_aggregation(&self, agg_func: &AggregationFunction) -> Option<PeriodValue<f64>> {
        if let Some(current_values) = &self.current_values {
            if let Some(agg_value) = agg_func.calc_period_values(current_values) {
                // SAFETY: The current_values vector is guaranteed to contain at least one value.
                let current_period_start = current_values
                    .first()
                    .expect("Aggregation state contains no values when at least one is expected.")
                    .start;

                let current_period_end = current_values
                    .last()
                    .expect("Aggregation state contains no values when at least one is expected.")
                    .start;
                let current_period_duration = current_period_end - current_period_start;
                Some(PeriodValue::new(
                    current_period_start,
                    current_period_duration.into(),
                    agg_value,
                ))
            } else {
                None
            }
        } else {
            None
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct PeriodValue<T> {
    pub start: NaiveDateTime,
    pub duration: PywrDuration,
    pub value: T,
}

impl<T> PeriodValue<T> {
    pub fn new(start: NaiveDateTime, duration: PywrDuration, value: T) -> Self {
        Self { start, duration, value }
    }

    /// The end of the period.
    pub fn end(&self) -> NaiveDateTime {
        self.duration + self.start
    }
}

impl<T> PeriodValue<Vec<T>> {
    pub fn index(&self, index: usize) -> PeriodValue<T>
    where
        T: Copy,
    {
        PeriodValue {
            start: self.start,
            duration: self.duration,
            value: self.value[index],
        }
    }
    pub fn len(&self) -> usize {
        self.value.len()
    }
}

impl<T> From<&[PeriodValue<T>]> for PeriodValue<Vec<T>>
where
    T: Copy,
{
    fn from(values: &[PeriodValue<T>]) -> Self {
        let start = values.first().expect("Empty vector of period values.").start;
        let duration = values.last().expect("Empty vector of period values.").duration;

        let value = values.iter().map(|v| v.value).collect();
        Self { start, duration, value }
    }
}

#[derive(Clone, Debug)]
pub struct PeriodicAggregator {
    frequency: Option<AggregationFrequency>,
    function: AggregationFunction,
}

impl PeriodicAggregator {
    pub fn new(frequency: Option<AggregationFrequency>, function: AggregationFunction) -> Self {
        Self { frequency, function }
    }

    /// Append a new value to the aggregator.
    ///
    /// The new value should sequentially follow from the previously processed values. If the
    /// value completes a new aggregation period then a value representing that aggregation is
    /// returned.
    pub fn process_value(
        &self,
        current_state: &mut PeriodicAggregatorState,
        value: PeriodValue<f64>,
    ) -> Option<PeriodValue<f64>> {
        // Split the given period into separate periods that align with the aggregation period.
        let mut agg_value = None;

        if let Some(period) = &self.frequency {
            for v in period.split_value_into_periods(value) {
                let av = current_state.process_value(v, period, &self.function);
                if av.is_some() {
                    if agg_value.is_some() {
                        panic!("Multiple aggregated values yielded from aggregator. This indicates that the given value spans multiple aggregation periods which is not supported.")
                    }
                    agg_value = av;
                }
            }
        } else {
            current_state.process_value_no_period(value);
        }
        agg_value
    }

    pub fn calc_aggregation(&self, state: &PeriodicAggregatorState) -> Option<PeriodValue<f64>> {
        state.calc_aggregation(&self.function)
    }

    /// Expected number of periods in the given time domain.
    pub fn number_of_periods(&self, time_domain: &TimeDomain) -> usize {
        match &self.frequency {
            Some(frequency) => frequency.number_of_periods(time_domain),
            None => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AggregationFrequency, AggregationFunction, PeriodicAggregator, PeriodicAggregatorState};
    use crate::recorders::aggregator::PeriodValue;
    use chrono::{NaiveDate, TimeDelta};
    use float_cmp::assert_approx_eq;

    #[test]
    fn test_periodic_aggregator() {
        let agg = PeriodicAggregator {
            frequency: Some(AggregationFrequency::Monthly),
            function: AggregationFunction::Sum,
        };

        let mut state = PeriodicAggregatorState::default();

        let start = NaiveDate::from_ymd_opt(2023, 1, 30)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let agg_value = agg.process_value(&mut state, PeriodValue::new(start, TimeDelta::days(1).into(), 1.0));
        assert!(agg_value.is_none());

        let start = NaiveDate::from_ymd_opt(2023, 1, 31)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let agg_value = agg.process_value(&mut state, PeriodValue::new(start, TimeDelta::days(1).into(), 1.0));
        assert!(agg_value.is_none());

        let start = NaiveDate::from_ymd_opt(2023, 2, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let agg_value = agg.process_value(&mut state, PeriodValue::new(start, TimeDelta::days(1).into(), 1.0));
        assert!(agg_value.is_some());

        let start = NaiveDate::from_ymd_opt(2023, 2, 2)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let agg_value = agg.process_value(&mut state, PeriodValue::new(start, TimeDelta::days(1).into(), 1.0));
        assert!(agg_value.is_none());
    }

    #[test]
    fn test_sub_daily_aggregation() {
        let values = vec![
            PeriodValue::new(
                NaiveDate::from_ymd_opt(2023, 1, 1)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap(),
                TimeDelta::hours(1).into(),
                2.0,
            ),
            PeriodValue::new(
                NaiveDate::from_ymd_opt(2023, 1, 1)
                    .unwrap()
                    .and_hms_opt(1, 0, 0)
                    .unwrap(),
                TimeDelta::hours(2).into(),
                1.0,
            ),
            PeriodValue::new(
                NaiveDate::from_ymd_opt(2023, 1, 1)
                    .unwrap()
                    .and_hms_opt(3, 0, 0)
                    .unwrap(),
                TimeDelta::hours(1).into(),
                3.0,
            ),
        ];

        let agg_value = AggregationFunction::Mean.calc_period_values(values.as_slice()).unwrap();
        assert_approx_eq!(f64, agg_value, 7.0 / 4.0);

        let agg_value = AggregationFunction::Sum.calc_period_values(values.as_slice()).unwrap();
        let expected = 2.0 * (1.0 / 24.0) + 1.0 * (2.0 / 24.0) + 3.0 * (1.0 / 24.0);
        assert_approx_eq!(f64, agg_value, expected);
    }
}
