use crate::NoiseError;
use baileys_crypto::{aes_256_gcm_decrypt, aes_256_gcm_encrypt};

pub const IV_LENGTH: usize = 12;

/// Post-handshake encrypted transport session.
/// Maintains independent encryption and decryption keys with monotonic 32-bit IV counters.
#[derive(Debug)]
pub struct TransportState {
    enc_key: [u8; 32],
    dec_key: [u8; 32],
    read_counter: u32,
    write_counter: u32,
}

impl TransportState {
    pub fn new(enc_key: [u8; 32], dec_key: [u8; 32]) -> Self {
        Self {
            enc_key,
            dec_key,
            read_counter: 0,
            write_counter: 0,
        }
    }

    fn generate_iv(counter: u32) -> [u8; IV_LENGTH] {
        let mut iv = [0u8; IV_LENGTH];
        iv[8..12].copy_from_slice(&counter.to_be_bytes());
        iv
    }

    /// Encrypts plaintext using AES-256-GCM with the current write counter, then increments the counter.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, NoiseError> {
        let iv = Self::generate_iv(self.write_counter);
        self.write_counter = self
            .write_counter
            .checked_add(1)
            .ok_or(NoiseError::CounterOverflow)?;

        aes_256_gcm_encrypt(plaintext, &self.enc_key, &iv, b"")
            .map_err(|e| NoiseError::Crypto(e.to_string()))
    }

    /// Decrypts ciphertext using AES-256-GCM with the current read counter, then increments the counter.
    pub fn decrypt(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, NoiseError> {
        let iv = Self::generate_iv(self.read_counter);
        self.read_counter = self
            .read_counter
            .checked_add(1)
            .ok_or(NoiseError::CounterOverflow)?;

        aes_256_gcm_decrypt(ciphertext, &self.dec_key, &iv, b"")
            .map_err(|e| NoiseError::Crypto(e.to_string()))
    }
}
