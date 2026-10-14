use crate::group::sender_chain_key::SenderChainKey;
use crate::group::sender_message_key::SenderMessageKey;
use serde::{Deserialize, Serialize};

const MAX_MESSAGE_KEYS: usize = 2000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderKeyState {
    key_id: u32,
    sender_chain_key: SenderChainKey,
    signing_key_public: [u8; 32],
    signing_key_private: Option<[u8; 32]>,
    message_keys: Vec<SenderMessageKey>,
    #[serde(default)]
    seed: Vec<u8>,
}

impl SenderKeyState {
    pub fn new(
        key_id: u32,
        iteration: u32,
        chain_key: Vec<u8>,
        signing_key_public: [u8; 32],
        signing_key_private: Option<[u8; 32]>,
    ) -> Self {
        let seed = chain_key.clone();
        Self {
            key_id,
            sender_chain_key: SenderChainKey::new(iteration, chain_key),
            signing_key_public,
            signing_key_private,
            message_keys: Vec::new(),
            seed,
        }
    }

    pub fn seed(&self) -> &[u8] {
        if !self.seed.is_empty() {
            &self.seed
        } else {
            self.sender_chain_key.chain_key()
        }
    }

    pub fn key_id(&self) -> u32 {
        self.key_id
    }

    pub fn sender_chain_key(&self) -> &SenderChainKey {
        &self.sender_chain_key
    }

    pub fn set_sender_chain_key(&mut self, chain_key: SenderChainKey) {
        self.sender_chain_key = chain_key;
    }

    pub fn signing_key_public(&self) -> &[u8; 32] {
        &self.signing_key_public
    }

    pub fn signing_key_private(&self) -> Option<&[u8; 32]> {
        self.signing_key_private.as_ref()
    }

    pub fn has_sender_message_key(&self, iteration: u32) -> bool {
        self.message_keys.iter().any(|k| k.iteration() == iteration)
    }

    pub fn add_sender_message_key(&mut self, key: SenderMessageKey) {
        if self.message_keys.len() >= MAX_MESSAGE_KEYS {
            self.message_keys.remove(0);
        }
        self.message_keys.push(key);
    }

    pub fn remove_sender_message_key(&mut self, iteration: u32) -> Option<SenderMessageKey> {
        if let Some(pos) = self
            .message_keys
            .iter()
            .position(|k| k.iteration() == iteration)
        {
            Some(self.message_keys.remove(pos))
        } else {
            None
        }
    }
}
