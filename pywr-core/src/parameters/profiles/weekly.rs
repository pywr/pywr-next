use crate::PywrError;
use crate::parameters::{Parameter, ParameterMeta, ParameterName, ParameterState, SimpleParameter};
use crate::scenario::ScenarioIndex;
use crate::state::SimpleParameterValues;
use crate::timestep::Timestep;
use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};
use thiserror::Error;

pub enum WeeklyInterpDay {
    First,
    Last,
}

// A weekly profile can be 52 or 53 week long
pub enum WeeklyProfileValues {
    FiftyTwo([f64; 52]),
    FiftyThree([f64; 53]),
}

impl WeeklyProfileValues {
    /// Get the week position in a calendar year from date. In the first year week, the position
    /// starts from 0 on the first week day and ends with 1 on the last day. Seconds may be
    /// included in the position by setting with_seconds to true.
    fn current_pos(&self, date: &NaiveDateTime, with_seconds: bool) -> f64 {
        let mut current_day = date.ordinal() as f64;
        if with_seconds {
            let seconds_in_day = date.num_seconds_from_midnight() as f64 / 86400.0;
            current_day += seconds_in_day;
        }
        (current_day - 1.0) / 7.0
    }

    /// Get the week index from the provided date
    fn current_index(&self, date: &NaiveDateTime) -> usize {
        let current_day = date.ordinal();
        let current_pos = self.current_pos(date, false) as usize;

        // if year is leap the last week starts on the 365th day
        let is_leap_year = NaiveDate::from_ymd_opt(date.year(), 1, 1).unwrap().leap_year();
        let last_week_day_start = if is_leap_year { 365 } else { 364 };

        match self {
            Self::FiftyTwo(_) => {
                if current_day >= last_week_day_start {
                    51
                } else {
                    current_pos
                }
            }
            Self::FiftyThree(_) => {
                if current_day >= last_week_day_start {
                    52
                } else {
                    current_pos
                }
            }
        }
    }

    /// Get the value corresponding to the week index for the provided date
    fn current(&self, date: &NaiveDateTime) -> f64 {
        // The current_index function always returns and index between 0 and
        // 52 (for Self::FiftyTwo) or 53 (Self::FiftyThree). This ensures
        // that the index is always in range in the value array below
        let current_index = self.current_index(date);

        match self {
            Self::FiftyTwo(values) => values[current_index],
            Self::FiftyThree(values) => values[current_index],
        }
    }

    /// Get the next week's value based on the week index of the provided date. If the current
    /// week is larger than the array length, the value corresponding to the first week is
    /// returned.
    fn next(&self, date: &NaiveDateTime) -> f64 {
        let current_week_index = self.current_index(date);

        match self {
            Self::FiftyTwo(values) => {
                if current_week_index >= 51 {
                    values[0]
                } else {
                    values[current_week_index + 1]
                }
            }
            Self::FiftyThree(values) => {
                if current_week_index >= 52 {
                    values[0]
                } else {
                    values[current_week_index + 1]
                }
            }
        }
    }

    /// Get the previous week's value based on the week index of the provided date. If the
    /// current week index is 0 than the last array value is returned.
    fn prev(&self, date: &NaiveDateTime) -> f64 {
        let current_week_index = self.current_index(date);

        match self {
            Self::FiftyTwo(values) => {
                if current_week_index == 0 {
                    values[51]
                } else {
                    values[current_week_index - 1]
                }
            }
            Self::FiftyThree(values) => {
                if current_week_index == 0 {
                    values[52]
                } else {
                    values[current_week_index - 1]
                }
            }
        }
    }

    /// Find the value corresponding to the given date by linearly interpolating between two
    /// consecutive week's values.
    fn interpolate(&self, date: &NaiveDateTime, first_value: f64, last_value: f64) -> f64 {
        let current_pos = self.current_pos(date, true);
        let week_delta = current_pos - current_pos.floor();
        first_value + (last_value - first_value) * week_delta
    }

    /// Calculate the value on the given date using the interpolation method option. In a 52-week
    /// interpolated profile, the upper boundary in the 52nd and 53rd week is the same when
    /// WeeklyInterpDay is First (i.e. the value on 1st January). When WeeklyInterpDay is Last the
    /// 1st and last week will share the same lower bound (i.e. the value on the last week).
    fn value(&self, date: &NaiveDateTime, interp_day: &Option<WeeklyInterpDay>) -> f64 {
        match interp_day {
            None => self.current(date),
            Some(interp_day) => match interp_day {
                WeeklyInterpDay::First => {
                    let first_value = self.current(date);
                    let last_value = self.next(date);
                    self.interpolate(date, first_value, last_value)
                }
                WeeklyInterpDay::Last => {
                    let first_value = self.prev(date);
                    let last_value = self.current(date);
                    self.interpolate(date, first_value, last_value)
                }
            },
        }
    }
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum WeeklyProfileError {
    #[error("52 or 53 values must be given for a weekly profile parameter")]
    InvalidLength,
}

impl TryFrom<&[f64]> for WeeklyProfileValues {
    type Error = WeeklyProfileError;

    fn try_from(value: &[f64]) -> Result<Self, Self::Error> {
        match value.len() {
            52 => Ok(WeeklyProfileValues::FiftyTwo(value.try_into().unwrap())),
            53 => Ok(WeeklyProfileValues::FiftyThree(value.try_into().unwrap())),
            _ => Err(WeeklyProfileError::InvalidLength),
        }
    }
}

/// Weekly profile parameter. This supports a profile with either 52 or 53 weeks, with or without interpolation.
pub struct WeeklyProfileParameter {
    meta: ParameterMeta,
    values: WeeklyProfileValues,
    interp_day: Option<WeeklyInterpDay>,
}

impl WeeklyProfileParameter {
    pub fn new(name: ParameterName, values: WeeklyProfileValues, interp_day: Option<WeeklyInterpDay>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            values,
            interp_day,
        }
    }
}

impl Parameter for WeeklyProfileParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl SimpleParameter<f64> for WeeklyProfileParameter {
    fn compute(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _values: &SimpleParameterValues,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, PywrError> {
        Ok(self.values.value(&timestep.date, &self.interp_day))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::parameters::profiles::weekly::{WeeklyInterpDay, WeeklyProfileValues};
    use crate::test_utils::assert_approx_array_eq;
    use chrono::{Datelike, NaiveDate, TimeDelta};
    use float_cmp::{F64Margin, assert_approx_eq};

    /// Build a time-series from the weekly profile
    fn collect(week_size: &WeeklyProfileValues, interp_day: Option<WeeklyInterpDay>) -> Vec<f64> {
        let dt0 = NaiveDate::from_ymd_opt(2020, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let dt1 = NaiveDate::from_ymd_opt(2020, 12, 31)
            .unwrap()
            .and_hms_opt(23, 59, 59)
            .unwrap();

        let mut dt = dt0;
        let mut data: Vec<f64> = Vec::new();
        while dt <= dt1 {
            let date = NaiveDate::from_ymd_opt(dt.year(), dt.month(), dt.day())
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap();
            let value = week_size.value(&date, &interp_day);
            data.push(value);

            dt += TimeDelta::days(1);
        }
        data
    }

    /// Test a leap year with a profile of 52 values
    #[test]
    fn test_52_values() {
        let profile = [
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0, 19.0,
            20.0, 21.0, 22.0, 23.0, 24.0, 25.0, 26.0, 27.0, 28.0, 29.0, 30.0, 31.0, 32.0, 33.0, 34.0, 35.0, 36.0, 37.0,
            38.0, 39.0, 40.0, 41.0, 42.0, 43.0, 44.0, 45.0, 46.0, 47.0, 48.0, 49.0, 50.0, 51.0, 52.0,
        ];
        let week_size = WeeklyProfileValues::FiftyTwo(profile);

        // No interpolation
        let expected_values_interp_none = [
            1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0,
            4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 6.0, 6.0, 6.0, 6.0, 6.0, 6.0, 6.0,
            7.0, 7.0, 7.0, 7.0, 7.0, 7.0, 7.0, 8.0, 8.0, 8.0, 8.0, 8.0, 8.0, 8.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0,
            10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 11.0, 11.0, 11.0, 11.0, 11.0, 11.0, 11.0, 12.0, 12.0, 12.0, 12.0,
            12.0, 12.0, 12.0, 13.0, 13.0, 13.0, 13.0, 13.0, 13.0, 13.0, 14.0, 14.0, 14.0, 14.0, 14.0, 14.0, 14.0, 15.0,
            15.0, 15.0, 15.0, 15.0, 15.0, 15.0, 16.0, 16.0, 16.0, 16.0, 16.0, 16.0, 16.0, 17.0, 17.0, 17.0, 17.0, 17.0,
            17.0, 17.0, 18.0, 18.0, 18.0, 18.0, 18.0, 18.0, 18.0, 19.0, 19.0, 19.0, 19.0, 19.0, 19.0, 19.0, 20.0, 20.0,
            20.0, 20.0, 20.0, 20.0, 20.0, 21.0, 21.0, 21.0, 21.0, 21.0, 21.0, 21.0, 22.0, 22.0, 22.0, 22.0, 22.0, 22.0,
            22.0, 23.0, 23.0, 23.0, 23.0, 23.0, 23.0, 23.0, 24.0, 24.0, 24.0, 24.0, 24.0, 24.0, 24.0, 25.0, 25.0, 25.0,
            25.0, 25.0, 25.0, 25.0, 26.0, 26.0, 26.0, 26.0, 26.0, 26.0, 26.0, 27.0, 27.0, 27.0, 27.0, 27.0, 27.0, 27.0,
            28.0, 28.0, 28.0, 28.0, 28.0, 28.0, 28.0, 29.0, 29.0, 29.0, 29.0, 29.0, 29.0, 29.0, 30.0, 30.0, 30.0, 30.0,
            30.0, 30.0, 30.0, 31.0, 31.0, 31.0, 31.0, 31.0, 31.0, 31.0, 32.0, 32.0, 32.0, 32.0, 32.0, 32.0, 32.0, 33.0,
            33.0, 33.0, 33.0, 33.0, 33.0, 33.0, 34.0, 34.0, 34.0, 34.0, 34.0, 34.0, 34.0, 35.0, 35.0, 35.0, 35.0, 35.0,
            35.0, 35.0, 36.0, 36.0, 36.0, 36.0, 36.0, 36.0, 36.0, 37.0, 37.0, 37.0, 37.0, 37.0, 37.0, 37.0, 38.0, 38.0,
            38.0, 38.0, 38.0, 38.0, 38.0, 39.0, 39.0, 39.0, 39.0, 39.0, 39.0, 39.0, 40.0, 40.0, 40.0, 40.0, 40.0, 40.0,
            40.0, 41.0, 41.0, 41.0, 41.0, 41.0, 41.0, 41.0, 42.0, 42.0, 42.0, 42.0, 42.0, 42.0, 42.0, 43.0, 43.0, 43.0,
            43.0, 43.0, 43.0, 43.0, 44.0, 44.0, 44.0, 44.0, 44.0, 44.0, 44.0, 45.0, 45.0, 45.0, 45.0, 45.0, 45.0, 45.0,
            46.0, 46.0, 46.0, 46.0, 46.0, 46.0, 46.0, 47.0, 47.0, 47.0, 47.0, 47.0, 47.0, 47.0, 48.0, 48.0, 48.0, 48.0,
            48.0, 48.0, 48.0, 49.0, 49.0, 49.0, 49.0, 49.0, 49.0, 49.0, 50.0, 50.0, 50.0, 50.0, 50.0, 50.0, 50.0, 51.0,
            51.0, 51.0, 51.0, 51.0, 51.0, 51.0, 52.0, 52.0, 52.0, 52.0, 52.0, 52.0, 52.0, 52.0, 52.0,
        ];
        let values_interp_none = collect(&week_size, None);
        assert_approx_array_eq(&values_interp_none, &expected_values_interp_none);

        // WeeklyInterpDay::First
        let expected_values_interp_first = [
            1.0, 1.14286, 1.28571, 1.42857, 1.57143, 1.71429, 1.85714, 2.0, 2.14286, 2.28571, 2.42857, 2.57143,
            2.71429, 2.85714, 3.0, 3.14286, 3.28571, 3.42857, 3.57143, 3.71429, 3.85714, 4.0, 4.14286, 4.28571,
            4.42857, 4.57143, 4.71429, 4.85714, 5.0, 5.14286, 5.28571, 5.42857, 5.57143, 5.71429, 5.85714, 6.0,
            6.14286, 6.28571, 6.42857, 6.57143, 6.71429, 6.85714, 7.0, 7.14286, 7.28571, 7.42857, 7.57143, 7.71429,
            7.85714, 8.0, 8.14286, 8.28571, 8.42857, 8.57143, 8.71429, 8.85714, 9.0, 9.14286, 9.28571, 9.42857,
            9.57143, 9.71429, 9.85714, 10.0, 10.14286, 10.28571, 10.42857, 10.57143, 10.71429, 10.85714, 11.0,
            11.14286, 11.28571, 11.42857, 11.57143, 11.71429, 11.85714, 12.0, 12.14286, 12.28571, 12.42857, 12.57143,
            12.71429, 12.85714, 13.0, 13.14286, 13.28571, 13.42857, 13.57143, 13.71429, 13.85714, 14.0, 14.14286,
            14.28571, 14.42857, 14.57143, 14.71429, 14.85714, 15.0, 15.14286, 15.28571, 15.42857, 15.57143, 15.71429,
            15.85714, 16.0, 16.14286, 16.28571, 16.42857, 16.57143, 16.71429, 16.85714, 17.0, 17.14286, 17.28571,
            17.42857, 17.57143, 17.71429, 17.85714, 18.0, 18.14286, 18.28571, 18.42857, 18.57143, 18.71429, 18.85714,
            19.0, 19.14286, 19.28571, 19.42857, 19.57143, 19.71429, 19.85714, 20.0, 20.14286, 20.28571, 20.42857,
            20.57143, 20.71429, 20.85714, 21.0, 21.14286, 21.28571, 21.42857, 21.57143, 21.71429, 21.85714, 22.0,
            22.14286, 22.28571, 22.42857, 22.57143, 22.71429, 22.85714, 23.0, 23.14286, 23.28571, 23.42857, 23.57143,
            23.71429, 23.85714, 24.0, 24.14286, 24.28571, 24.42857, 24.57143, 24.71429, 24.85714, 25.0, 25.14286,
            25.28571, 25.42857, 25.57143, 25.71429, 25.85714, 26.0, 26.14286, 26.28571, 26.42857, 26.57143, 26.71429,
            26.85714, 27.0, 27.14286, 27.28571, 27.42857, 27.57143, 27.71429, 27.85714, 28.0, 28.14286, 28.28571,
            28.42857, 28.57143, 28.71429, 28.85714, 29.0, 29.14286, 29.28571, 29.42857, 29.57143, 29.71429, 29.85714,
            30.0, 30.14286, 30.28571, 30.42857, 30.57143, 30.71429, 30.85714, 31.0, 31.14286, 31.28571, 31.42857,
            31.57143, 31.71429, 31.85714, 32.0, 32.14286, 32.28571, 32.42857, 32.57143, 32.71429, 32.85714, 33.0,
            33.14286, 33.28571, 33.42857, 33.57143, 33.71429, 33.85714, 34.0, 34.14286, 34.28571, 34.42857, 34.57143,
            34.71429, 34.85714, 35.0, 35.14286, 35.28571, 35.42857, 35.57143, 35.71429, 35.85714, 36.0, 36.14286,
            36.28571, 36.42857, 36.57143, 36.71429, 36.85714, 37.0, 37.14286, 37.28571, 37.42857, 37.57143, 37.71429,
            37.85714, 38.0, 38.14286, 38.28571, 38.42857, 38.57143, 38.71429, 38.85714, 39.0, 39.14286, 39.28571,
            39.42857, 39.57143, 39.71429, 39.85714, 40.0, 40.14286, 40.28571, 40.42857, 40.57143, 40.71429, 40.85714,
            41.0, 41.14286, 41.28571, 41.42857, 41.57143, 41.71429, 41.85714, 42.0, 42.14286, 42.28571, 42.42857,
            42.57143, 42.71429, 42.85714, 43.0, 43.14286, 43.28571, 43.42857, 43.57143, 43.71429, 43.85714, 44.0,
            44.14286, 44.28571, 44.42857, 44.57143, 44.71429, 44.85714, 45.0, 45.14286, 45.28571, 45.42857, 45.57143,
            45.71429, 45.85714, 46.0, 46.14286, 46.28571, 46.42857, 46.57143, 46.71429, 46.85714, 47.0, 47.14286,
            47.28571, 47.42857, 47.57143, 47.71429, 47.85714, 48.0, 48.14286, 48.28571, 48.42857, 48.57143, 48.71429,
            48.85714, 49.0, 49.14286, 49.28571, 49.42857, 49.57143, 49.71429, 49.85714, 50.0, 50.14286, 50.28571,
            50.42857, 50.57143, 50.71429, 50.85714, 51.0, 51.14286, 51.28571, 51.42857, 51.57143, 51.71429, 51.85714,
            52.0, 44.71429, 37.42857, 30.14286, 22.85714, 15.57143, 8.28571, 52.0, 44.71429, 37.42857,
        ];
        let values_interp_first = collect(&week_size, Some(WeeklyInterpDay::First));
        assert_approx_array_eq(&values_interp_first, &expected_values_interp_first);

        // WeeklyInterpDay::Last
        let expected_values_interp_last = [
            52.0, 44.71429, 37.42857, 30.14286, 22.85714, 15.57143, 8.28571, 1.0, 1.14286, 1.28571, 1.42857, 1.57143,
            1.71429, 1.85714, 2.0, 2.14286, 2.28571, 2.42857, 2.57143, 2.71429, 2.85714, 3.0, 3.14286, 3.28571,
            3.42857, 3.57143, 3.71429, 3.85714, 4.0, 4.14286, 4.28571, 4.42857, 4.57143, 4.71429, 4.85714, 5.0,
            5.14286, 5.28571, 5.42857, 5.57143, 5.71429, 5.85714, 6.0, 6.14286, 6.28571, 6.42857, 6.57143, 6.71429,
            6.85714, 7.0, 7.14286, 7.28571, 7.42857, 7.57143, 7.71429, 7.85714, 8.0, 8.14286, 8.28571, 8.42857,
            8.57143, 8.71429, 8.85714, 9.0, 9.14286, 9.28571, 9.42857, 9.57143, 9.71429, 9.85714, 10.0, 10.14286,
            10.28571, 10.42857, 10.57143, 10.71429, 10.85714, 11.0, 11.14286, 11.28571, 11.42857, 11.57143, 11.71429,
            11.85714, 12.0, 12.14286, 12.28571, 12.42857, 12.57143, 12.71429, 12.85714, 13.0, 13.14286, 13.28571,
            13.42857, 13.57143, 13.71429, 13.85714, 14.0, 14.14286, 14.28571, 14.42857, 14.57143, 14.71429, 14.85714,
            15.0, 15.14286, 15.28571, 15.42857, 15.57143, 15.71429, 15.85714, 16.0, 16.14286, 16.28571, 16.42857,
            16.57143, 16.71429, 16.85714, 17.0, 17.14286, 17.28571, 17.42857, 17.57143, 17.71429, 17.85714, 18.0,
            18.14286, 18.28571, 18.42857, 18.57143, 18.71429, 18.85714, 19.0, 19.14286, 19.28571, 19.42857, 19.57143,
            19.71429, 19.85714, 20.0, 20.14286, 20.28571, 20.42857, 20.57143, 20.71429, 20.85714, 21.0, 21.14286,
            21.28571, 21.42857, 21.57143, 21.71429, 21.85714, 22.0, 22.14286, 22.28571, 22.42857, 22.57143, 22.71429,
            22.85714, 23.0, 23.14286, 23.28571, 23.42857, 23.57143, 23.71429, 23.85714, 24.0, 24.14286, 24.28571,
            24.42857, 24.57143, 24.71429, 24.85714, 25.0, 25.14286, 25.28571, 25.42857, 25.57143, 25.71429, 25.85714,
            26.0, 26.14286, 26.28571, 26.42857, 26.57143, 26.71429, 26.85714, 27.0, 27.14286, 27.28571, 27.42857,
            27.57143, 27.71429, 27.85714, 28.0, 28.14286, 28.28571, 28.42857, 28.57143, 28.71429, 28.85714, 29.0,
            29.14286, 29.28571, 29.42857, 29.57143, 29.71429, 29.85714, 30.0, 30.14286, 30.28571, 30.42857, 30.57143,
            30.71429, 30.85714, 31.0, 31.14286, 31.28571, 31.42857, 31.57143, 31.71429, 31.85714, 32.0, 32.14286,
            32.28571, 32.42857, 32.57143, 32.71429, 32.85714, 33.0, 33.14286, 33.28571, 33.42857, 33.57143, 33.71429,
            33.85714, 34.0, 34.14286, 34.28571, 34.42857, 34.57143, 34.71429, 34.85714, 35.0, 35.14286, 35.28571,
            35.42857, 35.57143, 35.71429, 35.85714, 36.0, 36.14286, 36.28571, 36.42857, 36.57143, 36.71429, 36.85714,
            37.0, 37.14286, 37.28571, 37.42857, 37.57143, 37.71429, 37.85714, 38.0, 38.14286, 38.28571, 38.42857,
            38.57143, 38.71429, 38.85714, 39.0, 39.14286, 39.28571, 39.42857, 39.57143, 39.71429, 39.85714, 40.0,
            40.14286, 40.28571, 40.42857, 40.57143, 40.71429, 40.85714, 41.0, 41.14286, 41.28571, 41.42857, 41.57143,
            41.71429, 41.85714, 42.0, 42.14286, 42.28571, 42.42857, 42.57143, 42.71429, 42.85714, 43.0, 43.14286,
            43.28571, 43.42857, 43.57143, 43.71429, 43.85714, 44.0, 44.14286, 44.28571, 44.42857, 44.57143, 44.71429,
            44.85714, 45.0, 45.14286, 45.28571, 45.42857, 45.57143, 45.71429, 45.85714, 46.0, 46.14286, 46.28571,
            46.42857, 46.57143, 46.71429, 46.85714, 47.0, 47.14286, 47.28571, 47.42857, 47.57143, 47.71429, 47.85714,
            48.0, 48.14286, 48.28571, 48.42857, 48.57143, 48.71429, 48.85714, 49.0, 49.14286, 49.28571, 49.42857,
            49.57143, 49.71429, 49.85714, 50.0, 50.14286, 50.28571, 50.42857, 50.57143, 50.71429, 50.85714, 51.0,
            51.14286, 51.28571, 51.42857, 51.57143, 51.71429, 51.85714, 52.0, 51.875,
        ];
        let values_interp_none = collect(&week_size, Some(WeeklyInterpDay::Last));
        assert_approx_array_eq(&values_interp_none, &expected_values_interp_last);
    }

    /// Test a leap year with a profile of 53 values
    #[test]
    fn test_53_values() {
        let profile = [
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0, 19.0,
            20.0, 21.0, 22.0, 23.0, 24.0, 25.0, 26.0, 27.0, 28.0, 29.0, 30.0, 31.0, 32.0, 33.0, 34.0, 35.0, 36.0, 37.0,
            38.0, 39.0, 40.0, 41.0, 42.0, 43.0, 44.0, 45.0, 46.0, 47.0, 48.0, 49.0, 50.0, 51.0, 52.0, 53.0,
        ];
        let week_size = WeeklyProfileValues::FiftyThree(profile);

        // No interpolation
        let expected_values_interp_none = [
            1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0,
            4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 6.0, 6.0, 6.0, 6.0, 6.0, 6.0, 6.0,
            7.0, 7.0, 7.0, 7.0, 7.0, 7.0, 7.0, 8.0, 8.0, 8.0, 8.0, 8.0, 8.0, 8.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0,
            10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 11.0, 11.0, 11.0, 11.0, 11.0, 11.0, 11.0, 12.0, 12.0, 12.0, 12.0,
            12.0, 12.0, 12.0, 13.0, 13.0, 13.0, 13.0, 13.0, 13.0, 13.0, 14.0, 14.0, 14.0, 14.0, 14.0, 14.0, 14.0, 15.0,
            15.0, 15.0, 15.0, 15.0, 15.0, 15.0, 16.0, 16.0, 16.0, 16.0, 16.0, 16.0, 16.0, 17.0, 17.0, 17.0, 17.0, 17.0,
            17.0, 17.0, 18.0, 18.0, 18.0, 18.0, 18.0, 18.0, 18.0, 19.0, 19.0, 19.0, 19.0, 19.0, 19.0, 19.0, 20.0, 20.0,
            20.0, 20.0, 20.0, 20.0, 20.0, 21.0, 21.0, 21.0, 21.0, 21.0, 21.0, 21.0, 22.0, 22.0, 22.0, 22.0, 22.0, 22.0,
            22.0, 23.0, 23.0, 23.0, 23.0, 23.0, 23.0, 23.0, 24.0, 24.0, 24.0, 24.0, 24.0, 24.0, 24.0, 25.0, 25.0, 25.0,
            25.0, 25.0, 25.0, 25.0, 26.0, 26.0, 26.0, 26.0, 26.0, 26.0, 26.0, 27.0, 27.0, 27.0, 27.0, 27.0, 27.0, 27.0,
            28.0, 28.0, 28.0, 28.0, 28.0, 28.0, 28.0, 29.0, 29.0, 29.0, 29.0, 29.0, 29.0, 29.0, 30.0, 30.0, 30.0, 30.0,
            30.0, 30.0, 30.0, 31.0, 31.0, 31.0, 31.0, 31.0, 31.0, 31.0, 32.0, 32.0, 32.0, 32.0, 32.0, 32.0, 32.0, 33.0,
            33.0, 33.0, 33.0, 33.0, 33.0, 33.0, 34.0, 34.0, 34.0, 34.0, 34.0, 34.0, 34.0, 35.0, 35.0, 35.0, 35.0, 35.0,
            35.0, 35.0, 36.0, 36.0, 36.0, 36.0, 36.0, 36.0, 36.0, 37.0, 37.0, 37.0, 37.0, 37.0, 37.0, 37.0, 38.0, 38.0,
            38.0, 38.0, 38.0, 38.0, 38.0, 39.0, 39.0, 39.0, 39.0, 39.0, 39.0, 39.0, 40.0, 40.0, 40.0, 40.0, 40.0, 40.0,
            40.0, 41.0, 41.0, 41.0, 41.0, 41.0, 41.0, 41.0, 42.0, 42.0, 42.0, 42.0, 42.0, 42.0, 42.0, 43.0, 43.0, 43.0,
            43.0, 43.0, 43.0, 43.0, 44.0, 44.0, 44.0, 44.0, 44.0, 44.0, 44.0, 45.0, 45.0, 45.0, 45.0, 45.0, 45.0, 45.0,
            46.0, 46.0, 46.0, 46.0, 46.0, 46.0, 46.0, 47.0, 47.0, 47.0, 47.0, 47.0, 47.0, 47.0, 48.0, 48.0, 48.0, 48.0,
            48.0, 48.0, 48.0, 49.0, 49.0, 49.0, 49.0, 49.0, 49.0, 49.0, 50.0, 50.0, 50.0, 50.0, 50.0, 50.0, 50.0, 51.0,
            51.0, 51.0, 51.0, 51.0, 51.0, 51.0, 52.0, 52.0, 52.0, 52.0, 52.0, 52.0, 52.0, 53.0, 53.0,
        ];
        let values_interp_none = collect(&week_size, None);
        assert_approx_array_eq(&values_interp_none, &expected_values_interp_none);

        // WeeklyInterpDay::First
        let expected_values_interp_first = [
            1.0, 1.14286, 1.28571, 1.42857, 1.57143, 1.71429, 1.85714, 2.0, 2.14286, 2.28571, 2.42857, 2.57143,
            2.71429, 2.85714, 3.0, 3.14286, 3.28571, 3.42857, 3.57143, 3.71429, 3.85714, 4.0, 4.14286, 4.28571,
            4.42857, 4.57143, 4.71429, 4.85714, 5.0, 5.14286, 5.28571, 5.42857, 5.57143, 5.71429, 5.85714, 6.0,
            6.14286, 6.28571, 6.42857, 6.57143, 6.71429, 6.85714, 7.0, 7.14286, 7.28571, 7.42857, 7.57143, 7.71429,
            7.85714, 8.0, 8.14286, 8.28571, 8.42857, 8.57143, 8.71429, 8.85714, 9.0, 9.14286, 9.28571, 9.42857,
            9.57143, 9.71429, 9.85714, 10.0, 10.14286, 10.28571, 10.42857, 10.57143, 10.71429, 10.85714, 11.0,
            11.14286, 11.28571, 11.42857, 11.57143, 11.71429, 11.85714, 12.0, 12.14286, 12.28571, 12.42857, 12.57143,
            12.71429, 12.85714, 13.0, 13.14286, 13.28571, 13.42857, 13.57143, 13.71429, 13.85714, 14.0, 14.14286,
            14.28571, 14.42857, 14.57143, 14.71429, 14.85714, 15.0, 15.14286, 15.28571, 15.42857, 15.57143, 15.71429,
            15.85714, 16.0, 16.14286, 16.28571, 16.42857, 16.57143, 16.71429, 16.85714, 17.0, 17.14286, 17.28571,
            17.42857, 17.57143, 17.71429, 17.85714, 18.0, 18.14286, 18.28571, 18.42857, 18.57143, 18.71429, 18.85714,
            19.0, 19.14286, 19.28571, 19.42857, 19.57143, 19.71429, 19.85714, 20.0, 20.14286, 20.28571, 20.42857,
            20.57143, 20.71429, 20.85714, 21.0, 21.14286, 21.28571, 21.42857, 21.57143, 21.71429, 21.85714, 22.0,
            22.14286, 22.28571, 22.42857, 22.57143, 22.71429, 22.85714, 23.0, 23.14286, 23.28571, 23.42857, 23.57143,
            23.71429, 23.85714, 24.0, 24.14286, 24.28571, 24.42857, 24.57143, 24.71429, 24.85714, 25.0, 25.14286,
            25.28571, 25.42857, 25.57143, 25.71429, 25.85714, 26.0, 26.14286, 26.28571, 26.42857, 26.57143, 26.71429,
            26.85714, 27.0, 27.14286, 27.28571, 27.42857, 27.57143, 27.71429, 27.85714, 28.0, 28.14286, 28.28571,
            28.42857, 28.57143, 28.71429, 28.85714, 29.0, 29.14286, 29.28571, 29.42857, 29.57143, 29.71429, 29.85714,
            30.0, 30.14286, 30.28571, 30.42857, 30.57143, 30.71429, 30.85714, 31.0, 31.14286, 31.28571, 31.42857,
            31.57143, 31.71429, 31.85714, 32.0, 32.14286, 32.28571, 32.42857, 32.57143, 32.71429, 32.85714, 33.0,
            33.14286, 33.28571, 33.42857, 33.57143, 33.71429, 33.85714, 34.0, 34.14286, 34.28571, 34.42857, 34.57143,
            34.71429, 34.85714, 35.0, 35.14286, 35.28571, 35.42857, 35.57143, 35.71429, 35.85714, 36.0, 36.14286,
            36.28571, 36.42857, 36.57143, 36.71429, 36.85714, 37.0, 37.14286, 37.28571, 37.42857, 37.57143, 37.71429,
            37.85714, 38.0, 38.14286, 38.28571, 38.42857, 38.57143, 38.71429, 38.85714, 39.0, 39.14286, 39.28571,
            39.42857, 39.57143, 39.71429, 39.85714, 40.0, 40.14286, 40.28571, 40.42857, 40.57143, 40.71429, 40.85714,
            41.0, 41.14286, 41.28571, 41.42857, 41.57143, 41.71429, 41.85714, 42.0, 42.14286, 42.28571, 42.42857,
            42.57143, 42.71429, 42.85714, 43.0, 43.14286, 43.28571, 43.42857, 43.57143, 43.71429, 43.85714, 44.0,
            44.14286, 44.28571, 44.42857, 44.57143, 44.71429, 44.85714, 45.0, 45.14286, 45.28571, 45.42857, 45.57143,
            45.71429, 45.85714, 46.0, 46.14286, 46.28571, 46.42857, 46.57143, 46.71429, 46.85714, 47.0, 47.14286,
            47.28571, 47.42857, 47.57143, 47.71429, 47.85714, 48.0, 48.14286, 48.28571, 48.42857, 48.57143, 48.71429,
            48.85714, 49.0, 49.14286, 49.28571, 49.42857, 49.57143, 49.71429, 49.85714, 50.0, 50.14286, 50.28571,
            50.42857, 50.57143, 50.71429, 50.85714, 51.0, 51.14286, 51.28571, 51.42857, 51.57143, 51.71429, 51.85714,
            52.0, 52.14286, 52.28571, 52.42857, 52.57143, 52.71429, 52.85714, 53.0, 45.57143,
        ];
        let values_interp_first = collect(&week_size, Some(WeeklyInterpDay::First));
        assert_approx_array_eq(&values_interp_first, &expected_values_interp_first);

        // WeeklyInterpDay::Last
        let expected_values_interp_last = [
            53.0, 45.57143, 38.14286, 30.71429, 23.28571, 15.85714, 8.42857, 1.0, 1.14286, 1.28571, 1.42857, 1.57143,
            1.71429, 1.85714, 2.0, 2.14286, 2.28571, 2.42857, 2.57143, 2.71429, 2.85714, 3.0, 3.14286, 3.28571,
            3.42857, 3.57143, 3.71429, 3.85714, 4.0, 4.14286, 4.28571, 4.42857, 4.57143, 4.71429, 4.85714, 5.0,
            5.14286, 5.28571, 5.42857, 5.57143, 5.71429, 5.85714, 6.0, 6.14286, 6.28571, 6.42857, 6.57143, 6.71429,
            6.85714, 7.0, 7.14286, 7.28571, 7.42857, 7.57143, 7.71429, 7.85714, 8.0, 8.14286, 8.28571, 8.42857,
            8.57143, 8.71429, 8.85714, 9.0, 9.14286, 9.28571, 9.42857, 9.57143, 9.71429, 9.85714, 10.0, 10.14286,
            10.28571, 10.42857, 10.57143, 10.71429, 10.85714, 11.0, 11.14286, 11.28571, 11.42857, 11.57143, 11.71429,
            11.85714, 12.0, 12.14286, 12.28571, 12.42857, 12.57143, 12.71429, 12.85714, 13.0, 13.14286, 13.28571,
            13.42857, 13.57143, 13.71429, 13.85714, 14.0, 14.14286, 14.28571, 14.42857, 14.57143, 14.71429, 14.85714,
            15.0, 15.14286, 15.28571, 15.42857, 15.57143, 15.71429, 15.85714, 16.0, 16.14286, 16.28571, 16.42857,
            16.57143, 16.71429, 16.85714, 17.0, 17.14286, 17.28571, 17.42857, 17.57143, 17.71429, 17.85714, 18.0,
            18.14286, 18.28571, 18.42857, 18.57143, 18.71429, 18.85714, 19.0, 19.14286, 19.28571, 19.42857, 19.57143,
            19.71429, 19.85714, 20.0, 20.14286, 20.28571, 20.42857, 20.57143, 20.71429, 20.85714, 21.0, 21.14286,
            21.28571, 21.42857, 21.57143, 21.71429, 21.85714, 22.0, 22.14286, 22.28571, 22.42857, 22.57143, 22.71429,
            22.85714, 23.0, 23.14286, 23.28571, 23.42857, 23.57143, 23.71429, 23.85714, 24.0, 24.14286, 24.28571,
            24.42857, 24.57143, 24.71429, 24.85714, 25.0, 25.14286, 25.28571, 25.42857, 25.57143, 25.71429, 25.85714,
            26.0, 26.14286, 26.28571, 26.42857, 26.57143, 26.71429, 26.85714, 27.0, 27.14286, 27.28571, 27.42857,
            27.57143, 27.71429, 27.85714, 28.0, 28.14286, 28.28571, 28.42857, 28.57143, 28.71429, 28.85714, 29.0,
            29.14286, 29.28571, 29.42857, 29.57143, 29.71429, 29.85714, 30.0, 30.14286, 30.28571, 30.42857, 30.57143,
            30.71429, 30.85714, 31.0, 31.14286, 31.28571, 31.42857, 31.57143, 31.71429, 31.85714, 32.0, 32.14286,
            32.28571, 32.42857, 32.57143, 32.71429, 32.85714, 33.0, 33.14286, 33.28571, 33.42857, 33.57143, 33.71429,
            33.85714, 34.0, 34.14286, 34.28571, 34.42857, 34.57143, 34.71429, 34.85714, 35.0, 35.14286, 35.28571,
            35.42857, 35.57143, 35.71429, 35.85714, 36.0, 36.14286, 36.28571, 36.42857, 36.57143, 36.71429, 36.85714,
            37.0, 37.14286, 37.28571, 37.42857, 37.57143, 37.71429, 37.85714, 38.0, 38.14286, 38.28571, 38.42857,
            38.57143, 38.71429, 38.85714, 39.0, 39.14286, 39.28571, 39.42857, 39.57143, 39.71429, 39.85714, 40.0,
            40.14286, 40.28571, 40.42857, 40.57143, 40.71429, 40.85714, 41.0, 41.14286, 41.28571, 41.42857, 41.57143,
            41.71429, 41.85714, 42.0, 42.14286, 42.28571, 42.42857, 42.57143, 42.71429, 42.85714, 43.0, 43.14286,
            43.28571, 43.42857, 43.57143, 43.71429, 43.85714, 44.0, 44.14286, 44.28571, 44.42857, 44.57143, 44.71429,
            44.85714, 45.0, 45.14286, 45.28571, 45.42857, 45.57143, 45.71429, 45.85714, 46.0, 46.14286, 46.28571,
            46.42857, 46.57143, 46.71429, 46.85714, 47.0, 47.14286, 47.28571, 47.42857, 47.57143, 47.71429, 47.85714,
            48.0, 48.14286, 48.28571, 48.42857, 48.57143, 48.71429, 48.85714, 49.0, 49.14286, 49.28571, 49.42857,
            49.57143, 49.71429, 49.85714, 50.0, 50.14286, 50.28571, 50.42857, 50.57143, 50.71429, 50.85714, 51.0,
            51.14286, 51.28571, 51.42857, 51.57143, 51.71429, 51.85714, 52.0, 52.125,
        ];
        let values_interp_none = collect(&week_size, Some(WeeklyInterpDay::Last));
        assert_approx_array_eq(&values_interp_none, &expected_values_interp_last);
    }

    /// Test the interpolation with the time
    #[test]
    fn test_time_interpolation() {
        let profile = [
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0, 19.0,
            20.0, 21.0, 22.0, 23.0, 24.0, 25.0, 26.0, 27.0, 28.0, 29.0, 30.0, 31.0, 32.0, 33.0, 34.0, 35.0, 36.0, 37.0,
            38.0, 39.0, 40.0, 41.0, 42.0, 43.0, 44.0, 45.0, 46.0, 47.0, 48.0, 49.0, 50.0, 51.0, 52.0,
        ];
        let week_size = WeeklyProfileValues::FiftyTwo(profile);

        let t0 = NaiveDate::from_ymd_opt(2016, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        assert_eq!(week_size.interpolate(&t0, 0.0, 1.0), 0.0);

        let t0 = NaiveDate::from_ymd_opt(2016, 1, 7)
            .unwrap()
            .and_hms_opt(12, 00, 00)
            .unwrap();
        let margins = F64Margin {
            epsilon: 2.0,
            ulps: (f64::EPSILON * 2.0) as i64,
        };
        assert_approx_eq!(f64, week_size.interpolate(&t0, 0.0, 1.0), 1.928571429, margins);
    }
}
