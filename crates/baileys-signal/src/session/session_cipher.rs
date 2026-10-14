use baileys_crypto::{
    aes_256_cbc_decrypt, aes_256_cbc_encrypt, calculate_agreement, format_signal_pubkey,
    generate_key_pair, hkdf_sha256, hmac_sha256, KeyPair,
};
use baileys_proto::proto::{PreKeySignalMessage, SignalMessage};
use prost::Message;

use crate::session::session_builder::SessionBuilder;
use crate::session::session_record::{
    ChainKey, ChainState, ChainType, SessionRecord, SessionState,
};
use crate::SignalError;

pub const SIGNAL_PROTOCOL_VERSION: u8 = 3;
pub const VERSION_BYTE: u8 = (SIGNAL_PROTOCOL_VERSION << 4) | SIGNAL_PROTOCOL_VERSION; // 0x33
pub const MAC_LENGTH: usize = 8;

pub struct SessionCipher;

impl SessionCipher {
    /// Decrypts a normal WhisperMessage (`msg`).
    pub fn decrypt_whisper_message(
        record: &mut SessionRecord,
        message_bytes: &[u8],
        our_identity: &KeyPair,
    ) -> Result<Vec<u8>, SignalError> {
        if message_bytes.len() <= 1 + MAC_LENGTH {
            return Err(SignalError::InvalidMessageLength);
        }

        let version = message_bytes[0] >> 4;
        if version != SIGNAL_PROTOCOL_VERSION {
            return Err(SignalError::UnsupportedVersion(version));
        }

        let mut errors = Vec::new();
        let sessions = record.get_sessions_mut();
        if sessions.is_empty() {
            return Err(SignalError::NoSession);
        }

        for session in sessions.iter_mut() {
            match Self::do_decrypt_whisper_message(session, message_bytes, our_identity) {
                Ok(plaintext) => {
                    session.index_info.used = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    return Ok(plaintext);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }

        Err(errors.into_iter().next().unwrap_or(SignalError::NoSession))
    }

    /// Decrypts a PreKeyWhisperMessage (`pkmsg`).
    /// Returns the decrypted bytes and the one-time pre_key_id consumed (if any).
    pub fn decrypt_pre_key_whisper_message(
        record: &mut SessionRecord,
        data: &[u8],
        our_identity: &KeyPair,
        our_signed_pre_key: &KeyPair,
        our_one_time_pre_key: Option<&KeyPair>,
    ) -> Result<(Vec<u8>, Option<u32>), SignalError> {
        if data.len() < 2 {
            return Err(SignalError::InvalidMessageLength);
        }

        let version = data[0] >> 4;
        if version != SIGNAL_PROTOCOL_VERSION {
            return Err(SignalError::UnsupportedVersion(version));
        }

        let prekey_proto = PreKeySignalMessage::decode(&data[1..])
            .map_err(|e| SignalError::Deserialization(e.to_string()))?;

        let consumed_prekey = SessionBuilder::init_incoming(
            record,
            &prekey_proto,
            our_identity,
            our_signed_pre_key,
            our_one_time_pre_key,
        )?;

        let base_key = prekey_proto
            .base_key
            .as_deref()
            .ok_or_else(|| SignalError::Deserialization("Missing baseKey".into()))?;

        let inner_msg = prekey_proto.message.as_deref().ok_or_else(|| {
            SignalError::Deserialization("Missing inner message in PreKeySignalMessage".into())
        })?;

        let session = record
            .get_session_mut(base_key)
            .ok_or(SignalError::NoSession)?;

        let plaintext = Self::do_decrypt_whisper_message(session, inner_msg, our_identity)?;
        Ok((plaintext, consumed_prekey))
    }

    fn do_decrypt_whisper_message(
        session: &mut SessionState,
        message_bytes: &[u8],
        our_identity: &KeyPair,
    ) -> Result<Vec<u8>, SignalError> {
        if message_bytes.len() <= 1 + MAC_LENGTH {
            return Err(SignalError::InvalidMessageLength);
        }

        let proto_len = message_bytes.len() - 1 - MAC_LENGTH;
        let message_proto = &message_bytes[1..1 + proto_len];
        let received_mac = &message_bytes[1 + proto_len..];

        let msg = SignalMessage::decode(message_proto)
            .map_err(|e| SignalError::Deserialization(e.to_string()))?;

        let remote_ratchet_key = msg
            .ratchet_key
            .as_deref()
            .ok_or_else(|| SignalError::Deserialization("Missing ratchetKey".into()))?;
        let counter = msg.counter.unwrap_or(0);
        let previous_counter = msg.previous_counter.unwrap_or(0);
        let ciphertext = msg
            .ciphertext
            .as_deref()
            .ok_or_else(|| SignalError::Deserialization("Missing ciphertext".into()))?;

        // 1. Step ratchet if remote ratchet key changed
        Self::maybe_step_ratchet(session, remote_ratchet_key, previous_counter)?;

        // 2. Fetch message key from receiving chain
        let remote_key_formatted = format_signal_pubkey(remote_ratchet_key);
        let chain = session
            .get_chain_mut(&remote_key_formatted)
            .ok_or(SignalError::NoSession)?;

        if chain.chain_type != ChainType::Receiving {
            return Err(SignalError::Crypto(
                "Tried to decrypt on a sending chain".into(),
            ));
        }

        Self::fill_message_keys(chain, counter)?;

        let message_key =
            chain
                .message_keys
                .remove(&counter)
                .ok_or(SignalError::DuplicateOrOldCounter(
                    chain.chain_key.counter as u32,
                    counter,
                ))?;

        // 3. Derive secrets: cipher_key (32), mac_key (32), iv (16)
        let keys = hkdf_sha256(
            &message_key,
            80,
            Some(&[0u8; 32]),
            Some(b"WhisperMessageKeys"),
        )
        .map_err(|e| SignalError::Crypto(e.to_string()))?;

        let cipher_key = &keys[0..32];
        let mac_key = &keys[32..64];
        let iv = &keys[64..80];

        // 4. Verify MAC
        let our_id_formatted = format_signal_pubkey(&our_identity.public_key);
        let their_id_formatted = format_signal_pubkey(&session.index_info.remote_identity_key);

        let mut mac_input = Vec::with_capacity(33 + 33 + 1 + message_proto.len());
        mac_input.extend_from_slice(&their_id_formatted);
        mac_input.extend_from_slice(&our_id_formatted);
        mac_input.push(VERSION_BYTE);
        mac_input.extend_from_slice(message_proto);

        let computed_mac =
            hmac_sha256(&mac_input, mac_key).map_err(|e| SignalError::Crypto(e.to_string()))?;

        if subtle::ConstantTimeEq::ct_eq(&computed_mac[..MAC_LENGTH], received_mac).unwrap_u8() != 1
        {
            return Err(SignalError::InvalidSignature);
        }

        // 5. Decrypt ciphertext
        let plaintext = aes_256_cbc_decrypt(ciphertext, cipher_key, iv)
            .map_err(|e| SignalError::Crypto(e.to_string()))?;

        session.pending_pre_key = None;
        Ok(plaintext)
    }

    /// Encrypts an outgoing message using an established SessionRecord.
    /// Returns `(message_type, ciphertext)` where type=3 for pkmsg and type=1 for msg.
    pub fn encrypt_message(
        record: &mut SessionRecord,
        plaintext: &[u8],
        our_identity: &KeyPair,
    ) -> Result<(u32, Vec<u8>), SignalError> {
        let session = record
            .get_open_session_mut()
            .ok_or(SignalError::NoSession)?;

        let ephemeral_pub =
            format_signal_pubkey(&session.current_ratchet.ephemeral_key_pair.public_key);
        let chain = session
            .get_chain_mut(&ephemeral_pub)
            .ok_or(SignalError::NoSession)?;

        if chain.chain_type != ChainType::Sending {
            return Err(SignalError::Crypto(
                "Tried to encrypt on a receiving chain".into(),
            ));
        }

        let counter = (chain.chain_key.counter + 1) as u32;
        Self::fill_message_keys(chain, counter)?;

        let message_key = chain
            .message_keys
            .remove(&counter)
            .ok_or_else(|| SignalError::Crypto("Failed to get message key".into()))?;

        let keys = hkdf_sha256(
            &message_key,
            80,
            Some(&[0u8; 32]),
            Some(b"WhisperMessageKeys"),
        )
        .map_err(|e| SignalError::Crypto(e.to_string()))?;

        let cipher_key = &keys[0..32];
        let mac_key = &keys[32..64];
        let iv = &keys[64..80];

        let ciphertext = aes_256_cbc_encrypt(plaintext, cipher_key, iv)
            .map_err(|e| SignalError::Crypto(e.to_string()))?;

        let whisper_msg = SignalMessage {
            ratchet_key: Some(ephemeral_pub.clone()),
            counter: Some(counter),
            previous_counter: Some(session.current_ratchet.previous_counter),
            ciphertext: Some(ciphertext),
        };

        let msg_bytes = whisper_msg.encode_to_vec();

        let our_id_formatted = format_signal_pubkey(&our_identity.public_key);
        let their_id_formatted = format_signal_pubkey(&session.index_info.remote_identity_key);

        let mut mac_input = Vec::with_capacity(33 + 33 + 1 + msg_bytes.len());
        mac_input.extend_from_slice(&our_id_formatted);
        mac_input.extend_from_slice(&their_id_formatted);
        mac_input.push(VERSION_BYTE);
        mac_input.extend_from_slice(&msg_bytes);

        let full_mac =
            hmac_sha256(&mac_input, mac_key).map_err(|e| SignalError::Crypto(e.to_string()))?;

        let mut result = Vec::with_capacity(1 + msg_bytes.len() + MAC_LENGTH);
        result.push(VERSION_BYTE);
        result.extend_from_slice(&msg_bytes);
        result.extend_from_slice(&full_mac[..MAC_LENGTH]);

        if let Some(ref pending) = session.pending_pre_key {
            let prekey_msg = PreKeySignalMessage {
                registration_id: Some(session.registration_id),
                pre_key_id: pending.pre_key_id,
                signed_pre_key_id: Some(pending.signed_key_id),
                base_key: Some(pending.base_key.clone()),
                identity_key: Some(our_id_formatted),
                message: Some(result),
                ..Default::default()
            };

            let mut out = Vec::with_capacity(1 + prekey_msg.encoded_len());
            out.push(VERSION_BYTE);
            prekey_msg
                .encode(&mut out)
                .map_err(|e| SignalError::Serialization(e.to_string()))?;

            Ok((3, out))
        } else {
            Ok((1, result))
        }
    }

    fn maybe_step_ratchet(
        session: &mut SessionState,
        remote_ratchet_key: &[u8],
        previous_counter: u32,
    ) -> Result<(), SignalError> {
        let formatted = format_signal_pubkey(remote_ratchet_key);
        if session.get_chain(&formatted).is_some() {
            return Ok(());
        }

        // Close previous receiving chain
        let last_remote = session.current_ratchet.last_remote_ephemeral_key.clone();
        if let Some(prev) = session.get_chain_mut(&last_remote) {
            Self::fill_message_keys(prev, previous_counter)?;
            prev.chain_key.key = None;
        }

        // 1. Calculate receiving ratchet with the new remote ephemeral key
        Self::calculate_ratchet(session, remote_ratchet_key, false)?;

        // 2. Advance our sending ratchet
        let our_current_pub =
            format_signal_pubkey(&session.current_ratchet.ephemeral_key_pair.public_key);
        if let Some(prev_send) = session.get_chain(&our_current_pub) {
            session.current_ratchet.previous_counter = prev_send.chain_key.counter as u32;
            session.delete_chain(&our_current_pub);
        }

        let new_key_pair = generate_key_pair();
        session.current_ratchet.ephemeral_key_pair = (&new_key_pair).into();
        Self::calculate_ratchet(session, remote_ratchet_key, true)?;

        session.current_ratchet.last_remote_ephemeral_key = formatted;
        Ok(())
    }

    fn calculate_ratchet(
        session: &mut SessionState,
        remote_key: &[u8],
        is_sending: bool,
    ) -> Result<(), SignalError> {
        let priv_key = session.current_ratchet.ephemeral_key_pair.private_key;
        let shared_secret = calculate_agreement(&priv_key, remote_key)
            .map_err(|e| SignalError::Crypto(e.to_string()))?;

        let master_key = hkdf_sha256(
            &shared_secret,
            64,
            Some(&session.current_ratchet.root_key),
            Some(b"WhisperRatchet"),
        )
        .map_err(|e| SignalError::Crypto(e.to_string()))?;

        session
            .current_ratchet
            .root_key
            .copy_from_slice(&master_key[0..32]);

        let chain_key_bytes = master_key[32..64].to_vec();
        let chain_key_id = if is_sending {
            format_signal_pubkey(&session.current_ratchet.ephemeral_key_pair.public_key)
        } else {
            format_signal_pubkey(remote_key)
        };

        session.add_chain(
            &chain_key_id,
            ChainState {
                chain_type: if is_sending {
                    ChainType::Sending
                } else {
                    ChainType::Receiving
                },
                chain_key: ChainKey {
                    counter: -1,
                    key: Some(chain_key_bytes),
                },
                message_keys: std::collections::HashMap::new(),
            },
        );

        Ok(())
    }

    fn fill_message_keys(chain: &mut ChainState, target_counter: u32) -> Result<(), SignalError> {
        let current = chain.chain_key.counter;
        if current >= target_counter as i32 {
            return Ok(());
        }

        if target_counter as i32 - current > 2000 {
            return Err(SignalError::CounterTooFarAhead);
        }

        let key = chain
            .chain_key
            .key
            .as_ref()
            .ok_or_else(|| SignalError::Crypto("Chain key is closed".into()))?;

        let mut curr_key = key.clone();
        for c in (current + 1)..=target_counter as i32 {
            let msg_key =
                hmac_sha256(&[1u8], &curr_key).map_err(|e| SignalError::Crypto(e.to_string()))?;
            let next_key =
                hmac_sha256(&[2u8], &curr_key).map_err(|e| SignalError::Crypto(e.to_string()))?;

            chain.message_keys.insert(c as u32, msg_key.to_vec());
            curr_key = next_key.to_vec();
        }

        chain.chain_key.key = Some(curr_key);
        chain.chain_key.counter = target_counter as i32;
        Ok(())
    }
}
