use crate::PywrError;
use chrono::Datelike;
use chrono::{Months, NaiveDateTime, TimeDelta};
use polars::datatypes::TimeUnit;
use polars::time::ClosedWindow;
#[cfg(feature = "pyo3")]
use pyo3::pyclass;
use std::ops::Add;

const SECS_IN_DAY: i64 = 60 * 60 * 24;
const MILLISECS_IN_DAY: i64 = 1000 * SECS_IN_DAY;
const MILLISECS_IN_HOUR: i64 = 1000 * 60 * 60;
const MILLISECS_IN_MINUTE: i64 = 1000 * 60;
const MILLISECS_IN_SECOND: i64 = 1000;

fn is_leap_year(year: i32) -> bool {
    // see http://stackoverflow.com/a/11595914/1300519
    (year & 3) == 0 && ((year % 25) != 0 || (year & 15) == 0)
}

/// A newtype for `chrono::TimeDelta` that provides a couple of useful convenience methods.
#[derive(Debug, Copy, Clone)]
pub struct PywrDuration(TimeDelta);

impl From<TimeDelta> for PywrDuration {
    fn from(duration: TimeDelta) -> Self {
        Self(duration)
    }
}

impl PartialEq<TimeDelta> for PywrDuration {
    fn eq(&self, other: &TimeDelta) -> bool {
        self.0 == *other
    }
}

impl PartialEq<PywrDuration> for PywrDuration {
    fn eq(&self, other: &PywrDuration) -> bool {
        self.0 == other.0
    }
}

impl Add<NaiveDateTime> for PywrDuration {
    type Output = NaiveDateTime;

    fn add(self, datetime: NaiveDateTime) -> NaiveDateTime {
        datetime + self.0
    }
}

impl PywrDuration {
    /// Create a new `PywrDuration` from a number of days.
    pub fn days(days: i64) -> Self {
        Self(TimeDelta::days(days))
    }

    /// Returns the number of whole days in the duration, if the total duration is a whole number of days.
    pub fn whole_days(&self) -> Option<i64> {
        if self.0.num_seconds() % SECS_IN_DAY == 0 {
            Some(self.0.num_days())
        } else {
            None
        }
    }

    /// Returns the fractional number of days in the duration.
    pub fn fractional_days(&self) -> f64 {
        self.0.num_seconds() as f64 / SECS_IN_DAY as f64
    }

    /// Returns the number of milliseconds in the duration.
    pub fn milliseconds(&self) -> i64 {
        self.0.num_milliseconds()
    }

    /// Convert the duration to a string representation that can be parsed by polars
    /// see: <https://docs.rs/polars/latest/polars/prelude/struct.Duration.html#method.parse>
    pub fn duration_string(&self) -> String {
        let milliseconds = self.milliseconds();
        let mut duration = String::new();
        let days = milliseconds / MILLISECS_IN_DAY;
        if days > 0 {
            duration.push_str(&format!("{days}d",));
        }
        let hours = (milliseconds % MILLISECS_IN_DAY) / MILLISECS_IN_HOUR;
        if hours > 0 {
            duration.push_str(&format!("{hours}h",));
        }
        let minutes = (milliseconds % MILLISECS_IN_HOUR) / MILLISECS_IN_MINUTE;
        if minutes > 0 {
            duration.push_str(&format!("{minutes}m",));
        }
        let seconds = (milliseconds % MILLISECS_IN_MINUTE) / MILLISECS_IN_SECOND;
        if seconds > 0 {
            duration.push_str(&format!("{seconds}s",));
        }
        let milliseconds = milliseconds % MILLISECS_IN_SECOND;
        if milliseconds > 0 {
            duration.push_str(&format!("{milliseconds}ms",));
        }
        duration
    }
}

pub type TimestepIndex = usize;

#[cfg_attr(feature = "pyo3", pyclass)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Timestep {
    pub date: NaiveDateTime,
    pub index: TimestepIndex,
    pub duration: PywrDuration,
}

impl Timestep {
    pub fn new(date: NaiveDateTime, index: TimestepIndex, duration: PywrDuration) -> Self {
        Self { date, index, duration }
    }

    pub fn is_first(&self) -> bool {
        self.index == 0
    }

    pub(crate) fn days(&self) -> f64 {
        self.duration.fractional_days()
    }

    /// Returns the day of the year index of the timestep.
    ///
    /// The index is zero-based and accounts for leaps days. In non-leap years, 1 is added is added to the index for
    /// days after Feb 28th.
    pub fn day_of_year_index(&self) -> usize {
        let mut i = self.date.ordinal() as usize - 1;
        if !is_leap_year(self.date.year()) && i > 58 {
            i += 1;
        }
        i
    }
}

impl Add<PywrDuration> for Timestep {
    type Output = Timestep;

    fn add(self, other: PywrDuration) -> Self {
        Self {
            date: self.date + other.0,
            index: self.index + 1,
            duration: other,
        }
    }
}

#[derive(Debug)]
pub enum TimestepDuration {
    Days(i64),
    Frequency(String),
}

#[derive(Debug)]
pub struct Timestepper {
    start: NaiveDateTime,
    end: NaiveDateTime,
    timestep: TimestepDuration,
}

impl Timestepper {
    pub fn new(start: NaiveDateTime, end: NaiveDateTime, timestep: TimestepDuration) -> Self {
        Self { start, end, timestep }
    }

    /// Create a vector of `Timestep`s between the start and end dates at the given duration.
    fn timesteps(&self) -> Result<Vec<Timestep>, PywrError> {
        match &self.timestep {
            TimestepDuration::Days(days) => Ok(self.generate_timesteps_from_days(*days)),
            TimestepDuration::Frequency(frequency) => self.generate_timesteps_from_frequency(frequency.as_str()),
        }
    }

    /// Creates a vector of `Timestep`s between the start and end dates at the given duration of days.
    fn generate_timesteps_from_days(&self, days: i64) -> Vec<Timestep> {
        let mut timesteps: Vec<Timestep> = Vec::new();
        let duration = PywrDuration::days(days);
        let mut current = Timestep::new(self.start, 0, duration);

        while current.date <= self.end {
            let next = current + duration;
            timesteps.push(current);
            current = next;
        }
        timesteps
    }

    /// Creates a vector of `Timestep`s between the start and end dates for a given frequency `&str`.
    ///
    /// Valid frequency strings are those that can be parsed by `polars::time::Duration::parse`. See: [https://docs.rs/polars-time/latest/polars_time/struct.Duration.html#method.parse]
    fn generate_timesteps_from_frequency(&self, frequency: &str) -> Result<Vec<Timestep>, PywrError> {
        let duration = polars::time::Duration::parse(frequency);

        // Need to add an extra day to the end date so that the duration of the last timestep can be calculated.
        let end = if duration.days_only() {
            self.end + TimeDelta::days(duration.days())
        } else if duration.weeks_only() {
            self.end + TimeDelta::weeks(duration.weeks())
        } else if duration.months_only() {
            let months = Months::new(duration.months() as u32);
            self.end + months
        } else {
            let months = Months::new(duration.months() as u32);
            self.end
                + months
                + TimeDelta::days(duration.days())
                + TimeDelta::weeks(duration.weeks())
                + TimeDelta::nanoseconds(duration.nanoseconds())
        };

        let dates = polars::time::date_range(
            "timesteps".into(),
            self.start,
            end,
            duration,
            ClosedWindow::Both,
            TimeUnit::Milliseconds,
            None,
        )
        .map_err(|e| PywrError::TimestepRangeGenerationError(e.to_string()))?
        .as_datetime_iter()
        .map(|x| x.ok_or(PywrError::TimestepGenerationError(frequency.to_string())))
        .collect::<Result<Vec<NaiveDateTime>, PywrError>>()?;

        let timesteps = dates
            .windows(2)
            .enumerate()
            .map(|(i, dates)| {
                let duration = dates[1] - dates[0];
                Timestep::new(dates[0], i, duration.into())
            })
            .collect::<Vec<Timestep>>();

        Ok(timesteps)
    }
}

/// The time domain that a model will be simulated over.
#[derive(Debug, Clone)]
pub struct TimeDomain {
    timesteps: Vec<Timestep>,
    duration: PywrDuration,
}

impl TimeDomain {
    /// Return the duration of each time-step.
    pub fn step_duration(&self) -> PywrDuration {
        self.duration
    }

    pub fn timesteps(&self) -> &[Timestep] {
        &self.timesteps
    }

    /// The total number of time-steps in the domain.
    pub fn len(&self) -> usize {
        self.timesteps.len()
    }

    pub fn first_timestep(&self) -> &Timestep {
        self.timesteps.first().expect("No time-steps defined.")
    }

    pub fn last_timestep(&self) -> &Timestep {
        self.timesteps.last().expect("No time-steps defined.")
    }

    pub fn is_empty(&self) -> bool {
        self.timesteps.is_empty()
    }
}

impl TryFrom<Timestepper> for TimeDomain {
    type Error = PywrError;

    fn try_from(value: Timestepper) -> Result<Self, Self::Error> {
        let timesteps = value.timesteps()?;
        let duration = timesteps.first().expect("No time-steps defined.").duration;
        match timesteps.iter().all(|t| t.duration == duration) {
            true => Ok(Self { timesteps, duration }),
            false => Err(PywrError::TimestepDurationMismatch),
        }
    }
}

#[cfg(test)]
mod test {
    use chrono::{NaiveDateTime, TimeDelta};

    use crate::timestep::{PywrDuration, SECS_IN_DAY, is_leap_year};

    use super::{TimestepDuration, Timestepper};

    #[test]
    fn test_days() {
        let start = NaiveDateTime::parse_from_str("2021-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let end = NaiveDateTime::parse_from_str("2021-01-10 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let timestep = TimestepDuration::Days(1);

        let timestepper = Timestepper::new(start, end, timestep);
        let timesteps = timestepper.timesteps().unwrap();
        assert!(timesteps.len() == 10);
        assert_eq!(timesteps.first().unwrap().duration, TimeDelta::days(1));
        assert_eq!(timesteps.last().unwrap().duration, TimeDelta::days(1));

        let timestep = TimestepDuration::Frequency(String::from("1d"));

        let timestepper = Timestepper::new(start, end, timestep);
        let timesteps = timestepper.timesteps().unwrap();
        assert!(timesteps.len() == 10);
        assert_eq!(timesteps.first().unwrap().duration, TimeDelta::days(1));
        assert_eq!(timesteps.last().unwrap().duration, TimeDelta::days(1));
    }

    #[test]
    fn test_weeks() {
        let start = NaiveDateTime::parse_from_str("2021-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let end = NaiveDateTime::parse_from_str("2021-01-22 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let timestep = TimestepDuration::Frequency(String::from("1w"));

        let timestepper = Timestepper::new(start, end, timestep);
        let timesteps = timestepper.timesteps().unwrap();

        assert!(timesteps.len() == 4);
        assert_eq!(timesteps.first().unwrap().duration, TimeDelta::days(7));
        assert_eq!(timesteps.last().unwrap().duration, TimeDelta::days(7));
    }

    #[test]
    fn test_months() {
        let start = NaiveDateTime::parse_from_str("2021-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let end = NaiveDateTime::parse_from_str("2021-04-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let timestep = TimestepDuration::Frequency(String::from("1mo"));

        let timestepper = Timestepper::new(start, end, timestep);
        let timesteps = timestepper.timesteps().unwrap();
        assert!(timesteps.len() == 4);
        assert_eq!(timesteps[0].duration, TimeDelta::days(31));
        assert_eq!(timesteps[1].duration, TimeDelta::days(28));
        assert_eq!(timesteps[2].duration, TimeDelta::days(31));
        assert_eq!(timesteps[3].duration, TimeDelta::days(30));
    }

    #[test]
    fn test_hours() {
        let start = NaiveDateTime::parse_from_str("2021-01-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let end = NaiveDateTime::parse_from_str("2021-01-01 16:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let timestep = TimestepDuration::Frequency(String::from("1h"));

        let timestepper = Timestepper::new(start, end, timestep);
        let timesteps = timestepper.timesteps().unwrap();
        assert!(timesteps.len() == 5);
        assert_eq!(timesteps.first().unwrap().duration, TimeDelta::hours(1));
        assert_eq!(timesteps.last().unwrap().duration, TimeDelta::hours(1));
    }

    #[test]
    fn test_pywr_duration() {
        let duration = PywrDuration::days(5);
        assert_eq!(duration.whole_days(), Some(5));
        assert_eq!(duration.fractional_days(), 5.0);
        assert_eq!(duration.duration_string(), String::from("5d"));

        let duration: PywrDuration = TimeDelta::hours(12).into();
        assert_eq!(duration.whole_days(), None);
        assert_eq!(duration.fractional_days(), 0.5);
        assert_eq!(duration.duration_string(), String::from("12h"));

        let duration: PywrDuration = TimeDelta::minutes(30).into();
        assert_eq!(duration.whole_days(), None);
        assert_eq!(duration.fractional_days(), 1.0 / 48.0);
        assert_eq!(duration.duration_string(), String::from("30m"));

        let duration_secs = SECS_IN_DAY + 1;
        let duration: PywrDuration = TimeDelta::seconds(duration_secs).into();
        assert_eq!(duration.whole_days(), None);
        assert_eq!(duration.fractional_days(), duration_secs as f64 / SECS_IN_DAY as f64);
        assert_eq!(duration.duration_string(), String::from("1d1s"));

        let duration_secs = SECS_IN_DAY - 1;
        let duration: PywrDuration = TimeDelta::seconds(duration_secs).into();
        assert_eq!(duration.whole_days(), None);
        assert_eq!(duration.fractional_days(), duration_secs as f64 / SECS_IN_DAY as f64);
        assert_eq!(duration.duration_string(), String::from("23h59m59s"));
    }

    #[test]
    fn test_is_leap_year() {
        assert!(is_leap_year(2016));
        assert!(!is_leap_year(2017));
        assert!(is_leap_year(2000));
        assert!(!is_leap_year(1900));
    }
}
