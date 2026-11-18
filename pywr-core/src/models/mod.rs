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

/// Build the Rayon thread pool used to run scenarios in parallel.
///
/// With the `pyo3` feature each worker thread holds a persistent (detached) Python thread state
/// for its lifetime. Without this every `Python::attach` on a worker creates and destroys a thread
/// state, which costs tens of microseconds per call and serialises on CPython's runtime lock.
pub(crate) fn build_thread_pool(num_threads: usize) -> rayon::ThreadPool {
    let builder = rayon::ThreadPoolBuilder::new().num_threads(num_threads);
    #[cfg(feature = "pyo3")]
    let builder = builder
        .start_handler(|_| py_thread_state::attach())
        .exit_handler(|_| py_thread_state::release());
    builder.build().unwrap()
}

#[cfg(feature = "pyo3")]
mod py_thread_state {
    use pyo3::ffi;
    use std::cell::Cell;

    thread_local! {
        static STATE: Cell<Option<(ffi::PyGILState_STATE, *mut ffi::PyThreadState)>> = const { Cell::new(None) };
    }

    /// Create a thread state for this thread and immediately detach it, releasing the GIL.
    pub(super) fn attach() {
        // SAFETY: Only called once per worker thread, and only when the interpreter is initialised.
        unsafe {
            if ffi::Py_IsInitialized() == 0 {
                return;
            }
            let gil_state = ffi::PyGILState_Ensure();
            let thread_state = ffi::PyEval_SaveThread();
            STATE.set(Some((gil_state, thread_state)));
        }
    }

    /// Re-attach and destroy the thread state created by [`attach`].
    pub(super) fn release() {
        if let Some((gil_state, thread_state)) = STATE.take() {
            // SAFETY: The thread state was created on this thread by `attach` and is currently detached.
            unsafe {
                if ffi::Py_IsInitialized() != 0 {
                    ffi::PyEval_RestoreThread(thread_state);
                    ffi::PyGILState_Release(gil_state);
                }
            }
        }
    }
}

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

        Ok(ModelDomain { time, scenario })
    }
}
