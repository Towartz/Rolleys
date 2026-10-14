use async_trait::async_trait;
use baileys_crypto::KeyPair;
use prost::Message as ProstMessage;
use std::sync::Arc;

use crate::group::group_cipher::GroupCipher;
use crate::group::sender_key_distribution_message::SenderKeyDistributionMessage;
use crate::group::sender_key_record::SenderKeyRecord;
use crate::lid::LidMappingStore;
use crate::session::session_cipher::SessionCipher;
use crate::session::session_record::SessionRecord;
use crate::SignalError;

/// Storage interface for Signal Protocol keys, sessions, and group sender keys.
#[async_trait]
pub trait SignalStore: Send + Sync {
    async fn get_session(&self, address: &str) -> Result<Option<Vec<u8>>, SignalError>;
    async fn set_session(&self, address: &str, session: &[u8]) -> Result<(), SignalError>;
    async fn get_sender_key(&self, key_id: &str) -> Result<Option<Vec<u8>>, SignalError>;
    async fn set_sender_key(&self, key_id: &str, sender_key: &[u8]) -> Result<(), SignalError>;
    async fn get_pre_key(&self, pre_key_id: u32) -> Result<Option<KeyPair>, SignalError>;
    async fn remove_pre_key(&self, pre_key_id: u32) -> Result<(), SignalError>;
}

/// High-level Signal Protocol repository managing session encryption, group keys, and LID translations.
pub struct SignalRepository {
    store: Arc<dyn SignalStore>,
    lid_mappings: Arc<LidMappingStore>,
}

impl SignalRepository {
    pub fn new(store: Arc<dyn SignalStore>) -> Self {
        Self {
            store,
            lid_mappings: Arc::new(LidMappingStore::new()),
        }
    }

    pub fn lid_mappings(&self) -> &Arc<LidMappingStore> {
        &self.lid_mappings
    }

    /// Normalizes a JID to a Signal protocol address (e.g. `6283162498175.0:37` or `6283162498175:0`).
    pub fn jid_to_signal_address(&self, jid: &str) -> String {
        let (user_part, _) = jid.split_once('@').unwrap_or((jid, ""));
        let (user, device) = if let Some((u, d)) = user_part.split_once(':') {
            (u, d)
        } else if let Some((u, d)) = user_part.split_once('.') {
            (u, d)
        } else {
            (user_part, "0")
        };
        format!("{}:{}", user, device)
    }

    pub async fn load_session(&self, address: &str) -> Result<SessionRecord, SignalError> {
        let sig_addr = self.jid_to_signal_address(address);
        match self.store.get_session(&sig_addr).await {
            Ok(Some(bytes)) => serde_json::from_slice(&bytes)
                .map_err(|e| SignalError::Deserialization(e.to_string())),
            Ok(None) => Ok(SessionRecord::new()),
            Err(e) => Err(e),
        }
    }

    pub async fn save_session(
        &self,
        address: &str,
        record: &SessionRecord,
    ) -> Result<(), SignalError> {
        let sig_addr = self.jid_to_signal_address(address);
        let bytes =
            serde_json::to_vec(record).map_err(|e| SignalError::Serialization(e.to_string()))?;
        self.store.set_session(&sig_addr, &bytes).await
    }

    pub async fn load_sender_key(
        &self,
        group: &str,
        author: &str,
    ) -> Result<SenderKeyRecord, SignalError> {
        let (user_part, _) = author.split_once('@').unwrap_or((author, ""));
        let (user, device) = if let Some((u, d)) = user_part.split_once(':') {
            (u, d)
        } else if let Some((u, d)) = user_part.split_once('.') {
            (u, d)
        } else {
            (user_part, "0")
        };
        let key_id1 = format!("{}::{}::{}", group, user, device);
        let key_id2 = format!("{}::{}:{}", group, user, device);

        if let Ok(Some(bytes)) = self.store.get_sender_key(&key_id1).await {
            if let Ok(rec) = serde_json::from_slice(&bytes) {
                return Ok(rec);
            }
        }
        if let Ok(Some(bytes)) = self.store.get_sender_key(&key_id2).await {
            if let Ok(rec) = serde_json::from_slice(&bytes) {
                return Ok(rec);
            }
        }
        Ok(SenderKeyRecord::new())
    }

    pub async fn save_sender_key(
        &self,
        group: &str,
        author: &str,
        record: &SenderKeyRecord,
    ) -> Result<(), SignalError> {
        let (user_part, _) = author.split_once('@').unwrap_or((author, ""));
        let (user, device) = if let Some((u, d)) = user_part.split_once(':') {
            (u, d)
        } else if let Some((u, d)) = user_part.split_once('.') {
            (u, d)
        } else {
            (user_part, "0")
        };
        let key_id = format!("{}::{}:{}", group, user, device);
        let bytes =
            serde_json::to_vec(record).map_err(|e| SignalError::Serialization(e.to_string()))?;
        self.store.set_sender_key(&key_id, &bytes).await
    }

    /// Decrypts an incoming 1-on-1 WhisperMessage (`msg`) or PreKeyWhisperMessage (`pkmsg`).
    pub async fn decrypt_message(
        &self,
        jid: &str,
        msg_type: &str,
        ciphertext: &[u8],
        our_identity: &KeyPair,
        our_signed_pre_key: &KeyPair,
    ) -> Result<Vec<u8>, SignalError> {
        let mut session_record = self.load_session(jid).await?;

        let (plaintext, consumed_prekey) = match msg_type {
            "pkmsg" => {
                let prekey_proto = if ciphertext.len() > 1 {
                    baileys_proto::proto::PreKeySignalMessage::decode(&ciphertext[1..]).ok()
                } else {
                    None
                };

                let base_key = prekey_proto.as_ref().and_then(|p| p.base_key.as_deref());
                let prekey_id = prekey_proto.as_ref().and_then(|p| p.pre_key_id);

                // If session is already established for this base_key, we do not require the one-time prekey
                let already_has_session = base_key
                    .map(|bk| session_record.get_session(bk).is_some())
                    .unwrap_or(false);

                let one_time_key = if !already_has_session {
                    if let Some(pk_id) = prekey_id {
                        match self.store.get_pre_key(pk_id).await {
                            Ok(Some(k)) => Some(k),
                            Ok(None) => return Err(SignalError::PreKeyNotFound(pk_id)),
                            Err(e) => return Err(e),
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                let res = SessionCipher::decrypt_pre_key_whisper_message(
                    &mut session_record,
                    ciphertext,
                    our_identity,
                    our_signed_pre_key,
                    one_time_key.as_ref(),
                )?;

                (res.0, res.1)
            }
            "msg" => {
                let p = SessionCipher::decrypt_whisper_message(
                    &mut session_record,
                    ciphertext,
                    our_identity,
                )?;
                (p, None)
            }
            _ => return Err(SignalError::InvalidMessage),
        };

        self.save_session(jid, &session_record).await?;

        if let Some(consumed_id) = consumed_prekey {
            let _ = self.store.remove_pre_key(consumed_id).await;
        }

        Ok(plaintext)
    }

    /// Decrypts an incoming group message using group SenderKeyRecord.
    pub async fn decrypt_group_message(
        &self,
        group_jid: &str,
        author_jid: &str,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, SignalError> {
        let mut record = self.load_sender_key(group_jid, author_jid).await?;
        let plaintext = GroupCipher::decrypt(&mut record, ciphertext)?;
        self.save_sender_key(group_jid, author_jid, &record).await?;
        Ok(plaintext)
    }

    /// Processes an incoming SenderKeyDistributionMessage from a group participant.
    pub async fn process_sender_key_distribution_message(
        &self,
        group_jid: &str,
        author_jid: &str,
        skdm_bytes: &[u8],
    ) -> Result<(), SignalError> {
        let skdm = SenderKeyDistributionMessage::deserialize(skdm_bytes)?;
        let mut record = self.load_sender_key(group_jid, author_jid).await?;
        GroupCipher::process_distribution_message(&mut record, &skdm)?;
        self.save_sender_key(group_jid, author_jid, &record).await?;
        Ok(())
    }

    /// Encrypts an outgoing 1-on-1 message.
    pub async fn encrypt_message(
        &self,
        jid: &str,
        plaintext: &[u8],
        our_identity: &KeyPair,
    ) -> Result<(String, Vec<u8>), SignalError> {
        let mut session_record = self.load_session(jid).await?;
        let (msg_type, ciphertext) =
            SessionCipher::encrypt_message(&mut session_record, plaintext, our_identity)?;
        self.save_session(jid, &session_record).await?;

        let type_str = if msg_type == 3 { "pkmsg" } else { "msg" };
        Ok((type_str.to_string(), ciphertext))
    }

    /// Encrypts an outgoing group message using our own sender key for this group.
    pub async fn encrypt_group_message(
        &self,
        group_jid: &str,
        our_jid: &str,
        plaintext: &[u8],
    ) -> Result<(Vec<u8>, Option<SenderKeyDistributionMessage>), SignalError> {
        let mut record = self
            .load_sender_key(group_jid, author_jid_or_our(our_jid))
            .await?;

        if record.is_empty()
            || record
                .get_sender_key_state()
                .and_then(|s| s.signing_key_private())
                .is_none()
        {
            let key_id = rand::random::<u32>();
            GroupCipher::create_session(&mut record, key_id)?;
        }

        let skdm = record.get_distribution_message();
        let ciphertext = GroupCipher::encrypt(&mut record, plaintext)?;
        self.save_sender_key(group_jid, our_jid, &record).await?;
        Ok((ciphertext, skdm))
    }

    /// Retrieves our active SenderKeyDistributionMessage for a group (or creates a new session if none exists).
    pub async fn get_sender_key_distribution_message(
        &self,
        group_jid: &str,
        our_jid: &str,
    ) -> Result<Option<SenderKeyDistributionMessage>, SignalError> {
        let mut record = self
            .load_sender_key(group_jid, author_jid_or_our(our_jid))
            .await?;
        if record.is_empty()
            || record
                .get_sender_key_state()
                .and_then(|s| s.signing_key_private())
                .is_none()
        {
            let key_id = rand::random::<u32>();
            let dist = GroupCipher::create_session(&mut record, key_id)?;
            self.save_sender_key(group_jid, our_jid, &record).await?;
            Ok(Some(dist))
        } else {
            Ok(record.get_distribution_message())
        }
    }
}

fn author_jid_or_our(our_jid: &str) -> &str {
    our_jid
}
