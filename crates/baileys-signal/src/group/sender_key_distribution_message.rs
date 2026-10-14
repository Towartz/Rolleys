use baileys_proto::proto::SenderKeyDistributionMessage as ProtoSkdm;
use prost::Message;

use crate::SignalError;

const SENDER_KEY_DIST_VERSION: u8 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderKeyDistributionMessage {
    id: u32,
    iteration: u32,
    chain_key: Vec<u8>,
    signing_key: Vec<u8>,
    serialized: Vec<u8>,
}

impl SenderKeyDistributionMessage {
    pub fn new(
        id: u32,
        iteration: u32,
        chain_key: Vec<u8>,
        signing_key: Vec<u8>,
    ) -> Result<Self, SignalError> {
        let version_byte = (SENDER_KEY_DIST_VERSION << 4) | SENDER_KEY_DIST_VERSION;

        let proto_msg = ProtoSkdm {
            id: Some(id),
            iteration: Some(iteration),
            chain_key: Some(chain_key.clone()),
            signing_key: Some(signing_key.clone()),
        };

        let mut msg_bytes = Vec::new();
        proto_msg
            .encode(&mut msg_bytes)
            .map_err(|e| SignalError::Serialization(e.to_string()))?;

        let mut serialized = Vec::with_capacity(1 + msg_bytes.len());
        serialized.push(version_byte);
        serialized.extend_from_slice(&msg_bytes);

        Ok(Self {
            id,
            iteration,
            chain_key,
            signing_key,
            serialized,
        })
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self, SignalError> {
        if bytes.is_empty() {
            return Err(SignalError::InvalidMessageLength);
        }

        let proto_msg = if bytes.len() > 1 && (bytes[0] >> 4 == 3 || bytes[0] == 0x33) {
            ProtoSkdm::decode(&bytes[1..])
                .or_else(|_| ProtoSkdm::decode(bytes))
                .map_err(|e| SignalError::Deserialization(e.to_string()))?
        } else {
            ProtoSkdm::decode(bytes)
                .or_else(|_| ProtoSkdm::decode(&bytes[1..]))
                .map_err(|e| SignalError::Deserialization(e.to_string()))?
        };

        let id = proto_msg.id.ok_or(SignalError::InvalidMessage)?;
        let iteration = proto_msg.iteration.ok_or(SignalError::InvalidMessage)?;
        let chain_key = proto_msg.chain_key.ok_or(SignalError::InvalidMessage)?;
        let signing_key = proto_msg.signing_key.ok_or(SignalError::InvalidMessage)?;

        Ok(Self {
            id,
            iteration,
            chain_key,
            signing_key,
            serialized: bytes.to_vec(),
        })
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn iteration(&self) -> u32 {
        self.iteration
    }

    pub fn chain_key(&self) -> &[u8] {
        &self.chain_key
    }

    pub fn signing_key(&self) -> &[u8] {
        &self.signing_key
    }

    pub fn serialize(&self) -> &[u8] {
        &self.serialized
    }
}
