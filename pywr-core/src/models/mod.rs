mod multi;
mod simple;

use crate::scenario::{ScenarioDomain, ScenarioDomainBuilder};
use crate::timestep::{TimeDomain, Timestepper};
pub use multi::{
    InterNetworkTransferError, MultiNetworkModel, MultiNetworkModelError, MultiNetworkModelFinaliseError,
    MultiNetworkModelResult, MultiNetworkModelRunError, MultiNetworkModelSetupError, MultiNetworkModelTimings,
    MultiNetworkTransferIndex,
};
pub use simple::{
    Model, ModelFinaliseError, ModelResult, ModelRunError, ModelSetupError, ModelState, ModelStepError, ModelTimings,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ModelDomainError {
    #[error("Error in time domain: {0}")]
    TimestepError(#[from] crate::timestep::TimestepError),
    #[error("Error in scenario domain: {0}")]
    ScenarioError(#[from] crate::scenario::ScenarioError),
}

#[derive(Debug, Clone)]
pub struct ModelDomain {
    time: TimeDomain,
    scenarios: ScenarioDomain,
}

impl ModelDomain {
    pub fn new(time: TimeDomain, scenarios: ScenarioDomain) -> Self {
        Self { time, scenarios }
    }

    pub fn try_from(
        timestepper: Timestepper,
        scenario_builder: ScenarioDomainBuilder,
    ) -> Result<Self, ModelDomainError> {
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
    type Error = ModelDomainError;

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
