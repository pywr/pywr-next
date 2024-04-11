mod multi;
mod simple;

use crate::scenario::{ScenarioDomain, ScenarioGroupCollection};
use crate::timestep::{TimeDomain, Timestepper};
use crate::PywrError;
pub use multi::{MultiNetworkModel, MultiNetworkTransferIndex};
pub use simple::{Model, ModelState};

#[derive(Debug)]
pub struct ModelDomain {
    time: TimeDomain,
    scenarios: ScenarioDomain,
}

impl ModelDomain {
    pub fn new(time: TimeDomain, scenarios: ScenarioDomain) -> Self {
        Self { time, scenarios }
    }

    pub fn from(timestepper: Timestepper, scenario_collection: ScenarioGroupCollection) -> Result<Self, PywrError> {
        Ok(Self {
            time: TimeDomain::try_from(timestepper)?,
            scenarios: scenario_collection.into(),
        })
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
}

impl TryFrom<Timestepper> for ModelDomain {
    type Error = PywrError;

    fn try_from(value: Timestepper) -> Result<Self, Self::Error> {
        let time = TimeDomain::try_from(value)?;
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
