//! Framed transport implemented with synchronous `interprocess` local sockets.
//!
//! Local sockets are byte streams and do not preserve message boundaries.
//! This adapter frames each message as:
//!
//! ```text
//! +--------------------------+-------------------------+
//! | payload length: u32 BE   | payload bytes           |
//! +--------------------------+-------------------------+
//! ```

use super::{
    PeerIdentity, ReceiveOutcome, TransportConnection, TransportError, TransportListener, TransportReader,
    TransportWriter,
};
use interprocess::local_socket::prelude::*;
use interprocess::local_socket::traits::RecvHalf as _;
use interprocess::local_socket::{
    GenericNamespaced, ListenerOptions, RecvHalf as LocalSocketRecvHalf, SendHalf as LocalSocketSendHalf,
};
use std::io::{ErrorKind, Read, Write};
use std::time::Duration;

/// Protect the process against corrupt or malicious frame lengths.
///
/// Increase this later if model documents larger than 64 MiB are expected.
const MAX_FRAME_SIZE: usize = 64 * 1024 * 1024;

/// A synchronous local-socket listener.
pub struct InterprocessLocalSocketListener {
    inner: LocalSocketListener,
    name: String,
}

impl InterprocessLocalSocketListener {
    /// Binds a portable namespaced local socket.
    ///
    /// On Linux this normally maps to an abstract Unix-domain socket. On
    /// Windows it maps to an appropriate named-pipe-backed local socket.
    pub fn bind_namespaced(name: impl Into<String>) -> Result<Self, TransportError> {
        let name = name.into();
        let socket_name = name.clone().to_ns_name::<GenericNamespaced>()?;

        let inner = ListenerOptions::new().name(socket_name).create_sync()?;

        Ok(Self { inner, name })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn accept(&self) -> Result<InterprocessLocalSocketConnection, TransportError> {
        TransportListener::accept(self)
    }
}

impl TransportListener for InterprocessLocalSocketListener {
    type Connection = InterprocessLocalSocketConnection;

    fn accept(&self) -> Result<Self::Connection, TransportError> {
        let stream = self.inner.accept()?;
        Ok(InterprocessLocalSocketConnection { stream })
    }
}

/// An accepted synchronous local-socket connection.
pub struct InterprocessLocalSocketConnection {
    stream: LocalSocketStream,
}

impl InterprocessLocalSocketConnection {
    pub fn connect_namespaced(name: impl AsRef<str>) -> Result<Self, TransportError> {
        let socket_name = name.as_ref().to_ns_name::<GenericNamespaced>()?;

        let stream = LocalSocketStream::connect(socket_name)?;

        Ok(Self { stream })
    }
}

impl TransportConnection for InterprocessLocalSocketConnection {
    type Reader = InterprocessLocalSocketReader;
    type Writer = InterprocessLocalSocketWriter;

    fn peer_identity(&self) -> Result<PeerIdentity, TransportError> {
        let credentials = self.stream.peer_creds()?;

        Ok(PeerIdentity {
            description: format!("{credentials:?}"),
        })
    }

    fn split(self) -> Result<(Self::Reader, Self::Writer), TransportError> {
        let (recv, send) = self.stream.split();

        Ok((
            InterprocessLocalSocketReader {
                inner: recv,
                buffered: Vec::new(),
                expected_payload_len: None,
            },
            InterprocessLocalSocketWriter { inner: Some(send) },
        ))
    }
}

/// Receive half with state retained across timeout boundaries.
pub struct InterprocessLocalSocketReader {
    inner: LocalSocketRecvHalf,

    /// Bytes received for the current frame, including its four-byte header.
    buffered: Vec<u8>,

    /// Decoded length once the complete header has arrived.
    expected_payload_len: Option<usize>,
}

impl InterprocessLocalSocketReader {
    fn take_complete_frame(&mut self) -> Result<Option<Vec<u8>>, TransportError> {
        if self.expected_payload_len.is_none() && self.buffered.len() >= 4 {
            let header: [u8; 4] = self.buffered[..4].try_into().expect("slice has exactly four bytes");

            let payload_len = u32::from_be_bytes(header) as usize;

            if payload_len > MAX_FRAME_SIZE {
                return Err(TransportError::InvalidFrame(format!(
                    "declared frame size {payload_len} exceeds maximum \
                     {MAX_FRAME_SIZE}"
                )));
            }

            self.expected_payload_len = Some(payload_len);
        }

        let Some(payload_len) = self.expected_payload_len else {
            return Ok(None);
        };

        let complete_len = 4usize
            .checked_add(payload_len)
            .ok_or_else(|| TransportError::InvalidFrame("frame length overflowed usize".to_owned()))?;

        if self.buffered.len() < complete_len {
            return Ok(None);
        }

        // Remove the length prefix.
        self.buffered.drain(..4);

        // Keep any bytes already read for the following frame.
        let remaining = self.buffered.split_off(payload_len);
        let payload = std::mem::replace(&mut self.buffered, remaining);

        self.expected_payload_len = None;

        Ok(Some(payload))
    }
}

impl TransportReader for InterprocessLocalSocketReader {
    fn receive_frame(&mut self, timeout: Option<Duration>) -> Result<ReceiveOutcome, TransportError> {
        self.inner.set_timeout(timeout)?;

        loop {
            if let Some(frame) = self.take_complete_frame()? {
                return Ok(ReceiveOutcome::Frame(frame));
            }

            let mut chunk = [0_u8; 8192];

            match self.inner.read(&mut chunk) {
                Ok(0) if self.buffered.is_empty() => {
                    return Ok(ReceiveOutcome::Closed);
                }

                Ok(0) => {
                    return Err(TransportError::InvalidFrame(format!(
                        "connection closed with {} bytes of an incomplete frame",
                        self.buffered.len()
                    )));
                }

                Ok(read) => {
                    self.buffered.extend_from_slice(&chunk[..read]);
                }

                Err(error) if error.kind() == ErrorKind::Interrupted => {
                    continue;
                }

                // Depending on the operating system, socket receive timeouts
                // can be reported as either TimedOut or WouldBlock.
                Err(error) if matches!(error.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => {
                    return Ok(ReceiveOutcome::TimedOut);
                }

                Err(error) => return Err(error.into()),
            }
        }
    }
}

/// Send half of a framed local-socket connection.
pub struct InterprocessLocalSocketWriter {
    // Option allows `close()` to drop the send half immediately.
    inner: Option<LocalSocketSendHalf>,
}

impl TransportWriter for InterprocessLocalSocketWriter {
    fn send_frame(&mut self, frame: &[u8]) -> Result<(), TransportError> {
        let payload_len = u32::try_from(frame.len()).map_err(|_| {
            TransportError::InvalidFrame(format!(
                "outgoing frame size {} cannot be represented as u32",
                frame.len()
            ))
        })?;

        if frame.len() > MAX_FRAME_SIZE {
            return Err(TransportError::InvalidFrame(format!(
                "outgoing frame size {} exceeds maximum {MAX_FRAME_SIZE}",
                frame.len()
            )));
        }

        let writer = self
            .inner
            .as_mut()
            .ok_or_else(|| TransportError::InvalidFrame("attempted to write to a closed connection".to_owned()))?;

        writer.write_all(&payload_len.to_be_bytes())?;
        writer.write_all(frame)?;
        writer.flush()?;

        Ok(())
    }

    fn close(&mut self) -> Result<(), TransportError> {
        if let Some(mut writer) = self.inner.take() {
            writer.flush()?;
            drop(writer);
        }

        Ok(())
    }
}
