use chrono::{Duration, Months, NaiveDateTime};
use polars::datatypes::TimeUnit;
use polars::time::ClosedWindow;
use pyo3::prelude::*;
use std::ops::Add;

type TimestepIndex = usize;

#[pyclass]
#[derive(Debug, Copy, Clone)]
pub struct Timestep {
    pub date: NaiveDateTime,
    pub index: TimestepIndex,
    pub duration: Duration,
}

impl Timestep {
    pub fn new(date: NaiveDateTime, index: TimestepIndex, duration: Duration) -> Self {
        Self { date, index, duration }
    }

    pub fn is_first(&self) -> bool {
        self.index == 0
    }

    pub(crate) fn days(&self) -> f64 {
        self.duration.num_seconds() as f64 / 3600.0 / 24.0
    }
}

impl Add<Duration> for Timestep {
    type Output = Timestep;

    fn add(self, other: Duration) -> Self {
        Self {
            date: self.date + other,
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
    fn timesteps(&self) -> Vec<Timestep> {
        match self.timestep {
            TimestepDuration::Days(days) => self.generate_timesteps_from_days(days),
            TimestepDuration::Frequency(ref frequency) => self.generate_timesteps_from_frequency(frequency.clone()),
        }
    }

    fn generate_timesteps_from_days(&self, days: i64) -> Vec<Timestep> {
        let mut timesteps: Vec<Timestep> = Vec::new();
        let duration = Duration::days(days);
        let mut current = Timestep::new(self.start, 0, duration);

        while current.date <= self.end {
            let next = current + duration;
            timesteps.push(current);
            current = next;
        }
        timesteps
    }

    fn generate_timesteps_from_frequency(&self, frequency: String) -> Vec<Timestep> {
        let duration = polars::time::Duration::parse(&frequency);

        // Need to add an extra day to the end date so that the duration of the last timestep can be calculated.
        let end = if duration.days_only() {
            self.end + Duration::days(duration.days())
        } else if duration.weeks_only() {
            self.end + Duration::weeks(duration.weeks())
        } else if duration.months_only() {
            let months = Months::new(duration.months() as u32);
            self.end + months
        } else {
            let months = Months::new(duration.months() as u32);
            self.end
                + months
                + Duration::days(duration.days())
                + Duration::weeks(duration.weeks())
                + Duration::nanoseconds(duration.nanoseconds())
        };

        let dates: Vec<Option<NaiveDateTime>> = polars::time::date_range(
            "timesteps",
            self.start,
            end,
            duration,
            ClosedWindow::Both,
            TimeUnit::Milliseconds,
            None,
        )
        .unwrap()
        .as_datetime_iter()
        .collect();

        dbg!(&dates);

        let timesteps = dates
            .windows(2)
            .enumerate()
            .map(|(i, dates)| {
                let d1 = dates[0].unwrap();
                let d2 = dates[1].unwrap();
                let duration = d2 - d1;
                Timestep::new(d1, i, duration)
            })
            .collect();

        timesteps
    }
}

/// The time domain that a model will be simulated over.
pub struct TimeDomain {
    timesteps: Vec<Timestep>,
}

impl TimeDomain {
    /// Return the duration of each time-step.
    pub fn step_duration(&self) -> Duration {
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
}

impl From<Timestepper> for TimeDomain {
    fn from(value: Timestepper) -> Self {
        Self {
            timesteps: value.timesteps(),
        }
    }
}

#[cfg(test)]
mod test {
    use chrono::{Duration, NaiveDateTime};

    use super::{TimestepDuration, Timestepper};

    /// Basic functional test of the delay parameter.
    #[test]
    fn test_days() {
        let start = NaiveDateTime::parse_from_str("2021-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let end = NaiveDateTime::parse_from_str("2021-01-10 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let timestep = TimestepDuration::Days(1);

        let timestepper = Timestepper::new(start, end, timestep);
        let timesteps = timestepper.timesteps();
        assert!(timesteps.len() == 10);
        assert_eq!(timesteps.first().unwrap().duration, Duration::days(1));
        assert_eq!(timesteps.last().unwrap().duration, Duration::days(1));

        let timestep = TimestepDuration::Frequency(String::from("1d"));

        let timestepper = Timestepper::new(start, end, timestep);
        let timesteps = timestepper.timesteps();
        assert!(timesteps.len() == 10);
        assert_eq!(timesteps.first().unwrap().duration, Duration::days(1));
        assert_eq!(timesteps.last().unwrap().duration, Duration::days(1));
    }

    #[test]
    fn test_weeks() {
        let start = NaiveDateTime::parse_from_str("2021-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let end = NaiveDateTime::parse_from_str("2021-01-22 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let timestep = TimestepDuration::Frequency(String::from("1w"));

        let timestepper = Timestepper::new(start, end, timestep);
        let timesteps = timestepper.timesteps();

        assert!(timesteps.len() == 4);
        assert_eq!(timesteps.first().unwrap().duration, Duration::days(7));
        assert_eq!(timesteps.last().unwrap().duration, Duration::days(7));
    }

    #[test]
    fn test_months() {
        let start = NaiveDateTime::parse_from_str("2021-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let end = NaiveDateTime::parse_from_str("2021-04-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let timestep = TimestepDuration::Frequency(String::from("1mo"));

        let timestepper = Timestepper::new(start, end, timestep);
        let timesteps = timestepper.timesteps();
        assert!(timesteps.len() == 4);
        assert_eq!(timesteps[0].duration, Duration::days(31));
        assert_eq!(timesteps[1].duration, Duration::days(28));
        assert_eq!(timesteps[2].duration, Duration::days(31));
        assert_eq!(timesteps[3].duration, Duration::days(30));
    }

    #[test]
    fn test_hours() {
        let start = NaiveDateTime::parse_from_str("2021-01-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let end = NaiveDateTime::parse_from_str("2021-01-01 16:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let timestep = TimestepDuration::Frequency(String::from("1h"));

        let timestepper = Timestepper::new(start, end, timestep);
        let timesteps = timestepper.timesteps();
        assert!(timesteps.len() == 5);
        assert_eq!(timesteps.first().unwrap().duration, Duration::hours(1));
        assert_eq!(timesteps.last().unwrap().duration, Duration::hours(1));
    }
}
