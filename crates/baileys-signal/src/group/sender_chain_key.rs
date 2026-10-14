use crate::group::sender_message_key::SenderMessageKey;
use crate::SignalError;
use baileys_crypto::hmac_sha256;

use serde::{Deserialize, Serialize};

const MESSAGE_KEY_SEED: &[u8] = &[0x01];
const CHAIN_KEY_SEED: &[u8] = &[0x02];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SenderChainKey {
    iteration: u32,
    chain_key: Vec<u8>,
}

impl SenderChainKey {
    pub fn new(iteration: u32, chain_key: Vec<u8>) -> Self {
        Self {
            iteration,
            chain_key,
        }
    }

    pub fn iteration(&self) -> u32 {
        self.iteration
    }

    pub fn chain_key(&self) -> &[u8] {
        &self.chain_key
    }

    pub fn get_sender_message_key(&self) -> Result<SenderMessageKey, SignalError> {
        let seed = hmac_sha256(MESSAGE_KEY_SEED, &self.chain_key)
            .map_err(|e| SignalError::Crypto(e.to_string()))?;
        SenderMessageKey::new(self.iteration, &seed)
    }

    pub fn get_next(&self) -> Result<SenderChainKey, SignalError> {
        let next_key = hmac_sha256(CHAIN_KEY_SEED, &self.chain_key)
            .map_err(|e| SignalError::Crypto(e.to_string()))?;
        Ok(SenderChainKey::new(self.iteration + 1, next_key.to_vec()))
    }
}
