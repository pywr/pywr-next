mod codec;
mod output;
mod session;

use crate::codec::{JsonV1Codec, ServerEnvelope, SessionCodec};
use crate::output::ServiceOutput;
use crate::session::Session;
use pywr_runner_engine::{PywrBackend, RunnerBackend};
use pywr_runner_protocol::{ClientHello, Envelope, HandshakeRejection, ProtocolVersion, v1};
use pywr_runner_transport::{
    InterprocessLocalSocketListener, ReceiveOutcome, TransportConnection, TransportError, TransportReader,
    TransportWriter,
};
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use thiserror::Error;

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

const RUNNING_POLL_INTERVAL: Duration = Duration::from_millis(10);

pub struct RunnerService<B, R> {
    backend: B,
    protocols: R,
    config: RunnerServiceConfig,
}

impl<B, R> RunnerService<B, R>
where
    B: RunnerBackend,
    R: ProtocolRegistry,
{
    pub fn new(backend: B, protocols: R, config: &RunnerServiceConfig) -> Self {
        Self {
            backend,
            protocols,
            config: config.clone(),
        }
    }

    pub fn serve<C>(self, connection: C) -> Result<ServiceExit, ServiceError>
    where
        C: TransportConnection,
    {
        use pywr_runner_engine::{EngineCommand, EngineEvent, EngineStatus, RunnerEngine};
        use pywr_runner_protocol::{BootstrapClientMessage, BootstrapServerMessage, ServerHello, SessionId};

        let (mut reader, mut writer) = connection.split()?;

        // Bootstrap handshake.
        let frame = match reader.receive_frame(Some(self.config.handshake_timeout))? {
            ReceiveOutcome::Frame(frame) => frame,
            ReceiveOutcome::TimedOut => return Err(ServiceError::HandshakeTimeout),
            ReceiveOutcome::Closed => return Ok(ServiceExit::ClientDisconnected),
        };

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
        let mut engine: Option<RunnerEngine<B, ServiceOutput>> = None;
        let mut backend = Some(self.backend);

        loop {
            // Check for cancellation before polling the engine or receiving frames.
            if self
                .config
                .cancel_flag
                .as_ref()
                .is_some_and(|c| c.load(Ordering::Relaxed))
            {
                return Ok(ServiceExit::ClientShutdown);
            }

            let running = engine.as_ref().is_some_and(|engine| {
                matches!(
                    engine.status(),
                    EngineStatus::Initialising
                        | EngineStatus::Running
                        | EngineStatus::Pausing
                        | EngineStatus::Cancelling
                        | EngineStatus::Finalising
                )
            });

            // Poll while the engine has work; block while it is ready.
            let receive_timeout = if running {
                Some(RUNNING_POLL_INTERVAL)
            } else {
                self.config.idle_timeout
            };

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

                            let request = request.try_into()?;

                            let backend = backend.take().ok_or_else(|| {
                                ServiceError::ProtocolViolation("the backend has already been assigned to a run".into())
                            })?;

                            engine = Some(RunnerEngine::initialise(request, backend, output.clone()));
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
                            let current = engine.take().ok_or_else(|| {
                                ServiceError::ProtocolViolation("the run has not been initialised".into())
                            })?;

                            let command: EngineCommand = command.try_into()?;

                            engine = Some(
                                current
                                    .handle_command(command)
                                    .map_err(|error| ServiceError::EngineCommand(error.to_string()))?,
                            );
                        }
                    }
                }

                ReceiveOutcome::TimedOut if !running => {
                    return Err(ServiceError::IdleTimeout);
                }

                ReceiveOutcome::TimedOut => {
                    // Expected while cooperatively running the engine.
                }

                ReceiveOutcome::Closed => {
                    return Ok(ServiceExit::ClientDisconnected);
                }
            }

            if engine.as_ref().is_some_and(|e| e.needs_tick()) {
                let current = engine.take().expect("engine checked above");

                engine = Some(
                    current
                        .tick()
                        .map_err(|error| ServiceError::EngineTick(error.to_string()))?,
                );
            }

            let mut terminal_exit = None;

            for event in output.drain() {
                match &event {
                    EngineEvent::Completed { .. } => {
                        terminal_exit = Some(ServiceExit::RunCompleted);
                    }
                    EngineEvent::Cancelled { .. } => {
                        terminal_exit = Some(ServiceExit::RunCancelled);
                    }
                    EngineEvent::Failed { .. } => {
                        // Add RunFailed if failure should have a distinct exit.
                        terminal_exit = Some(ServiceExit::RunCompleted);
                    }
                    _ => {}
                }

                let message: v1::ServerMessage = event.try_into()?;

                send_server_message(&mut *codec, &mut writer, &mut session, message)?;
            }

            if let Some(exit) = terminal_exit {
                writer.close()?;
                return Ok(exit);
            }
        }
    }
}

fn send_server_message<W>(
    codec: &mut dyn SessionCodec,
    writer: &mut W,
    session: &mut Session,
    message: pywr_runner_protocol::v1::ServerMessage,
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
    cancel_flag: Option<Arc<AtomicBool>>,
}

pub struct RunnerServiceConfigBuilder {
    handshake_timeout: Duration,
    idle_timeout: Option<Duration>,
    cancel_flag: Option<Arc<AtomicBool>>,
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
            cancel_flag: None,
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

    pub fn cancel_flag(&mut self, cancel: Arc<AtomicBool>) -> &mut Self {
        self.cancel_flag = Some(cancel);
        self
    }

    pub fn build(self) -> RunnerServiceConfig {
        RunnerServiceConfig {
            handshake_timeout: self.handshake_timeout,
            idle_timeout: self.idle_timeout,
            cancel_flag: self.cancel_flag,
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

pub fn run_local_socket_server(socket_name: &str, config: &RunnerServiceConfig) -> Result<(), LocalSocketServerError> {
    let listener = InterprocessLocalSocketListener::bind_namespaced(socket_name).map_err(|source| {
        LocalSocketServerError::Bind {
            socket_name: socket_name.to_string(),
            source,
        }
    })?;

    println!("Pywr runner service is listening: {}", listener.name());

    loop {
        let connection = match listener.accept() {
            Ok(connection) => connection,
            Err(error) => {
                println!("failed to accept local-socket connection: {error:?}",);
                continue;
            }
        };

        let service = RunnerService::new(PywrBackend::default(), DefaultProtocolRegistry, config);

        match service.serve(connection) {
            Ok(exit) => {
                println!("runner session exited: {exit:?}");
            }
            Err(error) => {
                println!("runner session failed: {error:?}");
            }
        }
    }
}
