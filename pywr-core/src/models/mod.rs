mod multi;
mod simple;

use crate::PywrError;
use crate::scenario::{ScenarioDomain, ScenarioDomainBuilder};
use crate::timestep::{TimeDomain, Timestepper};
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

    pub fn from(timestepper: Timestepper, scenario_builder: ScenarioDomainBuilder) -> Result<Self, PywrError> {
        Ok(Self {
            time: TimeDomain::try_from(timestepper)?,
            scenarios: scenario_builder.build()?,
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
            scenarios: ScenarioDomainBuilder::default().build()?,
        })
    }
}

impl From<TimeDomain> for ModelDomain {
    fn from(value: TimeDomain) -> Self {
        Self {
            time: value,
            scenarios: ScenarioDomainBuilder::default().build().unwrap(),
        }
    }
}
