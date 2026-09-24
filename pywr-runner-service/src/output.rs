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
        let mut events = self.events.lock().expect("service output mutex poisoned");

        if matches!(event, EngineEvent::Progress { .. }) {
            // The service publishes progress at its configured update interval. Retain only
            // the latest value while the engine is running faster than that interval.
            events.retain(|queued| !matches!(queued, EngineEvent::Progress { .. }));
        }

        events.push_back(event);

        Ok(())
    }
}
