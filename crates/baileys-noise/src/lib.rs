//! WhatsApp Noise_XX_25519_AESGCM_SHA256 handshake and transport framing.

pub mod constants;
pub mod handshake;
pub mod transport;

pub use constants::*;
pub use handshake::*;
pub use transport::*;

use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum NoiseError {
    #[error("Noise cryptographic failure: {0}")]
    Crypto(String),

    #[error("WABinary decoding error: {0}")]
    Binary(String),

    #[error("Certificate validation failed: {0}")]
    CertificateInvalid(String),

    #[error("32-bit IV counter overflow")]
    CounterOverflow,

    #[error("Invalid transport frame")]
    InvalidFrame,
}

#[cfg(test)]
mod tests {
    use super::*;
    use baileys_binary::BinaryEncoder;
    use baileys_crypto::generate_key_pair;

    #[test]
    fn test_transport_encryption_counter_roundtrip() {
        let enc_key = [0xAAu8; 32];
        let dec_key = [0xBBu8; 32];

        // Party A encrypts with enc_key, Party B decrypts with enc_key
        let mut alice = TransportState::new(enc_key, dec_key);
        let mut bob = TransportState::new(dec_key, enc_key);

        for i in 0..5 {
            let msg = format!("Message payload chunk #{}", i);
            let ciphertext = alice.encrypt(msg.as_bytes()).expect("Encrypt failed");
            let decrypted = bob.decrypt(&ciphertext).expect("Decrypt failed");
            assert_eq!(decrypted, msg.as_bytes());
        }
    }

    #[test]
    fn test_noise_handler_frame_encoding() {
        let ephemeral = generate_key_pair();
        let mut handler = NoiseHandler::new(ephemeral, None);

        let node = baileys_binary::BinaryNode::new("iq")
            .with_attr("id", "1")
            .with_attr("type", "get");
        let encoded_node = BinaryEncoder::encode(&node).unwrap();

        let frame = handler
            .encode_frame(&encoded_node)
            .expect("Frame encoding failed");

        // First frame must begin with NOISE_WA_HEADER (4 bytes: "WA\x06\x03")
        assert_eq!(&frame[0..4], NOISE_WA_HEADER);

        // Next 3 bytes are length prefix
        let payload_len =
            ((frame[4] as usize) << 16) | ((frame[5] as usize) << 8) | (frame[6] as usize);
        assert_eq!(frame.len(), 4 + 3 + payload_len);
    }
}
