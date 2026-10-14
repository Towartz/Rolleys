use baileys_crypto::{
    calculate_agreement, format_signal_pubkey, generate_key_pair, hkdf_sha256, KeyPair,
};
use baileys_proto::proto::PreKeySignalMessage;
use std::collections::HashMap;

use crate::session::session_record::{
    ChainKey, ChainState, ChainType, PendingPreKey, RatchetState, SerializableRatchetKeyPair,
    SessionIndexInfo, SessionRecord, SessionState,
};
use crate::SignalError;

pub struct SessionBuilder;

impl SessionBuilder {
    /// Initializes an incoming Signal session from a received PreKeySignalMessage (`pkmsg`).
    /// Returns the one-time pre_key_id consumed (if any).
    pub fn init_incoming(
        record: &mut SessionRecord,
        message: &PreKeySignalMessage,
        our_identity: &KeyPair,
        our_signed_pre_key: &KeyPair,
        our_one_time_pre_key: Option<&KeyPair>,
    ) -> Result<Option<u32>, SignalError> {
        let base_key = message.base_key.as_deref().ok_or_else(|| {
            SignalError::Deserialization("Missing baseKey in PreKeySignalMessage".into())
        })?;
        let identity_key = message.identity_key.as_deref().ok_or_else(|| {
            SignalError::Deserialization("Missing identityKey in PreKeySignalMessage".into())
        })?;
        let registration_id = message.registration_id.unwrap_or(0);

        if record.get_session(base_key).is_some() {
            // Already initialized for this base_key - do not consume prekey again
            return Ok(None);
        }

        if let Some(pk_id) = message.pre_key_id {
            if our_one_time_pre_key.is_none() {
                return Err(SignalError::PreKeyNotFound(pk_id));
            }
        }

        // X3DH key agreement
        let a1 = calculate_agreement(&our_identity.private_key, base_key)
            .map_err(|e| SignalError::Crypto(e.to_string()))?;
        let a2 = calculate_agreement(&our_signed_pre_key.private_key, identity_key)
            .map_err(|e| SignalError::Crypto(e.to_string()))?;
        let a3 = calculate_agreement(&our_signed_pre_key.private_key, base_key)
            .map_err(|e| SignalError::Crypto(e.to_string()))?;

        let mut shared_secret = Vec::with_capacity(32 * 5);
        shared_secret.extend_from_slice(&[0xFF; 32]);
        shared_secret.extend_from_slice(&a2);
        shared_secret.extend_from_slice(&a1);
        shared_secret.extend_from_slice(&a3);

        if let Some(one_time_key) = our_one_time_pre_key {
            let a4 = calculate_agreement(&one_time_key.private_key, base_key)
                .map_err(|e| SignalError::Crypto(e.to_string()))?;
            shared_secret.extend_from_slice(&a4);
        }

        let master_key = hkdf_sha256(&shared_secret, 64, Some(&[0u8; 32]), Some(b"WhisperText"))
            .map_err(|e| SignalError::Crypto(e.to_string()))?;

        let mut root_key = [0u8; 32];
        root_key.copy_from_slice(&master_key[0..32]);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let session = SessionState {
            registration_id,
            current_ratchet: RatchetState {
                root_key,
                ephemeral_key_pair: SerializableRatchetKeyPair::from(our_signed_pre_key),
                last_remote_ephemeral_key: base_key.to_vec(),
                previous_counter: 0,
            },
            index_info: SessionIndexInfo {
                created: now,
                used: now,
                remote_identity_key: identity_key.to_vec(),
                base_key: base_key.to_vec(),
                closed: -1,
            },
            chains: HashMap::new(),
            pending_pre_key: None,
        };

        record.set_session(session);
        Ok(message.pre_key_id)
    }

    /// Initializes an outgoing Signal session using peer's pre-key bundle.
    pub fn init_outgoing(
        record: &mut SessionRecord,
        our_identity: &KeyPair,
        their_identity_pub: &[u8],
        their_signed_pre_key_pub: &[u8],
        their_signed_pre_key_id: u32,
        their_one_time_pre_key: Option<(&[u8], u32)>,
        registration_id: u32,
    ) -> Result<PendingPreKey, SignalError> {
        let base_key = generate_key_pair();

        let a1 = calculate_agreement(&our_identity.private_key, their_signed_pre_key_pub)
            .map_err(|e| SignalError::Crypto(e.to_string()))?;
        let a2 = calculate_agreement(&base_key.private_key, their_identity_pub)
            .map_err(|e| SignalError::Crypto(e.to_string()))?;
        let a3 = calculate_agreement(&base_key.private_key, their_signed_pre_key_pub)
            .map_err(|e| SignalError::Crypto(e.to_string()))?;

        let mut shared_secret = Vec::with_capacity(32 * 5);
        shared_secret.extend_from_slice(&[0xFF; 32]);
        shared_secret.extend_from_slice(&a1);
        shared_secret.extend_from_slice(&a2);
        shared_secret.extend_from_slice(&a3);

        let mut pre_key_id = None;
        if let Some((one_time_pub, pk_id)) = their_one_time_pre_key {
            let a4 = calculate_agreement(&base_key.private_key, one_time_pub)
                .map_err(|e| SignalError::Crypto(e.to_string()))?;
            shared_secret.extend_from_slice(&a4);
            pre_key_id = Some(pk_id);
        }

        let master_key = hkdf_sha256(&shared_secret, 64, Some(&[0u8; 32]), Some(b"WhisperText"))
            .map_err(|e| SignalError::Crypto(e.to_string()))?;

        let mut root_key = [0u8; 32];
        root_key.copy_from_slice(&master_key[0..32]);

        let sending_ratchet_key = generate_key_pair();
        let dh_send =
            calculate_agreement(&sending_ratchet_key.private_key, their_signed_pre_key_pub)
                .map_err(|e| SignalError::Crypto(e.to_string()))?;
        let ratchet_key_material =
            hkdf_sha256(&dh_send, 64, Some(&root_key), Some(b"WhisperRatchet"))
                .map_err(|e| SignalError::Crypto(e.to_string()))?;

        let mut new_root_key = [0u8; 32];
        new_root_key.copy_from_slice(&ratchet_key_material[0..32]);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let mut chains = HashMap::new();
        let send_chain_pub = format_signal_pubkey(&sending_ratchet_key.public_key);
        chains.insert(
            hex::encode(&send_chain_pub),
            ChainState {
                chain_type: ChainType::Sending,
                chain_key: ChainKey {
                    counter: -1,
                    key: Some(ratchet_key_material[32..64].to_vec()),
                },
                message_keys: HashMap::new(),
            },
        );

        let pending = PendingPreKey {
            signed_key_id: their_signed_pre_key_id,
            base_key: format_signal_pubkey(&base_key.public_key),
            pre_key_id,
        };

        let session = SessionState {
            registration_id,
            current_ratchet: RatchetState {
                root_key: new_root_key,
                ephemeral_key_pair: SerializableRatchetKeyPair::from(&sending_ratchet_key),
                last_remote_ephemeral_key: format_signal_pubkey(their_signed_pre_key_pub),
                previous_counter: 0,
            },
            index_info: SessionIndexInfo {
                created: now,
                used: now,
                remote_identity_key: format_signal_pubkey(their_identity_pub),
                base_key: format_signal_pubkey(&base_key.public_key),
                closed: -1,
            },
            chains,
            pending_pre_key: Some(pending.clone()),
        };

        record.set_session(session);
        Ok(pending)
    }
}
