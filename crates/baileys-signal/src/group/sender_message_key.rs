use crate::SignalError;
use baileys_crypto::hkdf_sha256;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SenderMessageKey {
    iteration: u32,
    iv: [u8; 16],
    cipher_key: [u8; 32],
    seed: Vec<u8>,
}

impl SenderMessageKey {
    pub fn new(iteration: u32, seed: &[u8]) -> Result<Self, SignalError> {
        let salt = [0u8; 32];
        let derived = hkdf_sha256(seed, 48, Some(&salt), Some(b"WhisperGroup"))
            .map_err(|e| SignalError::Crypto(e.to_string()))?;

        let mut iv = [0u8; 16];
        let mut cipher_key = [0u8; 32];

        iv.copy_from_slice(&derived[0..16]);
        cipher_key.copy_from_slice(&derived[16..48]);

        Ok(Self {
            iteration,
            iv,
            cipher_key,
            seed: seed.to_vec(),
        })
    }

    pub fn iteration(&self) -> u32 {
        self.iteration
    }

    pub fn iv(&self) -> &[u8; 16] {
        &self.iv
    }

    pub fn cipher_key(&self) -> &[u8; 32] {
        &self.cipher_key
    }

    pub fn seed(&self) -> &[u8] {
        &self.seed
    }
}
