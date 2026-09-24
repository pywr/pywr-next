use serde::{Deserialize, Serialize};
use uuid::Uuid;

mod bootstrap;

pub mod v1;
pub use bootstrap::{
    AuthenticationCredentials, BootstrapClientMessage, BootstrapServerMessage, ClientHello, HandshakeRejection,
    ServerHello,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(Uuid);

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(Uuid);

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl RunId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
/// A struct representing a protocol version with major and minor version numbers.
///
/// Compatibility rules:
///   - different major versions are incompatible;
///   - a higher minor version may add optional messages, fields, or capabilities;
///   - unknown optional fields should be ignored where the encoding permits;
///   - unsupported required capabilities must reject initialisation;
///   - protocol negotiation happens before model data is sent.
///
///
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub session_id: SessionId,
    pub run_id: Option<RunId>,
    pub sequence: u64,
    pub payload: T,
}
