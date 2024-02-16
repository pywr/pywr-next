mod multi;
mod simple;

use crate::scenario::{ScenarioDomain, ScenarioGroupCollection};
use crate::timestep::{TimeDomain, Timestepper};
use crate::PywrError;
pub use multi::{MultiNetworkModel, MultiNetworkTransferIndex};
pub use simple::{Model, ModelState};

pub struct ModelDomain {
    time: TimeDomain,
    scenarios: ScenarioDomain,
}

impl ModelDomain {
    pub fn new(time: TimeDomain, scenarios: ScenarioDomain) -> Self {
        Self { time, scenarios }
    }

    pub fn from(timestepper: Timestepper, scenario_collection: ScenarioGroupCollection) -> Self {
        Self {
            time: TimeDomain::from_timestepper(timestepper).unwrap(),
            scenarios: scenario_collection.into(),
        }
    }

    pub fn time(&self) -> &TimeDomain {
        &self.time
    }

    pub fn scenarios(&self) -> &ScenarioDomain {
        &self.scenarios
    }

    pub fn shape(&self) -> (usize, usize) {
        (self.time.timesteps().len(), self.scenarios.indices().len())
    }

    pub fn from_timestepper(timestepper: Timestepper) -> Result<Self, PywrError> {
        let time = TimeDomain::from_timestepper(timestepper)?;
        Ok(Self {
            time,
            scenarios: ScenarioGroupCollection::default().into(),
        })
    }
}

impl From<TimeDomain> for ModelDomain {
    fn from(value: TimeDomain) -> Self {
        Self {
            time: value,
            scenarios: ScenarioGroupCollection::default().into(),
        }
    }
}
