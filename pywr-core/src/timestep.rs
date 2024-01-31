use pyo3::prelude::*;
use std::ops::Add;
use time::{Date, Duration};

type TimestepIndex = usize;

#[pyclass]
#[derive(Debug, Copy, Clone)]
pub struct Timestep {
    pub date: Date,
    pub index: TimestepIndex,
    pub duration: Duration,
}

impl Timestep {
    pub fn new(date: Date, index: TimestepIndex, duration: Duration) -> Self {
        Self { date, index, duration }
    }

    pub fn is_first(&self) -> bool {
        self.index == 0
    }

    // pub fn parse_from_str(date: &str, fmt: &str, index: TimestepIndex, timestep: i64) -> Result<Self, PywrError> {
    //     Ok(Self {
    //         date: Date::parse_from_str(date, fmt)?,
    //         index,
    //         duration: Duration::days(timestep),
    //     })
    // }

    pub(crate) fn days(&self) -> f64 {
        self.duration.as_seconds_f64() / 3600.0 / 24.0
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
pub struct Timestepper {
    start: Date,
    end: Date,
    timestep: Duration,
}

impl Timestepper {
    pub fn new(start: Date, end: Date, timestep: i64) -> Self {
        Self {
            start,
            end,
            timestep: Duration::days(timestep),
        }
    }

    /// Create a vector of `Timestep`s between the start and end dates at the given duration.
    fn timesteps(&self) -> Vec<Timestep> {
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
