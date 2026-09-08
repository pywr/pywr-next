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
//!
//! # Receiving
//!
//! Frames are received on a dedicated thread per connection. The thread performs
//! blocking reads, decodes frames and hands them over a channel to
//! [`InterprocessLocalSocketReader::receive_frame`], which waits on that channel
//! with an optional timeout.
//!
//! This indirection exists because `interprocess` does not support receive
//! timeouts on Windows named pipes, so a timeout cannot be implemented with a
//! socket-level option on every platform. Waiting on a channel works identically
//! everywhere.
//!
//! Dropping the reader stops its thread. On Unix the thread reads with a short
//! timeout and polls a stop flag; on Windows the pending read is cancelled with
//! `CancelIoEx`. Either way the underlying stream is released promptly, so the
//! peer observes end-of-stream once both halves are dropped.

use super::{
    PeerIdentity, ReceiveOutcome, TransportConnection, TransportError, TransportListener, TransportReader,
    TransportWriter,
};
use crate::framing::{FrameDecoder, MAX_FRAME_SIZE};
use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{
    GenericNamespaced, ListenerOptions, RecvHalf as LocalSocketRecvHalf, SendHalf as LocalSocketSendHalf,
};
use std::io::{ErrorKind, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

/// Number of decoded frames the reader thread may queue ahead of the consumer
/// before it stops reading from the stream.
const READER_QUEUE_CAPACITY: usize = 16;

/// How often a dropped reader re-attempts to interrupt its thread while
/// waiting for it to exit.
const READER_SHUTDOWN_RETRY_INTERVAL: Duration = Duration::from_millis(1);

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
            InterprocessLocalSocketReader::spawn(recv)?,
            InterprocessLocalSocketWriter { inner: Some(send) },
        ))
    }
}

type FrameResult = Result<ReceiveOutcome, TransportError>;

/// Receive half of a framed local-socket connection.
///
/// See the module documentation for how frames are received.
pub struct InterprocessLocalSocketReader {
    frames: Receiver<FrameResult>,
    worker: Option<ReaderWorker>,
}

struct ReaderWorker {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
    interrupter: platform::ReadInterrupter,
}

impl InterprocessLocalSocketReader {
    fn spawn(stream: LocalSocketRecvHalf) -> Result<Self, TransportError> {
        platform::configure_stream(&stream)?;
        let interrupter = platform::ReadInterrupter::new(&stream)?;

        let (sender, frames) = sync_channel(READER_QUEUE_CAPACITY);
        let stop = Arc::new(AtomicBool::new(false));

        let handle = std::thread::Builder::new()
            .name("pywr-runner-transport-reader".to_owned())
            .spawn({
                let stop = stop.clone();
                move || read_frames(stream, sender, &stop)
            })?;

        Ok(Self {
            frames,
            worker: Some(ReaderWorker {
                stop,
                handle,
                interrupter,
            }),
        })
    }
}

impl Drop for InterprocessLocalSocketReader {
    fn drop(&mut self) {
        let Some(worker) = self.worker.take() else {
            return;
        };

        worker.stop.store(true, Ordering::SeqCst);

        // Wait for the thread to exit so that the stream is released. The thread may be
        // blocked either in a read (interrupted below where the platform requires it) or
        // in a send to a full queue (released by draining the queue).
        while !worker.handle.is_finished() {
            worker.interrupter.interrupt();
            while self.frames.try_recv().is_ok() {}
            std::thread::sleep(READER_SHUTDOWN_RETRY_INTERVAL);
        }

        // The thread has finished; there is nothing useful to do with a panic payload here.
        let _ = worker.handle.join();
    }
}

impl TransportReader for InterprocessLocalSocketReader {
    fn receive_frame(&mut self, timeout: Option<Duration>) -> Result<ReceiveOutcome, TransportError> {
        let received = match timeout {
            None => self.frames.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(timeout) => self.frames.recv_timeout(timeout),
        };

        match received {
            Ok(outcome) => outcome,
            Err(RecvTimeoutError::Timeout) => Ok(ReceiveOutcome::TimedOut),
            // The reader thread has exited. It reports why (a closed connection or an error)
            // before doing so, so anything after that is simply a closed connection.
            Err(RecvTimeoutError::Disconnected) => Ok(ReceiveOutcome::Closed),
        }
    }
}

/// Reads from the stream until it closes, fails, the consumer goes away, or a stop is requested.
fn read_frames(mut stream: LocalSocketRecvHalf, sender: SyncSender<FrameResult>, stop: &AtomicBool) {
    let mut decoder = FrameDecoder::default();
    let mut chunk = [0_u8; 8192];

    loop {
        // Deliver every complete frame already buffered before reading more.
        loop {
            if stop.load(Ordering::SeqCst) {
                return;
            }

            match decoder.take_complete_frame() {
                Ok(Some(frame)) => {
                    if sender.send(Ok(ReceiveOutcome::Frame(frame))).is_err() {
                        // The consumer has been dropped.
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

        let final_outcome = match stream.read(&mut chunk) {
            Ok(0) => closed_outcome(&decoder),
            Ok(read) => {
                decoder.push(&chunk[..read]);
                continue;
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            // Receive timeouts (used on platforms that support them, see `configure_stream`)
            // are reported as either TimedOut or WouldBlock depending on the operating system.
            Err(error) if matches!(error.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => continue,
            // Some platforms report a peer that has gone away as an error rather than as EOF.
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::BrokenPipe | ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted
                ) =>
            {
                closed_outcome(&decoder)
            }
            Err(error) => Err(error.into()),
        };

        // A read that failed because the reader was dropped is not worth reporting.
        if !stop.load(Ordering::SeqCst) {
            let _ = sender.send(final_outcome);
        }

        return;
    }
}

fn closed_outcome(decoder: &FrameDecoder) -> FrameResult {
    if decoder.buffered_len() == 0 {
        Ok(ReceiveOutcome::Closed)
    } else {
        Err(TransportError::InvalidFrame(format!(
            "connection closed with {} bytes of an incomplete frame",
            decoder.buffered_len()
        )))
    }
}

/// Platform-specific support for stopping a reader thread that is blocked in a read.
///
/// Unix sockets support receive timeouts, so the thread simply reads with a short timeout
/// and polls the stop flag. Windows named pipes do not, so the pending read is cancelled.
#[cfg(unix)]
mod platform {
    use super::{LocalSocketRecvHalf, TransportError};
    use std::time::Duration;

    /// How often the reader thread checks whether it has been asked to stop while no data
    /// is arriving.
    const STOP_POLL_INTERVAL: Duration = Duration::from_millis(50);

    pub(super) fn configure_stream(stream: &LocalSocketRecvHalf) -> Result<(), TransportError> {
        use interprocess::local_socket::traits::RecvHalf as _;

        stream.set_timeout(Some(STOP_POLL_INTERVAL))?;
        Ok(())
    }

    /// Nothing to do: the receive timeout lets the thread observe the stop flag by itself.
    pub(super) struct ReadInterrupter;

    impl ReadInterrupter {
        pub(super) fn new(_stream: &LocalSocketRecvHalf) -> Result<Self, TransportError> {
            Ok(Self)
        }

        pub(super) fn interrupt(&self) {}
    }
}

#[cfg(windows)]
mod platform {
    use super::{LocalSocketRecvHalf, TransportError};
    use std::os::windows::io::{AsHandle, AsRawHandle, OwnedHandle};
    use windows_sys::Win32::System::IO::CancelIoEx;

    /// Named pipes do not support receive timeouts (and `interprocess` reports an error if
    /// one is requested), so the stream is used as-is.
    pub(super) fn configure_stream(_stream: &LocalSocketRecvHalf) -> Result<(), TransportError> {
        Ok(())
    }

    /// Cancels the reader thread's pending read on the pipe.
    ///
    /// Holds a duplicate of the pipe handle. It refers to the same pipe object as the
    /// handle the thread reads from, so cancelling through it reaches that read, and it
    /// guarantees the handle value is still valid when the cancellation is issued.
    pub(super) struct ReadInterrupter {
        pipe: OwnedHandle,
    }

    impl ReadInterrupter {
        pub(super) fn new(stream: &LocalSocketRecvHalf) -> Result<Self, TransportError> {
            // On Windows the local-socket enum only has the named-pipe variant.
            let LocalSocketRecvHalf::NamedPipe(pipe) = stream;
            let pipe = pipe.as_handle().try_clone_to_owned()?;
            Ok(Self { pipe })
        }

        /// Cancel all of this process's pending I/O on the pipe. Note that this includes
        /// a write in flight on the send half, which shares the pipe; the send half should
        /// therefore be closed or idle before the reader is dropped.
        pub(super) fn interrupt(&self) {
            // SAFETY: `pipe` is a valid open handle for as long as `self` exists, and a null
            // OVERLAPPED pointer requests cancellation of every pending operation on it.
            // Failure (for example, no operation pending) is harmless and ignored.
            unsafe {
                CancelIoEx(self.pipe.as_raw_handle(), std::ptr::null());
            }
        }
    }
}

/// Send half of a framed local-socket connection.
pub struct InterprocessLocalSocketWriter {
    // Option allows `close()` to drop the send half immediately.
    inner: Option<LocalSocketSendHalf>,
}

impl InterprocessLocalSocketWriter {
    /// Write raw bytes without framing. Used by tests to exercise the decoder.
    #[cfg(test)]
    fn send_raw(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        let writer = self.inner.as_mut().expect("connection is open");
        writer.write_all(bytes)?;
        writer.flush()?;
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    const WAIT: Duration = Duration::from_secs(5);

    fn unique_socket_name() -> String {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        format!(
            "pywr-runner-transport-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        )
    }

    type SplitConnection = (InterprocessLocalSocketReader, InterprocessLocalSocketWriter);

    /// Bind a listener and connect a client to it, returning the split server and client ends.
    fn connected_pair() -> (SplitConnection, SplitConnection) {
        let name = unique_socket_name();
        let listener = InterprocessLocalSocketListener::bind_namespaced(name.clone()).expect("bind");

        let client =
            std::thread::spawn(move || InterprocessLocalSocketConnection::connect_namespaced(&name).expect("connect"));

        let server = listener.accept().expect("accept");
        let client = client.join().expect("client thread");

        (
            server.split().expect("split server"),
            client.split().expect("split client"),
        )
    }

    #[test]
    fn frames_round_trip_with_timeouts() {
        let ((mut server_reader, mut server_writer), (mut client_reader, mut client_writer)) = connected_pair();

        client_writer.send_frame(b"hello").unwrap();
        assert_eq!(
            server_reader.receive_frame(Some(WAIT)).unwrap(),
            ReceiveOutcome::Frame(b"hello".to_vec())
        );

        // Nothing pending: a bounded wait times out rather than blocking.
        assert_eq!(
            server_reader.receive_frame(Some(Duration::from_millis(50))).unwrap(),
            ReceiveOutcome::TimedOut
        );

        // Empty frames and multiple frames in flight are delivered in order.
        server_writer.send_frame(b"").unwrap();
        server_writer.send_frame(b"second").unwrap();
        assert_eq!(
            client_reader.receive_frame(None).unwrap(),
            ReceiveOutcome::Frame(Vec::new())
        );
        assert_eq!(
            client_reader.receive_frame(None).unwrap(),
            ReceiveOutcome::Frame(b"second".to_vec())
        );

        // Closing the client makes the server observe end-of-stream.
        client_writer.close().unwrap();
        drop(client_reader);
        drop(client_writer);
        assert_eq!(server_reader.receive_frame(Some(WAIT)).unwrap(), ReceiveOutcome::Closed);
    }

    #[test]
    fn fragmented_frames_are_reassembled() {
        let ((mut server_reader, _server_writer), (_client_reader, mut client_writer)) = connected_pair();

        let payload = b"fragmented payload";
        let mut bytes = (payload.len() as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(payload);
        // A second frame so that the tail of one read belongs to the next frame.
        bytes.extend_from_slice(&3_u32.to_be_bytes());
        bytes.extend_from_slice(b"abc");

        for piece in bytes.chunks(3) {
            client_writer.send_raw(piece).unwrap();
            std::thread::sleep(Duration::from_millis(2));
        }

        assert_eq!(
            server_reader.receive_frame(Some(WAIT)).unwrap(),
            ReceiveOutcome::Frame(payload.to_vec())
        );
        assert_eq!(
            server_reader.receive_frame(Some(WAIT)).unwrap(),
            ReceiveOutcome::Frame(b"abc".to_vec())
        );
    }

    #[test]
    fn oversized_frame_is_rejected() {
        let ((mut server_reader, _server_writer), (_client_reader, mut client_writer)) = connected_pair();

        client_writer
            .send_raw(&(MAX_FRAME_SIZE as u32 + 1).to_be_bytes())
            .unwrap();

        let error = server_reader.receive_frame(Some(WAIT)).unwrap_err();
        assert!(matches!(error, TransportError::InvalidFrame(_)), "{error:?}");
    }

    #[test]
    fn dropping_the_reader_releases_the_connection() {
        let ((server_reader, server_writer), (mut client_reader, _client_writer)) = connected_pair();

        // The server's reader thread is blocked waiting for data when both halves are dropped.
        drop(server_writer);
        drop(server_reader);

        // The client must observe end-of-stream rather than waiting forever.
        assert_eq!(client_reader.receive_frame(Some(WAIT)).unwrap(), ReceiveOutcome::Closed);
    }
}
