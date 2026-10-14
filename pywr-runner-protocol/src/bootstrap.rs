//! This module defines the messages used in the bootstrap handshake between a client and a server.
//!
//! These should remain stable across protocol versions, as they are used to negotiate the protocol
//! version and capabilities before any version-specific messages are exchanged.
use crate::{ProtocolVersion, SessionId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum BootstrapClientMessage {
    Hello(ClientHello),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClientHello {
    pub supported_versions: Vec<ProtocolVersion>,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    pub authentication: Option<AuthenticationCredentials>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum AuthenticationCredentials {
    Token(String),
    // Other authentication methods can be added here in the future.
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum BootstrapServerMessage {
    Accepted(ServerHello),
    Rejected(HandshakeRejection),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerHello {
    pub selected_version: ProtocolVersion,
    pub session_id: SessionId,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum HandshakeRejection {
    UnsupportedVersion { supported: Vec<ProtocolVersion> },
    MissingCapabilities { capabilities: Vec<String> },
    AuthenticationRequired,
    AuthenticationFailed,
    MalformedHello,
}
