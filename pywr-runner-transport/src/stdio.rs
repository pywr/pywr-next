//! Framed IPC transport over a process's standard input and output.
//!
//! Stdin carries incoming frames and stdout carries outgoing frames. Consequently,
//! applications using this transport must reserve stdout exclusively for protocol
//! bytes and write human-readable diagnostics to stderr.

use crate::framing::{FrameDecoder, MAX_FRAME_SIZE};
use crate::{PeerIdentity, ReceiveOutcome, TransportConnection, TransportError, TransportReader, TransportWriter};
use std::io::{self, Read, Write};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

const READ_CHUNK_SIZE: usize = 8192;

type FrameResult = Result<ReceiveOutcome, TransportError>;

/// A framed, single-connection transport over standard input and standard output.
///
/// `StdioConnection::stdio` uses the process standard streams. `from_streams` is
/// provided for embedding and testing with alternate stream implementations.
pub struct StdioConnection<R = io::Stdin, W = io::Stdout> {
    input: R,
    output: W,
}

impl StdioConnection {
    /// Creates a connection using the process's standard input and standard output.
    pub fn stdio() -> Self {
        Self::from_streams(io::stdin(), io::stdout())
    }
}

impl<R, W> StdioConnection<R, W> {
    /// Creates a connection from its input and output byte streams.
    pub fn from_streams(input: R, output: W) -> Self {
        Self { input, output }
    }
}

impl<R, W> TransportConnection for StdioConnection<R, W>
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
{
    type Reader = StdioReader;
    type Writer = StdioWriter<W>;

    fn peer_identity(&self) -> Result<PeerIdentity, TransportError> {
        Err(TransportError::Unsupported(
            "peer identity is unavailable for standard input/output",
        ))
    }

    fn split(self) -> Result<(Self::Reader, Self::Writer), TransportError> {
        Ok((
            StdioReader::spawn(self.input)?,
            StdioWriter {
                inner: Some(self.output),
            },
        ))
    }
}

/// Receive half of a framed standard-I/O connection.
///
/// A worker thread performs blocking reads so `receive_frame` can honor timeouts.
/// Standard input cannot be portably interrupted without closing the process-wide
/// input handle, so dropping this reader does not wait for a blocked worker. The
/// worker exits on EOF, an input error, or its next decoded frame after the reader
/// has been dropped.
pub struct StdioReader {
    frames: Receiver<FrameResult>,
}

impl StdioReader {
    fn spawn<R: Read + Send + 'static>(input: R) -> Result<Self, TransportError> {
        let (sender, frames) = channel();
        std::thread::Builder::new()
            .name("pywr-runner-transport-stdio-reader".to_owned())
            .spawn(move || read_frames(input, sender))?;
        Ok(Self { frames })
    }
}

impl TransportReader for StdioReader {
    fn receive_frame(&mut self, timeout: Option<Duration>) -> Result<ReceiveOutcome, TransportError> {
        let received = match timeout {
            None => self.frames.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(timeout) => self.frames.recv_timeout(timeout),
        };

        match received {
            Ok(outcome) => outcome,
            Err(RecvTimeoutError::Timeout) => Ok(ReceiveOutcome::TimedOut),
            Err(RecvTimeoutError::Disconnected) => Ok(ReceiveOutcome::Closed),
        }
    }
}

fn read_frames<R: Read>(mut input: R, sender: Sender<FrameResult>) {
    let mut decoder = FrameDecoder::default();
    let mut chunk = [0_u8; READ_CHUNK_SIZE];

    loop {
        loop {
            match decoder.take_complete_frame() {
                Ok(Some(frame)) => {
                    if sender.send(Ok(ReceiveOutcome::Frame(frame))).is_err() {
                        return;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    let _ = sender.send(Err(error));
                    return;
                }
            }
        }

        match input.read(&mut chunk) {
            Ok(0) => {
                let outcome = if decoder.buffered_len() == 0 {
                    Ok(ReceiveOutcome::Closed)
                } else {
                    Err(TransportError::InvalidFrame(format!(
                        "connection closed with {} bytes of an incomplete frame",
                        decoder.buffered_len()
                    )))
                };
                let _ = sender.send(outcome);
                return;
            }
            Ok(read) => decoder.push(&chunk[..read]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                let _ = sender.send(Err(error.into()));
                return;
            }
        }
    }
}

/// Send half of a framed standard-I/O connection.
///
/// `close` flushes and logically closes this writer. For the default process
/// stdout handle, end-of-stream is observable by the peer when the process exits.
pub struct StdioWriter<W = io::Stdout> {
    inner: Option<W>,
}

impl<W: Write> TransportWriter for StdioWriter<W> {
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

        let output = self
            .inner
            .as_mut()
            .ok_or_else(|| TransportError::InvalidFrame("attempted to write to a closed connection".to_owned()))?;
        output.write_all(&payload_len.to_be_bytes())?;
        output.write_all(frame)?;
        output.flush()?;
        Ok(())
    }

    fn close(&mut self) -> Result<(), TransportError> {
        if let Some(mut output) = self.inner.take() {
            output.flush()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{mpsc, Arc, Mutex};

    const WAIT: Duration = Duration::from_secs(1);

    #[derive(Clone, Default)]
    struct SharedOutput(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedOutput {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct ChunkedInput {
        bytes: Vec<u8>,
        chunk_size: usize,
    }

    impl Read for ChunkedInput {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            let count = output.len().min(self.chunk_size).min(self.bytes.len());
            output[..count].copy_from_slice(&self.bytes[..count]);
            self.bytes.drain(..count);
            Ok(count)
        }
    }

    #[test]
    fn frames_are_decoded_and_encoded() {
        let mut input = Vec::new();
        input.extend_from_slice(&5_u32.to_be_bytes());
        input.extend_from_slice(b"hello");
        input.extend_from_slice(&0_u32.to_be_bytes());
        input.extend_from_slice(&3_u32.to_be_bytes());
        input.extend_from_slice(b"bye");
        let output = SharedOutput::default();
        let output_bytes = output.0.clone();
        let connection = StdioConnection::from_streams(
            ChunkedInput {
                bytes: input,
                chunk_size: 2,
            },
            output,
        );
        let (mut reader, mut writer) = connection.split().unwrap();

        assert_eq!(
            reader.receive_frame(Some(WAIT)).unwrap(),
            ReceiveOutcome::Frame(b"hello".to_vec())
        );
        assert_eq!(
            reader.receive_frame(Some(WAIT)).unwrap(),
            ReceiveOutcome::Frame(Vec::new())
        );
        assert_eq!(
            reader.receive_frame(Some(WAIT)).unwrap(),
            ReceiveOutcome::Frame(b"bye".to_vec())
        );
        assert_eq!(reader.receive_frame(Some(WAIT)).unwrap(), ReceiveOutcome::Closed);

        writer.send_frame(b"reply").unwrap();
        writer.close().unwrap();
        assert_eq!(
            *output_bytes.lock().unwrap(),
            [0, 0, 0, 5, b'r', b'e', b'p', b'l', b'y']
        );
    }

    #[test]
    fn peer_identity_is_unsupported() {
        let connection = StdioConnection::from_streams(std::io::empty(), SharedOutput::default());
        assert!(matches!(
            connection.peer_identity(),
            Err(TransportError::Unsupported(_))
        ));
    }

    #[test]
    fn receives_timeout_while_input_is_blocked() {
        struct BlockingInput(mpsc::Receiver<()>);
        impl Read for BlockingInput {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                let _ = self.0.recv();
                Ok(0)
            }
        }

        let (release, input) = mpsc::channel();
        let connection = StdioConnection::from_streams(BlockingInput(input), SharedOutput::default());
        let (mut reader, _writer) = connection.split().unwrap();
        assert_eq!(
            reader.receive_frame(Some(Duration::from_millis(10))).unwrap(),
            ReceiveOutcome::TimedOut
        );
        drop(release);
        assert_eq!(reader.receive_frame(Some(WAIT)).unwrap(), ReceiveOutcome::Closed);
    }

    #[test]
    fn rejects_incomplete_and_oversized_input_frames() {
        let cases = [vec![0, 0, 0, 2, 1], (MAX_FRAME_SIZE as u32 + 1).to_be_bytes().to_vec()];

        for bytes in cases {
            let connection = StdioConnection::from_streams(std::io::Cursor::new(bytes), SharedOutput::default());
            let (mut reader, _writer) = connection.split().unwrap();
            assert!(matches!(
                reader.receive_frame(Some(WAIT)),
                Err(TransportError::InvalidFrame(_))
            ));
        }
    }
}
