mod codec;
mod output;
mod session;

use crate::codec::{JsonV1Codec, ServerEnvelope, SessionCodec};
use crate::output::ServiceOutput;
use crate::session::Session;
use log::{error, info};
use pywr_runner_engine::{PywrBackend, RunnerBackend};
use pywr_runner_protocol::{ClientHello, Envelope, HandshakeRejection, ProtocolVersion, v1};
use pywr_runner_transport::{
    InterprocessLocalSocketListener, ReceiveOutcome, StdioConnection, TransportConnection, TransportError,
    TransportReader, TransportWriter,
};
use std::convert::Infallible;
use std::sync::{OnceLock, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;
use thiserror::Error;

pub use pywr_runner_engine::install_log_router_with;

static INTERRUPT_HANDLER: OnceLock<Box<dyn Fn() -> bool + 'static + Send + Sync>> = OnceLock::new();

pub fn install_interrupt_handler<F>(handler: F)
where
    F: Fn() -> bool + Send + Sync + 'static,
{
    INTERRUPT_HANDLER.set(Box::new(handler)).ok();
}

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("JSON codec error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type BootstrapCodecError = serde_json::Error;

pub trait ProtocolRegistry: Send + Sync + 'static {
    fn negotiate(&self, hello: &ClientHello) -> Result<NegotiatedProtocol, HandshakeRejection>;
}

#[derive(Debug, Default)]
pub struct DefaultProtocolRegistry;

impl ProtocolRegistry for DefaultProtocolRegistry {
    fn negotiate(&self, hello: &ClientHello) -> Result<NegotiatedProtocol, HandshakeRejection> {
        let version = hello
            .supported_versions
            .iter()
            .copied()
            .find(|version| version.major == 1)
            .ok_or_else(|| HandshakeRejection::UnsupportedVersion {
                supported: vec![ProtocolVersion { major: 1, minor: 0 }],
            })?;

        if !hello.required_capabilities.is_empty() {
            return Err(HandshakeRejection::MissingCapabilities {
                capabilities: hello.required_capabilities.clone(),
            });
        }

        Ok(NegotiatedProtocol {
            version,
            capabilities: Vec::new(),
            codec: Box::new(JsonV1Codec),
        })
    }
}

pub struct NegotiatedProtocol {
    pub version: ProtocolVersion,
    pub capabilities: Vec<String>,
    pub codec: Box<dyn SessionCodec>,
}

#[derive(Debug)]
pub enum ServiceExit {
    ClientShutdown,
    ClientDisconnected,
    RunCompleted,
    RunCancelled,
    RunFailed,
    HandshakeRejected,
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("transport error: {0}")]
    Transport(#[from] TransportError),

    #[error("bootstrap codec error: {0}")]
    BootstrapCodec(#[from] BootstrapCodecError),

    #[error("session codec error: {0}")]
    Codec(#[from] CodecError),

    #[error("handshake timed out")]
    HandshakeTimeout,

    #[error("session timed out")]
    IdleTimeout,

    #[error("invalid session id")]
    InvalidSession,

    #[error("received sequence {received}; expected {expected}")]
    InvalidSequence { expected: u64, received: u64 },

    #[error("engine worker terminated unexpectedly")]
    EngineUnavailable,

    #[error("protocol violation: {0}")]
    ProtocolViolation(String),

    #[error("invalid run id")]
    InvalidRun,

    #[error("engine command failed: {0}")]
    EngineCommand(String),

    #[error("engine tick failed: {0}")]
    EngineTick(String),

    #[error("Infallible error: {0}")]
    Infallible(#[from] Infallible),
}

struct EngineWorker {
    commands: mpsc::Sender<pywr_runner_engine::EngineCommand>,
    handle: JoinHandle<Result<(), String>>,
}

impl EngineWorker {
    fn spawn<B>(engine: pywr_runner_engine::RunnerEngine<B, ServiceOutput>) -> Self
    where
        B: RunnerBackend + Send + 'static,
        B::Runtime: Send + 'static,
    {
        let (commands, receiver) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            let mut engine = engine;
            loop {
                while let Ok(command) = receiver.try_recv() {
                    engine.handle_command(command).map_err(|error| error.to_string())?;
                }

                if engine.needs_tick() {
                    engine = engine.tick().map_err(|error| error.to_string())?;
                    if engine.is_terminal() {
                        return Ok(());
                    }
                } else {
                    let command = receiver
                        .recv()
                        .map_err(|_| "engine command channel disconnected".to_string())?;
                    engine.handle_command(command).map_err(|error| error.to_string())?;
                }
            }
        });
        Self { commands, handle }
    }

    fn send(&self, command: pywr_runner_engine::EngineCommand) -> Result<(), ServiceError> {
        self.commands.send(command).map_err(|_| ServiceError::EngineUnavailable)
    }

    fn join(self) -> Result<(), ServiceError> {
        self.handle
            .join()
            .map_err(|_| ServiceError::EngineUnavailable)?
            .map_err(ServiceError::EngineTick)
    }

    fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }
}

pub struct RunnerService<B, R> {
    backend: B,
    protocols: R,
    config: RunnerServiceConfig,
}

impl<B, R> RunnerService<B, R>
where
    B: RunnerBackend + Send + 'static,
    B::Runtime: Send + 'static,
    R: ProtocolRegistry,
{
    pub fn new(backend: B, protocols: R, config: RunnerServiceConfig) -> Self {
        Self {
            backend,
            protocols,
            config,
        }
    }

    pub fn serve<C>(self, connection: C) -> Result<ServiceExit, ServiceError>
    where
        C: TransportConnection,
    {
        use pywr_runner_engine::{EngineCommand, EngineEvent, RunnerEngine};
        use pywr_runner_protocol::{BootstrapClientMessage, BootstrapServerMessage, ServerHello, SessionId};

        let (mut reader, mut writer) = connection.split()?;

        // Bootstrap handshake.
        info!("Waiting for client handshake...");
        let frame = match reader.receive_frame(Some(self.config.handshake_timeout))? {
            ReceiveOutcome::Frame(frame) => frame,
            ReceiveOutcome::TimedOut => return Err(ServiceError::HandshakeTimeout),
            ReceiveOutcome::Closed => return Ok(ServiceExit::ClientDisconnected),
        };

        info!("Handshake complete!");

        let bootstrap: BootstrapClientMessage = serde_json::from_slice(&frame)?;
        let BootstrapClientMessage::Hello(hello) = bootstrap;

        let negotiated = match self.protocols.negotiate(&hello) {
            Ok(negotiated) => negotiated,
            Err(rejection) => {
                let response = BootstrapServerMessage::Rejected(rejection);
                writer.send_frame(&serde_json::to_vec(&response)?)?;
                writer.close()?;
                return Ok(ServiceExit::HandshakeRejected);
            }
        };

        let session_id = SessionId::new();

        let response = BootstrapServerMessage::Accepted(ServerHello {
            selected_version: negotiated.version,
            session_id,
            capabilities: negotiated.capabilities,
        });

        writer.send_frame(&serde_json::to_vec(&response)?)?;

        let mut codec = negotiated.codec;
        let mut session = Session::new(session_id);
        let output = ServiceOutput::default();

        // The service has no engine until Initialise is received.
        let mut engine: Option<EngineWorker> = None;
        let mut backend = Some(self.backend);
        let mut current_progress: Option<v1::RunProgress> = None;

        loop {
            // Check for cancellation before polling the engine or receiving frames.
            if INTERRUPT_HANDLER.get().is_some_and(|f| f()) {
                info!("Runner service received interrupt signal; shutting down");
                return Ok(ServiceExit::ClientShutdown);
            }

            let running = engine.is_some();

            // Poll while the engine has work and while an unbounded-idle
            // session is waiting, so interrupts are observed promptly.
            let receive_timeout = self.config.receive_timeout(running);

            match reader.receive_frame(receive_timeout)? {
                ReceiveOutcome::Frame(frame) => {
                    let envelope = codec.decode_client(&frame)?;
                    session.validate_client_envelope(&envelope)?;

                    match envelope.payload {
                        v1::ClientCommand::Initialise { request } => {
                            if engine.is_some() {
                                return Err(ServiceError::ProtocolViolation(
                                    "a run has already been initialised".into(),
                                ));
                            }

                            let run_id = envelope.run_id.unwrap_or_default();
                            session.run_id = Some(run_id);

                            let request = (*request).try_into()?;

                            let backend = backend.take().ok_or_else(|| {
                                ServiceError::ProtocolViolation("the backend has already been assigned to a run".into())
                            })?;

                            engine = Some(EngineWorker::spawn(RunnerEngine::initialise(
                                request,
                                backend,
                                output.clone(),
                            )));
                        }

                        v1::ClientCommand::Ping { nonce } => {
                            send_server_message(
                                &mut *codec,
                                &mut writer,
                                &mut session,
                                v1::ServerMessage::Pong { nonce },
                            )?;
                        }

                        v1::ClientCommand::Shutdown => {
                            send_server_message(
                                &mut *codec,
                                &mut writer,
                                &mut session,
                                v1::ServerMessage::Goodbye {
                                    reason: v1::GoodbyeReason::Normal,
                                },
                            )?;

                            writer.close()?;
                            return Ok(ServiceExit::ClientShutdown);
                        }

                        command => {
                            let current = engine.as_ref().ok_or_else(|| {
                                ServiceError::ProtocolViolation("the run has not been initialised".into())
                            })?;

                            let command: EngineCommand = command.try_into()?;

                            current.send(command)?;
                        }
                    }
                }

                ReceiveOutcome::TimedOut if !running && self.config.idle_timeout.is_some() => {
                    return Err(ServiceError::IdleTimeout);
                }

                ReceiveOutcome::TimedOut => {
                    // Expected while cooperatively running the engine.
                }

                ReceiveOutcome::Closed => {
                    return Ok(ServiceExit::ClientDisconnected);
                }
            }

            let mut terminal_exit = None;
            let mut arrow_stream_commits = Vec::new();
            let mut messages = Vec::new();
            let mut progress_changed = false;

            for event in output.drain() {
                match &event {
                    EngineEvent::Completed { .. } => {
                        terminal_exit = Some(ServiceExit::RunCompleted);
                    }
                    EngineEvent::Cancelled { .. } => {
                        terminal_exit = Some(ServiceExit::RunCancelled);
                    }
                    EngineEvent::Failed { .. } => {
                        terminal_exit = Some(ServiceExit::RunFailed);
                    }
                    EngineEvent::Progress { progress } => {
                        current_progress = Some(progress.clone().try_into()?);
                        progress_changed = true;
                        continue;
                    }
                    EngineEvent::Initialised { progress, .. } => {
                        current_progress = Some(progress.clone().try_into()?);
                    }
                    EngineEvent::ArrowStreamCommitted { commit } => {
                        arrow_stream_commits.push(commit.clone());
                        continue;
                    }
                    EngineEvent::StateChanged { .. } => {}
                    EngineEvent::Log { .. } => {}
                    EngineEvent::CommandRejected { .. } => {}
                }

                let message: v1::ServerMessage = event.try_into()?;
                messages.push(message);
            }

            if progress_changed || !arrow_stream_commits.is_empty() {
                send_server_message(
                    &mut *codec,
                    &mut writer,
                    &mut session,
                    v1::ServerMessage::Update {
                        // Commits are never discarded. An initialised event is
                        // emitted before a recorder can commit; retain the most
                        // recent progress so commit-only drains remain useful.
                        progress: current_progress.clone().unwrap_or(v1::RunProgress {
                            completed_timesteps: 0,
                            total_timesteps: 0,
                            last_completed_date: None,
                            next_date: None,
                        }),
                        arrow_stream_commits: arrow_stream_commits.into_iter().map(Into::into).collect(),
                    },
                )?;
            }

            for message in messages {
                send_server_message(&mut *codec, &mut writer, &mut session, message)?;
            }

            if let Some(exit) = terminal_exit {
                engine.take().expect("terminal engine worker must exist").join()?;
                writer.close()?;
                return Ok(exit);
            }

            if engine.as_ref().is_some_and(EngineWorker::is_finished) {
                engine.take().expect("finished engine worker must exist").join()?;
                return Err(ServiceError::EngineUnavailable);
            }
        }
    }
}

fn send_server_message<W>(
    codec: &mut dyn SessionCodec,
    writer: &mut W,
    session: &mut Session,
    message: v1::ServerMessage,
) -> Result<(), ServiceError>
where
    W: TransportWriter,
{
    let envelope = ServerEnvelope {
        session_id: session.session_id,
        run_id: session.run_id,
        sequence: session.next_server_sequence,
        payload: message,
    };

    let frame = codec.encode_server(&envelope)?;
    writer.send_frame(&frame)?;

    session.next_server_sequence += 1;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct RunnerServiceConfig {
    handshake_timeout: Duration,
    idle_timeout: Option<Duration>,
    update_interval: Duration,
}

impl RunnerServiceConfig {
    /// Returns a bounded receive timeout so an unbounded-idle service can poll interrupts.
    fn receive_timeout(&self, running: bool) -> Option<Duration> {
        if running {
            Some(self.update_interval)
        } else {
            Some(self.idle_timeout.unwrap_or(self.update_interval))
        }
    }
}

pub struct RunnerServiceConfigBuilder {
    handshake_timeout: Duration,
    idle_timeout: Option<Duration>,
    update_interval: Duration,
}

impl Default for RunnerServiceConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl RunnerServiceConfigBuilder {
    pub fn new() -> Self {
        Self {
            handshake_timeout: Duration::from_secs(10),
            idle_timeout: None,
            update_interval: Duration::from_millis(100),
        }
    }

    pub fn handshake_timeout(&mut self, timeout: Duration) -> &mut Self {
        self.handshake_timeout = timeout;
        self
    }

    pub fn idle_timeout(&mut self, timeout: Duration) -> &mut Self {
        self.idle_timeout = Some(timeout);
        self
    }

    pub fn update_interval(&mut self, interval: Duration) -> &mut Self {
        self.update_interval = interval;
        self
    }

    pub fn build(self) -> RunnerServiceConfig {
        RunnerServiceConfig {
            handshake_timeout: self.handshake_timeout,
            idle_timeout: self.idle_timeout,
            update_interval: self.update_interval,
        }
    }
}

#[derive(Debug, Error)]
pub enum LocalSocketServerError {
    #[error("failed to bind local socket {socket_name:?}: {source}")]
    Bind {
        socket_name: String,
        #[source]
        source: TransportError,
    },

    #[error("failed to accept local socket connection: {0}")]
    Accept(#[from] std::io::Error),
}

pub fn run_local_socket_server(socket_name: &str, config: RunnerServiceConfig) -> Result<(), LocalSocketServerError> {
    let listener = InterprocessLocalSocketListener::bind_namespaced(socket_name).map_err(|source| {
        LocalSocketServerError::Bind {
            socket_name: socket_name.to_string(),
            source,
        }
    })?;

    info!("Pywr runner service is listening: {}", listener.name());

    loop {
        if INTERRUPT_HANDLER.get().is_some_and(|f| f()) {
            info!("Runner service received interrupt signal; shutting down");
            break;
        }

        let connection = match listener.accept() {
            Ok(connection) => connection,
            Err(error) => {
                match &error {
                    TransportError::Io(io_error) => {
                        match io_error.kind() {
                            std::io::ErrorKind::WouldBlock => {
                                // No connection is available yet; continue polling.
                                std::thread::sleep(Duration::from_millis(10));
                                continue;
                            }
                            _ => {
                                error!("failed to accept local-socket connection: {error:?}",);
                                continue;
                            }
                        }
                    }
                    _ => {
                        error!("failed to accept local-socket connection: {error:?}",);
                        continue;
                    }
                }
            }
        };

        let service = RunnerService::new(PywrBackend::default(), DefaultProtocolRegistry, config.clone());

        match service.serve(connection) {
            Ok(exit) => match exit {
                ServiceExit::ClientShutdown => {
                    info!("runner session exited: client shutdown");
                    break;
                }
                ServiceExit::ClientDisconnected => {
                    info!("runner session exited: client disconnected");
                }
                ServiceExit::RunCompleted => {
                    info!("runner session exited: run completed");
                }
                ServiceExit::RunCancelled => {
                    info!("runner session exited: run cancelled");
                }
                ServiceExit::RunFailed => {
                    info!("runner session exited: run failed");
                }
                ServiceExit::HandshakeRejected => {
                    info!("runner session exited: handshake rejected");
                }
            },
            Err(error) => {
                error!("runner session failed: {error:?}");
            }
        }
    }

    Ok(())
}

/// Runs one runner-service session using the process standard input and output.
///
/// Stdout is reserved for framed protocol output. All diagnostics are emitted through
/// the logging facade and must therefore be configured to use stderr by the caller.
pub fn run_stdio_server(config: RunnerServiceConfig) -> Result<ServiceExit, ServiceError> {
    let service = RunnerService::new(PywrBackend::default(), DefaultProtocolRegistry, config);
    let exit = service.serve(StdioConnection::stdio())?;
    info!("runner stdio session exited: {exit:?}");
    Ok(exit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unbounded_idle_sessions_poll_at_the_update_interval() {
        let mut builder = RunnerServiceConfigBuilder::new();
        builder.update_interval(Duration::from_millis(25));
        let config = builder.build();

        assert_eq!(config.receive_timeout(false), Some(Duration::from_millis(25)));
        assert_eq!(config.receive_timeout(true), Some(Duration::from_millis(25)));
    }

    #[test]
    fn configured_idle_timeout_remains_the_idle_receive_deadline() {
        let mut builder = RunnerServiceConfigBuilder::new();
        builder
            .idle_timeout(Duration::from_secs(3))
            .update_interval(Duration::from_millis(25));
        let config = builder.build();

        assert_eq!(config.receive_timeout(false), Some(Duration::from_secs(3)));
        assert_eq!(config.receive_timeout(true), Some(Duration::from_millis(25)));
    }
}
