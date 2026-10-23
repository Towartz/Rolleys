use crate::community::{CommunityMetadata, CommunitySubgroup};
use crate::newsletter::{NewsletterMessage, NewsletterMetadata};
use crate::polls::{InteractiveButton, ListSection};
use baileys_binary::jid::S_WHATSAPP_NET;
use baileys_binary::BinaryNode;
use baileys_crypto::{pad_random_max16, unpad_random_max16, KeyPair};
use baileys_proto::proto::message::{
    AudioMessage, DocumentMessage, ExtendedTextMessage, StickerMessage, VideoMessage,
};
use baileys_proto::proto::{ContextInfo, Message, MessageKey, WebMessageInfo};
use baileys_signal::{SessionRecord, SignalRepository};
use baileys_store::{
    AuthCreds, BaileysStore, PollTally, SignalStoreAdapter, StoredCall, StoredChat,
    StoredCommunitySubgroup, StoredContact, StoredLabel, StoredMessage, StoredNewsletter,
    StoredOrder, StoredPoll,
};
use base64::prelude::*;
use prost::Message as ProstMessage;
use rand::RngCore;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::companion::CompanionPairingState;
use crate::config::BaileysConfig;
use crate::events::BaileysEvent;
use crate::registration::{
    build_ack_node, build_passive_iq, build_retry_receipt, configure_successful_pairing,
    create_retry_keys_node, decode_big_endian, upload_pre_keys,
};
use crate::socket::WhatsAppSocket;
use crate::CoreError;

use crate::inner::ClientInner;

#[derive(Clone)]
pub struct BaileysClient {
    pub(crate) inner: Arc<ClientInner>,
}

impl std::ops::Deref for BaileysClient {
    type Target = ClientInner;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// Options for relaying / sending messages (matching Baileys `MessageRelayOptions`).
#[derive(Debug, Clone, Default)]
pub struct MessageOptions {
    /// Audience participant JIDs for `status@broadcast` stories.
    pub status_jid_list: Option<Vec<String>>,
    /// Media handle returned by unencrypted newsletter media upload
    pub media_handle: Option<String>,
    /// Additional stanza child XML nodes to append to the outbound `<message>`.
    pub additional_nodes: Option<Vec<BinaryNode>>,
}

impl MessageOptions {
    pub fn with_additional_nodes(mut self, nodes: Vec<BinaryNode>) -> Self {
        self.additional_nodes = Some(nodes);
        self
    }

    pub fn with_additional_node(mut self, node: BinaryNode) -> Self {
        match &mut self.additional_nodes {
            Some(ref mut list) => list.push(node),
            None => self.additional_nodes = Some(vec![node]),
        }
        self
    }
}

/// Summary metrics of an AppState synchronization operation.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AppStateSyncSummary {
    pub collections_synced: usize,
    pub mutations_applied: usize,
    pub collections_failed: usize,
}

pub mod business;
pub mod channels;
pub mod chat_actions;
pub mod community;
pub mod companions;
pub mod connection;
pub mod groups;
pub mod media;
pub mod messaging;
pub mod privacy;
pub mod status;

impl BaileysClient {
    pub fn new(store: Arc<dyn BaileysStore>, config: BaileysConfig) -> Self {
        let signal_adapter = Arc::new(SignalStoreAdapter::new(Arc::clone(&store)));
        let signal = Arc::new(SignalRepository::new(signal_adapter));
        let inner = Arc::new(ClientInner::new(store, config, signal));

        Self { inner }
    }

    pub fn inner(&self) -> &Arc<ClientInner> {
        &self.inner
    }

    pub fn events(&self) -> broadcast::Receiver<BaileysEvent> {
        self.events.subscribe()
    }

    pub fn is_connected(&self) -> bool {
        self.is_connected.load(Ordering::SeqCst)
    }

    /// Returns the current formal connection state of the client.
    pub fn get_connection_state(&self) -> crate::state::ConnectionState {
        self.inner.session.get_connection_state()
    }

    /// Subscribes to reactive connection state transitions (FSM).
    pub fn subscribe_connection_state(
        &self,
    ) -> tokio::sync::watch::Receiver<crate::state::ConnectionState> {
        self.inner.session.subscribe_connection_state()
    }

    /// Accessor for real-time engine telemetry counters.
    pub fn metrics(&self) -> &crate::telemetry::TelemetryMetrics {
        &self.inner.metrics
    }

    /// Captures a point-in-time snapshot of engine telemetry metrics.
    pub fn telemetry_snapshot(&self) -> crate::telemetry::TelemetrySnapshot {
        self.inner.metrics.snapshot()
    }

    /// Returns true if Ghost Mode is active (suppressing read receipts and status view receipts).
    pub fn is_ghost_mode(&self) -> bool {
        self.ghost_mode.load(Ordering::Relaxed)
    }

    /// Dynamically toggles Ghost Mode on or off.
    pub fn set_ghost_mode(&self, enabled: bool) {
        self.ghost_mode.store(enabled, Ordering::Relaxed);
    }

    pub async fn get_last_qr(&self) -> Option<String> {
        let lock = self.last_qr.lock().await;
        lock.clone()
    }

    /// Accessor for WhatsApp Channel / Newsletter domain operations.
    pub fn newsletters(&self) -> crate::services::NewsletterService {
        crate::services::NewsletterService::new(self.clone())
    }

    /// Accessor for WhatsApp Messaging operations (text, media, polls, reactions).
    pub fn messages(&self) -> crate::services::MessageService {
        crate::services::MessageService::new(self.clone())
    }

    /// Accessor for WhatsApp Group administration and participation.
    pub fn groups(&self) -> crate::services::GroupService {
        crate::services::GroupService::new(self.clone())
    }

    /// Accessor for WhatsApp Status Stories and ephemeral updates.
    pub fn status(&self) -> crate::services::StatusService {
        crate::services::StatusService::new(self.clone())
    }

    /// Accessor for WhatsApp Privacy, Safety, and Blocklist operations.
    pub fn privacy(&self) -> crate::services::PrivacyService {
        crate::services::PrivacyService::new(self.clone())
    }

    /// Accessor for WhatsApp Business Commerce, Product Catalogs, and Orders.
    pub fn commerce(&self) -> crate::services::CommerceService {
        crate::services::CommerceService::new(self.clone())
    }

    /// Accessor for Multi-Device Companion Pairing & QR code authentication.
    pub fn companion(&self) -> crate::services::CompanionService {
        crate::services::CompanionService::new(self.clone())
    }

    /// Sends an IQ stanza and awaits the matching result/error response.
    pub async fn send_iq(
        &self,
        node: BinaryNode,
        timeout: Duration,
    ) -> Result<BinaryNode, CoreError> {
        self.inner.send_iq(node, timeout).await
    }

    /// Sends an XML stanza node directly over the active WebSocket connection.
    pub async fn send_node(&self, node: BinaryNode) -> Result<(), CoreError> {
        self.inner.send_node(node).await
    }

    /// Pings the WhatsApp Web Gateway and returns the round-trip latency.
    pub async fn ping(&self) -> Result<Duration, CoreError> {
        let ping_id = format!("3EB0{:X}", rand::random::<u64>());
        let ping_node = BinaryNode::new("iq")
            .with_attr("to", S_WHATSAPP_NET)
            .with_attr("xmlns", "w:p")
            .with_attr("type", "get")
            .with_attr("id", &ping_id)
            .with_child(BinaryNode::new("ping"));

        let start = std::time::Instant::now();
        self.send_iq(ping_node, Duration::from_secs(8)).await?;
        let elapsed = start.elapsed();
        self.metrics().record_ping_latency(elapsed);
        Ok(elapsed)
    }

    /// Returns the currently authenticated JID (if connected/paired).
    pub async fn me_jid(&self) -> Result<Option<String>, CoreError> {
        let creds = self
            .store
            .get_creds()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;
        Ok(creds.and_then(|c| c.me_jid))
    }

    /// Returns the currently authenticated LID (if available).
    pub async fn me_lid(&self) -> Result<Option<String>, CoreError> {
        let creds = self
            .store
            .get_creds()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;
        Ok(creds.and_then(|c| c.me_lid))
    }

    /// Disconnects from WhatsApp Web and cleans up active socket tasks.
    pub async fn disconnect(&self) {
        self.is_running.store(false, Ordering::SeqCst);
        self.set_connection_state(crate::state::ConnectionState::Disconnected);
        let socket_opt = {
            let mut lock = self.socket.lock().await;
            lock.take()
        };
        if let Some(s) = socket_opt {
            s.close().await;
        }
        self.events.emit(BaileysEvent::Disconnected {
            reason: "User requested disconnect".to_string(),
        });
    }

    /// Logs out of WhatsApp, sends companion unregistration to server, and resets local session credentials.
    pub async fn logout(&self) -> Result<(), CoreError> {
        let creds = self
            .store
            .get_creds()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;
        if let Some(c) = creds {
            if let Some(ref me) = c.me_jid {
                let socket_opt = {
                    let lock = self.socket.lock().await;
                    lock.clone()
                };
                if let Some(s) = socket_opt {
                    let unreg = BinaryNode::new("iq")
                        .with_attr("to", S_WHATSAPP_NET)
                        .with_attr("type", "set")
                        .with_attr("id", format!("3EB0{:X}", rand::random::<u64>()))
                        .with_attr("xmlns", "md")
                        .with_child(
                            BinaryNode::new("remove-companion-device")
                                .with_attr("jid", me)
                                .with_attr("reason", "user_initiated"),
                        );
                    let _ = s.send_node(unreg).await;
                }
            }
        }
        self.disconnect().await;
        let fresh = AuthCreds::init();
        let _ = self.store.save_creds(&fresh).await;
        // Sessions, pre-keys and app-state versions belong to the identity just given up.
        crate::registration::clear_identity_bound_keys(self.store.as_ref()).await?;
        Ok(())
    }

    /// Ensures that a Signal Double Ratchet session exists for the given 1-on-1 JID.
    /// If not present, queries WhatsApp server for the recipient's prekey bundle.
    pub async fn assert_session(&self, to_jid: &str, creds: &AuthCreds) -> Result<(), CoreError> {
        let sig_addr = self.signal.jid_to_signal_address(to_jid);
        let session = self
            .signal
            .load_session(&sig_addr)
            .await
            .map_err(|e| CoreError::Crypto(e.to_string()))?;

        if !session.is_empty() {
            return Ok(());
        }

        if self.config.wa_websocket_url.starts_with("mock://") {
            let our_identity = KeyPair::try_from(&creds.signed_identity_key)
                .map_err(|e| CoreError::Crypto(e.to_string()))?;
            let mock_bob_identity = baileys_crypto::generate_key_pair();
            let mock_bob_spk = baileys_crypto::generate_key_pair();
            let mut mock_session = SessionRecord::new();
            baileys_signal::SessionBuilder::init_outgoing(
                &mut mock_session,
                &our_identity,
                &mock_bob_identity.public_key,
                &mock_bob_spk.public_key,
                1,
                None,
                1234,
            )
            .map_err(|e| CoreError::Crypto(e.to_string()))?;
            self.signal
                .save_session(&sig_addr, &mock_session)
                .await
                .map_err(|e| CoreError::Crypto(e.to_string()))?;
            return Ok(());
        }

        debug!(
            "No existing Signal session for '{}', requesting PreKey bundle from server...",
            to_jid
        );
        let iq_id = format!("{}.{}-1", rand::random::<u16>(), rand::random::<u16>());

        // Baileys parity: query with the exact device JID (user[:device]@server),
        // preserving @lid. Stripping the device returned device-0's bundle for
        // companions; forcing @s.whatsapp.net on LIDs caused 400 bad-request.
        let query_jid = to_jid.to_string();

        let query_iq = BinaryNode::new("iq")
            .with_attr("to", S_WHATSAPP_NET)
            .with_attr("xmlns", "encrypt")
            .with_attr("type", "get")
            .with_attr("id", &iq_id)
            .with_child(
                BinaryNode::new("key")
                    .with_child(BinaryNode::new("user").with_attr("jid", query_jid)),
            );

        let resp = match self.send_iq(query_iq, Duration::from_secs(10)).await {
            Ok(resp) => resp,
            Err(e) => {
                // 406: the server knows no such device (unlinked since the list was cached).
                // Drop it so the next send does not ask again.
                if e.to_string().contains("code 406") {
                    self.forget_device(to_jid).await;
                }
                return Err(e);
            }
        };

        let list_node = resp
            .get_child("list")
            .ok_or_else(|| CoreError::Protocol("Missing <list> node in PreKey response".into()))?;

        let user_node = list_node
            .get_child("user")
            .ok_or_else(|| CoreError::Protocol("Missing <user> node in PreKey response".into()))?;

        let reg_id_bytes = user_node
            .get_child("registration")
            .and_then(|r| r.content_as_bytes())
            .ok_or_else(|| {
                CoreError::Protocol("Missing registration id in PreKey bundle".into())
            })?;
        let registration_id = decode_big_endian(reg_id_bytes) as u32;

        let identity_bytes = user_node
            .get_child("identity")
            .and_then(|i| i.content_as_bytes())
            .ok_or_else(|| {
                CoreError::Protocol("Missing identity pubkey in PreKey bundle".into())
            })?;

        let skey_node = user_node
            .get_child("skey")
            .ok_or_else(|| CoreError::Protocol("Missing skey node in PreKey bundle".into()))?;

        let skey_id_bytes = skey_node
            .get_child("id")
            .and_then(|i| i.content_as_bytes())
            .ok_or_else(|| CoreError::Protocol("Missing skey id in PreKey bundle".into()))?;
        let skey_id = decode_big_endian(skey_id_bytes) as u32;

        let skey_val = skey_node
            .get_child("value")
            .and_then(|v| v.content_as_bytes())
            .ok_or_else(|| CoreError::Protocol("Missing skey value in PreKey bundle".into()))?;

        let onetime_key = if let Some(key_node) = user_node.get_child("key") {
            let k_id = key_node
                .get_child("id")
                .and_then(|i| i.content_as_bytes())
                .map(|b| decode_big_endian(b) as u32);
            let k_val = key_node
                .get_child("value")
                .and_then(|v| v.content_as_bytes());
            match (k_val, k_id) {
                (Some(val), Some(id)) => Some((val, id)),
                _ => None,
            }
        } else {
            None
        };

        let our_identity = KeyPair::try_from(&creds.signed_identity_key)
            .map_err(|e| CoreError::Crypto(e.to_string()))?;

        let mut new_session = SessionRecord::new();
        baileys_signal::SessionBuilder::init_outgoing(
            &mut new_session,
            &our_identity,
            identity_bytes,
            skey_val,
            skey_id,
            onetime_key,
            registration_id,
        )
        .map_err(|e| CoreError::Crypto(e.to_string()))?;

        self.signal
            .save_session(&sig_addr, &new_session)
            .await
            .map_err(|e| CoreError::Crypto(e.to_string()))?;

        debug!(
            "Successfully established new Signal session with '{}'!",
            to_jid
        );
        Ok(())
    }

    /// Queries the local SQLite/persistent store for all saved conversations ordered by last message time.
    pub async fn get_chats(&self) -> Result<Vec<StoredChat>, CoreError> {
        self.store
            .get_chats()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }

    /// Queries historical messages for a given chat from SQLite store.
    pub async fn get_messages(
        &self,
        chat_jid: &str,
        limit: u32,
        before_timestamp: Option<u64>,
    ) -> Result<Vec<StoredMessage>, CoreError> {
        self.store
            .get_messages(chat_jid, limit, before_timestamp)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }

    /// Queries contact information (push name, LID) from SQLite store.
    pub async fn get_contact(&self, jid: &str) -> Result<Option<StoredContact>, CoreError> {
        self.store
            .get_contact(jid)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }

    /// Generates participant hash v2 (phash) for multi-device participant lists, matching WhatsApp Web & whatsmeow.
    pub(crate) fn generate_participant_hash_v2(participants: &[String]) -> String {
        let mut ad_strings: Vec<String> = participants
            .iter()
            .map(|p| {
                let (user_part, server) = p.split_once('@').unwrap_or((p.as_str(), ""));
                let (user, device) = if let Some((u, d)) = user_part.split_once(':') {
                    (u, d)
                } else {
                    (user_part, "0")
                };
                let (clean_user, agent) = if let Some((u, a)) = user.split_once('.') {
                    (u, a)
                } else {
                    (user, "0")
                };
                if server.is_empty() {
                    p.clone()
                } else {
                    format!("{}.{}:{}@{}", clean_user, agent, device, server)
                }
            })
            .collect();
        ad_strings.sort();
        let joined = ad_strings.concat();
        let hash = baileys_crypto::sha256(joined.as_bytes());
        let b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(&hash[..6]);
        format!("2:{}", b64)
    }
}

/// Returns the user portion of a JID (`628123:12@s.whatsapp.net` -> `628123`).
pub(crate) fn jid_user_part(jid: &str) -> &str {
    let user = jid.split('@').next().unwrap_or(jid);
    user.split(':').next().unwrap_or(user)
}

/// Strips the device suffix from a JID (`628123:12@s.whatsapp.net` -> `628123@s.whatsapp.net`).
pub(crate) fn normalize_user_jid(jid: &str) -> String {
    match jid.split_once('@') {
        Some((user, server)) => format!("{}@{}", user.split(':').next().unwrap_or(user), server),
        None => jid.to_string(),
    }
}

#[cfg(test)]
mod logout_tests {
    use super::*;

    #[tokio::test]
    async fn logout_resets_credentials_and_clears_identity_bound_keys() {
        use baileys_store::{KeyEntry, MemoryStore, SessionStore, SignalKeysStore};
        let store = std::sync::Arc::new(MemoryStore::new());
        let entry = |t: &str, id: &str| KeyEntry {
            key_type: t.into(),
            id: id.into(),
            data: vec![1],
        };
        store
            .set_keys(vec![
                entry("session", "628111:5@s.whatsapp.net"),
                entry("pre-key", "1"),
                entry("app-state-sync-version", "regular_low"),
                entry("lid-mapping", "628111@s.whatsapp.net"),
            ])
            .await
            .unwrap();
        let mut creds = AuthCreds::init();
        creds.me_jid = Some("628111:5@s.whatsapp.net".into());
        store.save_creds(&creds).await.unwrap();

        let config = crate::BaileysConfigBuilder::new()
            .with_websocket_url("mock://logout")
            .build();
        let client = BaileysClient::new(store.clone(), config);
        client.logout().await.unwrap();

        assert_eq!(
            store.get_creds().await.unwrap().unwrap().me_jid,
            None,
            "credentials reset"
        );
        for gone in ["session", "pre-key", "app-state-sync-version"] {
            assert_eq!(store.get_key_count(gone).await.unwrap(), 0, "{gone}");
        }
        assert_eq!(
            store.get_key_count("lid-mapping").await.unwrap(),
            1,
            "contact facts stay"
        );
    }
}
