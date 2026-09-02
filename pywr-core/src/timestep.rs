use jiff::civil::DateTime;
use jiff::{SignedDuration, Span};
use std::num::NonZeroU64;
use std::ops::Add;
use thiserror::Error;

const SECS_IN_DAY: i64 = 60 * 60 * 24;
const MILLISECS_IN_DAY: i64 = 1000 * SECS_IN_DAY;
const MILLISECS_IN_HOUR: i64 = 1000 * 60 * 60;
const MILLISECS_IN_MINUTE: i64 = 1000 * 60;
const MILLISECS_IN_SECOND: i64 = 1000;

/// A new type for `jiff::SignedDuration` that provides a couple of useful convenience methods.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct PywrDuration(SignedDuration);

impl From<SignedDuration> for PywrDuration {
    fn from(duration: SignedDuration) -> Self {
        Self(duration)
    }
}

impl Add<DateTime> for PywrDuration {
    type Output = DateTime;

    fn add(self, datetime: DateTime) -> DateTime {
        datetime + self.0
    }
}

impl PywrDuration {
    /// Create a new `PywrDuration` from a number of days.
    pub fn from_days(days: i64) -> Self {
        Self(SignedDuration::from_hours(days * 24))
    }

    pub fn from_hours(hours: i64) -> Self {
        Self(SignedDuration::from_hours(hours))
    }

    pub fn from_minutes(hours: i64) -> Self {
        Self(SignedDuration::from_mins(hours))
    }

    pub fn from_seconds(seconds: i64) -> Self {
        Self(SignedDuration::from_secs(seconds))
    }

    /// Returns the number of whole days in the duration, if the total duration is a whole number of days.
    pub fn whole_days(&self) -> Option<i64> {
        let fractional_days = self.fractional_days();
        if fractional_days.fract() == 0.0 {
            Some(fractional_days as i64)
        } else {
            None
        }
    }

    /// Returns the fractional number of days in the duration.
    pub fn fractional_days(&self) -> f64 {
        self.0.as_secs_f64() / SECS_IN_DAY as f64
    }

    /// Returns the number of milliseconds in the duration.
    pub fn milliseconds(&self) -> i64 {
        self.0.as_millis() as i64
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

/// A time-step in a simulation.
///
/// This struct represents a single time-step in a simulation, including the date, index, and duration of the time-step.
#[derive(Debug, Copy, Clone)]
pub struct Timestep {
    pub date: DateTime,
    pub index: TimestepIndex,
    pub duration: PywrDuration,
}

impl Timestep {
    pub fn new(date: DateTime, index: TimestepIndex, duration: PywrDuration) -> Self {
        Self { date, index, duration }
    }

    pub fn is_first(&self) -> bool {
        self.index == 0
    }

    /// Returns the duration of the timestep in number of days including any fractional part.
    pub fn days(&self) -> f64 {
        self.duration.fractional_days()
    }

    pub fn is_leap_year(&self) -> bool {
        self.date.in_leap_year()
    }

    /// Returns the day of the year index of the timestep.
    ///
    /// The day of the year is one-based, meaning January 1st is day 1 and December 31st is day 365 (or 366 in leap years).
    /// See [`day_of_year_index`](Timestep::day_of_year_index) for a zero-based index.
    pub fn day_of_year(&self) -> usize {
        self.date.day_of_year() as usize
    }

    /// Returns the day of the year index of the timestep.
    ///
    /// The index is zero-based and accounts for leaps days. In non-leap years, 1 is added to the index for
    /// days after Feb 28th.
    pub fn day_of_year_index(&self) -> usize {
        let mut i = self.date.day_of_year() as usize - 1;
        if !self.date.in_leap_year() && i > 58 {
            i += 1;
        }
        i
    }

    /// Returns the fraction day of the year of the timestep.
    ///
    /// The index is zero-based and accounts for leaps days. In non-leap years, 1 is added to the index for
    /// days after Feb 28th. The fractional part is the fraction of the day that has passed since midnight
    /// (calculated to the nearest second).
    pub fn fractional_day_of_year(&self) -> f64 {
        let start_day = self.date.start_of_day();
        let since_midnight = self.date.duration_since(start_day);

        let seconds_since_midnight = since_midnight.as_secs_f64();
        let fraction_of_day = seconds_since_midnight / 86400.0;

        self.day_of_year_index() as f64 + fraction_of_day
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

#[derive(Debug, Clone)]
pub enum TimestepDuration {
    Hours(NonZeroU64),
    Days(NonZeroU64),
    Frequency(String),
}

#[derive(Debug, Error)]
pub enum TimeDomainBuilderError {
    #[error("Could not create timesteps for frequency '{0}'")]
    GenerationError(String),
    #[error("The time domain defined no timesteps.")]
    NoTimesteps,
    #[error("Timestep duration must be a positive value.")]
    NonPositiveTimestepDuration,
    #[error("Could not parse frequency '{source}'")]
    FrequencyParseError {
        #[source]
        source: jiff::Error,
    },
}

#[derive(Debug, Clone)]
pub struct TimeDomainBuilder {
    start: DateTime,
    end: DateTime,
    timestep: TimestepDuration,
}

impl TimeDomainBuilder {
    pub fn new(start: DateTime, end: DateTime, timestep: TimestepDuration) -> Self {
        Self { start, end, timestep }
    }

    /// Create a vector of `Timestep`s between the start and end dates at the given duration.
    fn timesteps(&self) -> Result<Vec<Timestep>, TimeDomainBuilderError> {
        match &self.timestep {
            TimestepDuration::Hours(hours) => {
                Ok(self.generate_timesteps_from_fixed_duration(PywrDuration::from_hours(hours.get() as i64)))
            }
            TimestepDuration::Days(days) => {
                Ok(self.generate_timesteps_from_fixed_duration(PywrDuration::from_days(days.get() as i64)))
            }
            TimestepDuration::Frequency(freq) => {
                let span: Span = freq
                    .parse()
                    .map_err(|source| TimeDomainBuilderError::FrequencyParseError { source })?;
                self.generate_timesteps_from_span(span)
            }
        }
    }

    /// Creates a vector of `Timestep`s between the start and end dates at the given duration.
    fn generate_timesteps_from_fixed_duration(&self, duration: PywrDuration) -> Vec<Timestep> {
        let mut timesteps: Vec<Timestep> = Vec::new();
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
    fn generate_timesteps_from_span(&self, span: Span) -> Result<Vec<Timestep>, TimeDomainBuilderError> {
        let mut timesteps: Vec<Timestep> = Vec::new();
        let mut current = self.start;
        while current <= self.end {
            let next = self.start + span * (timesteps.len() as i64 + 1);
            // Calculate the duration of this timestep potentially accounting for non-uniform lengths.
            let duration = next.duration_since(current).into();
            let ts = Timestep::new(current, timesteps.len(), duration);
            timesteps.push(ts);
            current = next;
        }

        Ok(timesteps)
    }

    pub fn build(&self) -> Result<TimeDomain, TimeDomainBuilderError> {
        let timesteps = self.timesteps()?;

        // Final check we have at least one timestep.
        if timesteps.is_empty() {
            return Err(TimeDomainBuilderError::NoTimesteps);
        }

        Ok(TimeDomain { timesteps })
    }
}

/// The time domain that a model will be simulated over.
///
///
#[derive(Debug, Clone)]
pub struct TimeDomain {
    timesteps: Vec<Timestep>,
}

impl TimeDomain {
    /// If the time-steps are of uniform duration, this returns the duration of each time-step.
    /// Otherwise, it returns `None`.
    pub fn fixed_step_duration(&self) -> Option<PywrDuration> {
        let first_duration = self.timesteps.first().map(|ts| ts.duration);

        first_duration.filter(|&first_duration| self.timesteps.iter().all(|ts| ts.duration == first_duration))
    }

    pub fn timesteps(&self) -> &[Timestep] {
        &self.timesteps
    }

    /// The total number of time-steps in the domain.
    pub fn len(&self) -> usize {
        self.timesteps.len()
    }

    pub fn first_timestep(&self) -> &Timestep {
        self.timesteps.first().expect("TimeDomain has no timesteps; this should be impossible if the TimeDomain was created using TimeDomainBuilder.")
    }

    pub fn last_timestep(&self) -> &Timestep {
        self.timesteps.last().expect("TimeDomain has no timesteps; this should be impossible if the TimeDomain was created using TimeDomainBuilder.")
    }

    /// Returns true if the time domain has no time-steps. The time domain should always have at
    /// least one time-step.
    pub fn is_empty(&self) -> bool {
        self.timesteps.is_empty()
    }
}

#[cfg(test)]
mod test {
    use super::{TimeDomainBuilder, TimestepDuration};
    use crate::timestep::{PywrDuration, SECS_IN_DAY};
    use jiff::civil::{DateTime, date};
    use std::num::NonZeroU64;

    #[test]
    fn test_days() {
        let start: DateTime = "2021-01-01 00:00:00".parse().unwrap();
        let end: DateTime = "2021-01-10 00:00:00".parse().unwrap();
        let timestep = TimestepDuration::Days(NonZeroU64::new(1).unwrap());

        let timestepper = TimeDomainBuilder::new(start, end, timestep);
        let timesteps = timestepper.timesteps().unwrap();
        assert_eq!(timesteps.len(), 10);
        assert_eq!(timesteps.first().unwrap().duration, PywrDuration::from_days(1));
        assert_eq!(timesteps.last().unwrap().duration, PywrDuration::from_days(1));

        let timestep = TimestepDuration::Frequency(String::from("1d"));

        let timestepper = TimeDomainBuilder::new(start, end, timestep);
        let timesteps = timestepper.timesteps().unwrap();
        println!("Timesteps: {:?}", timesteps);
        assert_eq!(timesteps.len(), 10);
        assert_eq!(timesteps.first().unwrap().duration, PywrDuration::from_days(1));
        assert_eq!(timesteps.last().unwrap().duration, PywrDuration::from_days(1));
    }

    #[test]
    fn test_weeks() {
        let start: DateTime = "2021-01-01 00:00:00".parse().unwrap();
        let end: DateTime = "2021-01-22 00:00:00".parse().unwrap();
        let timestep = TimestepDuration::Frequency(String::from("1w"));

        let timestepper = TimeDomainBuilder::new(start, end, timestep);
        let timesteps = timestepper.timesteps().unwrap();

        assert_eq!(timesteps.len(), 4);
        assert_eq!(timesteps.first().unwrap().duration, PywrDuration::from_days(7));
        assert_eq!(timesteps.last().unwrap().duration, PywrDuration::from_days(7));
    }

    #[test]
    fn test_months() {
        let start: DateTime = "2021-01-01 00:00:00".parse().unwrap();
        let end: DateTime = "2021-04-01 00:00:00".parse().unwrap();
        let timestep = TimestepDuration::Frequency(String::from("1mo"));

        let timestepper = TimeDomainBuilder::new(start, end, timestep);
        let timesteps = timestepper.timesteps().unwrap();
        assert_eq!(timesteps.len(), 4);
        assert_eq!(timesteps[0].duration, PywrDuration::from_days(31));
        assert_eq!(timesteps[1].duration, PywrDuration::from_days(28));
        assert_eq!(timesteps[2].duration, PywrDuration::from_days(31));
        assert_eq!(timesteps[3].duration, PywrDuration::from_days(30));
    }

    #[test]
    fn test_month_ends() {
        let start: DateTime = "2021-01-31 00:00:00".parse().unwrap();
        let end: DateTime = "2021-04-30 00:00:00".parse().unwrap();
        let timestep = TimestepDuration::Frequency(String::from("1mo"));

        let timestepper = TimeDomainBuilder::new(start, end, timestep);
        let timesteps = timestepper.timesteps().unwrap();
        assert_eq!(timesteps.len(), 4);
        assert_eq!(timesteps[0].duration, PywrDuration::from_days(28));
        assert_eq!(timesteps[0].date, date(2021, 1, 31).at(0, 0, 0, 0));
        assert_eq!(timesteps[1].duration, PywrDuration::from_days(31));
        assert_eq!(timesteps[1].date, date(2021, 2, 28).at(0, 0, 0, 0));
        assert_eq!(timesteps[2].duration, PywrDuration::from_days(30));
        assert_eq!(timesteps[2].date, date(2021, 3, 31).at(0, 0, 0, 0));
        assert_eq!(timesteps[3].duration, PywrDuration::from_days(31));
        assert_eq!(timesteps[3].date, date(2021, 4, 30).at(0, 0, 0, 0));
    }

    #[test]
    fn test_hours() {
        let start: DateTime = "2021-01-01 12:00:00".parse().unwrap();
        let end: DateTime = "2021-01-01 16:00:00".parse().unwrap();
        let timestep = TimestepDuration::Frequency(String::from("1h"));

        let timestepper = TimeDomainBuilder::new(start, end, timestep);
        let timesteps = timestepper.timesteps().unwrap();
        assert_eq!(timesteps.len(), 5);
        assert_eq!(timesteps.first().unwrap().duration, PywrDuration::from_hours(1));
        assert_eq!(timesteps.last().unwrap().duration, PywrDuration::from_hours(1));
    }

    #[test]
    fn test_pywr_duration() {
        let duration = PywrDuration::from_days(5);
        assert_eq!(duration.whole_days(), Some(5));
        assert_eq!(duration.fractional_days(), 5.0);
        assert_eq!(duration.duration_string(), String::from("5d"));

        let duration = PywrDuration::from_hours(12);
        assert_eq!(duration.whole_days(), None);
        assert_eq!(duration.fractional_days(), 0.5);
        assert_eq!(duration.duration_string(), String::from("12h"));

        let duration = PywrDuration::from_minutes(30);
        assert_eq!(duration.whole_days(), None);
        assert_eq!(duration.fractional_days(), 1.0 / 48.0);
        assert_eq!(duration.duration_string(), String::from("30m"));

        let duration_secs = SECS_IN_DAY + 1;
        let duration = PywrDuration::from_seconds(duration_secs);
        assert_eq!(duration.whole_days(), None);
        assert_eq!(duration.fractional_days(), duration_secs as f64 / SECS_IN_DAY as f64);
        assert_eq!(duration.duration_string(), String::from("1d1s"));

        let duration_secs = SECS_IN_DAY - 1;
        let duration = PywrDuration::from_seconds(duration_secs);
        assert_eq!(duration.whole_days(), None);
        assert_eq!(duration.fractional_days(), duration_secs as f64 / SECS_IN_DAY as f64);
        assert_eq!(duration.duration_string(), String::from("23h59m59s"));
    }
}
