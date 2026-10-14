use crate::group::sender_key_distribution_message::SenderKeyDistributionMessage;
use crate::group::sender_key_state::SenderKeyState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SenderKeyRecord {
    sender_key_states: Vec<SenderKeyState>,
}

impl SenderKeyRecord {
    pub fn new() -> Self {
        Self {
            sender_key_states: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.sender_key_states.is_empty()
    }

    pub fn get_sender_key_state(&self) -> Option<&SenderKeyState> {
        self.sender_key_states.last()
    }

    pub fn get_distribution_message(&self) -> Option<SenderKeyDistributionMessage> {
        let state = self.get_sender_key_state()?;
        SenderKeyDistributionMessage::new(
            state.key_id(),
            state.sender_chain_key().iteration(),
            state.sender_chain_key().chain_key().to_vec(),
            baileys_crypto::format_signal_pubkey(state.signing_key_public()),
        )
        .ok()
    }

    pub fn get_sender_key_state_mut(&mut self) -> Option<&mut SenderKeyState> {
        self.sender_key_states.last_mut()
    }

    pub fn get_sender_key_state_by_id(&self, key_id: u32) -> Option<&SenderKeyState> {
        self.sender_key_states.iter().find(|s| s.key_id() == key_id)
    }

    pub fn get_sender_key_state_by_id_mut(&mut self, key_id: u32) -> Option<&mut SenderKeyState> {
        self.sender_key_states
            .iter_mut()
            .find(|s| s.key_id() == key_id)
    }

    pub fn set_sender_key_state(
        &mut self,
        key_id: u32,
        iteration: u32,
        chain_key: Vec<u8>,
        signing_key_public: [u8; 32],
        signing_key_private: Option<[u8; 32]>,
    ) {
        self.sender_key_states.clear();
        self.sender_key_states.push(SenderKeyState::new(
            key_id,
            iteration,
            chain_key,
            signing_key_public,
            signing_key_private,
        ));
    }

    pub fn add_sender_key_state(
        &mut self,
        key_id: u32,
        iteration: u32,
        chain_key: Vec<u8>,
        signing_key_public: [u8; 32],
    ) {
        self.sender_key_states.retain(|s| s.key_id() != key_id);
        self.sender_key_states.push(SenderKeyState::new(
            key_id,
            iteration,
            chain_key,
            signing_key_public,
            None,
        ));
    }
}
