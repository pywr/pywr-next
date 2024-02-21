use chrono::{Months, NaiveDateTime, TimeDelta};
use polars::datatypes::TimeUnit;
use polars::time::ClosedWindow;
use pyo3::prelude::*;
use std::ops::Add;

use crate::PywrError;

#[pyclass]
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

impl PywrDuration {
    pub fn days(days: i64) -> Self {
        Self(TimeDelta::days(days))
    }

    pub fn whole_days(&self) -> Option<i64> {
        if self.fractional_days() % 1.0 == 0.0 {
            Some(self.0.num_days())
        } else {
            None
        }
    }

    pub fn fractional_days(&self) -> f64 {
        self.0.num_seconds() as f64 / 3600.0 / 24.0
    }

    pub fn time_delta(&self) -> &TimeDelta {
        &self.0
    }
}

type TimestepIndex = usize;

#[pyclass]
#[derive(Debug, Copy, Clone)]
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
        match self.timestep {
            TimestepDuration::Days(days) => Ok(self.generate_timesteps_from_days(days)),
            TimestepDuration::Frequency(ref frequency) => self.generate_timesteps_from_frequency(frequency.clone()),
        }
    }

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

    fn generate_timesteps_from_frequency(&self, frequency: String) -> Result<Vec<Timestep>, PywrError> {
        let duration = polars::time::Duration::parse(&frequency);

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
            "timesteps",
            self.start,
            end,
            duration,
            ClosedWindow::Both,
            TimeUnit::Milliseconds,
            None,
        )
        .map_err(|e| PywrError::TimestepRangeGenerationError(e.to_string()))?
        .as_datetime_iter()
        .map(|x| x.ok_or(PywrError::TimestepGenerationError(frequency.clone())))
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
pub struct TimeDomain {
    timesteps: Vec<Timestep>,
}

impl TimeDomain {
    /// Return the duration of each time-step.
    pub fn step_duration(&self) -> PywrDuration {
        // This relies on the assumption that all time-steps are the same length.
        // Ideally, this invariant would be refactored to have the duration stored here in `TimeDomain`,
        // rather than in `Timestep`.
        self.timesteps.first().expect("Not time-steps defined.").duration
    }

    pub fn timesteps(&self) -> &[Timestep] {
        &self.timesteps
    }

    /// The total number of time-steps in the domain.
    pub fn len(&self) -> usize {
        self.timesteps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.timesteps.is_empty()
    }
}

impl TryFrom<Timestepper> for TimeDomain {
    type Error = PywrError;

    fn try_from(value: Timestepper) -> Result<Self, Self::Error> {
        let timesteps = value.timesteps()?;
        Ok(Self { timesteps })
    }
}

#[cfg(test)]
mod test {
    use chrono::{NaiveDateTime, TimeDelta};

    use super::{TimestepDuration, Timestepper};

    /// Basic functional test of the delay parameter.
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
}
