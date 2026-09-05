use pywr_runner_engine::{EngineEvent, OutputError, OutputSink};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct ServiceOutput {
    events: Arc<Mutex<VecDeque<EngineEvent>>>,
}

impl ServiceOutput {
    pub fn drain(&self) -> Vec<EngineEvent> {
        self.events
            .lock()
            .expect("service output mutex poisoned")
            .drain(..)
            .collect()
    }
}

impl OutputSink for ServiceOutput {
    fn emit(&mut self, event: EngineEvent) -> Result<(), OutputError> {
        self.events
            .lock()
            .expect("service output mutex poisoned")
            .push_back(event);

        Ok(())
    }
}
