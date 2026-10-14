use baileys_crypto::{sign_ed25519, verify_ed25519};
use baileys_proto::proto::SenderKeyMessage as ProtoSenderKeyMessage;
use prost::Message;

use crate::SignalError;

pub const SENDER_KEY_MESSAGE_VERSION: u8 = 3;
pub const SIGNATURE_LENGTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderKeyMessage {
    key_id: u32,
    iteration: u32,
    ciphertext: Vec<u8>,
    signature: [u8; SIGNATURE_LENGTH],
    serialized: Vec<u8>,
}

impl SenderKeyMessage {
    pub fn new(
        key_id: u32,
        iteration: u32,
        ciphertext: Vec<u8>,
        signing_key_private: &[u8; 32],
    ) -> Result<Self, SignalError> {
        let version_byte = (SENDER_KEY_MESSAGE_VERSION << 4) | SENDER_KEY_MESSAGE_VERSION;

        let proto_msg = ProtoSenderKeyMessage {
            id: Some(key_id),
            iteration: Some(iteration),
            ciphertext: Some(ciphertext.clone()),
        };

        let mut msg_bytes = Vec::new();
        proto_msg
            .encode(&mut msg_bytes)
            .map_err(|e| SignalError::Serialization(e.to_string()))?;

        let mut to_sign = Vec::with_capacity(1 + msg_bytes.len());
        to_sign.push(version_byte);
        to_sign.extend_from_slice(&msg_bytes);

        let signature = sign_ed25519(signing_key_private, &to_sign);

        let mut serialized = Vec::with_capacity(to_sign.len() + SIGNATURE_LENGTH);
        serialized.extend_from_slice(&to_sign);
        serialized.extend_from_slice(&signature);

        Ok(Self {
            key_id,
            iteration,
            ciphertext,
            signature,
            serialized,
        })
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self, SignalError> {
        if bytes.len() <= 1 + SIGNATURE_LENGTH {
            return Err(SignalError::InvalidMessageLength);
        }

        let version_byte = bytes[0];
        let version = version_byte >> 4;
        if version != SENDER_KEY_MESSAGE_VERSION {
            return Err(SignalError::UnsupportedVersion(version));
        }

        let message_bytes = &bytes[1..bytes.len() - SIGNATURE_LENGTH];
        let sig_bytes = &bytes[bytes.len() - SIGNATURE_LENGTH..];

        let mut signature = [0u8; SIGNATURE_LENGTH];
        signature.copy_from_slice(sig_bytes);

        let proto_msg = ProtoSenderKeyMessage::decode(message_bytes)
            .map_err(|e| SignalError::Deserialization(e.to_string()))?;

        let key_id = proto_msg.id.ok_or(SignalError::InvalidMessage)?;
        let iteration = proto_msg.iteration.ok_or(SignalError::InvalidMessage)?;
        let ciphertext = proto_msg.ciphertext.ok_or(SignalError::InvalidMessage)?;

        Ok(Self {
            key_id,
            iteration,
            ciphertext,
            signature,
            serialized: bytes.to_vec(),
        })
    }

    pub fn key_id(&self) -> u32 {
        self.key_id
    }

    pub fn iteration(&self) -> u32 {
        self.iteration
    }

    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }

    pub fn serialize(&self) -> &[u8] {
        &self.serialized
    }

    pub fn verify_signature(&self, public_key: &[u8]) -> Result<bool, SignalError> {
        if self.serialized.len() <= SIGNATURE_LENGTH {
            return Ok(false);
        }
        let signed_content = &self.serialized[..self.serialized.len() - SIGNATURE_LENGTH];
        verify_ed25519(public_key, signed_content, &self.signature)
            .map_err(|e| SignalError::Crypto(e.to_string()))
    }
}
