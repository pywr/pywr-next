//! Shared framing primitives for byte-stream transports.

use crate::TransportError;

/// Protect transports against corrupt or malicious frame lengths.
///
/// Increase this later if model documents larger than 64 MiB are expected.
pub(crate) const MAX_FRAME_SIZE: usize = 64 * 1024 * 1024;

/// Incremental decoder for u32-big-endian length-prefixed frames.
#[derive(Debug, Default)]
pub(crate) struct FrameDecoder {
    /// Bytes received for the current frame, including its four-byte header.
    buffered: Vec<u8>,
    /// Decoded length once the complete header has arrived.
    expected_payload_len: Option<usize>,
}

impl FrameDecoder {
    pub(crate) fn push(&mut self, bytes: &[u8]) {
        self.buffered.extend_from_slice(bytes);
    }

    pub(crate) fn buffered_len(&self) -> usize {
        self.buffered.len()
    }

    pub(crate) fn take_complete_frame(&mut self) -> Result<Option<Vec<u8>>, TransportError> {
        if self.expected_payload_len.is_none() && self.buffered.len() >= 4 {
            let header: [u8; 4] = self.buffered[..4].try_into().expect("slice has exactly four bytes");
            let payload_len = u32::from_be_bytes(header) as usize;

            if payload_len > MAX_FRAME_SIZE {
                return Err(TransportError::InvalidFrame(format!(
                    "declared frame size {payload_len} exceeds maximum {MAX_FRAME_SIZE}"
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

        self.buffered.drain(..4);
        let remaining = self.buffered.split_off(payload_len);
        let payload = std::mem::replace(&mut self.buffered, remaining);
        self.expected_payload_len = None;

        Ok(Some(payload))
    }
}
