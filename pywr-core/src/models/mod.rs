mod multi;
mod simple;

use crate::scenario::{ScenarioDomain, ScenarioGroupCollection};
use crate::timestep::{TimeDomain, Timestepper};
pub use multi::{MultiNetworkModel, MultiNetworkTransferIndex};
pub use simple::Model;

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
            time: timestepper.into(),
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
}

impl From<Timestepper> for ModelDomain {
    fn from(value: Timestepper) -> Self {
        Self {
            time: value.into(),
            scenarios: ScenarioGroupCollection::default().into(),
        }
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
