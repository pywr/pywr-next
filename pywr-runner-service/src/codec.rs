use crate::{CodecError, Envelope};
use pywr_runner_protocol::v1;

pub trait SessionCodec: Send + 'static {
    fn decode_client(&mut self, frame: &[u8]) -> Result<ClientEnvelope, CodecError>;

    fn encode_server(&mut self, envelope: &ServerEnvelope) -> Result<Vec<u8>, CodecError>;
}

#[derive(Debug, Default)]
pub struct JsonV1Codec;

pub type ClientEnvelope = Envelope<v1::ClientCommand>;
pub type ServerEnvelope = Envelope<v1::ServerMessage>;

impl SessionCodec for JsonV1Codec {
    fn decode_client(&mut self, frame: &[u8]) -> Result<ClientEnvelope, CodecError> {
        Ok(serde_json::from_slice(frame)?)
    }

    fn encode_server(&mut self, envelope: &ServerEnvelope) -> Result<Vec<u8>, CodecError> {
        Ok(serde_json::to_vec(envelope)?)
    }
}
