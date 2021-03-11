use crate::PywrError;
use chrono::{Duration as ChronoDuration, NaiveDate};
use std::ops::Add;

type TimestepIndex = usize;

#[derive(Debug, Copy, Clone)]
pub struct Timestep {
    pub date: NaiveDate,
    pub index: TimestepIndex,
    pub duration: ChronoDuration,
}

impl Timestep {
    pub fn new(date: NaiveDate, index: TimestepIndex, duration: ChronoDuration) -> Self {
        Self { date, index, duration }
    }

    pub fn parse_from_str(date: &str, fmt: &str, index: TimestepIndex, timestep: i64) -> Result<Self, PywrError> {
        Ok(Self {
            date: NaiveDate::parse_from_str(date, fmt)?,
            index,
            duration: ChronoDuration::days(timestep),
        })
    }

    pub(crate) fn days(&self) -> f64 {
        self.duration.num_seconds() as f64 / 3600.0 / 24.0
    }
}

impl Add<ChronoDuration> for Timestep {
    type Output = Timestep;

    fn add(self, other: ChronoDuration) -> Self {
        Self {
            date: self.date + other,
            index: self.index + 1,
            duration: other,
        }
    }
}

#[derive(Debug)]
pub struct Timestepper {
    start: NaiveDate,
    end: NaiveDate,
    timestep: ChronoDuration,
}

impl Timestepper {
    pub(crate) fn new(start: &str, end: &str, fmt: &str, timestep: i64) -> Result<Self, PywrError> {
        Ok(Self {
            start: NaiveDate::parse_from_str(start, fmt)?,
            end: NaiveDate::parse_from_str(end, fmt)?,
            timestep: ChronoDuration::days(timestep),
        })
    }

    /// Create a vector of `Timestep`s between the start and end dates at the given duration.
    pub(crate) fn timesteps(&self) -> Vec<Timestep> {
        let mut timesteps: Vec<Timestep> = Vec::new();
        let mut current = Timestep::new(self.start, 0, self.timestep);

        while current.date <= self.end {
            let next = current + self.timestep;
            timesteps.push(current);
            current = next;
        }
        timesteps
    }
}
