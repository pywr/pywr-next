mod multi;
mod simple;

use crate::scenario::{ScenarioDomain, ScenarioDomainBuilder, ScenarioDomainBuilderError};
use crate::timestep::{TimeDomain, TimeDomainBuilder, TimeDomainBuilderError};
pub use multi::{
    InterNetworkTransferError, MultiNetworkEntryBuilder, MultiNetworkModel, MultiNetworkModelBuilder,
    MultiNetworkModelBuilderError, MultiNetworkModelFinaliseError, MultiNetworkModelResult, MultiNetworkModelRunError,
    MultiNetworkModelSetupError, MultiNetworkModelTimings, MultiNetworkTransferBuilder, MultiNetworkTransferIndex,
};
pub use simple::{
    Model, ModelBuilder, ModelBuilderError, ModelFinaliseError, ModelResult, ModelRunError, ModelSetupError,
    ModelState, ModelStepError, ModelTimings,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ModelDomainError {
    #[error("Error in time domain: {0}")]
    TimestepError(#[from] crate::timestep::TimeDomainBuilderError),
    #[error("Error in scenario domain: {0}")]
    ScenarioError(#[from] crate::scenario::ScenarioDomainBuilderError),
}

#[derive(Debug, Clone)]
pub struct ModelDomain {
    time: TimeDomain,
    scenario: ScenarioDomain,
}

impl ModelDomain {
    pub fn time(&self) -> &TimeDomain {
        &self.time
    }

    pub fn scenarios(&self) -> &ScenarioDomain {
        &self.scenario
    }

    pub fn shape(&self) -> (usize, usize) {
        (self.time.timesteps().len(), self.scenario.indices().len())
    }
}

#[derive(Debug, Error)]
pub enum ModelDomainBuilderError {
    #[error("Error building time domain: {0}")]
    Time(#[from] TimeDomainBuilderError),
    #[error("Error building scenario domain: {0}")]
    Scenario(#[from] ScenarioDomainBuilderError),
}

#[derive(Clone)]
pub struct ModelDomainBuilder {
    time: TimeDomainBuilder,
    scenario: Option<ScenarioDomainBuilder>,
}

impl ModelDomainBuilder {
    pub fn new(time: TimeDomainBuilder) -> Self {
        Self { time, scenario: None }
    }

    pub fn scenario(&mut self, scenario: ScenarioDomainBuilder) -> &mut Self {
        self.scenario = Some(scenario);
        self
    }

    pub fn build(self) -> Result<ModelDomain, ModelDomainBuilderError> {
        let time = self.time.build()?;
        let scenario = self.scenario.unwrap_or_default().build()?;

        Ok(ModelDomain { time, scenario })
    }
}
