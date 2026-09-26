use pywr_runner_protocol::{RunId, SessionId};

pub(crate) struct Session {
    pub session_id: SessionId,
    pub run_id: Option<RunId>,
    pub next_client_sequence: u64,
    pub next_server_sequence: u64,
}

impl Session {
    pub fn new(session_id: SessionId) -> Self {
        Self {
            session_id,
            run_id: None,
            next_client_sequence: 0,
            next_server_sequence: 0,
        }
    }

    pub fn validate_client_envelope<T>(
        &mut self,
        envelope: &pywr_runner_protocol::Envelope<T>,
    ) -> Result<(), crate::ServiceError> {
        if envelope.session_id != self.session_id {
            return Err(crate::ServiceError::InvalidSession);
        }

        if envelope.sequence != self.next_client_sequence {
            return Err(crate::ServiceError::InvalidSequence {
                expected: self.next_client_sequence,
                received: envelope.sequence,
            });
        }

        if let Some(expected_run_id) = self.run_id {
            if envelope.run_id != Some(expected_run_id) {
                return Err(crate::ServiceError::InvalidRun);
            }
        }

        self.next_client_sequence += 1;
        Ok(())
    }
}
