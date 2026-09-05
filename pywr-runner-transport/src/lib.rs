mod interprocess_local_socket;

pub use interprocess_local_socket::{InterprocessLocalSocketConnection, InterprocessLocalSocketListener};

use std::time::Duration;
use thiserror::Error;

#[derive(Debug)]
pub struct PeerIdentity {
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiveOutcome {
    Frame(Vec<u8>),
    TimedOut,
    Closed,
}

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("transport I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("transport operation is unsupported: {0}")]
    Unsupported(&'static str),

    #[error("invalid transport frame: {0}")]
    InvalidFrame(String),
}

pub trait TransportConnection {
    type Reader: TransportReader + Send + 'static;
    type Writer: TransportWriter + Send + 'static;

    fn peer_identity(&self) -> Result<PeerIdentity, TransportError>;
    fn split(self) -> Result<(Self::Reader, Self::Writer), TransportError>;
}

pub trait TransportReader {
    fn receive_frame(&mut self, timeout: Option<Duration>) -> Result<ReceiveOutcome, TransportError>;
}

pub trait TransportWriter {
    fn send_frame(&mut self, frame: &[u8]) -> Result<(), TransportError>;
    fn close(&mut self) -> Result<(), TransportError>;
}

pub trait TransportListener {
    type Connection: TransportConnection;

    fn accept(&self) -> Result<Self::Connection, TransportError>;
}
