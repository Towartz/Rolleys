use baileys_crypto::KeyPair;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChainType {
    Sending,
    Receiving,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainKey {
    pub counter: i32,
    pub key: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainState {
    pub chain_type: ChainType,
    pub chain_key: ChainKey,
    pub message_keys: HashMap<u32, Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableRatchetKeyPair {
    pub public_key: [u8; 32],
    pub private_key: [u8; 32],
}

impl From<&KeyPair> for SerializableRatchetKeyPair {
    fn from(kp: &KeyPair) -> Self {
        Self {
            public_key: kp.public_key,
            private_key: kp.private_key,
        }
    }
}

impl From<&SerializableRatchetKeyPair> for KeyPair {
    fn from(skp: &SerializableRatchetKeyPair) -> Self {
        Self {
            public_key: skp.public_key,
            private_key: skp.private_key,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatchetState {
    pub root_key: [u8; 32],
    pub ephemeral_key_pair: SerializableRatchetKeyPair,
    pub last_remote_ephemeral_key: Vec<u8>,
    pub previous_counter: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPreKey {
    pub signed_key_id: u32,
    pub base_key: Vec<u8>,
    pub pre_key_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionIndexInfo {
    pub created: u64,
    pub used: u64,
    pub remote_identity_key: Vec<u8>,
    pub base_key: Vec<u8>,
    pub closed: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub registration_id: u32,
    pub current_ratchet: RatchetState,
    pub index_info: SessionIndexInfo,
    pub chains: HashMap<String, ChainState>,
    pub pending_pre_key: Option<PendingPreKey>,
}

impl SessionState {
    pub fn get_chain(&self, ephemeral_key: &[u8]) -> Option<&ChainState> {
        let key_hex = hex::encode(ephemeral_key);
        self.chains.get(&key_hex)
    }

    pub fn get_chain_mut(&mut self, ephemeral_key: &[u8]) -> Option<&mut ChainState> {
        let key_hex = hex::encode(ephemeral_key);
        self.chains.get_mut(&key_hex)
    }

    pub fn add_chain(&mut self, ephemeral_key: &[u8], chain: ChainState) {
        let key_hex = hex::encode(ephemeral_key);
        self.chains.insert(key_hex, chain);
    }

    pub fn delete_chain(&mut self, ephemeral_key: &[u8]) {
        let key_hex = hex::encode(ephemeral_key);
        self.chains.remove(&key_hex);
    }
}

/// A record maintaining active and past Signal sessions with a specific peer address.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionRecord {
    sessions: Vec<SessionState>,
}

impl SessionRecord {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    pub fn get_open_session(&self) -> Option<&SessionState> {
        self.sessions.iter().find(|s| s.index_info.closed == -1)
    }

    pub fn get_open_session_mut(&mut self) -> Option<&mut SessionState> {
        self.sessions.iter_mut().find(|s| s.index_info.closed == -1)
    }

    pub fn get_session(&self, base_key: &[u8]) -> Option<&SessionState> {
        self.sessions
            .iter()
            .find(|s| s.index_info.base_key == base_key)
    }

    pub fn get_session_mut(&mut self, base_key: &[u8]) -> Option<&mut SessionState> {
        self.sessions
            .iter_mut()
            .find(|s| s.index_info.base_key == base_key)
    }

    pub fn get_sessions(&self) -> &[SessionState] {
        &self.sessions
    }

    pub fn get_sessions_mut(&mut self) -> &mut [SessionState] {
        &mut self.sessions
    }

    pub fn close_session(&mut self, session_idx: usize) {
        if let Some(s) = self.sessions.get_mut(session_idx) {
            s.index_info.closed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
        }
    }

    pub fn set_session(&mut self, session: SessionState) {
        // Close any currently open session
        for s in &mut self.sessions {
            if s.index_info.closed == -1 {
                s.index_info.closed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64;
            }
        }
        self.sessions.insert(0, session);
        // Keep at most 40 sessions
        if self.sessions.len() > 40 {
            self.sessions.truncate(40);
        }
    }
}
