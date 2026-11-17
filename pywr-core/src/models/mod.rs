mod multi;
mod simple;

use crate::scenario::{ScenarioDomain, ScenarioDomainBuilder, ScenarioDomainBuilderError};
use crate::timestep::{TimeDomain, TimeDomainBuilder, TimeDomainBuilderError};
pub use multi::{
    InterNetworkTransferError, MultiNetworkEntryBuilder, MultiNetworkModel, MultiNetworkModelBuilder,
    MultiNetworkModelBuilderError, MultiNetworkModelFinaliseError, MultiNetworkModelResult, MultiNetworkModelRunError,
    MultiNetworkModelSetupError, MultiNetworkModelTimings, MultiNetworkTransferBuilder, MultiNetworkTransferIndex,
};
pub use pywr_output_metadata::RunMetadata;
pub use simple::{
    Model, ModelBuilder, ModelBuilderError, ModelFinaliseError, ModelResult, ModelRunError, ModelSetupError,
    ModelState, ModelStepError, ModelTimings,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ModelDomainError {
    #[error("Error in time domain.")]
    TimestepError(#[from] TimeDomainBuilderError),
    #[error("Error in scenario domain.")]
    ScenarioError(#[from] ScenarioDomainBuilderError),
}

#[derive(Debug, Clone)]
pub struct ModelDomain {
    time: TimeDomain,
    scenario: ScenarioDomain,
    /// Metadata describing the run, written to the model's Arrow and Parquet outputs.
    run_metadata: Option<RunMetadata>,
}

impl ModelDomain {
    /// The metadata describing the run, if any was given.
    pub fn run_metadata(&self) -> Option<&RunMetadata> {
        self.run_metadata.as_ref()
    }

    /// Set the metadata describing the run, which the model's Arrow and Parquet outputs record.
    pub fn set_run_metadata(&mut self, run_metadata: Option<RunMetadata>) {
        self.run_metadata = run_metadata;
    }

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
    #[error("Error building time domain.")]
    Time(#[from] TimeDomainBuilderError),
    #[error("Error building scenario domain.")]
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

        Ok(ModelDomain {
            time,
            scenario,
            run_metadata: None,
        })
    }
}
