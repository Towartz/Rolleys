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

    /// Connects to the WhatsApp Web gateway, managing initial pairing, automatic post-pairing
    /// reconnect, session authentication, and background keepalive.
    pub async fn connect(&self) -> Result<(), CoreError> {
        if self
            .is_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            debug!(
                "Connection supervisor is already running; skipping redundant supervisor spawn."
            );
            if self.is_connected.load(Ordering::SeqCst) {
                if let Ok(Some(creds)) = self.store.get_creds().await {
                    if let Some(ref me) = creds.me_jid {
                        self.events
                            .emit(BaileysEvent::Connected { me_jid: me.clone() });
                    }
                }
            } else {
                self.events.emit(BaileysEvent::Connecting);
            }
            return Ok(());
        }

        self.events.emit(BaileysEvent::Connecting);
        self.set_connection_state(crate::state::ConnectionState::Connecting);

        let creds = match self
            .store
            .get_creds()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?
        {
            Some(c) => c,
            None => {
                let fresh = AuthCreds::init();
                self.store
                    .save_creds(&fresh)
                    .await
                    .map_err(|e| CoreError::Store(e.to_string()))?;
                fresh
            }
        };

        // Support mock URL for offline unit test suites
        if self.config.wa_websocket_url.starts_with("mock://") {
            info!("Mock URL detected, running simulated offline handshake...");
            if creds.me_jid.is_none() {
                if let Some(ref _phone) = self.config.phone_number_pairing {
                    self.events.emit(BaileysEvent::PairingCode {
                        code: "1234-ABCD".into(),
                    });
                } else {
                    let qr = format!(
                        "1@MOCK_REF,{},{},{}",
                        BASE64_STANDARD.encode(&creds.noise_key.public),
                        BASE64_STANDARD.encode(&creds.signed_identity_key.public),
                        BASE64_STANDARD.encode(&creds.adv_secret_key),
                    );
                    self.events.emit(BaileysEvent::QrCode { code: qr });
                }
            } else if let Some(me) = &creds.me_jid {
                self.is_connected.store(true, Ordering::SeqCst);
                self.events
                    .emit(BaileysEvent::Connected { me_jid: me.clone() });
            }
            return Ok(());
        }

        let client = self.clone();

        // Spawn background connection supervisor
        tokio::spawn(async move {
            let store_clone = &client.store;
            let config_clone = client.config.clone();
            let events_clone = &client.events;
            let is_connected_clone = &client.is_connected;
            let is_running_clone = &client.is_running;
            let socket_holder = &client.socket;
            let signal_clone = &client.signal;
            let pending_iq_clone = &client.pending_iq;
            let recent_messages_clone = &client.recent_messages;
            let auto_download_clone = &client.auto_download;
            let active_calls_clone = &client.active_calls;
            let companion_state_holder = &client.companion_state;
            let last_qr_holder = &client.last_qr;

            let mut is_first_connection = true;
            let mut consecutive_connect_failures: u32 = 0;

            while is_running_clone.load(Ordering::SeqCst) {
                let mut creds = match store_clone.get_creds().await {
                    Ok(Some(c)) => c,
                    Ok(None) => {
                        let fresh = AuthCreds::init();
                        let _ = store_clone.save_creds(&fresh).await;
                        fresh
                    }
                    Err(e) => {
                        error!("Failed to read credentials from store: {}", e);
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        continue;
                    }
                };

                if !is_first_connection {
                    events_clone.emit(BaileysEvent::Connecting);
                    client.set_connection_state(crate::state::ConnectionState::Connecting);
                }

                // 1. Resolve dynamic WhatsApp Web version if enabled
                let mut active_config = config_clone.clone();
                if active_config.dynamic_version
                    && !active_config.wa_websocket_url.starts_with("mock://")
                {
                    let resolved_version =
                        crate::version::resolve_wa_version(store_clone.as_ref(), &active_config)
                            .await;
                    active_config.app_version = resolved_version;
                }

                client.set_connection_state(crate::state::ConnectionState::Handshaking);

                // Establish live WebSocket connection and complete Noise_XX handshake
                let (raw_socket, mut incoming_rx) =
                    match WhatsAppSocket::connect(&creds, &active_config).await {
                        Ok(s) => {
                            consecutive_connect_failures = 0;
                            s
                        }
                        Err(e) => {
                            consecutive_connect_failures += 1;
                            let backoff =
                                (3u64 * (1 << (consecutive_connect_failures - 1).min(3))).min(30);
                            let jitter = rand::random::<u64>() % 1000;
                            let delay = Duration::from_millis(backoff * 1000 + jitter);
                            client.set_connection_state(
                                crate::state::ConnectionState::Reconnecting {
                                    attempt: consecutive_connect_failures,
                                    next_retry_millis: delay.as_millis() as u64,
                                },
                            );
                            client.metrics().record_reconnect();
                            error!(
                            "Failed to connect to WhatsApp Web Gateway: {}. Retrying in {:.1}s...",
                            e,
                            delay.as_secs_f32()
                        );
                            if !is_running_clone.load(Ordering::SeqCst) {
                                break;
                            }
                            tokio::time::sleep(delay).await;
                            continue;
                        }
                    };

                let socket = Arc::new(raw_socket);
                {
                    let mut lock = socket_holder.lock().await;
                    *lock = Some(Arc::clone(&socket));
                }

                // 2. Stanza processing loop for the current connection
                let mut ping_interval = tokio::time::interval(Duration::from_secs(15));
                ping_interval.tick().await;

                let mut session_terminated = false;
                let mut me_lid: Option<String> = creds.me_lid.clone();
                let mut reconnect_delay = Duration::from_secs(3);
                let mut msg_retries: HashMap<String, u32> = HashMap::new();
                let mut last_ping_sent_at: Option<tokio::time::Instant> = None;
                let mut ticks_since_prekey_check: u32 = 0;
                let mut pairing_code_sent = false;

                while is_running_clone.load(Ordering::SeqCst) && !session_terminated {
                    tokio::select! {
                        _ = ping_interval.tick() => {
                            // Dead socket check: if previous ping had no response and no traffic arrived
                            if let Some(sent_at) = last_ping_sent_at {
                                warn!(
                                    "Dead socket detected: WhatsApp server failed to respond within {:.1}s to keepalive ping. Aborting zombie TCP connection...",
                                    sent_at.elapsed().as_secs_f32()
                                );
                                session_terminated = true;
                                continue;
                            }

                            // Auto-replenish pre-keys if low (checked every ~60s)
                            ticks_since_prekey_check += 1;
                            if ticks_since_prekey_check >= 4 {
                                ticks_since_prekey_check = 0;
                                if is_connected_clone.load(Ordering::SeqCst) {
                                    if let Ok(count) = store_clone.get_key_count("pre-key").await {
                                        if count < 20 {
                                            info!(
                                                "Available one-time prekeys low (count: {} < 20). Automatically uploading 50 new prekeys to ensure uninterrupted message delivery...",
                                                count
                                            );
                                            if let Err(e) = upload_pre_keys(&mut creds, store_clone, &socket, 50).await {
                                                warn!("Failed to auto-replenish prekeys: {}", e);
                                            } else {
                                                info!("Successfully auto-replenished and uploaded 50 prekeys to WhatsApp server.");
                                            }
                                        }
                                    }
                                }
                            }

                            // Send keepalive ping (using standard WhatsApp IQ message tag format)
                            let ping_id = format!("{}.{}-1", rand::random::<u16>(), rand::random::<u16>());
                            let ping = BinaryNode::new("iq")
                                .with_attr("to", S_WHATSAPP_NET)
                                .with_attr("type", "get")
                                .with_attr("id", &ping_id)
                                .with_attr("xmlns", "w:p")
                                .with_child(BinaryNode::new("ping"));

                            if let Err(e) = socket.send_node(ping).await {
                                warn!("Failed to send keepalive ping: {}", e);
                                session_terminated = true;
                            } else {
                                last_ping_sent_at = Some(tokio::time::Instant::now());
                            }
                        }

                        maybe_node = incoming_rx.recv() => {
                            match maybe_node {
                                Some(node) => {
                                    // Received traffic confirms connection is actively alive
                                    if let Some(sent_at) = last_ping_sent_at.take() {
                                        client.metrics().record_ping_latency(sent_at.elapsed());
                                    }
                                    client.metrics().record_msg_in();

                                    debug!("Processing incoming stanza: <{}> (id={:?}, type={:?})",
                                        node.tag, node.get_attr("id"), node.get_attr("type"));

                                    match node.tag.as_str() {
                                        "notification" => {
                                            // Send ACK to WhatsApp server for notification stanza (Baileys parity)
                                            let ack_node = crate::registration::build_ack_node(&node, None, None);
                                            let _ = socket.send_node(ack_node).await;

                                            let active_companion = {
                                                let lock = companion_state_holder.lock().await;
                                                lock.clone()
                                            };
                                            if let Some(ref companion) = active_companion {
                                                if let Some(link_reg) = node.get_child("link_code_companion_reg") {
                                                    let stage = link_reg.get_attr("stage");
                                                    if stage == Some("refresh_code") {
                                                        warn!("Server sent stage='refresh_code' (phone requested pairing code refresh). Automatically generating fresh code...");
                                                        let new_state = Arc::new(CompanionPairingState::new(&companion.phone_number));
                                                        match new_state.build_companion_hello_iq(&creds, &config_clone) {
                                                            Ok(hello_node) => {
                                                                let _ = socket.send_node(hello_node).await;
                                                                let display_code = if new_state.pairing_code.len() == 8 {
                                                                    format!("{}-{}", &new_state.pairing_code[0..4], &new_state.pairing_code[4..8])
                                                                } else {
                                                                    new_state.pairing_code.clone()
                                                                };
                                                                info!("WhatsApp Pairing Code refreshed: {}", display_code);
                                                                events_clone.emit(BaileysEvent::PairingCode {
                                                                    code: display_code,
                                                                });
                                                                {
                                                                    let mut lock = companion_state_holder.lock().await;
                                                                    *lock = Some(Arc::clone(&new_state));
                                                                }
                                                            }
                                                            Err(e) => {
                                                                error!("Failed to rebuild companion_hello on refresh_code: {}", e);
                                                            }
                                                        }
                                                    }

                                                    if link_reg.get_child("primary_identity_pub").is_some() {
                                                        debug!("Received <notification> with primary_identity_pub, computing key bundle reply...");
                                                        match companion.handle_companion_finish_iq(&node, &mut creds) {
                                                            Ok(reply) => {
                                                                debug!("Sending companion_finish IQ reply to server!");
                                                                let _ = socket.send_node(reply).await;
                                                                let _ = store_clone.save_creds(&creds).await;
                                                            }
                                                            Err(e) => {
                                                                error!("Failed to handle companion notification: {}", e);
                                                            }
                                                        }
                                                    } else if stage != Some("refresh_code") {
                                                        debug!("Received <notification> with link_code_companion_reg stage={:?}", stage);
                                                    }
                                                }
                                            }

                                            // Check blocklist push notification
                                            let notif_type = node.get_attr("type").unwrap_or("");
                                            if notif_type == "blocklist" || node.get_child("blocklist").is_some() {
                                                let (added, removed) = crate::privacy::parse_blocklist_notification(&node);
                                                if !added.is_empty() || !removed.is_empty() {
                                                    let store_bl = Arc::clone(store_clone);
                                                    let added_clone = added.clone();
                                                    let removed_clone = removed.clone();
                                                    tokio::spawn(async move {
                                                        for jid in &added_clone {
                                                            let _ = store_bl.save_blocked_contact(jid).await;
                                                        }
                                                        for jid in &removed_clone {
                                                            let _ = store_bl.remove_blocked_contact(jid).await;
                                                        }
                                                    });
                                                    events_clone.emit(BaileysEvent::BlocklistUpdate { added, removed });
                                                }
                                            }

                                            // Check ephemeral timer update push notification
                                            if notif_type == "ephemeral" || node.get_child("ephemeral").is_some() || node.get_child("not_ephemeral").is_some() {
                                                let from_chat = node.get_attr("from").unwrap_or("").to_string();
                                                if !from_chat.is_empty() {
                                                    let expiration = if node.get_child("not_ephemeral").is_some() {
                                                        0
                                                    } else {
                                                        node.get_child("ephemeral")
                                                            .and_then(|e| e.get_attr("expiration"))
                                                            .or_else(|| node.get_attr("expiration"))
                                                            .and_then(|exp| exp.parse::<u32>().ok())
                                                            .unwrap_or(0)
                                                    };
                                                    let store_eph = Arc::clone(store_clone);
                                                    let chat_eph = from_chat.clone();
                                                    tokio::spawn(async move {
                                                        let _ = store_eph.update_chat_ephemeral(&chat_eph, expiration, None).await;
                                                    });
                                                    events_clone.emit(BaileysEvent::ChatEphemeralUpdate {
                                                        chat_jid: from_chat,
                                                        ephemeral_expiration: expiration,
                                                    });
                                                }
                                            }

                                            // Check devices / md companion push notification
                                            if notif_type == "devices" || notif_type == "md" {
                                                let from_user = node.get_attr("from").unwrap_or("").to_string();
                                                let mut updated_devs = Vec::new();
                                                let mut removed_devs = Vec::new();

                                                if let Some(update_node) = node.get_child("update") {
                                                    for dev in update_node.get_children().iter().filter(|c| c.tag == "device") {
                                                        if let Some(id_str) = dev.get_attr("id") {
                                                            if let Ok(id) = id_str.parse::<u32>() {
                                                                updated_devs.push(id);
                                                            }
                                                        }
                                                    }
                                                }

                                                if let Some(remove_node) = node.get_child("remove") {
                                                    for dev in remove_node.get_children().iter().filter(|c| c.tag == "device") {
                                                        if let Some(id_str) = dev.get_attr("id") {
                                                            if let Ok(id) = id_str.parse::<u32>() {
                                                                removed_devs.push(id);
                                                            }
                                                        }
                                                    }
                                                }

                                                // Check if our own companion was removed
                                                let our_jid = creds.me_jid.as_deref().unwrap_or("");
                                                let our_device_id = our_jid.split_once(':')
                                                    .and_then(|(_, rest)| rest.split_once('@'))
                                                    .and_then(|(d, _)| d.parse::<u32>().ok());

                                                if let Some(our_id) = our_device_id {
                                                    if removed_devs.contains(&our_id) {
                                                        warn!("Our companion device ID {} was revoked by primary. Disconnecting...", our_id);
                                                        events_clone.emit(BaileysEvent::Disconnected {
                                                            reason: "Logged out by primary device".to_string(),
                                                        });
                                                    }
                                                }

                                                if !updated_devs.is_empty() || !removed_devs.is_empty() {
                                                    let store_dev = Arc::clone(store_clone);
                                                    let user_target = from_user.clone();
                                                    let u_devs = updated_devs.clone();
                                                    let r_devs = removed_devs.clone();
                                                    tokio::spawn(async move {
                                                        if !u_devs.is_empty() {
                                                            let _ = store_dev.save_user_devices(&user_target, &u_devs).await;
                                                        }
                                                        for r in r_devs {
                                                            let _ = store_dev.remove_user_device(&user_target, r).await;
                                                        }
                                                    });
                                                    events_clone.emit(BaileysEvent::DeviceUpdate {
                                                        user_jid: from_user,
                                                        devices: updated_devs,
                                                    });
                                                }
                                            }

                                            // Check identity change notification
                                            if notif_type == "identity" || node.get_child("identity").is_some() {
                                                let from_user = node.get_attr("from").unwrap_or("").to_string();
                                                let timestamp = node.get_attr("t").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
                                                let is_companion = from_user.contains(':') && !from_user.contains(":0@");
                                                let our_jid = creds.me_jid.as_deref().unwrap_or("");
                                                let is_self = !from_user.is_empty() && (from_user == our_jid || our_jid.starts_with(&format!("{}:", from_user.split('@').next().unwrap_or(""))));
                                                if !is_companion && !is_self && !from_user.is_empty() {
                                                    info!("Received identity change notification for JID: {}", from_user);
                                                    let store_id = Arc::clone(store_clone);
                                                    let target_id = from_user.clone();
                                                    tokio::spawn(async move {
                                                        let _ = store_id.remove_keys("session", std::slice::from_ref(&target_id)).await;
                                                        let _ = store_id.remove_keys("tctoken", std::slice::from_ref(&target_id)).await;
                                                    });
                                                    events_clone.emit(BaileysEvent::IdentityChanged {
                                                        jid: from_user,
                                                        timestamp,
                                                    });
                                                }
                                            }

                                            // Check mediaretry push notification
                                            if notif_type == "mediaretry" {
                                                let msg_id = node.get_attr("id").unwrap_or("").to_string();
                                                debug!("Received mediaretry notification for message ID: {}", msg_id);
                                                if let Some(encrypt_node) = node.get_child("encrypt") {
                                                    let enc_p = encrypt_node.get_child("enc_p").and_then(|c| c.content_as_bytes());
                                                    let enc_iv = encrypt_node.get_child("enc_iv").and_then(|c| c.content_as_bytes());
                                                    if let (Some(ciphertext), Some(iv)) = (enc_p, enc_iv) {
                                                        let store_m = Arc::clone(store_clone);
                                                        let events_m = events_clone.clone();
                                                        let ct_vec = ciphertext.to_vec();
                                                        let iv_vec = iv.to_vec();
                                                        let target_id = msg_id.clone();
                                                        tokio::spawn(async move {
                                                            if let Ok(Some(stored_msg)) = store_m.get_message(&target_id).await {
                                                                if let Ok(proto_info) = baileys_proto::proto::WebMessageInfo::decode(&stored_msg.proto_bytes[..]) {
                                                                    if let Some(ref m) = proto_info.message {
                                                                        let media_key = m.image_message.as_ref().and_then(|img| img.media_key.clone())
                                                                            .or_else(|| m.video_message.as_ref().and_then(|v| v.media_key.clone()))
                                                                            .or_else(|| m.audio_message.as_ref().and_then(|a| a.media_key.clone()))
                                                                            .or_else(|| m.document_message.as_ref().and_then(|d| d.media_key.clone()))
                                                                            .or_else(|| m.sticker_message.as_ref().and_then(|s| s.media_key.clone()));

                                                                        if let Some(m_key) = media_key {
                                                                            match baileys_media::decrypt_media_retry_notification(&ct_vec, &iv_vec, &m_key, &target_id) {
                                                                                Ok(retry_notif) => {
                                                                                    if let Some(ref new_path) = retry_notif.direct_path {
                                                                                        info!("Media retry succeeded for {}, new direct_path: {}", target_id, new_path);
                                                                                        let mut updated_info = proto_info.clone();
                                                                                        if let Some(ref mut up_m) = updated_info.message {
                                                                                            if let Some(ref mut img) = up_m.image_message { img.direct_path = Some(new_path.clone()); }
                                                                                            if let Some(ref mut v) = up_m.video_message { v.direct_path = Some(new_path.clone()); }
                                                                                            if let Some(ref mut a) = up_m.audio_message { a.direct_path = Some(new_path.clone()); }
                                                                                            if let Some(ref mut d) = up_m.document_message { d.direct_path = Some(new_path.clone()); }
                                                                                            if let Some(ref mut s) = up_m.sticker_message { s.direct_path = Some(new_path.clone()); }
                                                                                        }
                                                                                        let mut updated_bytes = Vec::new();
                                                                                        let _ = updated_info.encode(&mut updated_bytes);
                                                                                        let _ = store_m.update_message_content(&stored_msg.chat_jid, &target_id, &stored_msg.summary, Some(&updated_bytes)).await;
                                                                                        events_m.emit(BaileysEvent::MediaUpdated {
                                                                                            message_id: target_id,
                                                                                            direct_path: new_path.clone(),
                                                                                        });
                                                                                    }
                                                                                }
                                                                                Err(e) => {
                                                                                    warn!("Failed to decrypt media retry notification for {}: {}", target_id, e);
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        });
                                                    }
                                                }
                                            }

                                            // Check server_sync notification for app state
                                            if notif_type == "server_sync" || node.get_child("collection").is_some() {
                                                debug!("Received server_sync notification, scheduling AppState resync...");
                                                let client_sync = client.clone();
                                                tokio::spawn(async move {
                                                    if let Err(e) = client_sync.resync_app_state(crate::syncd::ALL_COLLECTIONS, false).await {
                                                        debug!("Server sync notification AppState resync note: {}", e);
                                                    }
                                                });
                                            }

                                            // Send ACK after processing notification
                                            let ack = build_ack_node(&node, None, creds.me_jid.as_deref());
                                            let _ = socket.send_node(ack).await;
                                        }

                                        "iq" => {
                                            let iq_type = node.get_attr("type").unwrap_or("");
                                            let msg_id = node.get_attr("id").unwrap_or("").to_string();

                                            // Fulfill any registered oneshot sender awaiting this IQ response
                                            if iq_type == "result" || iq_type == "error" {
                                                let mut pending = pending_iq_clone.lock().await;
                                                if let Some(tx) = pending.remove(&msg_id) {
                                                    let _ = tx.send(node.clone());
                                                }
                                            }

                                            if iq_type == "error" {
                                                warn!("WhatsApp server returned IQ error: {:?}", node);
                                            }

                                            // A. QR tokens or phone pairing
                                            if let Some(pair_device_node) = node.get_child("pair-device") {
                                                debug!("Received server <pair-device> stanza!");
                                                let ack = BinaryNode::new("iq")
                                                    .with_attr("to", S_WHATSAPP_NET)
                                                    .with_attr("type", "result")
                                                    .with_attr("id", &msg_id);
                                                let _ = socket.send_node(ack).await;

                                                if creds.me_jid.is_none() {
                                                    if let Some(ref phone) = config_clone.phone_number_pairing {
                                                        if !pairing_code_sent {
                                                            pairing_code_sent = true;
                                                            info!("Initiating WhatsApp companion pairing for phone: {}", phone);
                                                            let state = Arc::new(CompanionPairingState::new(phone));
                                                            match state.build_companion_hello_iq(&creds, &config_clone) {
                                                                Ok(hello_node) => {
                                                                    let _ = socket.send_node(hello_node).await;
                                                                    let display_code = if state.pairing_code.len() == 8 {
                                                                        format!("{}-{}", &state.pairing_code[0..4], &state.pairing_code[4..8])
                                                                    } else {
                                                                        state.pairing_code.clone()
                                                                    };
                                                                    info!("WhatsApp Pairing Code generated: {}", display_code);
                                                                    events_clone.emit(BaileysEvent::PairingCode {
                                                                        code: display_code,
                                                                    });
                                                                    {
                                                                        let mut lock = companion_state_holder.lock().await;
                                                                        *lock = Some(Arc::clone(&state));
                                                                    }
                                                                }
                                                                Err(e) => {
                                                                    error!("Failed to build companion_hello: {}", e);
                                                                }
                                                            }
                                                        }
                                                    } else {
                                                        let ref_children = pair_device_node.get_children();
                                                        for ref_child in ref_children {
                                                            if ref_child.tag == "ref" {
                                                                if let Some(ref_str) = ref_child.content_as_str() {
                                                                    let noise_b64 = BASE64_STANDARD.encode(&creds.noise_key.public);
                                                                    let ident_b64 = BASE64_STANDARD.encode(&creds.signed_identity_key.public);
                                                                    let adv_b64 = BASE64_STANDARD.encode(&creds.adv_secret_key);
                                                                    let qr = if ref_str.contains('@') {
                                                                        format!("{},{},{},{}", ref_str, noise_b64, ident_b64, adv_b64)
                                                                    } else {
                                                                        format!("1@{},{},{},{}", ref_str, noise_b64, ident_b64, adv_b64)
                                                                    };
                                                                    debug!("WhatsApp QR Code generated (ref_len={})", ref_str.len());
                                                                    {
                                                                        let mut lock = last_qr_holder.lock().await;
                                                                        *lock = Some(qr.clone());
                                                                    }
                                                                    events_clone.emit(BaileysEvent::QrCode { code: qr });
                                                                    break;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            // B. Device paired successfully! (<pair-success>)
                                            if node.get_child("pair-success").is_some() {
                                                info!("Device paired successfully! Validating server signatures...");
                                                match configure_successful_pairing(&node, &creds) {
                                                    Ok((reply_node, updated)) => {
                                                        debug!("Pairing validated! Sending <pair-device-sign> confirmation...");
                                                        if let Err(e) = socket.send_node(reply_node).await {
                                                            error!("Failed to send pair-device-sign ACK: {}", e);
                                                        }

                                                        creds.me_jid = Some(updated.me_jid.clone());
                                                        if let Some(ref lid) = updated.lid {
                                                            creds.me_lid = Some(lid.clone());
                                                            me_lid = Some(lid.clone());
                                                        }
                                                        if let Some(ref acc) = updated.account {
                                                            creds.account = Some(acc.encode_to_vec());
                                                        }
                                                        if let Err(e) = store_clone.save_creds(&creds).await {
                                                            error!("Failed to save updated credentials: {}", e);
                                                        }

                                                        {
                                                            let mut lock = last_qr_holder.lock().await;
                                                            *lock = None;
                                                        }

                                                        info!("WhatsApp pairing authenticated for '{}'! Server will now restart connection...", updated.me_jid);
                                                    }
                                                    Err(e) => {
                                                        error!("Failed to configure successful pairing: {}", e);
                                                    }
                                                }
                                            }

                                            // C. Respond to incoming ping
                                            if iq_type == "get" && node.get_child("ping").is_some() {
                                                let pong = BinaryNode::new("iq")
                                                    .with_attr("to", S_WHATSAPP_NET)
                                                    .with_attr("type", "result")
                                                    .with_attr("id", &msg_id);
                                                let _ = socket.send_node(pong).await;
                                            }
                                        }

                                        "success" => {
                                            debug!("Server sent <success> node! Reconnect authentication complete.");
                                            reconnect_delay = Duration::from_secs(3);
                                            if let Some(lid) = node.get_attr("lid") {
                                                debug!("Associated LID: {}", lid);
                                                me_lid = Some(lid.to_string());
                                                creds.me_lid = Some(lid.to_string());
                                                let _ = store_clone.save_creds(&creds).await;
                                            }

                                            // Upload initial 30 PreKeys
                                            debug!("Uploading initial PreKey bundle to WhatsApp server...");
                                            if let Err(e) = upload_pre_keys(&mut creds, store_clone, &socket, 30).await {
                                                warn!("Failed to upload PreKeys: {}", e);
                                            }

                                            // Send passive active IQ
                                            debug!("Activating real-time stream via passive IQ...");
                                            let passive = build_passive_iq("active");
                                            if let Err(e) = socket.send_node(passive).await {
                                                warn!("Failed to send passive active IQ: {}", e);
                                            }

                                            // Send unified session telemetry
                                            let now_ms = std::time::SystemTime::now()
                                                .duration_since(std::time::UNIX_EPOCH)
                                                .map(|d| d.as_millis() as u64)
                                                .unwrap_or(0);
                                            let unified_id = ((now_ms + 3 * 86_400_000) % (7 * 86_400_000)).to_string();
                                            let unified = BinaryNode::new("ib")
                                                .with_child(BinaryNode::new("unified_session").with_attr("id", unified_id));
                                            let _ = socket.send_node(unified).await;

                                            if let Some(ref me) = creds.me_jid {
                                                is_connected_clone.store(true, Ordering::SeqCst);
                                                client.set_connection_state(crate::state::ConnectionState::Connected);
                                                debug!("WhatsApp authenticated and connected as '{}'!", me);
                                                events_clone.emit(BaileysEvent::Connected { me_jid: me.clone() });

                                                // Trigger automatic multi-device App State resync
                                                let client_sync = client.clone();
                                                tokio::spawn(async move {
                                                    tokio::time::sleep(Duration::from_millis(500)).await;
                                                    if let Err(e) = client_sync.resync_app_state(crate::syncd::ALL_COLLECTIONS, false).await {
                                                        debug!("Initial AppState auto-resync note: {}", e);
                                                    }
                                                });
                                            }
                                        }

                                        "message" => {
                                            let from = node.get_attr("from").unwrap_or("unknown").to_string();
                                            let msg_id = node.get_attr("id").unwrap_or("").to_string();
                                            let participant_opt = node.get_attr("participant").map(|s| s.to_string());
                                            let recipient_opt = node.get_attr("recipient").map(|s| s.to_string());
                                            let notify_opt = node.get_attr("notify").map(|s| s.to_string());
                                            let sender_pn_opt = node.get_attr("sender_pn").or_else(|| node.get_attr("participant_pn")).map(|s| s.to_string());
                                            let sender_lid_opt = node.get_attr("sender_lid").or_else(|| node.get_attr("participant_lid")).map(|s| s.to_string());

                                            if let Some(ref pn) = sender_pn_opt {
                                                let c = baileys_store::StoredContact {
                                                    jid: pn.clone(),
                                                    name: None,
                                                    push_name: notify_opt.clone(),
                                                    lid: sender_lid_opt.clone(),
                                                };
                                                let _ = store_clone.save_contact(&c).await;
                                            }

                                            let is_peer_category = node.get_attr("category") == Some("peer");
                                            let node_ts: u64 = node.get_attr("t")
                                                .and_then(|t| t.parse::<u64>().ok())
                                                .unwrap_or_else(|| std::time::SystemTime::now()
                                                    .duration_since(std::time::UNIX_EPOCH)
                                                    .map(|d| d.as_secs())
                                                    .unwrap_or(0));

                                            // 1. Encrypted messages are acknowledged with a <receipt> after decryption
                                            //    (delivery / peer_msg / hist_sync). Only unencrypted stanzas get a plain ACK here.
                                            let has_enc = node.get_children().iter().any(|c| c.tag == "enc");
                                            if !has_enc {
                                                let ack = build_ack_node(&node, None, creds.me_jid.as_deref());
                                                let _ = socket.send_node(ack).await;
                                            }

                                            // 2. Identify author and chat (participant takes precedence for groups and broadcast/status)
                                            let author_jid = if let Some(ref p) = participant_opt {
                                                p.clone()
                                            } else {
                                                from.clone()
                                            };
                                            let is_from_me = {
                                                let author_user = jid_user_part(&author_jid);
                                                creds.me_jid.as_deref().map(jid_user_part) == Some(author_user)
                                                    || me_lid.as_deref().map(jid_user_part) == Some(author_user)
                                            };
                                            // For 1:1 messages sent by us from another device, the chat is the `recipient`
                                            let chat_jid_raw = if is_from_me && participant_opt.is_none() {
                                                recipient_opt.clone().unwrap_or_else(|| from.clone())
                                            } else {
                                                from.clone()
                                            };
                                            let chat_jid_resolved = baileys_binary::jid::jid_normalized_user(&chat_jid_raw);

                                            // Ephemeral disappearing timer sync from XML attributes
                                            if let Some(eph_child) = node.get_child("ephemeral") {
                                                if let Some(exp_str) = eph_child.get_attr("expiration") {
                                                    if let Ok(expiration) = exp_str.parse::<u32>() {
                                                        let store_eph = Arc::clone(store_clone);
                                                        let chat_eph = chat_jid_resolved.clone();
                                                        tokio::spawn(async move {
                                                            let _ = store_eph.update_chat_ephemeral(&chat_eph, expiration, None).await;
                                                        });
                                                        events_clone.emit(BaileysEvent::ChatEphemeralUpdate {
                                                            chat_jid: chat_jid_resolved.clone(),
                                                            ephemeral_expiration: expiration,
                                                        });
                                                    }
                                                }
                                            } else if node.get_child("not_ephemeral").is_some() {
                                                let store_eph = Arc::clone(store_clone);
                                                let chat_eph = chat_jid_resolved.clone();
                                                tokio::spawn(async move {
                                                    let _ = store_eph.update_chat_ephemeral(&chat_eph, 0, None).await;
                                                });
                                                events_clone.emit(BaileysEvent::ChatEphemeralUpdate {
                                                    chat_jid: chat_jid_resolved.clone(),
                                                    ephemeral_expiration: 0,
                                                });
                                            }

                                            // 3. Scan children for <enc> nodes
                                            let enc_children: Vec<&BinaryNode> = node.get_children().iter()
                                                .filter(|c| c.tag == "enc")
                                                .collect();

                                            if enc_children.is_empty() {
                                                // Support unencrypted channel broadcasts (<plaintext> node)
                                                if let Some(plain_node) = node.get_child("plaintext") {
                                                    if let Some(bytes) = plain_node.content_as_bytes() {
                                                        if let Ok(proto) = <Message as ProstMessage>::decode(bytes) {
                                                            let msg_info = WebMessageInfo {
                                                                key: Some(MessageKey {
                                                                    remote_jid: Some(from.clone()),
                                                                    from_me: Some(false),
                                                                    id: Some(msg_id.clone()),
                                                                    participant: participant_opt,
                                                                }),
                                                                message: Some(proto),
                                                                message_timestamp: Some(
                                                                    std::time::SystemTime::now()
                                                                        .duration_since(std::time::UNIX_EPOCH)
                                                                        .unwrap()
                                                                        .as_secs(),
                                                                ),
                                                                ..Default::default()
                                                            };

                                                            // Cache in recent_messages
                                                            {
                                                                let mut cache = recent_messages_clone.lock().await;
                                                                if cache.len() > 500 {
                                                                    cache.clear();
                                                                }
                                                                cache.insert(msg_id.clone(), msg_info.clone());
                                                            }

                                                            events_clone.emit(BaileysEvent::Message {
                                                                message: Box::new(msg_info),
                                                            });
                                                        }
                                                    }
                                                } else if let Some(body_node) = node.get_child("body") {
                                                    if let Some(text) = body_node.content_as_string() {
                                                        let msg_info = WebMessageInfo {
                                                            key: Some(MessageKey {
                                                                remote_jid: Some(from.clone()),
                                                                from_me: Some(false),
                                                                id: Some(msg_id.clone()),
                                                                participant: participant_opt,
                                                            }),
                                                            message: Some(Message {
                                                                conversation: Some(text.to_string()),
                                                                ..Default::default()
                                                            }),
                                                            message_timestamp: Some(
                                                                std::time::SystemTime::now()
                                                                    .duration_since(std::time::UNIX_EPOCH)
                                                                    .unwrap()
                                                                    .as_secs(),
                                                            ),
                                                            ..Default::default()
                                                        };

                                                        // Cache in recent_messages
                                                        {
                                                            let mut cache = recent_messages_clone.lock().await;
                                                            if cache.len() > 500 {
                                                                cache.clear();
                                                            }
                                                            cache.insert(msg_id.clone(), msg_info.clone());
                                                        }

                                                        events_clone.emit(BaileysEvent::Message {
                                                            message: Box::new(msg_info),
                                                        });
                                                    }
                                                }
                                            } else {
                                                // Decrypt enc children. Prioritize pkmsg/msg before skmsg to ingest SenderKeyDistributionMessage
                                                let mut sorted_encs = enc_children.clone();
                                                sorted_encs.sort_by_key(|c| {
                                                    match c.get_attr("type").unwrap_or("") {
                                                        "pkmsg" => 0,
                                                        "msg" => 1,
                                                        "skmsg" => 2,
                                                        _ => 3,
                                                    }
                                                });

                                                let mut decrypted_message_proto: Option<Message> = None;
                                                let mut decryption_error: Option<String> = None;

                                                for enc in sorted_encs {
                                                    let enc_type = enc.get_attr("type").unwrap_or("");
                                                    if let Some(ciphertext) = enc.content_as_bytes() {
                                                        match enc_type {
                                                            "pkmsg" | "msg" => {
                                                                let our_id = KeyPair::try_from(&creds.signed_identity_key).ok();
                                                                let our_spk = KeyPair::try_from(&creds.signed_pre_key.key_pair).ok();
                                                                if let (Some(id), Some(spk)) = (our_id, our_spk) {
                                                                    match signal_clone.decrypt_message(&author_jid, enc_type, ciphertext, &id, &spk).await {
                                                                        Ok(plaintext) => {
                                                                            client.metrics().record_signal_hit();
                                                                            let unpadded = unpad_random_max16(&plaintext);
                                                                            if let Ok(proto) = Message::decode(unpadded) {
                                                                                if let Some(ref skdm) = proto.sender_key_distribution_message {
                                                                                    if let (Some(ref grp), Some(ref axolotl_bytes)) = (&skdm.group_id, &skdm.axolotl_sender_key_distribution_message) {
                                                                                        debug!("Processing incoming SenderKeyDistributionMessage for group {} from {}", grp, author_jid);
                                                                                        match signal_clone.process_sender_key_distribution_message(grp, &author_jid, axolotl_bytes).await {
                                                                                            Ok(_) => debug!("Successfully saved SenderKey for group {} from {}", grp, author_jid),
                                                                                            Err(e) => warn!("Failed to process SenderKeyDistributionMessage for group {} from {}: {}", grp, author_jid, e),
                                                                                        }
                                                                                    }
                                                                                }
                                                                                if crate::messages::has_real_message_content(&proto) {
                                                                                    decrypted_message_proto = Some(proto);
                                                                                }
                                                                            }
                                                                        }
                                                                        Err(e) => {
                                                                            client.metrics().record_signal_miss();
                                                                            warn!("Failed to decrypt 1-on-1 message ({}) from {}: {}", enc_type, author_jid, e);
                                                                            decryption_error = Some(e.to_string());
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            "skmsg" => {
                                                                match signal_clone.decrypt_group_message(&from, &author_jid, ciphertext).await {
                                                                    Ok(plaintext) => {
                                                                        client.metrics().record_signal_hit();
                                                                        let unpadded = unpad_random_max16(&plaintext);
                                                                        if let Ok(proto) = Message::decode(unpadded) {
                                                                            decrypted_message_proto = Some(proto);
                                                                        }
                                                                    }
                                                                    Err(e) => {
                                                                        client.metrics().record_signal_miss();
                                                                        let err_str = e.to_string();
                                                                        if err_str.contains("No established Signal session found") {
                                                                            debug!("Group sender {} in {} has no active Signal session yet; sending retry receipt for key exchange", author_jid, from);
                                                                        } else {
                                                                            warn!("Failed to decrypt group skmsg from {} in {}: {}", author_jid, from, err_str);
                                                                        }
                                                                        decryption_error = Some(err_str);
                                                                    }
                                                                }
                                                            }
                                                            _ => {}
                                                        }
                                                    }
                                                }

                                                // If decryption failed, send a retry receipt to request key re-transmission
                                                if decrypted_message_proto.is_none() && decryption_error.is_some() {
                                                    let retry_key = format!("{}:{}", msg_id, author_jid);
                                                    let count = msg_retries.entry(retry_key).or_insert(0);
                                                    *count += 1;
                                                    let current_retry = *count;

                                                    if current_retry <= 5 {
                                                        debug!("Key negotiation for msg {} from {} (attempt {}/5). Sending retry receipt...", msg_id, author_jid, current_retry);
                                                        match create_retry_keys_node(&mut creds, store_clone).await {
                                                            Ok(keys_node) => {
                                                                let retry = build_retry_receipt(&node, creds.registration_id, current_retry, Some(keys_node));
                                                                let _ = socket.send_node(retry).await;
                                                            }
                                                            Err(e) => {
                                                                warn!("Failed to create retry keys node: {}. Sending basic retry receipt...", e);
                                                                let retry = build_retry_receipt(&node, creds.registration_id, current_retry, None);
                                                                let _ = socket.send_node(retry).await;
                                                            }
                                                        }
                                                    } else {
                                                        warn!("Max retry attempts (5) reached for msg {} from {}. Dropping retry.", msg_id, author_jid);
                                                    }

                                                    // Acknowledge the stanza so the server does not redeliver it endlessly
                                                    let ack = build_ack_node(&node, None, creds.me_jid.as_deref());
                                                    let _ = socket.send_node(ack).await;
                                                } else {
                                                    // Decryption succeeded: send the delivery receipt (Baileys parity)
                                                    //  - category="peer" (messages from our own primary, e.g. history sync) => peer_msg
                                                    //  - sent by us from another device => sender
                                                    //  - otherwise plain delivery receipt (no type)
                                                    let mut receipt = BinaryNode::new("receipt").with_attr("id", &msg_id);
                                                    if is_peer_category {
                                                        receipt = receipt.with_attr("to", &from).with_attr("type", "peer_msg");
                                                    } else if is_from_me {
                                                        receipt = receipt.with_attr("type", "sender");
                                                        if participant_opt.is_none() && recipient_opt.is_some() {
                                                            receipt = receipt
                                                                .with_attr("to", &from)
                                                                .with_attr("recipient", recipient_opt.as_deref().unwrap_or_default());
                                                        } else {
                                                            receipt = receipt.with_attr("to", &from);
                                                            if let Some(ref p) = participant_opt {
                                                                receipt = receipt.with_attr("participant", p);
                                                            }
                                                        }
                                                    } else {
                                                        receipt = receipt.with_attr("to", &from);
                                                        if let Some(ref p) = participant_opt {
                                                            receipt = receipt.with_attr("participant", p);
                                                        }
                                                    }
                                                    let _ = socket.send_node(receipt).await;

                                                    // History sync notifications additionally require a `hist_sync` receipt;
                                                    // without it the primary phone stops after the bootstrap chunk and never
                                                    // streams RECENT / FULL history.
                                                    let is_history_msg = decrypted_message_proto.as_ref()
                                                        .and_then(|m| m.protocol_message.as_ref())
                                                        .map(|p| p.history_sync_notification.is_some())
                                                        .unwrap_or(false);
                                                    if is_history_msg {
                                                        let hist_receipt = BinaryNode::new("receipt")
                                                            .with_attr("id", &msg_id)
                                                            .with_attr("to", normalize_user_jid(&chat_jid_resolved))
                                                            .with_attr("type", "hist_sync");
                                                        debug!("Sending hist_sync receipt for history notification {}", msg_id);
                                                        let _ = socket.send_node(hist_receipt).await;
                                                    }
                                                }

                                                // If message successfully decrypted, emit Message event
                                                if let Some(proto) = decrypted_message_proto {
                                                    let retry_key = format!("{}:{}", msg_id, author_jid);
                                                    msg_retries.remove(&retry_key);
                                                    let msg_info = WebMessageInfo {
                                                        key: Some(MessageKey {
                                                            remote_jid: Some(chat_jid_resolved.clone()),
                                                            from_me: Some(is_from_me),
                                                            id: Some(msg_id.clone()),
                                                            participant: participant_opt,
                                                        }),
                                                        message: Some(proto.clone()),
                                                        message_timestamp: Some(node_ts),
                                                        push_name: notify_opt.clone(),
                                                        ..Default::default()
                                                    };

                                                    // Cache in recent_messages
                                                    {
                                                        let mut cache = recent_messages_clone.lock().await;
                                                        if cache.len() > 500 {
                                                            cache.clear();
                                                        }
                                                        cache.insert(msg_id.clone(), msg_info.clone());
                                                    }

                                                    // Auto-download media if enabled or anti-view-once active
                                                    let is_auto_dl = auto_download_clone.load(Ordering::SeqCst);
                                                    let is_auto_save_vo = config_clone.auto_save_view_once;
                                                    if is_auto_dl || is_auto_save_vo {
                                                        if let Some(media_attach) = baileys_media::extract_media_from_message(&proto) {
                                                            let should_download = is_auto_dl || (is_auto_save_vo && media_attach.is_view_once);
                                                            if should_download {
                                                                let msg_id_dl = msg_id.clone();
                                                                tokio::spawn(async move {
                                                                    let dest = if media_attach.is_view_once {
                                                                        std::path::Path::new("downloads/view_once")
                                                                    } else {
                                                                        std::path::Path::new("downloads")
                                                                    };
                                                                    let _ = std::fs::create_dir_all(dest);
                                                                    match baileys_media::download_media_to_dir(&media_attach, dest, &msg_id_dl).await {
                                                                        Ok(path) => {
                                                                            if media_attach.is_view_once {
                                                                                info!("[Anti View-Once] Intercepted and saved view-once media for {} -> {:?}", msg_id_dl, path);
                                                                            } else {
                                                                                debug!("Auto-downloaded media for {} -> {:?}", msg_id_dl, path);
                                                                            }
                                                                        }
                                                                        Err(e) => warn!("Auto-download failed for {}: {}", msg_id_dl, e),
                                                                    }
                                                                });
                                                            }
                                                        }
                                                    }

                                                    // Process protocol_message payloads (HistorySync, AppStateSyncKeyShare, etc.)
                                                    if let Some(ref protocol) = proto.protocol_message {
                                                        if let Some(ref key_share) = protocol.app_state_sync_key_share {
                                                            debug!("Received AppStateSyncKeyShare with {} keys", key_share.keys.len());
                                                            use base64::Engine;
                                                            let mut key_entries = Vec::new();
                                                            for k in &key_share.keys {
                                                                if let (Some(id), Some(data)) = (&k.key_id, &k.key_data) {
                                                                    if let (Some(ref id_bytes), Some(ref data_bytes)) = (&id.key_id, &data.key_data) {
                                                                        let id_b64 = base64::engine::general_purpose::STANDARD.encode(id_bytes);
                                                                        key_entries.push(baileys_store::KeyEntry {
                                                                            key_type: "app-state-sync-key".to_string(),
                                                                            id: id_b64,
                                                                            data: data_bytes.clone(),
                                                                        });
                                                                        key_entries.push(baileys_store::KeyEntry {
                                                                            key_type: "app-state-sync-key".to_string(),
                                                                            id: "primary".to_string(),
                                                                            data: data_bytes.clone(),
                                                                        });
                                                                        key_entries.push(baileys_store::KeyEntry {
                                                                            key_type: "app-state-sync-key".to_string(),
                                                                            id: "primary_id".to_string(),
                                                                            data: id_bytes.clone(),
                                                                        });
                                                                    }
                                                                }
                                                            }
                                                            if !key_entries.is_empty() {
                                                                let store_keys = Arc::clone(store_clone);
                                                                let client_sync = client.clone();
                                                                tokio::spawn(async move {
                                                                    let _ = store_keys.set_keys(key_entries).await;
                                                                    tokio::time::sleep(Duration::from_millis(300)).await;
                                                                    if let Err(e) = client_sync.resync_app_state(crate::syncd::ALL_COLLECTIONS, false).await {
                                                                        debug!("AppStateSyncKeyShare auto-resync note: {}", e);
                                                                    }
                                                                });
                                                            }
                                                        }

                                                        if let Some(ref history_notif) = protocol.history_sync_notification {
                                                            debug!("Received HistorySyncNotification (sync_type={:?}, chunk={:?}), downloading archive...",
                                                                history_notif.sync_type, history_notif.chunk_order);

                                                            let notif_clone = history_notif.clone();
                                                            let events_h = events_clone.clone();
                                                            let store_h = Arc::clone(store_clone);
                                                            let signal_h = Arc::clone(signal_clone);

                                                            tokio::spawn(async move {
                                                                match baileys_media::download_history_sync(&notif_clone).await {
                                                                    Ok(history) => {
                                                                        let sync_type = history.sync_type.unwrap_or(0);
                                                                        let progress = history.progress.unwrap_or(0);
                                                                        let conv_count = history.conversations.len();
                                                                        let status_count = history.status_v3_messages.len();
                                                                        debug!("Successfully decrypted HistorySync: {} conversations, {} status updates (type={}, progress={})",
                                                                            conv_count, status_count, sync_type, progress);

                                                                        // Persist LID-PN mappings
                                                                        let mut entries = Vec::new();
                                                                        for mapping in &history.phone_number_to_lid_mappings {
                                                                            if let (Some(ref pn), Some(ref lid)) = (&mapping.pn_jid, &mapping.lid_jid) {
                                                                                signal_h.lid_mappings().store_mapping(pn, lid);
                                                                                entries.push(baileys_store::KeyEntry {
                                                                                    key_type: "lid-mapping".to_string(),
                                                                                    id: pn.clone(),
                                                                                    data: lid.as_bytes().to_vec(),
                                                                                });
                                                                                let _ = store_h.save_contact(&StoredContact {
                                                                                    jid: pn.clone(),
                                                                                    name: None,
                                                                                    push_name: None,
                                                                                    lid: Some(lid.clone()),
                                                                                }).await;
                                                                            }
                                                                        }
                                                                        if !entries.is_empty() {
                                                                            let _ = store_h.set_keys(entries).await;
                                                                        }

                                                                        // Persist Pushnames and build lookup map for chat naming
                                                                        let mut pushname_map: HashMap<String, String> = HashMap::new();
                                                                        for pn in &history.pushnames {
                                                                            if let (Some(ref id), Some(ref name)) = (&pn.id, &pn.pushname) {
                                                                                pushname_map.insert(id.clone(), name.clone());
                                                                                let _ = store_h.save_contact(&StoredContact {
                                                                                    jid: id.clone(),
                                                                                    name: None,
                                                                                    push_name: Some(name.clone()),
                                                                                    lid: None,
                                                                                }).await;
                                                                            }
                                                                        }

                                                                        // Persist Conversations & Historical Messages
                                                                        for conv in &history.conversations {
                                                                            let chat_jid = conv.id.as_deref().unwrap_or_default();
                                                                            if chat_jid.is_empty() {
                                                                                continue;
                                                                            }

                                                                            let mut stored_msgs = Vec::new();
                                                                            let mut latest_ts = conv.last_msg_timestamp.or(conv.conversation_timestamp).unwrap_or(0);
                                                                            let mut latest_summary = None;

                                                                            for hist_msg in &conv.messages {
                                                                                if let Some(ref wmi) = hist_msg.message {
                                                                                    let msg_id = wmi.key.as_ref().and_then(|k| k.id.clone()).unwrap_or_default();
                                                                                    let sender = wmi.key.as_ref().and_then(|k| k.participant.clone())
                                                                                        .or_else(|| wmi.key.as_ref().and_then(|k| k.remote_jid.clone()))
                                                                                        .unwrap_or_else(|| chat_jid.to_string());
                                                                                    let from_me = wmi.key.as_ref().and_then(|k| k.from_me).unwrap_or(false);
                                                                                    let ts = wmi.message_timestamp.unwrap_or(0);
                                                                                    if ts > latest_ts {
                                                                                        latest_ts = ts;
                                                                                    }

                                                                                    let (summary, has_media) = if let Some(ref m) = wmi.message {
                                                                                        let s = crate::messages::extract_message_summary(m);
                                                                                        let h_m = crate::messages::message_has_media(m);
                                                                                        (s, h_m)
                                                                                    } else {
                                                                                        ("<empty>".to_string(), false)
                                                                                    };

                                                                                    if latest_summary.is_none() {
                                                                                        latest_summary = Some(summary.clone());
                                                                                    }

                                                                                    let mut raw_bytes = Vec::new();
                                                                                    if let Some(ref m) = wmi.message {
                                                                                        let _ = m.encode(&mut raw_bytes);
                                                                                    }

                                                                                    let is_starred = wmi.starred.unwrap_or(false);
                                                                                    stored_msgs.push(StoredMessage {
                                                                                        id: msg_id,
                                                                                        chat_jid: chat_jid.to_string(),
                                                                                        sender_jid: sender,
                                                                                        from_me,
                                                                                        timestamp: ts,
                                                                                        summary,
                                                                                        status: "delivered".to_string(),
                                                                                        has_media,
                                                                                        proto_bytes: raw_bytes,
                                                                                        is_starred,
                                                                                        is_revoked: false,
                                                                                        is_edited: false,
                                                                                    });
                                                                                }
                                                                            }

                                                                            if !stored_msgs.is_empty() {
                                                                                let _ = store_h.save_messages_batch(&stored_msgs).await;
                                                                            }

                                                                            let is_pinned = conv.pinned.map(|p| p > 0).unwrap_or(false);
                                                                            let is_archived = conv.archived.unwrap_or(false);
                                                                            let mute_until = conv.mute_end_time;

                                                                            let chat_name = conv.name.clone()
                                                                                .or_else(|| conv.display_name.clone())
                                                                                .or_else(|| conv.username.clone())
                                                                                .or_else(|| pushname_map.get(chat_jid).cloned());

                                                                            let stored_chat = StoredChat {
                                                                                jid: chat_jid.to_string(),
                                                                                name: chat_name,
                                                                                unread_count: conv.unread_count.unwrap_or(0),
                                                                                last_message_timestamp: latest_ts,
                                                                                last_message_summary: latest_summary,
                                                                                pinned: is_pinned,
                                                                                archived: is_archived,
                                                                                mute_until,
                                                                                ephemeral_expiration: conv.ephemeral_expiration.unwrap_or(0),
                                                                                ephemeral_setting_timestamp: conv.ephemeral_setting_timestamp,
                                                                            };
                                                                            let _ = store_h.save_chat(&stored_chat).await;
                                                                        }

                                                                        events_h.emit(BaileysEvent::HistorySync {
                                                                            sync_type,
                                                                            progress,
                                                                            conversations: history.conversations,
                                                                            status_v3_messages: history.status_v3_messages,
                                                                        });
                                                                    }
                                                                    Err(e) => {
                                                                        warn!("Failed to download/decrypt HistorySync archive: {}", e);
                                                                    }
                                                                }
                                                            });
                                                        }
                                                    }

                                                    // Persist live inbound message in SQLite
                                                    let store_live = Arc::clone(store_clone);
                                                    let events_live = events_clone.clone();
                                                    let msg_for_db = msg_info.clone();
                                                    let anti_revoke_live = config_clone.anti_revoke;
                                                    tokio::spawn(async move {
                                                        let msg_id = msg_for_db.key.as_ref().and_then(|k| k.id.clone()).unwrap_or_default();
                                                        let chat_jid = msg_for_db.key.as_ref().and_then(|k| k.remote_jid.clone()).unwrap_or_default();
                                                        let author = msg_for_db.key.as_ref().and_then(|k| k.participant.clone()).unwrap_or_else(|| chat_jid.clone());
                                                        let from_me = msg_for_db.key.as_ref().and_then(|k| k.from_me).unwrap_or(false);
                                                        let ts = msg_for_db.message_timestamp.unwrap_or(0);

                                                        if let Some(ref m) = msg_for_db.message {
                                                            if let Some(parsed_pm) = crate::ephemeral::parse_protocol_message(m, &chat_jid) {
                                                                match parsed_pm {
                                                                    crate::ephemeral::ParsedProtocolMessage::MessageEdit {
                                                                        target_chat_jid,
                                                                        target_msg_id,
                                                                        new_text,
                                                                        edited_message,
                                                                    } => {
                                                                        let mut edited_bytes = Vec::new();
                                                                        let _ = edited_message.encode(&mut edited_bytes);
                                                                        let _ = store_live.update_message_content(&target_chat_jid, &target_msg_id, &new_text, Some(&edited_bytes)).await;
                                                                        events_live.emit(BaileysEvent::MessageEdited {
                                                                            chat_jid: target_chat_jid,
                                                                            message_id: target_msg_id,
                                                                            new_text,
                                                                        });
                                                                        return;
                                                                    }
                                                                    crate::ephemeral::ParsedProtocolMessage::Revoke {
                                                                        target_chat_jid,
                                                                        target_msg_id,
                                                                        participant,
                                                                    } => {
                                                                        let _ = store_live.revoke_message_ex(&target_chat_jid, &target_msg_id, anti_revoke_live).await;
                                                                        if anti_revoke_live {
                                                                            info!("[Anti-Revoke] Message {} deleted by {:?} in {} - Original preserved in SQLite", target_msg_id, participant, target_chat_jid);
                                                                        }
                                                                        events_live.emit(BaileysEvent::MessageRevoked {
                                                                            chat_jid: target_chat_jid,
                                                                            message_id: target_msg_id,
                                                                            participant,
                                                                        });
                                                                        return;
                                                                    }
                                                                    crate::ephemeral::ParsedProtocolMessage::EphemeralSetting {
                                                                        target_chat_jid,
                                                                        expiration,
                                                                        timestamp,
                                                                    } => {
                                                                        let _ = store_live.update_chat_ephemeral(&target_chat_jid, expiration, timestamp).await;
                                                                        events_live.emit(BaileysEvent::ChatEphemeralUpdate {
                                                                            chat_jid: target_chat_jid,
                                                                            ephemeral_expiration: expiration,
                                                                        });
                                                                        return;
                                                                    }
                                                                    crate::ephemeral::ParsedProtocolMessage::Other => {}
                                                                }
                                                            }
                                                        }

                                                        let (summary, has_media) = if let Some(ref m) = msg_for_db.message {
                                                            let s = crate::messages::extract_message_summary(m);
                                                            let h_m = crate::messages::message_has_media(m);
                                                            (s, h_m)
                                                        } else {
                                                            ("<empty>".to_string(), false)
                                                        };

                                                        let mut raw_bytes = Vec::new();
                                                        if let Some(ref m) = msg_for_db.message {
                                                            let _ = m.encode(&mut raw_bytes);
                                                        }

                                                        let stored_m = StoredMessage {
                                                            id: msg_id,
                                                            chat_jid: chat_jid.clone(),
                                                            sender_jid: author,
                                                            from_me,
                                                            timestamp: ts,
                                                            summary: summary.clone(),
                                                            status: "received".to_string(),
                                                            has_media,
                                                            proto_bytes: raw_bytes,
                                                            is_starred: false,
                                                            is_revoked: false,
                                                            is_edited: false,
                                                        };
                                                        let _ = store_live.save_message(&stored_m).await;

                                                        let stored_c = StoredChat {
                                                            jid: chat_jid,
                                                            name: None,
                                                            unread_count: if from_me { 0 } else { 1 },
                                                            last_message_timestamp: ts,
                                                            last_message_summary: Some(summary),
                                                            pinned: false,
                                                            archived: false,
                                                            mute_until: None,
                                                            ephemeral_expiration: 0,
                                                            ephemeral_setting_timestamp: None,
                                                        };
                                                        let _ = store_live.save_chat(&stored_c).await;
                                                    });

                                                    // Process incoming PollCreationMessage or PollUpdateMessage
                                                    let store_poll = Arc::clone(store_clone);
                                                    let events_poll = events_clone.clone();
                                                    let proto_for_poll = proto.clone();
                                                    let msg_id_poll = msg_id.clone();
                                                    let from_poll = from.clone();
                                                    let author_poll = author_jid.clone();

                                                    tokio::spawn(async move {
                                                        if let Some(poll) = proto_for_poll.poll_creation_message.as_ref()
                                                            .or(proto_for_poll.poll_creation_message_v2.as_ref())
                                                            .or(proto_for_poll.poll_creation_message_v3.as_ref()) {
                                                            let q = poll.name.clone().unwrap_or_else(|| "Poll".to_string());
                                                            let opts: Vec<String> = poll.options.iter().filter_map(|o| o.option_name.clone()).collect();
                                                            let enc_key = poll.enc_key.clone().unwrap_or_default();
                                                            let sel = poll.selectable_options_count.unwrap_or(1);
                                                            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                                                            let stored_poll = StoredPoll {
                                                                id: msg_id_poll.clone(),
                                                                chat_jid: from_poll.clone(),
                                                                sender_jid: author_poll.clone(),
                                                                question: q,
                                                                options: opts,
                                                                enc_key,
                                                                selectable_count: sel,
                                                                created_at: now,
                                                            };
                                                            let _ = store_poll.save_poll(&stored_poll).await;
                                                        }

                                                        if let Some(ref update) = proto_for_poll.poll_update_message {
                                                            if let (Some(ref key), Some(ref vote)) = (&update.poll_creation_message_key, &update.vote) {
                                                                if let Some(ref poll_id) = key.id {
                                                                    if let Ok(Some(stored_poll)) = store_poll.get_poll(poll_id).await {
                                                                        if let Ok(selected_hashes) = crate::polls::decrypt_poll_vote(
                                                                            vote,
                                                                            &stored_poll.sender_jid,
                                                                            poll_id,
                                                                            &stored_poll.enc_key,
                                                                            &author_poll,
                                                                        ) {
                                                                            let mut selected_options = Vec::new();
                                                                            for opt in &stored_poll.options {
                                                                                let h = crate::polls::hash_poll_option(opt);
                                                                                if selected_hashes.iter().any(|sh| sh.as_slice() == &h[..]) {
                                                                                    selected_options.push(opt.clone());
                                                                                }
                                                                            }
                                                                            let _ = store_poll.save_poll_vote(poll_id, &author_poll, &selected_options).await;
                                                                            if let Ok(Some(tally)) = store_poll.get_poll_tally(poll_id).await {
                                                                                events_poll.emit(BaileysEvent::PollVote {
                                                                                    poll_id: poll_id.clone(),
                                                                                    voter_jid: author_poll.clone(),
                                                                                    selected_options,
                                                                                    tally,
                                                                                });
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }

                                                        if let Some(ref ord) = proto_for_poll.order_message {
                                                            let order_id = ord.order_id.clone().unwrap_or_else(|| msg_id_poll.clone());
                                                            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                                                            let stored_order = StoredOrder {
                                                                order_id,
                                                                chat_jid: from_poll.clone(),
                                                                seller_jid: ord.seller_jid.clone().unwrap_or_else(|| author_poll.clone()),
                                                                item_count: ord.item_count.unwrap_or(1),
                                                                status: "INQUIRY".to_string(),
                                                                surface: ord.catalog_type.clone().or_else(|| Some("CATALOG".to_string())),
                                                                order_title: ord.order_title.clone(),
                                                                message: ord.message.clone(),
                                                                total_amount_1000: ord.total_amount1000.unwrap_or(0),
                                                                total_currency_code: ord.total_currency_code.clone().unwrap_or_else(|| "USD".to_string()),
                                                                token: ord.token.clone(),
                                                                created_at: now,
                                                            };
                                                            let _ = store_poll.save_order(&stored_order).await;
                                                        }
                                                    });

                                                    // Process incoming Status / Broadcast Stories
                                                    if from == "status@broadcast" {
                                                        let store_story = Arc::clone(store_clone);
                                                        let events_story = events_clone.clone();
                                                        let proto_for_story = proto.clone();
                                                        let msg_id_story = msg_id.clone();
                                                        let author_story = author_jid.clone();
                                                        let now_story = std::time::SystemTime::now()
                                                            .duration_since(std::time::UNIX_EPOCH)
                                                            .unwrap()
                                                            .as_secs();

                                                        tokio::spawn(async move {
                                                            let story = crate::status::parse_story_from_proto(
                                                                &msg_id_story,
                                                                &author_story,
                                                                None,
                                                                now_story,
                                                                &proto_for_story,
                                                            );
                                                            let _ = store_story.save_story(&story).await;
                                                            events_story.emit(BaileysEvent::StoryReceived { story });
                                                        });
                                                    }

                                                    events_clone.emit(BaileysEvent::Message {
                                                        message: Box::new(msg_info),
                                                    });

                                                }
                                            }
                                        }

                                        "receipt" => {
                                            let id = node.get_attr("id").unwrap_or("").to_string();
                                            let from = node.get_attr("from").unwrap_or("unknown").to_string();
                                            let receipt_type = node.get_attr("type").unwrap_or("delivery").to_string();

                                            // Send ACK for the receipt
                                            let ack = build_ack_node(&node, None, creds.me_jid.as_deref());
                                            let _ = socket.send_node(ack).await;

                                            // If recipient could not decrypt our sent message, re-encrypt & resend with updated keys
                                            if receipt_type == "retry" {
                                                let retry_client = client.clone();
                                                let creds_snap = creds.clone();
                                                let retry_node = node.clone();

                                                tokio::spawn(async move {
                                                    if let Err(e) = crate::retry::handle_outbound_retry_receipt(
                                                        &retry_node,
                                                        &creds_snap,
                                                        retry_client.store.as_ref(),
                                                        &retry_client.signal,
                                                        &retry_client.socket,
                                                        &retry_client.pending_iq,
                                                        &retry_client.recent_messages,
                                                        &retry_client.outbound_retries,
                                                    ).await {
                                                        tracing::warn!("Failed to process outbound retry receipt: {}", e);
                                                    }
                                                });
                                            }

                                            events_clone.emit(BaileysEvent::Receipt {
                                                key_id: id,
                                                remote_jid: from,
                                                status: receipt_type,
                                            });
                                        }

                                        "chatstate" => {
                                            let from = node.get_attr("from").unwrap_or("unknown").to_string();
                                            let participant = node.get_attr("participant").map(|s| s.to_string());
                                            let jid = participant.unwrap_or(from);

                                            let state = if node.get_child("composing").is_some() {
                                                "composing"
                                            } else if node.get_child("paused").is_some() {
                                                "paused"
                                            } else {
                                                "active"
                                            };

                                            let ack = build_ack_node(&node, None, creds.me_jid.as_deref());
                                            let _ = socket.send_node(ack).await;

                                            events_clone.emit(BaileysEvent::Presence {
                                                jid,
                                                presence: state.to_string(),
                                            });
                                        }

                                        "call" => {
                                            // 1. Send ACK immediately for the call node
                                            let ack = build_ack_node(&node, None, creds.me_jid.as_deref());
                                            let _ = socket.send_node(ack).await;

                                            // 2. Parse call stanza into CallEvent
                                            match crate::call::parse_call_stanza(&node) {
                                                Ok(mut call_event) => {
                                                    let call_id = call_event.id.clone();
                                                    let status = call_event.status.clone();

                                                    // 3. Correlate with active_calls cache
                                                    {
                                                        let mut active = active_calls_clone.lock().await;
                                                        if status == "offer" {
                                                            active.insert(call_id.clone(), call_event.clone());
                                                        } else if let Some(existing) = active.get(&call_id) {
                                                            call_event.is_video = existing.is_video;
                                                            call_event.is_group = existing.is_group;
                                                            if call_event.caller_pn.is_none() {
                                                                call_event.caller_pn = existing.caller_pn.clone();
                                                            }
                                                            if call_event.group_jid.is_none() {
                                                                call_event.group_jid = existing.group_jid.clone();
                                                            }
                                                        }

                                                        if status == "reject"
                                                            || status == "accept"
                                                            || status == "timeout"
                                                            || status == "terminate"
                                                        {
                                                            active.remove(&call_id);
                                                        }
                                                    }

                                                    // 4. Save call record to SQLite
                                                    let store_call = Arc::clone(store_clone);
                                                    let stored = StoredCall {
                                                        call_id: call_event.id.clone(),
                                                        chat_jid: call_event.chat_jid.clone(),
                                                        caller_jid: call_event.from.clone(),
                                                        is_group: call_event.is_group,
                                                        group_jid: call_event.group_jid.clone(),
                                                        is_video: call_event.is_video,
                                                        status: call_event.status.clone(),
                                                        timestamp: call_event.timestamp,
                                                        duration: None,
                                                    };
                                                    tokio::spawn(async move {
                                                        let _ = store_call.save_call(&stored).await;
                                                    });

                                                    // 5. Emit Call event to client
                                                    events_clone.emit(BaileysEvent::Call { call: call_event });
                                                }
                                                Err(e) => {
                                                    warn!("Failed to parse incoming call stanza: {}", e);
                                                }
                                            }
                                        }

                                        "ib" => {
                                            if node.get_child("offline_preview").is_some() {
                                                debug!("Received <ib><offline_preview/>, acknowledging offline batch count=100...");
                                                let reply = BinaryNode::new("ib")
                                                    .with_child(BinaryNode::new("offline_batch").with_attr("count", "100"));
                                                let _ = socket.send_node(reply).await;
                                            }

                                            if let Some(dirty_node) = node.get_child("dirty") {
                                                let dirty_type = dirty_node.get_attr("type").unwrap_or("account_sync").to_string();
                                                debug!("Received <ib><dirty type='{}'/>, cleaning dirty bit and scheduling AppState resync...", dirty_type);
                                                let client_sync = client.clone();
                                                let dirty_type_clone = dirty_type.clone();
                                                tokio::spawn(async move {
                                                    let _ = client_sync.clean_dirty_bits(&dirty_type_clone, None).await;
                                                    if let Err(e) = client_sync.resync_app_state(crate::syncd::ALL_COLLECTIONS, false).await {
                                                        debug!("Dirty bit AppState resync note: {}", e);
                                                    }
                                                });
                                            }
                                        }

                                        "stream:error" => {
                                            let code = node.get_attr("code");
                                            let is_conflict = node.get_child("conflict").is_some() || code == Some("440");
                                            if code == Some("405") {
                                                warn!("WhatsApp server returned 405 (Method Not Allowed / Outdated App Version). Invalidating cached version to force fresh resolution...");
                                                crate::version::invalidate_cached_version(store_clone.as_ref()).await;
                                                reconnect_delay = Duration::from_millis(500);
                                            } else if is_conflict {
                                                warn!("WhatsApp server sent connection conflict (replaced): {:?}. Backing off reconnect for 15s to allow old socket release...", node);
                                                reconnect_delay = Duration::from_secs(15);
                                            } else if code == Some("515") {
                                                info!("WhatsApp server sent restart required (515) after pairing. Reconnecting in 2.0s...");
                                                reconnect_delay = Duration::from_secs(2);
                                            } else {
                                                warn!("WhatsApp server sent <stream:error>: {:?}", node);
                                                reconnect_delay = Duration::from_millis(1500);
                                            }
                                            session_terminated = true;
                                        }

                                        "failure" => {
                                            let reason = node.get_attr("reason").unwrap_or("unknown");
                                            warn!("WhatsApp server sent <failure reason='{}'>", reason);
                                            if reason == "405" {
                                                warn!("WhatsApp login failed with 405 (Outdated client version). Invalidating cached version...");
                                                crate::version::invalidate_cached_version(store_clone.as_ref()).await;
                                            } else if reason == "401" {
                                                warn!("Authentication failed (401 Unauthorized): resetting invalid stored session.");
                                                let reset_creds = AuthCreds::init();
                                                let _ = store_clone.save_creds(&reset_creds).await;
                                                events_clone.emit(BaileysEvent::Disconnected {
                                                    reason: "Logged out: invalid session credentials (401)".into(),
                                                });
                                            }
                                            session_terminated = true;
                                        }

                                        "ack" => {
                                            let id = node.get_attr("id").unwrap_or_default().to_string();
                                            let class = node.get_attr("class").unwrap_or_default();
                                            let from = node.get_attr("from").unwrap_or_default().to_string();
                                            let participant = node.get_attr("participant").unwrap_or_default().to_string();
                                            let err_attr = node.get_attr("error")
                                                .or_else(|| node.get_child("error").and_then(|e| e.get_attr("code")));
                                            let remote_jid = if !participant.is_empty() {
                                                participant
                                            } else if !from.is_empty() {
                                                from.clone()
                                            } else {
                                                node.get_attr("to").unwrap_or_default().to_string()
                                            };

                                            if let Some(err_code) = err_attr {
                                                warn!("Server returned NACK for stanza id='{}', class='{}', error={}: {:?}", id, class, err_code, node);

                                                // Automatic recovery for Group Sender Key Desynchronization (Error 421)
                                                if err_code == "421" && (from.ends_with("@g.us") || remote_jid.ends_with("@g.us")) {
                                                    let grp_jid = if from.ends_with("@g.us") { from.clone() } else { remote_jid.clone() };
                                                    info!("Error 421 detected for group {}: invalidating desynchronized sender key and clearing memory cache to force fresh session creation", grp_jid);
                                                    {
                                                        let mut mem = client.sender_key_memory.lock().await;
                                                        mem.remove(&grp_jid);
                                                    }
                                                    let store_ack = Arc::clone(store_clone);
                                                    let grp_target = grp_jid.clone();
                                                    let me_jid_opt = creds.me_jid.clone();
                                                    let me_lid_opt = creds.me_lid.clone();
                                                    tokio::spawn(async move {
                                                        let mut keys_to_remove = Vec::new();
                                                        if let Some(ref jid) = me_jid_opt {
                                                            let (u, _) = jid.split_once('@').unwrap_or((jid.as_str(), ""));
                                                            let (u_clean, d) = u.split_once(':').unwrap_or((u, "0"));
                                                            keys_to_remove.push(format!("{}::{}:{}", grp_target, u_clean, d));
                                                            keys_to_remove.push(format!("{}::{}::{}", grp_target, u_clean, d));
                                                        }
                                                        if let Some(ref lid) = me_lid_opt {
                                                            let (u, _) = lid.split_once('@').unwrap_or((lid.as_str(), ""));
                                                            let (u_clean, d) = u.split_once(':').unwrap_or((u, "0"));
                                                            keys_to_remove.push(format!("{}::{}:{}", grp_target, u_clean, d));
                                                            keys_to_remove.push(format!("{}::{}::{}", grp_target, u_clean, d));
                                                        }
                                                        let _ = store_ack.remove_keys("sender-key", &keys_to_remove).await;
                                                    });
                                                }

                                                let store_ack = Arc::clone(store_clone);
                                                let id_clone = id.clone();
                                                let err_status = format!("error_{}", err_code);
                                                let err_status_clone = err_status.clone();
                                                tokio::spawn(async move {
                                                    let _ = store_ack.update_message_status(&id_clone, &err_status_clone).await;
                                                });
                                                events_clone.emit(BaileysEvent::Receipt {
                                                    key_id: id,
                                                    remote_jid,
                                                    status: err_status,
                                                });
                                            } else {
                                                info!("Server returned ACK for stanza id='{}', class='{}'", id, class);
                                                let store_ack = Arc::clone(store_clone);
                                                let id_clone = id.clone();
                                                tokio::spawn(async move {
                                                    let _ = store_ack.update_message_status(&id_clone, "server_ack").await;
                                                });
                                                events_clone.emit(BaileysEvent::Receipt {
                                                    key_id: id,
                                                    remote_jid,
                                                    status: "server_ack".to_string(),
                                                });
                                            }
                                        }

                                        _ => {
                                            debug!("Unhandled stanza node: tag='{}', attrs={:?}", node.tag, node.attrs);
                                        }
                                    }
                                }
                                None => {
                                    warn!("WhatsApp gateway connection closed by server.");
                                    session_terminated = true;
                                }
                            }
                        }
                    }
                }

                is_connected_clone.store(false, Ordering::SeqCst);
                client.set_connection_state(crate::state::ConnectionState::Disconnected);
                {
                    let mut lock = socket_holder.lock().await;
                    if let Some(s) = lock.take() {
                        s.close().await;
                    }
                }
                socket.close().await;

                if !is_running_clone.load(Ordering::SeqCst) {
                    debug!("Client connection supervisor stopped.");
                    break;
                }

                is_first_connection = false;
                events_clone.emit(BaileysEvent::Disconnected {
                    reason: "Connection reset by server, reconnecting...".into(),
                });

                let jitter = Duration::from_millis(rand::random::<u64>() % 1500);
                let effective_delay = reconnect_delay.max(Duration::from_secs(3)) + jitter;
                reconnect_delay = (reconnect_delay * 2).min(Duration::from_secs(30));
                client.set_connection_state(crate::state::ConnectionState::Reconnecting {
                    attempt: 1,
                    next_retry_millis: effective_delay.as_millis() as u64,
                });
                client.metrics().record_reconnect();
                warn!(
                    "Reconnecting to WhatsApp Web Gateway in {:.1}s...",
                    effective_delay.as_secs_f32()
                );
                tokio::time::sleep(effective_delay).await;
            }
        });

        Ok(())
    }

    /// Generates participant hash v2 (phash) for multi-device participant lists, matching WhatsApp Web & whatsmeow.
    fn generate_participant_hash_v2(participants: &[String]) -> String {
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

    /// Sends an arbitrary Protobuf `Message` (text, media, reaction, etc.) to a recipient or group.
    pub async fn send_message(
        &self,
        to_jid: &str,
        proto_msg: Message,
    ) -> Result<String, CoreError> {
        self.send_message_with_options(to_jid, proto_msg, MessageOptions::default())
            .await
    }

    /// Relays an arbitrary Protobuf `Message` with options (alias for `send_message_with_options`).
    pub async fn relay_message(
        &self,
        to_jid: &str,
        proto_msg: Message,
        options: MessageOptions,
    ) -> Result<String, CoreError> {
        self.send_message_with_options(to_jid, proto_msg, options)
            .await
    }

    /// Sends an arbitrary Protobuf `Message` with relay options (such as custom status broadcast audience).
    pub async fn send_message_with_options(
        &self,
        to_jid: &str,
        proto_msg: Message,
        options: MessageOptions,
    ) -> Result<String, CoreError> {
        let creds = self
            .store
            .get_creds()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?
            .ok_or_else(|| CoreError::Auth("No active session credentials found".into()))?;

        let socket_opt = {
            let lock = self.socket.lock().await;
            lock.clone()
        };

        if socket_opt.is_none() && !self.config.wa_websocket_url.starts_with("mock://") {
            return Err(CoreError::Network("Client not connected".into()));
        }

        let normalized_jid = if !to_jid.contains('@') {
            format!("{}@s.whatsapp.net", to_jid)
        } else {
            to_jid.to_string()
        };

        let rand_bytes: [u8; 9] = rand::random();
        let msg_id = format!("3EB0{}", hex::encode(rand_bytes).to_uppercase());

        // If chat has an active disappearing timer and this is not already an ephemeral or protocol message
        let mut proto_msg = if proto_msg.protocol_message.is_none()
            && proto_msg.ephemeral_message.is_none()
            && !normalized_jid.ends_with("@newsletter")
        {
            if let Ok(Some((exp, _))) = self.store.get_chat_ephemeral(&normalized_jid).await {
                if exp > 0 {
                    crate::ephemeral::wrap_ephemeral_message(proto_msg)
                } else {
                    proto_msg
                }
            } else {
                proto_msg
            }
        } else {
            proto_msg
        };

        // If message contains an unwrapped interactive_message, wrap it into view_once_message
        // with MessageContextInfo v2 as required by official WhatsApp clients.
        if proto_msg.interactive_message.is_some() {
            if let Some(inter) = proto_msg.interactive_message.take() {
                proto_msg = crate::interactive::wrap_interactive_view_once(*inter);
            }
        }

        // Ensure message_context_info has a message_secret (WhatsApp Web / Baileys parity)
        if proto_msg.reaction_message.is_none()
            && proto_msg.poll_update_message.is_none()
            && !normalized_jid.ends_with("@newsletter")
        {
            let mut ctx = proto_msg.message_context_info.unwrap_or_default();
            if ctx.message_secret.as_ref().is_none_or(|s| s.is_empty()) {
                let mut secret = vec![0u8; 32];
                rand::thread_rng().fill_bytes(&mut secret);
                ctx.message_secret = Some(secret);
            }
            proto_msg.message_context_info = Some(ctx);
        }

        // 1. Serialize Protobuf Message
        let mut msg_bytes = Vec::new();
        proto_msg.encode(&mut msg_bytes).map_err(|e| {
            CoreError::Protocol(format!("Failed to encode Message protobuf: {}", e))
        })?;

        let padded = pad_random_max16(&msg_bytes);

        // 2. Encrypt based on target type
        let msg_node = if normalized_jid.ends_with("@newsletter") {
            let (msg_type, media_type) = if proto_msg.image_message.is_some() {
                ("media", Some("image"))
            } else if proto_msg.video_message.is_some() {
                ("media", Some("video"))
            } else if proto_msg.audio_message.is_some() {
                ("media", Some("audio"))
            } else if proto_msg.document_message.is_some() {
                ("media", Some("document"))
            } else if proto_msg.sticker_message.is_some() {
                ("media", Some("image"))
            } else {
                ("text", None)
            };

            let mut pt_node = BinaryNode::new("plaintext").with_bytes_content(msg_bytes.clone());
            if let Some(mt) = media_type {
                pt_node = pt_node.with_attr("mediatype", mt);
            }

            let mut node = BinaryNode::new("message")
                .with_attr("to", &normalized_jid)
                .with_attr("id", &msg_id)
                .with_attr("type", msg_type);
            if let Some(ref handle) = options.media_handle {
                node = node.with_attr("media_id", handle);
            }
            node.with_child(pt_node)
        } else if normalized_jid.ends_with("@g.us") {
            // ── Baileys relayMessage parity for groups ──
            // 1. Resolve participants + addressing mode
            let meta = self.group_fetch_metadata(&normalized_jid).await.ok();
            let raw_participants: Vec<String> = meta
                .as_ref()
                .map(|m| m.participants.iter().map(|p| p.jid.clone()).collect())
                .unwrap_or_default();

            let is_lid_group = meta
                .as_ref()
                .and_then(|m| m.addressing_mode.as_deref())
                .map(|m| m == "lid")
                .unwrap_or_else(|| raw_participants.iter().any(|j| j.ends_with("@lid")));
            let addressing_mode = if is_lid_group { "lid" } else { "pn" };

            // 2. Sender identity must match group addressing
            let me_pn = creds
                .me_jid
                .clone()
                .unwrap_or_else(|| S_WHATSAPP_NET.to_string());
            let me_lid = creds.me_lid.clone();
            let group_sender_identity = if is_lid_group {
                creds.me_lid.clone().unwrap_or_else(|| me_pn.clone())
            } else {
                me_pn.clone()
            };

            // 3. Encrypt group payload -> ciphertext + root SKDM
            let (ciphertext, skdm_opt) = self
                .signal
                .encrypt_group_message(&normalized_jid, &group_sender_identity, &padded)
                .await
                .map_err(|e| CoreError::Crypto(e.to_string()))?;

            // 4. Target participants must ONLY use the domain of the group (no mixed lid/pn!)
            let mut target_users: Vec<String> = if let Some(ref m) = meta {
                m.participants
                    .iter()
                    .map(|p| {
                        if is_lid_group {
                            if p.jid.ends_with("@lid") {
                                baileys_binary::jid::jid_normalized_user(&p.jid)
                            } else if let Some(ref lid) = p.lid {
                                baileys_binary::jid::jid_normalized_user(lid)
                            } else {
                                baileys_binary::jid::jid_normalized_user(&p.jid)
                            }
                        } else {
                            if !p.jid.ends_with("@lid") {
                                baileys_binary::jid::jid_normalized_user(&p.jid)
                            } else if let Some(ref pn) = p.phone_number {
                                baileys_binary::jid::jid_normalized_user(pn)
                            } else {
                                baileys_binary::jid::jid_normalized_user(&p.jid)
                            }
                        }
                    })
                    .collect()
            } else {
                raw_participants
                    .into_iter()
                    .filter(|j| {
                        if is_lid_group {
                            j.ends_with("@lid")
                        } else {
                            !j.ends_with("@lid")
                        }
                    })
                    .map(|j| baileys_binary::jid::jid_normalized_user(&j))
                    .collect()
            };

            let own_target_user = baileys_binary::jid::jid_normalized_user(&group_sender_identity);
            if !target_users.contains(&own_target_user) {
                target_users.push(own_target_user);
            }

            // 5. Expand devices for each user (excluding hosted devices)
            let mut all_target_devices: Vec<String> = Vec::new();
            let mut all_group_devices: Vec<String> = Vec::new();
            for user in &target_users {
                let (u, server) = user.split_once('@').unwrap_or((
                    user.as_str(),
                    if is_lid_group {
                        "lid"
                    } else {
                        "s.whatsapp.net"
                    },
                ));
                let full_user_jid = format!("{}@{}", u, server);
                let devs = self
                    .get_user_devices(&full_user_jid)
                    .await
                    .unwrap_or_default();
                let devs = if devs.is_empty() { vec![0u32] } else { devs };
                for d in devs {
                    if d == 99 {
                        continue; // Hosted devices (device 99) never receive SKDM and not in phash
                    }
                    let dj = if d == 0 {
                        format!("{}@{}", u, server)
                    } else {
                        format!("{}:{}@{}", u, d, server)
                    };
                    if !all_group_devices.contains(&dj) {
                        all_group_devices.push(dj.clone());
                    }
                    // Exclude this exact sending device from SKDM distribution
                    let me_pn_exact = me_pn.as_str();
                    let me_lid_exact = me_lid.as_deref().unwrap_or("");
                    if dj.as_str() == me_pn_exact || dj.as_str() == me_lid_exact {
                        continue;
                    }
                    if !all_target_devices.contains(&dj) {
                        all_target_devices.push(dj);
                    }
                }
            }

            // 6. Check sender-key-memory: only send SKDM to devices that haven't received it yet
            let mut sender_key_recipients = Vec::new();
            {
                let mut mem = self.sender_key_memory.lock().await;
                let group_mem = mem.entry(normalized_jid.clone()).or_default();
                for dj in &all_target_devices {
                    if !group_mem.contains(dj) {
                        sender_key_recipients.push(dj.clone());
                    }
                }
            }

            let mut participant_nodes = Vec::new();
            let mut include_device_identity = false;

            if !sender_key_recipients.is_empty() {
                if let Some(skdm) = skdm_opt {
                    let skdm_msg = Message {
                        sender_key_distribution_message: Some(
                            baileys_proto::proto::message::SenderKeyDistributionMessage {
                                group_id: Some(normalized_jid.clone()),
                                axolotl_sender_key_distribution_message: Some(
                                    skdm.serialize().to_vec(),
                                ),
                            },
                        ),
                        ..Default::default()
                    };
                    let mut skdm_bytes = Vec::new();
                    skdm_msg.encode(&mut skdm_bytes).map_err(|e| {
                        CoreError::Protocol(format!("Failed to encode SKDM: {}", e))
                    })?;
                    let skdm_padded = pad_random_max16(&skdm_bytes);
                    let our_identity = KeyPair::try_from(&creds.signed_identity_key)
                        .map_err(|e| CoreError::Crypto(e.to_string()))?;

                    for dj in &sender_key_recipients {
                        if let Err(e) = self.assert_session(dj, &creds).await {
                            warn!("[SKDM] No session for {} ({}), skipping", dj, e);
                            continue;
                        }
                        match self
                            .signal
                            .encrypt_message(dj, &skdm_padded, &our_identity)
                            .await
                        {
                            Ok((enc_type, ct)) => {
                                self.metrics().record_signal_hit();
                                if enc_type == "pkmsg" {
                                    include_device_identity = true;
                                }
                                participant_nodes.push(
                                    BinaryNode::new("to").with_attr("jid", dj).with_child(
                                        BinaryNode::new("enc")
                                            .with_attr("v", "2")
                                            .with_attr("type", enc_type)
                                            .with_bytes_content(ct),
                                    ),
                                );
                            }
                            Err(e) => warn!("[SKDM] Encrypt failed for {}: {}", dj, e),
                        }
                    }

                    // Mark successfully encrypted devices in sender_key_memory
                    {
                        let mut mem = self.sender_key_memory.lock().await;
                        let group_mem = mem.entry(normalized_jid.clone()).or_default();
                        for node in &participant_nodes {
                            if let Some(j) = node.get_attr("jid") {
                                group_mem.insert(j.to_string());
                            }
                        }
                    }
                    debug!(
                        "[GROUP-SEND] {} -> SKDM distributed to {} device(s), addressing={}",
                        normalized_jid,
                        participant_nodes.len(),
                        addressing_mode
                    );
                }
            }

            // 7. Assemble message stanza (Whatsmeow / WA Web parity):
            // - phash attribute on <message>
            // - participants FIRST, device-identity SECOND, enc THIRD
            let phash = Self::generate_participant_hash_v2(&all_group_devices);
            let mut node = BinaryNode::new("message")
                .with_attr("to", &normalized_jid)
                .with_attr("id", &msg_id)
                .with_attr("type", "text")
                .with_attr("addressing_mode", addressing_mode)
                .with_attr("phash", phash);
            if let Some(exp) = meta
                .as_ref()
                .and_then(|m| m.ephemeral_duration)
                .filter(|e| *e > 0)
            {
                node = node.with_attr("expiration", exp.to_string());
            }
            if !participant_nodes.is_empty() {
                node = node
                    .with_child(BinaryNode::new("participants").with_children(participant_nodes));
            }
            if include_device_identity {
                if let Some(ref acc) = creds.account {
                    node = node.with_child(
                        BinaryNode::new("device-identity").with_bytes_content(acc.clone()),
                    );
                }
            }
            node = node.with_child(
                BinaryNode::new("enc")
                    .with_attr("v", "2")
                    .with_attr("type", "skmsg")
                    .with_bytes_content(ciphertext),
            );
            node
        } else if normalized_jid == "status@broadcast" {
            // ── Full Signal Protocol Parity for status@broadcast Stories ──
            let our_identity = KeyPair::try_from(&creds.signed_identity_key)
                .map_err(|e| CoreError::Crypto(e.to_string()))?;

            let me_pn = creds
                .me_jid
                .clone()
                .unwrap_or_else(|| S_WHATSAPP_NET.to_string());
            let sender_identity = me_pn.clone();

            // 1. Determine message type & media type (Baileys Parity: getMessageType & getMediaType)
            let (msg_type, media_type) = if proto_msg.image_message.is_some() {
                ("media", Some("image"))
            } else if let Some(ref vid) = proto_msg.video_message {
                let mt = if vid.gif_playback.unwrap_or(false) {
                    "gif"
                } else {
                    "video"
                };
                ("media", Some(mt))
            } else if let Some(ref aud) = proto_msg.audio_message {
                let mt = if aud.ptt.unwrap_or(false) {
                    "ptt"
                } else {
                    "audio"
                };
                ("media", Some(mt))
            } else {
                ("text", None)
            };

            // 2. Encrypt group payload -> ciphertext + root SKDM
            let (ciphertext, skdm_opt) = self
                .signal
                .encrypt_group_message(&normalized_jid, &sender_identity, &padded)
                .await
                .map_err(|e| CoreError::Crypto(e.to_string()))?;

            // 3. Resolve target recipients (our own phone :0 & companion devices + status audience)
            let mut target_users: Vec<String> = Vec::new();
            let own_pn_user = baileys_binary::jid::jid_normalized_user(&sender_identity);
            target_users.push(own_pn_user);
            // CRITICAL: DO NOT add own_lid_user! Status broadcast is strictly @s.whatsapp.net domain.

            // Populate external contacts from options.status_jid_list or local store
            if let Some(ref custom_audience) = options.status_jid_list {
                for jid in custom_audience {
                    let norm = if !jid.contains('@') {
                        format!("{}@s.whatsapp.net", jid)
                    } else {
                        jid.clone()
                    };
                    let norm_user = baileys_binary::jid::jid_normalized_user(&norm);
                    if !target_users.contains(&norm_user) {
                        target_users.push(norm_user);
                    }
                }
            } else {
                // No explicit audience: load stored contacts/chats (capped to 50 for bounded stanza size)
                if let Ok(chats) = self.store.get_chats().await {
                    for c in chats {
                        if c.jid.ends_with("@s.whatsapp.net") {
                            let norm = baileys_binary::jid::jid_normalized_user(&c.jid);
                            if !target_users.contains(&norm) {
                                target_users.push(norm);
                                if target_users.len() >= 51 {
                                    break;
                                }
                            }
                        }
                    }
                }
                if target_users.len() < 2 {
                    if let Ok(contacts) = self.store.get_contacts().await {
                        for c in contacts {
                            if c.jid.ends_with("@s.whatsapp.net") {
                                let norm = baileys_binary::jid::jid_normalized_user(&c.jid);
                                if !target_users.contains(&norm) {
                                    target_users.push(norm);
                                    if target_users.len() >= 51 {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 4. Expand devices for each user (excluding this exact sending companion session)
            let mut all_target_devices: Vec<String> = Vec::new();
            for user in &target_users {
                let (u, server) = user
                    .split_once('@')
                    .unwrap_or((user.as_str(), "s.whatsapp.net"));
                let devs = self.get_user_devices(user).await.unwrap_or_default();
                let devs = if devs.is_empty() { vec![0u32] } else { devs };
                for d in devs {
                    let dj = if d == 0 {
                        format!("{}@{}", u, server)
                    } else {
                        format!("{}:{}@{}", u, d, server)
                    };
                    if dj == me_pn {
                        continue;
                    }
                    if !all_target_devices.contains(&dj) {
                        all_target_devices.push(dj);
                    }
                }
            }

            // 5. Baileys Parity: status@broadcast NEVER caches in sender_key_memory!
            // In Baileys (messages-send.ts: if (!participant && !isStatus)), status broadcasts
            // must distribute the SKDM to all target devices on EVERY update so every story
            // (text, link preview, HD media) can be decrypted by all devices.
            let sender_key_recipients = all_target_devices.clone();

            let mut participant_nodes = Vec::new();
            let mut include_device_identity = false;

            if !sender_key_recipients.is_empty() {
                if let Some(skdm) = skdm_opt {
                    let skdm_msg = Message {
                        sender_key_distribution_message: Some(
                            baileys_proto::proto::message::SenderKeyDistributionMessage {
                                group_id: Some(normalized_jid.clone()),
                                axolotl_sender_key_distribution_message: Some(
                                    skdm.serialize().to_vec(),
                                ),
                            },
                        ),
                        ..Default::default()
                    };
                    let mut skdm_bytes = Vec::new();
                    skdm_msg.encode(&mut skdm_bytes).map_err(|e| {
                        CoreError::Protocol(format!("Failed to encode status SKDM: {}", e))
                    })?;
                    let skdm_padded = pad_random_max16(&skdm_bytes);

                    for dj in &sender_key_recipients {
                        if let Err(e) = self.assert_session(dj, &creds).await {
                            warn!("[STATUS-SKDM] No session for {} ({}), skipping", dj, e);
                            continue;
                        }
                        match self
                            .signal
                            .encrypt_message(dj, &skdm_padded, &our_identity)
                            .await
                        {
                            Ok((enc_type, ct)) => {
                                if enc_type == "pkmsg" {
                                    include_device_identity = true;
                                }
                                participant_nodes.push(
                                    BinaryNode::new("to").with_attr("jid", dj).with_child(
                                        BinaryNode::new("enc")
                                            .with_attr("v", "2")
                                            .with_attr("type", enc_type)
                                            .with_bytes_content(ct),
                                    ),
                                );
                            }
                            Err(e) => warn!("[STATUS-SKDM] Encrypt failed for {}: {}", dj, e),
                        }
                    }

                    debug!(
                        "[STATUS-SEND] status@broadcast -> SKDM distributed to {} device(s)",
                        participant_nodes.len()
                    );
                }
            }

            // 6. Assemble status message stanza (Baileys Parity: NO addressing_mode, dynamic type and mediatype)
            let mut enc_node = BinaryNode::new("enc")
                .with_attr("v", "2")
                .with_attr("type", "skmsg")
                .with_bytes_content(ciphertext);
            if let Some(mt) = media_type {
                enc_node = enc_node.with_attr("mediatype", mt);
            }

            let mut node = BinaryNode::new("message")
                .with_attr("to", &normalized_jid)
                .with_attr("id", &msg_id)
                .with_attr("type", msg_type);
            node = node.with_child(enc_node);

            if !participant_nodes.is_empty() {
                node = node
                    .with_child(BinaryNode::new("participants").with_children(participant_nodes));
            }
            if include_device_identity {
                if let Some(ref acc) = creds.account {
                    node = node.with_child(
                        BinaryNode::new("device-identity").with_bytes_content(acc.clone()),
                    );
                }
            }
            node
        } else {
            // ── Baileys relayMessage parity for 1-on-1 chats ──
            let our_identity = KeyPair::try_from(&creds.signed_identity_key)
                .map_err(|e| CoreError::Crypto(e.to_string()))?;

            let me_pn = creds
                .me_jid
                .clone()
                .unwrap_or_else(|| S_WHATSAPP_NET.to_string());
            let me_lid = creds.me_lid.clone();

            let target_user_norm_init = baileys_binary::jid::jid_normalized_user(&normalized_jid);
            let own_pn_norm = baileys_binary::jid::jid_normalized_user(&me_pn);
            let own_lid_norm = me_lid
                .as_deref()
                .map(baileys_binary::jid::jid_normalized_user);

            let is_note_to_self = target_user_norm_init == own_pn_norm
                || (own_lid_norm.is_some()
                    && Some(&target_user_norm_init) == own_lid_norm.as_ref());

            // If note-to-self and LID is available, normalize conversation to LID for WhatsApp Web sync parity!
            let normalized_jid = if is_note_to_self {
                own_lid_norm.as_ref().unwrap_or(&normalized_jid).clone()
            } else {
                normalized_jid.clone()
            };

            let is_lid = normalized_jid.ends_with("@lid");
            let sender_identity = if is_lid {
                me_lid.as_ref().cloned().unwrap_or_else(|| me_pn.clone())
            } else {
                me_pn.clone()
            };

            let own_user_norm = baileys_binary::jid::jid_normalized_user(&sender_identity);
            let target_user_norm = baileys_binary::jid::jid_normalized_user(&normalized_jid);

            // 1. Enumerate own companion devices (excluding this exact sending device)
            let (own_u, own_server) = own_user_norm.split_once('@').unwrap_or((
                own_user_norm.as_str(),
                if is_lid { "lid" } else { "s.whatsapp.net" },
            ));
            let own_devs = self
                .get_user_devices(&own_user_norm)
                .await
                .unwrap_or_default();
            let own_devs = if own_devs.is_empty() {
                vec![0u32]
            } else {
                own_devs
            };

            let mut me_recipients: Vec<String> = Vec::new();
            for d in own_devs {
                let dj = if d == 0 {
                    format!("{}@{}", own_u, own_server)
                } else {
                    format!("{}:{}@{}", own_u, d, own_server)
                };
                if dj == me_pn || me_lid.as_deref() == Some(dj.as_str()) {
                    continue; // Skip exact sending device
                }
                if !me_recipients.contains(&dj) {
                    me_recipients.push(dj);
                }
            }

            // 2. Enumerate other recipient devices (empty if note-to-self)
            let mut other_recipients: Vec<String> = Vec::new();
            if !is_note_to_self {
                let (tgt_u, tgt_server) = target_user_norm.split_once('@').unwrap_or((
                    target_user_norm.as_str(),
                    if is_lid { "lid" } else { "s.whatsapp.net" },
                ));
                let tgt_devs = self
                    .get_user_devices(&target_user_norm)
                    .await
                    .unwrap_or_default();
                let tgt_devs = if tgt_devs.is_empty() {
                    vec![0u32]
                } else {
                    tgt_devs
                };
                for d in tgt_devs {
                    let dj = if d == 0 {
                        format!("{}@{}", tgt_u, tgt_server)
                    } else {
                        format!("{}:{}@{}", tgt_u, d, tgt_server)
                    };
                    if !other_recipients.contains(&dj) {
                        other_recipients.push(dj);
                    }
                }
            }

            let mut all_recipients = me_recipients.clone();
            for r in &other_recipients {
                if !all_recipients.contains(r) {
                    all_recipients.push(r.clone());
                }
            }

            let phash = Self::generate_participant_hash_v2(&all_recipients);

            // Determine mediatype attribute for Baileys/WhatsApp parity
            let media_type_attr = if proto_msg.image_message.is_some() {
                Some("image")
            } else if proto_msg.sticker_message.is_some() {
                Some("sticker")
            } else if proto_msg.document_message.is_some() {
                Some("document")
            } else if proto_msg.video_message.is_some() {
                Some("video")
            } else if proto_msg.audio_message.is_some() {
                Some("audio")
            } else {
                None
            };

            // 3. Encrypt for own devices with DeviceSentMessage
            let mut participant_nodes = Vec::new();
            let mut include_device_identity = false;

            if !me_recipients.is_empty() {
                let dsm_proto = Message {
                    device_sent_message: Some(Box::new(
                        baileys_proto::proto::message::DeviceSentMessage {
                            destination_jid: Some(normalized_jid.clone()),
                            message: Some(Box::new(proto_msg.clone())),
                            phash: Some(phash.clone()),
                        },
                    )),
                    message_context_info: proto_msg.message_context_info.clone(),
                    ..Default::default()
                };

                let mut dsm_bytes = Vec::new();
                if dsm_proto.encode(&mut dsm_bytes).is_ok() {
                    let dsm_padded = pad_random_max16(&dsm_bytes);
                    for r_jid in &me_recipients {
                        if self.assert_session(r_jid, &creds).await.is_ok() {
                            match self
                                .signal
                                .encrypt_message(r_jid, &dsm_padded, &our_identity)
                                .await
                            {
                                Ok((enc_type, ct)) => {
                                    self.metrics().record_signal_hit();
                                    if enc_type == "pkmsg" {
                                        include_device_identity = true;
                                    }
                                    let mut enc_node = BinaryNode::new("enc")
                                        .with_attr("v", "2")
                                        .with_attr("type", enc_type);
                                    if let Some(mt) = media_type_attr {
                                        enc_node = enc_node.with_attr("mediatype", mt);
                                    }
                                    enc_node = enc_node.with_bytes_content(ct);
                                    participant_nodes.push(
                                        BinaryNode::new("to")
                                            .with_attr("jid", r_jid)
                                            .with_child(enc_node),
                                    );
                                }
                                Err(_) => {
                                    self.metrics().record_signal_miss();
                                }
                            }
                        }
                    }
                }
            }

            // 4. Encrypt for other recipients with original message
            for r_jid in &other_recipients {
                if self.assert_session(r_jid, &creds).await.is_ok() {
                    match self
                        .signal
                        .encrypt_message(r_jid, &padded, &our_identity)
                        .await
                    {
                        Ok((enc_type, ct)) => {
                            self.metrics().record_signal_hit();
                            if enc_type == "pkmsg" {
                                include_device_identity = true;
                            }
                            let mut enc_node = BinaryNode::new("enc")
                                .with_attr("v", "2")
                                .with_attr("type", enc_type);
                            if let Some(mt) = media_type_attr {
                                enc_node = enc_node.with_attr("mediatype", mt);
                            }
                            enc_node = enc_node.with_bytes_content(ct);
                            participant_nodes.push(
                                BinaryNode::new("to")
                                    .with_attr("jid", r_jid)
                                    .with_child(enc_node),
                            );
                        }
                        Err(_) => {
                            self.metrics().record_signal_miss();
                        }
                    }
                }
            }

            if !participant_nodes.is_empty() {
                let mut node = BinaryNode::new("message")
                    .with_attr("to", &normalized_jid)
                    .with_attr("id", &msg_id)
                    .with_attr("type", "text")
                    .with_child(BinaryNode::new("participants").with_children(participant_nodes));
                if include_device_identity {
                    if let Some(ref acc) = creds.account {
                        node = node.with_child(
                            BinaryNode::new("device-identity").with_bytes_content(acc.clone()),
                        );
                    }
                }
                node
            } else {
                // Direct 1-to-1 fallback if no multi-device targets found
                self.assert_session(&normalized_jid, &creds).await?;
                let (enc_type, ciphertext) = match self
                    .signal
                    .encrypt_message(&normalized_jid, &padded, &our_identity)
                    .await
                {
                    Ok(res) => {
                        self.metrics().record_signal_hit();
                        res
                    }
                    Err(e) => {
                        self.metrics().record_signal_miss();
                        return Err(CoreError::Crypto(e.to_string()));
                    }
                };

                let mut enc_node = BinaryNode::new("enc")
                    .with_attr("type", enc_type)
                    .with_attr("v", "2");
                if let Some(mt) = media_type_attr {
                    enc_node = enc_node.with_attr("mediatype", mt);
                }
                enc_node = enc_node.with_bytes_content(ciphertext);

                BinaryNode::new("message")
                    .with_attr("to", &normalized_jid)
                    .with_attr("id", &msg_id)
                    .with_attr("type", "text")
                    .with_child(enc_node)
            }
        };

        // Attach reporting token (v2) if message_secret is present
        let mut final_msg_node = msg_node;

        // WhatsApp Protocol parity: Set edit attribute for REVOKE / EDIT messages
        if let Some(ref proto) = proto_msg.protocol_message {
            if proto.r#type
                == Some(baileys_proto::proto::message::protocol_message::Type::Revoke as i32)
            {
                let is_group_admin_delete = normalized_jid.ends_with("@g.us")
                    && proto.key.as_ref().and_then(|k| k.from_me) == Some(false);
                let edit_val = if is_group_admin_delete { "8" } else { "7" };
                final_msg_node = final_msg_node.with_attr("edit", edit_val);
            } else if proto.r#type
                == Some(baileys_proto::proto::message::protocol_message::Type::MessageEdit as i32)
            {
                final_msg_node = final_msg_node.with_attr("edit", "1");
            }
        }
        if let Some(ref ctx) = proto_msg.message_context_info {
            if let Some(ref sec) = ctx.message_secret {
                if !sec.is_empty()
                    && !normalized_jid.ends_with("@newsletter")
                    && normalized_jid != "status@broadcast"
                {
                    let is_lid_dest = normalized_jid.ends_with("@lid")
                        || (normalized_jid.ends_with("@g.us")
                            && final_msg_node.get_attr("addressing_mode") == Some("lid"));
                    let sender_jid = if is_lid_dest && creds.me_lid.is_some() {
                        baileys_binary::jid::jid_normalized_user(creds.me_lid.as_deref().unwrap())
                    } else if let Some(ref me) = creds.me_jid {
                        baileys_binary::jid::jid_normalized_user(me)
                    } else if let Some(ref lid) = creds.me_lid {
                        baileys_binary::jid::jid_normalized_user(lid)
                    } else {
                        String::new()
                    };

                    if !sender_jid.is_empty() {
                        if let Ok(Some(rep_node)) = crate::reporting::get_message_reporting_token(
                            &msg_bytes,
                            &msg_id,
                            &sender_jid,
                            &normalized_jid,
                            sec,
                        ) {
                            debug!(
                                "[REPORTING] Attached reporting token for msg {} (from={}, to={})",
                                msg_id, sender_jid, normalized_jid
                            );
                            final_msg_node = final_msg_node.with_child(rep_node);
                        }
                    }
                }
            }
        }

        // WhatsApp Protocol parity: Attach caller-specified additional_nodes
        if let Some(ref extra_nodes) = options.additional_nodes {
            for node in extra_nodes {
                final_msg_node = final_msg_node.with_child(node.clone());
            }
        }

        // WhatsApp Protocol parity: Inject <biz> and <bot> XML nodes for interactive messages
        let already_has_biz = final_msg_node.get_child("biz").is_some();
        if !already_has_biz {
            if let Some(biz_kind) = crate::biz::detect_biz_kind(&proto_msg) {
                let is_group = normalized_jid.ends_with("@g.us");
                let is_broadcast = normalized_jid.ends_with("@broadcast");
                let is_newsletter = normalized_jid.ends_with("@newsletter");

                if is_group || (!is_broadcast && !is_newsletter) {
                    final_msg_node =
                        final_msg_node.with_child(crate::biz::build_biz_node(&biz_kind));

                    if !is_group && !is_broadcast && !is_newsletter {
                        let has_bot = final_msg_node
                            .get_children()
                            .iter()
                            .any(|c| c.tag == "bot" && c.get_attr("biz_bot") == Some("1"));
                        if !has_bot {
                            final_msg_node =
                                final_msg_node.with_child(crate::biz::build_bot_node());
                        }
                    }
                }
            }
        }

        // 3. Send over socket if connected
        if let Some(socket) = socket_opt {
            info!(
                "Sending outbound message node to '{}' (id: {}): type={}, children={}",
                normalized_jid,
                msg_id,
                final_msg_node.get_attr("type").unwrap_or(""),
                final_msg_node.get_children().len()
            );
            debug!("Outbound message stanza: {:?}", final_msg_node);
            self.metrics().record_msg_out();
            socket.send_node(final_msg_node).await?;
        }

        // 4. Cache & emit Message event
        let msg_info = WebMessageInfo {
            key: Some(MessageKey {
                remote_jid: Some(normalized_jid.clone()),
                from_me: Some(true),
                id: Some(msg_id.clone()),
                participant: None,
            }),
            message: Some(proto_msg),
            message_timestamp: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            ),
            status: Some(baileys_proto::proto::web_message_info::Status::Pending as i32),
            ..Default::default()
        };

        {
            let mut cache = self.recent_messages.lock().await;
            if cache.len() > 500 {
                cache.clear();
            }
            cache.insert(msg_id.clone(), msg_info.clone());
        }

        // Persist sent message & update chat in SQLite
        let (summary, has_media) = if let Some(ref m) = msg_info.message {
            let s = crate::messages::extract_message_summary(m);
            let h_m = crate::messages::message_has_media(m);
            (s, h_m)
        } else {
            ("<empty>".to_string(), false)
        };

        let stored_m = StoredMessage {
            id: msg_id.clone(),
            chat_jid: normalized_jid.clone(),
            sender_jid: creds.me_jid.clone().unwrap_or_else(|| "me".to_string()),
            from_me: true,
            timestamp: msg_info.message_timestamp.unwrap_or(0),
            summary: summary.clone(),
            status: "pending".to_string(),
            has_media,
            proto_bytes: msg_bytes,
            is_starred: false,
            is_revoked: false,
            is_edited: false,
        };
        let _ = self.store.save_message(&stored_m).await;

        let stored_c = StoredChat {
            jid: normalized_jid.clone(),
            name: None,
            unread_count: 0,
            last_message_timestamp: msg_info.message_timestamp.unwrap_or(0),
            last_message_summary: Some(summary),
            pinned: false,
            archived: false,
            mute_until: None,
            ephemeral_expiration: 0,
            ephemeral_setting_timestamp: None,
        };
        let _ = self.store.save_chat(&stored_c).await;

        self.events.emit(BaileysEvent::Message {
            message: Box::new(msg_info),
        });

        Ok(msg_id)
    }

    /// Sends a plain text message to a user or group over the encrypted WhatsApp connection.
    pub async fn send_text_message(&self, to_jid: &str, text: &str) -> Result<String, CoreError> {
        let proto_msg = Message {
            conversation: Some(text.to_string()),
            ..Default::default()
        };
        self.send_message(to_jid, proto_msg).await
    }

    /// Sends a message constructed from dynamic JSON (e.g. from frontend builders or plugins).
    pub async fn send_message_json(
        &self,
        to_jid: &str,
        message_json: &serde_json::Value,
    ) -> Result<String, CoreError> {
        let proto_msg =
            crate::messages::parse_message_from_json(message_json).map_err(CoreError::Protocol)?;
        self.send_message(to_jid, proto_msg).await
    }

    /// Forwards an existing protobuf message to a recipient, setting is_forwarded and forwarding_score.
    pub async fn forward_message(
        &self,
        to_jid: &str,
        mut msg: Message,
        is_forwarded: bool,
        score: u32,
    ) -> Result<String, CoreError> {
        if let Some(ref mut ext) = msg.extended_text_message {
            let ctx = ext.context_info.get_or_insert_with(Default::default);
            ctx.is_forwarded = Some(is_forwarded);
            ctx.forwarding_score = Some(score);
        } else if let Some(ref mut img) = msg.image_message {
            let ctx = img.context_info.get_or_insert_with(Default::default);
            ctx.is_forwarded = Some(is_forwarded);
            ctx.forwarding_score = Some(score);
        } else if let Some(ref mut vid) = msg.video_message {
            let ctx = vid.context_info.get_or_insert_with(Default::default);
            ctx.is_forwarded = Some(is_forwarded);
            ctx.forwarding_score = Some(score);
        } else if let Some(ref mut aud) = msg.audio_message {
            let ctx = aud.context_info.get_or_insert_with(Default::default);
            ctx.is_forwarded = Some(is_forwarded);
            ctx.forwarding_score = Some(score);
        } else if let Some(ref mut doc) = msg.document_message {
            let ctx = doc.context_info.get_or_insert_with(Default::default);
            ctx.is_forwarded = Some(is_forwarded);
            ctx.forwarding_score = Some(score);
        } else if let Some(ref mut stk) = msg.sticker_message {
            let ctx = stk.context_info.get_or_insert_with(Default::default);
            ctx.is_forwarded = Some(is_forwarded);
            ctx.forwarding_score = Some(score);
        } else if let Some(conv) = msg.conversation.take() {
            let ctx = ContextInfo {
                is_forwarded: Some(is_forwarded),
                forwarding_score: Some(score),
                ..Default::default()
            };
            msg.extended_text_message = Some(Box::new(ExtendedTextMessage {
                text: Some(conv),
                context_info: Some(Box::new(ctx)),
                ..Default::default()
            }));
        }

        self.send_message(to_jid, msg).await
    }

    /// Pins or unpins a message in a chat using WhatsApp's PinInChatMessage protocol.
    pub async fn pin_message(
        &self,
        chat_jid: &str,
        msg_id: &str,
        pin: bool,
        _duration_secs: u32,
    ) -> Result<String, CoreError> {
        let normalized_jid = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid)
        } else {
            chat_jid.to_string()
        };

        let from_me = if let Ok(Some(m)) = self.store.get_message(msg_id).await {
            m.from_me
        } else {
            false
        };

        let pin_type = if pin { 1 } else { 2 };
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let proto_msg = Message {
            pin_in_chat_message: Some(baileys_proto::proto::message::PinInChatMessage {
                key: Some(MessageKey {
                    remote_jid: Some(normalized_jid.clone()),
                    from_me: Some(from_me),
                    id: Some(msg_id.to_string()),
                    participant: None,
                }),
                r#type: Some(pin_type),
                sender_timestamp_ms: Some(now_ms),
            }),
            ..Default::default()
        };

        self.send_message(&normalized_jid, proto_msg).await
    }

    /// Sends an emoji reaction to a specific message.
    pub async fn send_reaction(
        &self,
        to_jid: &str,
        target_msg_id: &str,
        emoji: &str,
    ) -> Result<String, CoreError> {
        let normalized_jid = if !to_jid.contains('@') {
            format!("{}@s.whatsapp.net", to_jid)
        } else {
            to_jid.to_string()
        };

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let proto_msg = Message {
            reaction_message: Some(baileys_proto::proto::message::ReactionMessage {
                key: Some(MessageKey {
                    remote_jid: Some(normalized_jid.clone()),
                    from_me: None,
                    id: Some(target_msg_id.to_string()),
                    participant: None,
                }),
                text: Some(emoji.to_string()),
                sender_timestamp_ms: Some(now_ms),
                ..Default::default()
            }),
            ..Default::default()
        };

        self.send_message(&normalized_jid, proto_msg).await
    }

    /// Sends a presence update (e.g. "available", "unavailable", "composing", "paused").
    pub async fn send_presence_update(
        &self,
        presence: &str,
        to_jid_opt: Option<&str>,
    ) -> Result<(), CoreError> {
        let socket_opt = {
            let lock = self.socket.lock().await;
            lock.clone()
        };
        let socket = socket_opt.ok_or_else(|| CoreError::Network("Client not connected".into()))?;

        let mut node = BinaryNode::new("presence").with_attr("type", presence);

        if let Some(to) = to_jid_opt {
            let normalized = if !to.contains('@') {
                format!("{}@s.whatsapp.net", to)
            } else {
                to.to_string()
            };
            node = node.with_attr("to", &normalized);
        }

        self.metrics().record_msg_out();
        socket.send_node(node).await
    }

    /// Returns a recently received or sent message by ID.
    pub async fn get_recent_message(&self, msg_id: &str) -> Option<WebMessageInfo> {
        let cache = self.recent_messages.lock().await;
        cache.get(msg_id).cloned()
    }

    /// Toggles auto-download for incoming media.
    pub fn set_auto_download(&self, enabled: bool) {
        self.auto_download.store(enabled, Ordering::SeqCst);
    }

    /// Checks if auto-download is enabled.
    pub fn is_auto_download(&self) -> bool {
        self.auto_download.load(Ordering::SeqCst)
    }

    /// Downloads and decrypts the media attachment for a cached or database message ID, saving to dest_dir.
    pub async fn download_media_for_message(
        &self,
        msg_id: &str,
        dest_dir: &std::path::Path,
    ) -> Result<std::path::PathBuf, CoreError> {
        let proto = {
            let cache = self.recent_messages.lock().await;
            cache.get(msg_id).and_then(|info| info.message.clone())
        };

        let proto = match proto {
            Some(p) => p,
            None => {
                // Persistent store fallback: search SQLite database
                match self.store.get_message(msg_id).await {
                    Ok(Some(stored)) => {
                        if !stored.proto_bytes.is_empty() {
                            Message::decode(&stored.proto_bytes[..]).map_err(|e| {
                                CoreError::Protocol(format!(
                                    "Failed to decode protobuf for message {}: {}",
                                    msg_id, e
                                ))
                            })?
                        } else {
                            return Err(CoreError::Protocol(format!(
                                "Message '{}' in database has empty protobuf body",
                                msg_id
                            )));
                        }
                    }
                    _ => {
                        return Err(CoreError::Protocol(format!(
                            "Message ID '{}' not found in recent cache or database",
                            msg_id
                        )));
                    }
                }
            }
        };

        let attachment = baileys_media::extract_media_from_message(&proto).ok_or_else(|| {
            CoreError::Media("Message does not contain any media attachment".into())
        })?;

        baileys_media::download_media_to_dir(&attachment, dest_dir, msg_id)
            .await
            .map_err(|e| CoreError::Media(e.to_string()))
    }

    /// Downloads and decrypts the media attachment for a message using the streaming pipeline,
    /// writing incrementally to disk with ~64KB memory usage and emitting real-time progress events.
    pub async fn download_message_media_to_path(
        &self,
        msg_id: &str,
        target_path: Option<&std::path::Path>,
    ) -> Result<std::path::PathBuf, CoreError> {
        let proto = {
            let cache = self.recent_messages.lock().await;
            cache.get(msg_id).and_then(|info| info.message.clone())
        };

        let proto = match proto {
            Some(p) => p,
            None => match self.store.get_message(msg_id).await {
                Ok(Some(stored)) => {
                    if !stored.proto_bytes.is_empty() {
                        Message::decode(&stored.proto_bytes[..]).map_err(|e| {
                            CoreError::Protocol(format!(
                                "Failed to decode protobuf for message {}: {}",
                                msg_id, e
                            ))
                        })?
                    } else {
                        return Err(CoreError::Protocol(format!(
                            "Message '{}' in database has empty protobuf body",
                            msg_id
                        )));
                    }
                }
                _ => {
                    return Err(CoreError::Protocol(format!(
                        "Message ID '{}' not found in recent cache or database",
                        msg_id
                    )));
                }
            },
        };

        let attachment = baileys_media::extract_media_from_message(&proto).ok_or_else(|| {
            CoreError::Media("Message does not contain any media attachment".into())
        })?;

        let dest_file_path = if let Some(p) = target_path {
            p.to_path_buf()
        } else {
            let ext = attachment
                .media_type
                .default_extension(attachment.mimetype.as_deref());
            let filename = if let Some(ref name) = attachment.filename {
                let clean = baileys_media::sanitize_filename(name);
                format!("{}_{}", msg_id, clean)
            } else {
                format!("{}.{}", msg_id, ext)
            };
            std::path::PathBuf::from("downloads").join(filename)
        };

        let download_url = attachment
            .download_url()
            .map_err(|e| CoreError::Media(e.to_string()))?;

        let events = self.events.clone();
        let task_id = msg_id.to_string();
        let progress_cb: baileys_media::MediaProgressCallback =
            std::sync::Arc::new(move |bytes_transferred, total_bytes, percentage| {
                events.emit(BaileysEvent::MediaProgress {
                    task_id: task_id.clone(),
                    direction: "download".to_string(),
                    bytes_transferred,
                    total_bytes,
                    percentage,
                });
            });

        match baileys_media::download_media_streaming(
            &download_url,
            &attachment.media_key,
            attachment.media_type,
            &dest_file_path,
            Some(progress_cb.clone()),
        )
        .await
        {
            Ok(p) => Ok(p),
            Err(err) => {
                // Fallback to DEF_MEDIA_HOST if direct_path exists
                if let Some(ref dp) = attachment.direct_path {
                    let fallback_url = if dp.starts_with('/') {
                        format!("https://{}{}", baileys_media::DEF_MEDIA_HOST, dp)
                    } else {
                        format!("https://{}/{}", baileys_media::DEF_MEDIA_HOST, dp)
                    };
                    if fallback_url != download_url {
                        baileys_media::download_media_streaming(
                            &fallback_url,
                            &attachment.media_key,
                            attachment.media_type,
                            &dest_file_path,
                            Some(progress_cb),
                        )
                        .await
                        .map_err(|e| CoreError::Media(e.to_string()))
                    } else {
                        Err(CoreError::Media(err.to_string()))
                    }
                } else {
                    Err(CoreError::Media(err.to_string()))
                }
            }
        }
    }

    /// Fetches or returns cached WhatsApp media CDN connection info (`media_conn`).
    pub async fn refresh_media_conn(
        &self,
        force: bool,
    ) -> Result<baileys_media::MediaConnInfo, CoreError> {
        if !force {
            let lock = self.media_conn.lock().await;
            if let Some(ref conn) = *lock {
                if !conn.is_expired() {
                    return Ok(conn.clone());
                }
            }
        }

        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", S_WHATSAPP_NET)
            .with_attr("xmlns", "w:m")
            .with_attr("type", "set")
            .with_attr("id", &iq_id)
            .with_child(BinaryNode::new("media_conn"));

        let resp = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        let media_conn_node = resp.get_child("media_conn").ok_or_else(|| {
            CoreError::Protocol("Server response missing <media_conn> node".into())
        })?;

        let auth = media_conn_node
            .get_attr("auth")
            .ok_or_else(|| CoreError::Protocol("Missing auth attribute in media_conn".into()))?
            .to_string();

        let ttl = media_conn_node
            .get_attr("ttl")
            .and_then(|t| t.parse::<u64>().ok())
            .unwrap_or(43200);

        let hosts = media_conn_node
            .get_children()
            .iter()
            .filter(|c| c.tag == "host")
            .filter_map(|h| {
                let hostname = h.get_attr("hostname")?.to_string();
                let max_len = h
                    .get_attr("maxContentLengthBytes")
                    .and_then(|l| l.parse::<u64>().ok());
                Some(baileys_media::MediaConnHost {
                    hostname,
                    max_content_length_bytes: max_len,
                })
            })
            .collect();

        let conn_info = baileys_media::MediaConnInfo {
            auth,
            ttl,
            hosts,
            fetched_at: std::time::SystemTime::now(),
        };

        {
            let mut lock = self.media_conn.lock().await;
            *lock = Some(conn_info.clone());
        }

        Ok(conn_info)
    }

    /// Encrypts, uploads, and sends an image message to a user or group.
    pub async fn send_image_message(
        &self,
        to_jid: &str,
        image_bytes: &[u8],
        caption: Option<&str>,
    ) -> Result<String, CoreError> {
        let encrypted =
            baileys_media::encrypt_media_bytes(image_bytes, None, baileys_media::MediaType::Image)
                .map_err(|e| CoreError::Media(e.to_string()))?;

        let media_conn = self.refresh_media_conn(false).await?;

        let upload_res = baileys_media::upload_encrypted_media(
            &encrypted.encrypted_bytes,
            &encrypted.file_enc_sha256,
            baileys_media::MediaType::Image,
            &media_conn,
        )
        .await
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let proto_msg = Message {
            image_message: Some(Box::new(baileys_proto::proto::message::ImageMessage {
                url: upload_res.url,
                mimetype: Some("image/jpeg".to_string()),
                caption: caption.map(|s| s.to_string()),
                file_sha256: Some(encrypted.file_sha256.to_vec()),
                file_length: Some(image_bytes.len() as u64),
                media_key: Some(encrypted.media_key.to_vec()),
                file_enc_sha256: Some(encrypted.file_enc_sha256.to_vec()),
                direct_path: upload_res.direct_path,
                ..Default::default()
            })),
            ..Default::default()
        };

        self.send_message(to_jid, proto_msg).await
    }

    /// Revokes (deletes for everyone) a previously sent message.
    pub async fn send_revoke_message(
        &self,
        to_jid: &str,
        target_msg_id: &str,
    ) -> Result<String, CoreError> {
        let normalized_jid = if !to_jid.contains('@') {
            format!("{}@s.whatsapp.net", to_jid)
        } else {
            to_jid.to_string()
        };

        let proto_msg = Message {
            protocol_message: Some(Box::new(baileys_proto::proto::message::ProtocolMessage {
                key: Some(MessageKey {
                    remote_jid: Some(normalized_jid.clone()),
                    from_me: Some(true),
                    id: Some(target_msg_id.to_string()),
                    participant: None,
                }),
                r#type: Some(baileys_proto::proto::message::protocol_message::Type::Revoke as i32),
                ..Default::default()
            })),
            ..Default::default()
        };

        self.send_message(&normalized_jid, proto_msg).await
    }

    /// Sends a text message with a forged quoted context from any sender.
    pub async fn send_fake_quoted_message(
        &self,
        to_jid: &str,
        reply_text: &str,
        fake_sender_jid: &str,
        fake_quoted_text: &str,
    ) -> Result<String, CoreError> {
        let normalized_jid = if !to_jid.contains('@') {
            format!("{}@s.whatsapp.net", to_jid)
        } else {
            to_jid.to_string()
        };
        let normalized_fake_sender = if !fake_sender_jid.contains('@') {
            format!("{}@s.whatsapp.net", fake_sender_jid)
        } else {
            fake_sender_jid.to_string()
        };
        let chat_context = if normalized_jid.ends_with("@g.us") {
            Some(normalized_jid.as_str())
        } else {
            None
        };

        let proto_msg = crate::proto_builders::build_fake_quoted_message(
            reply_text,
            &normalized_fake_sender,
            fake_quoted_text,
            chat_context,
        );

        self.send_message(&normalized_jid, proto_msg).await
    }

    /// Encrypts, uploads, and sends an audio buffer as a native WhatsApp Voice Note (PTT) with waveform.
    pub async fn send_voice_note(
        &self,
        to_jid: &str,
        audio_bytes: &[u8],
        seconds: u32,
        waveform: Option<&[u8]>,
    ) -> Result<String, CoreError> {
        let normalized_jid = if !to_jid.contains('@') {
            format!("{}@s.whatsapp.net", to_jid)
        } else {
            to_jid.to_string()
        };

        let encrypted =
            baileys_media::encrypt_media_bytes(audio_bytes, None, baileys_media::MediaType::Audio)
                .map_err(|e| CoreError::Media(e.to_string()))?;

        let media_conn = self.refresh_media_conn(false).await?;

        let upload_res = baileys_media::upload_encrypted_media(
            &encrypted.encrypted_bytes,
            &encrypted.file_enc_sha256,
            baileys_media::MediaType::Audio,
            &media_conn,
        )
        .await
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let proto_msg = crate::proto_builders::build_voice_note_message(
            upload_res.url,
            upload_res.direct_path,
            encrypted.media_key.to_vec(),
            encrypted.file_sha256.to_vec(),
            encrypted.file_enc_sha256.to_vec(),
            audio_bytes.len() as u64,
            seconds,
            waveform,
            Some("audio/ogg; codecs=opus"),
        );

        self.send_message(&normalized_jid, proto_msg).await
    }

    /// Sends a read receipt for a received message.
    pub async fn send_read_receipt(
        &self,
        remote_jid: &str,
        msg_id: &str,
        participant: Option<&str>,
    ) -> Result<(), CoreError> {
        if self.is_ghost_mode() {
            debug!(
                "[Ghost Mode] Suppressed read receipt for message {} in {}",
                msg_id, remote_jid
            );
            return Ok(());
        }

        let socket_opt = {
            let lock = self.socket.lock().await;
            lock.clone()
        };
        let socket = socket_opt.ok_or_else(|| CoreError::Network("Client not connected".into()))?;

        let mut node = BinaryNode::new("receipt")
            .with_attr("id", msg_id)
            .with_attr("to", remote_jid)
            .with_attr("type", "read");

        if let Some(part) = participant {
            node = node.with_attr("participant", part);
        }

        self.metrics().record_msg_out();
        socket.send_node(node).await
    }

    /// Queries group metadata (participants, subject, owner, creation time) from WhatsApp server.
    pub async fn group_metadata(&self, group_jid: &str) -> Result<BinaryNode, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", group_jid)
            .with_attr("xmlns", "w:g2")
            .with_attr("type", "get")
            .with_attr("id", &iq_id)
            .with_child(BinaryNode::new("query").with_attr("request", "interactive"));

        self.send_iq(iq_node, Duration::from_secs(10)).await
    }

    /// Creates a new WhatsApp group with the given subject and initial participants.
    /// Returns the created group's JID (e.g. "120363xxx@g.us").
    pub async fn group_create(
        &self,
        subject: &str,
        participants: &[String],
    ) -> Result<String, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let key = format!("3EB0{:X}", rand::random::<u64>());
        let mut create_node = BinaryNode::new("create")
            .with_attr("subject", subject)
            .with_attr("key", &key);

        for p in participants {
            let p_norm = baileys_binary::jid::jid_normalized_user(p);
            create_node =
                create_node.with_child(BinaryNode::new("participant").with_attr("jid", &p_norm));
        }

        let iq_node = BinaryNode::new("iq")
            .with_attr("to", "@g.us")
            .with_attr("xmlns", "w:g2")
            .with_attr("type", "set")
            .with_attr("id", &iq_id)
            .with_child(create_node);

        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        let group_node = res.get_child("group").unwrap_or(&res);
        if let Some(id) = group_node.get_attr("id") {
            let jid = if id.contains('@') {
                id.to_string()
            } else {
                format!("{}@g.us", id)
            };
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let stored_chat = baileys_store::StoredChat {
                jid: jid.clone(),
                name: Some(subject.to_string()),
                unread_count: 0,
                last_message_timestamp: now,
                last_message_summary: None,
                pinned: false,
                archived: false,
                mute_until: None,
                ephemeral_expiration: 0,
                ephemeral_setting_timestamp: None,
            };
            let _ = self.store.save_chat(&stored_chat).await;
            return Ok(jid);
        }
        Err(CoreError::Protocol(
            "Group creation failed: no group id returned".into(),
        ))
    }

    /// Updates participants in a group ("add", "remove", "promote", "demote").
    /// Returns a list of (participant_jid, status_code).
    pub async fn group_participants_update(
        &self,
        group_jid: &str,
        participants: &[String],
        action: &str,
    ) -> Result<Vec<(String, String)>, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let mut action_node = BinaryNode::new(action);

        for p in participants {
            let p_jid = if !p.contains('@') {
                format!("{}@s.whatsapp.net", p)
            } else {
                p.clone()
            };
            action_node =
                action_node.with_child(BinaryNode::new("participant").with_attr("jid", &p_jid));
        }

        let iq_node = BinaryNode::new("iq")
            .with_attr("to", group_jid)
            .with_attr("xmlns", "w:g2")
            .with_attr("type", "set")
            .with_attr("id", &iq_id)
            .with_child(action_node);

        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        let mut results = Vec::new();
        if let Some(act_res) = res.get_child(action) {
            for child in act_res.get_children() {
                if child.tag == "participant" {
                    let jid = child.get_attr("jid").unwrap_or("").to_string();
                    let code = child
                        .get_attr("error")
                        .or_else(|| child.get_attr("code"))
                        .unwrap_or("200")
                        .to_string();
                    results.push((jid, code));
                }
            }
        }
        Ok(results)
    }

    /// Changes the subject/title of a group.
    pub async fn group_update_subject(
        &self,
        group_jid: &str,
        subject: &str,
    ) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", group_jid)
            .with_attr("xmlns", "w:g2")
            .with_attr("type", "set")
            .with_attr("id", &iq_id)
            .with_child(BinaryNode::new("subject").with_content(subject.as_bytes().to_vec()));

        let res = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        if res.get_attr("type") == Some("result") {
            Ok(())
        } else {
            Err(CoreError::Protocol(format!(
                "Failed to update group subject: {:?}",
                res
            )))
        }
    }

    /// Changes the description of a group.
    pub async fn group_update_description(
        &self,
        group_jid: &str,
        description: &str,
    ) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let desc_id = format!("3EB0{:X}", rand::random::<u64>());

        let desc_node = BinaryNode::new("description")
            .with_attr("id", &desc_id)
            .with_child(BinaryNode::new("body").with_content(description.as_bytes().to_vec()));

        let iq_node = BinaryNode::new("iq")
            .with_attr("to", group_jid)
            .with_attr("xmlns", "w:g2")
            .with_attr("type", "set")
            .with_attr("id", &iq_id)
            .with_child(desc_node);

        let res = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        if res.get_attr("type") == Some("result") {
            Ok(())
        } else {
            Err(CoreError::Protocol(format!(
                "Failed to update group description: {:?}",
                res
            )))
        }
    }

    /// Fetches the group invite code (used in https://chat.whatsapp.com/<code>).
    pub async fn group_get_invite_code(&self, group_jid: &str) -> Result<String, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", group_jid)
            .with_attr("xmlns", "w:g2")
            .with_attr("type", "get")
            .with_attr("id", &iq_id)
            .with_child(BinaryNode::new("invite"));

        let res = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        if let Some(invite) = res.get_child("invite") {
            if let Some(code) = invite.get_attr("code") {
                return Ok(code.to_string());
            }
        }
        Err(CoreError::Protocol(
            "Failed to retrieve group invite code".into(),
        ))
    }

    /// Accepts a group invite using an invite code, joining the group.
    /// Returns the joined group's JID.
    pub async fn group_accept_invite(&self, code: &str) -> Result<String, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", "@g.us")
            .with_attr("xmlns", "w:g2")
            .with_attr("type", "set")
            .with_attr("id", &iq_id)
            .with_child(BinaryNode::new("invite").with_attr("code", code));

        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        if let Some(group) = res.get_child("group") {
            if let Some(jid) = group.get_attr("jid") {
                return Ok(jid.to_string());
            }
        }
        Err(CoreError::Protocol(
            "Failed to accept group invite code".into(),
        ))
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

        let resp = self.send_iq(query_iq, Duration::from_secs(10)).await?;

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

    /// Sends a text message quoting/replying to a previous message.
    pub async fn send_text_message_with_quote(
        &self,
        to_jid: &str,
        text: &str,
        quoted_id: &str,
        quoted_participant: Option<&str>,
        quoted_text: Option<&str>,
    ) -> Result<String, CoreError> {
        let normalized_jid = if !to_jid.contains('@') {
            format!("{}@s.whatsapp.net", to_jid)
        } else {
            to_jid.to_string()
        };

        let quoted_msg = quoted_text.map(|qt| {
            Box::new(Message {
                conversation: Some(qt.to_string()),
                ..Default::default()
            })
        });

        let context_info = Box::new(ContextInfo {
            stanza_id: Some(quoted_id.to_string()),
            participant: quoted_participant.map(|p| p.to_string()),
            quoted_message: quoted_msg,
            remote_jid: Some(normalized_jid.clone()),
            ..Default::default()
        });

        let ext_msg = ExtendedTextMessage {
            text: Some(text.to_string()),
            context_info: Some(context_info),
            ..Default::default()
        };

        let proto_msg = Message {
            extended_text_message: Some(Box::new(ext_msg)),
            ..Default::default()
        };

        self.send_message(&normalized_jid, proto_msg).await
    }

    /// Encrypts, uploads, and sends a video message to a user or group.
    pub async fn send_video_message(
        &self,
        to_jid: &str,
        video_bytes: &[u8],
        caption: Option<&str>,
        seconds: Option<u32>,
        gif_playback: bool,
    ) -> Result<String, CoreError> {
        let encrypted =
            baileys_media::encrypt_media_bytes(video_bytes, None, baileys_media::MediaType::Video)
                .map_err(|e| CoreError::Media(e.to_string()))?;

        let media_conn = self.refresh_media_conn(false).await?;

        let upload_res = baileys_media::upload_encrypted_media(
            &encrypted.encrypted_bytes,
            &encrypted.file_enc_sha256,
            baileys_media::MediaType::Video,
            &media_conn,
        )
        .await
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let proto_msg = Message {
            video_message: Some(Box::new(VideoMessage {
                url: upload_res.url,
                mimetype: Some("video/mp4".to_string()),
                caption: caption.map(|s| s.to_string()),
                file_sha256: Some(encrypted.file_sha256.to_vec()),
                file_length: Some(video_bytes.len() as u64),
                seconds,
                gif_playback: Some(gif_playback),
                media_key: Some(encrypted.media_key.to_vec()),
                file_enc_sha256: Some(encrypted.file_enc_sha256.to_vec()),
                direct_path: upload_res.direct_path,
                ..Default::default()
            })),
            ..Default::default()
        };

        self.send_message(to_jid, proto_msg).await
    }

    /// Encrypts, uploads, and sends an audio message or voice note (PTT) to a user or group.
    pub async fn send_audio_message(
        &self,
        to_jid: &str,
        audio_bytes: &[u8],
        ptt: bool,
        seconds: Option<u32>,
    ) -> Result<String, CoreError> {
        let encrypted =
            baileys_media::encrypt_media_bytes(audio_bytes, None, baileys_media::MediaType::Audio)
                .map_err(|e| CoreError::Media(e.to_string()))?;

        let media_conn = self.refresh_media_conn(false).await?;

        let upload_res = baileys_media::upload_encrypted_media(
            &encrypted.encrypted_bytes,
            &encrypted.file_enc_sha256,
            baileys_media::MediaType::Audio,
            &media_conn,
        )
        .await
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let proto_msg = Message {
            audio_message: Some(Box::new(AudioMessage {
                url: upload_res.url,
                mimetype: Some("audio/ogg; codecs=opus".to_string()),
                file_sha256: Some(encrypted.file_sha256.to_vec()),
                file_length: Some(audio_bytes.len() as u64),
                seconds,
                ptt: Some(ptt),
                media_key: Some(encrypted.media_key.to_vec()),
                file_enc_sha256: Some(encrypted.file_enc_sha256.to_vec()),
                direct_path: upload_res.direct_path,
                ..Default::default()
            })),
            ..Default::default()
        };

        self.send_message(to_jid, proto_msg).await
    }

    /// Encrypts, uploads, and sends a document/file message to a user or group.
    pub async fn send_document_message(
        &self,
        to_jid: &str,
        document_bytes: &[u8],
        file_name: &str,
        mimetype: &str,
        title: Option<&str>,
    ) -> Result<String, CoreError> {
        let encrypted = baileys_media::encrypt_media_bytes(
            document_bytes,
            None,
            baileys_media::MediaType::Document,
        )
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let media_conn = self.refresh_media_conn(false).await?;

        let upload_res = baileys_media::upload_encrypted_media(
            &encrypted.encrypted_bytes,
            &encrypted.file_enc_sha256,
            baileys_media::MediaType::Document,
            &media_conn,
        )
        .await
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let proto_msg = Message {
            document_message: Some(Box::new(DocumentMessage {
                url: upload_res.url,
                mimetype: Some(mimetype.to_string()),
                title: title.map(|s| s.to_string()),
                file_sha256: Some(encrypted.file_sha256.to_vec()),
                file_length: Some(document_bytes.len() as u64),
                media_key: Some(encrypted.media_key.to_vec()),
                file_name: Some(file_name.to_string()),
                file_enc_sha256: Some(encrypted.file_enc_sha256.to_vec()),
                direct_path: upload_res.direct_path,
                ..Default::default()
            })),
            ..Default::default()
        };

        self.send_message(to_jid, proto_msg).await
    }

    /// Encrypts, uploads, and sends a sticker message to a user or group.
    pub async fn send_sticker_message(
        &self,
        to_jid: &str,
        sticker_bytes: &[u8],
        is_animated: bool,
    ) -> Result<String, CoreError> {
        let encrypted = baileys_media::encrypt_media_bytes(
            sticker_bytes,
            None,
            baileys_media::MediaType::Sticker,
        )
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let media_conn = self.refresh_media_conn(false).await?;

        let upload_res = baileys_media::upload_encrypted_media(
            &encrypted.encrypted_bytes,
            &encrypted.file_enc_sha256,
            baileys_media::MediaType::Sticker,
            &media_conn,
        )
        .await
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let proto_msg = Message {
            sticker_message: Some(Box::new(StickerMessage {
                url: upload_res.url,
                mimetype: Some("image/webp".to_string()),
                file_sha256: Some(encrypted.file_sha256.to_vec()),
                file_length: Some(sticker_bytes.len() as u64),
                is_animated: Some(is_animated),
                media_key: Some(encrypted.media_key.to_vec()),
                file_enc_sha256: Some(encrypted.file_enc_sha256.to_vec()),
                direct_path: upload_res.direct_path,
                ..Default::default()
            })),
            ..Default::default()
        };

        self.send_message(to_jid, proto_msg).await
    }

    /// Sends a local media file using the streaming upload pipeline (zero-OOM spooling for >= 10MB)
    /// and emits real-time BaileysEvent::MediaProgress events.
    pub async fn send_media_file_streaming(
        &self,
        chat_jid: &str,
        file_path: &std::path::Path,
        media_type: baileys_media::MediaType,
        caption: Option<&str>,
    ) -> Result<String, CoreError> {
        let normalized_jid = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid.trim_start_matches('+'))
        } else {
            chat_jid.to_string()
        };

        let media_conn = self.refresh_media_conn(false).await?;
        let task_id = format!("upload_{:x}", rand::random::<u64>());

        let events = self.events.clone();
        let tid = task_id.clone();
        let progress_cb: baileys_media::MediaProgressCallback =
            std::sync::Arc::new(move |bytes_transferred, total_bytes, percentage| {
                events.emit(BaileysEvent::MediaProgress {
                    task_id: tid.clone(),
                    direction: "upload".to_string(),
                    bytes_transferred,
                    total_bytes,
                    percentage,
                });
            });

        let stream_res = baileys_media::upload_media_file_streaming(
            file_path,
            media_type,
            &media_conn,
            Some(progress_cb),
        )
        .await
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let file_name = file_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".to_string());

        let proto_msg = match media_type {
            baileys_media::MediaType::Image => Message {
                image_message: Some(Box::new(baileys_proto::proto::message::ImageMessage {
                    url: stream_res.upload_res.url,
                    mimetype: Some("image/jpeg".to_string()),
                    caption: caption.map(|s| s.to_string()),
                    file_sha256: Some(stream_res.file_sha256.to_vec()),
                    file_length: Some(stream_res.file_length),
                    media_key: Some(stream_res.media_key.to_vec()),
                    file_enc_sha256: Some(stream_res.file_enc_sha256.to_vec()),
                    direct_path: stream_res.upload_res.direct_path,
                    ..Default::default()
                })),
                ..Default::default()
            },
            baileys_media::MediaType::Video => Message {
                video_message: Some(Box::new(baileys_proto::proto::message::VideoMessage {
                    url: stream_res.upload_res.url,
                    mimetype: Some("video/mp4".to_string()),
                    caption: caption.map(|s| s.to_string()),
                    file_sha256: Some(stream_res.file_sha256.to_vec()),
                    file_length: Some(stream_res.file_length),
                    media_key: Some(stream_res.media_key.to_vec()),
                    file_enc_sha256: Some(stream_res.file_enc_sha256.to_vec()),
                    direct_path: stream_res.upload_res.direct_path,
                    ..Default::default()
                })),
                ..Default::default()
            },
            baileys_media::MediaType::Audio => Message {
                audio_message: Some(Box::new(baileys_proto::proto::message::AudioMessage {
                    url: stream_res.upload_res.url,
                    mimetype: Some("audio/ogg; codecs=opus".to_string()),
                    file_sha256: Some(stream_res.file_sha256.to_vec()),
                    file_length: Some(stream_res.file_length),
                    media_key: Some(stream_res.media_key.to_vec()),
                    file_enc_sha256: Some(stream_res.file_enc_sha256.to_vec()),
                    direct_path: stream_res.upload_res.direct_path,
                    ..Default::default()
                })),
                ..Default::default()
            },
            baileys_media::MediaType::Document | baileys_media::MediaType::History => Message {
                document_message: Some(Box::new(baileys_proto::proto::message::DocumentMessage {
                    url: stream_res.upload_res.url,
                    mimetype: Some("application/octet-stream".to_string()),
                    title: caption.map(|s| s.to_string()),
                    file_sha256: Some(stream_res.file_sha256.to_vec()),
                    file_length: Some(stream_res.file_length),
                    media_key: Some(stream_res.media_key.to_vec()),
                    file_name: Some(file_name),
                    file_enc_sha256: Some(stream_res.file_enc_sha256.to_vec()),
                    direct_path: stream_res.upload_res.direct_path,
                    ..Default::default()
                })),
                ..Default::default()
            },
            baileys_media::MediaType::Sticker => Message {
                sticker_message: Some(Box::new(baileys_proto::proto::message::StickerMessage {
                    url: stream_res.upload_res.url,
                    mimetype: Some("image/webp".to_string()),
                    file_sha256: Some(stream_res.file_sha256.to_vec()),
                    file_length: Some(stream_res.file_length),
                    media_key: Some(stream_res.media_key.to_vec()),
                    file_enc_sha256: Some(stream_res.file_enc_sha256.to_vec()),
                    direct_path: stream_res.upload_res.direct_path,
                    ..Default::default()
                })),
                ..Default::default()
            },
        };

        self.send_message(&normalized_jid, proto_msg).await
    }

    /// Leaves a WhatsApp group.
    pub async fn group_leave(&self, group_jid: &str) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let leave_node = BinaryNode::new("leave")
            .with_child(BinaryNode::new("group").with_attr("id", group_jid));
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", "@g.us")
            .with_attr("xmlns", "w:g2")
            .with_attr("type", "set")
            .with_attr("id", &iq_id)
            .with_child(leave_node);

        self.send_iq(iq_node, Duration::from_secs(10)).await?;
        Ok(())
    }

    /// Updates group settings ("announcement", "not_announcement", "locked", "unlocked").
    pub async fn group_setting_update(
        &self,
        group_jid: &str,
        setting: &str,
    ) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", group_jid)
            .with_attr("xmlns", "w:g2")
            .with_attr("type", "set")
            .with_attr("id", &iq_id)
            .with_child(BinaryNode::new(setting));

        self.send_iq(iq_node, Duration::from_secs(10)).await?;
        Ok(())
    }

    /// Revokes the current invite code of a group and generates a new one.
    pub async fn group_revoke_invite(&self, group_jid: &str) -> Result<String, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", group_jid)
            .with_attr("xmlns", "w:g2")
            .with_attr("type", "set")
            .with_attr("id", &iq_id)
            .with_child(BinaryNode::new("invite"));

        let res = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        if let Some(invite) = res.get_child("invite") {
            if let Some(code) = invite.get_attr("code") {
                return Ok(code.to_string());
            }
        }
        Err(CoreError::Protocol(
            "Failed to revoke group invite code".into(),
        ))
    }

    /// Queries metadata for a group invite code (before joining).
    pub async fn group_get_invite_info(&self, code: &str) -> Result<BinaryNode, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", "@g.us")
            .with_attr("xmlns", "w:g2")
            .with_attr("type", "get")
            .with_attr("id", &iq_id)
            .with_child(BinaryNode::new("invite").with_attr("code", code));

        self.send_iq(iq_node, Duration::from_secs(10)).await
    }

    /// Fetches the user's current privacy settings from WhatsApp gateway.
    pub async fn fetch_privacy_settings(
        &self,
    ) -> Result<crate::privacy::PrivacySettings, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = crate::privacy::build_privacy_query_iq(&iq_id);
        let res = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        Ok(crate::privacy::parse_privacy_settings_response(&res))
    }

    /// Updates a privacy setting (category: "readreceipts", "profile", "status", "online", "last", "groupadd", "calladd").
    pub async fn update_privacy_setting(
        &self,
        category: &str,
        value: &str,
    ) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = crate::privacy::build_privacy_update_iq(&iq_id, category, value);
        self.send_iq(iq_node, Duration::from_secs(10)).await?;
        Ok(())
    }

    /// Fetches the list of blocked JIDs from WhatsApp server and syncs to local SQLite store.
    pub async fn fetch_blocklist(&self) -> Result<Vec<String>, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = crate::privacy::build_fetch_blocklist_iq(&iq_id);
        let res = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        let list = crate::privacy::parse_blocklist_response(&res);
        let _ = self.store.set_blocked_contacts(&list).await;
        Ok(list)
    }

    /// Retrieves all locally cached blocked contacts from SQLite.
    pub async fn get_blocked_contacts(&self) -> Result<Vec<String>, CoreError> {
        self.store
            .get_blocked_contacts()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }

    /// Blocks or unblocks a contact ("block" or "unblock") and persists to SQLite.
    pub async fn update_block_status(&self, jid: &str, action: &str) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let normalized = if !jid.contains('@') {
            format!("{}@s.whatsapp.net", jid.trim_start_matches('+'))
        } else {
            jid.to_string()
        };

        // If contact has LID/PN stored, pass pn_jid
        let pn_opt = if normalized.ends_with("@lid") {
            if let Ok(Some(c)) = self.store.get_contact(&normalized).await {
                c.lid.clone()
            } else {
                None
            }
        } else {
            None
        };

        let iq_node =
            crate::privacy::build_block_iq(&iq_id, &normalized, action, pn_opt.as_deref());
        self.send_iq(iq_node, Duration::from_secs(10)).await?;

        // Update local SQLite store
        if action == "block" || action == "add" {
            let _ = self.store.save_blocked_contact(&normalized).await;
        } else if action == "unblock" || action == "remove" {
            let _ = self.store.remove_blocked_contact(&normalized).await;
        }

        Ok(())
    }

    /// Updates your WhatsApp About / Bio status string.
    pub async fn update_profile_status(&self, status: &str) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = crate::privacy::build_update_profile_status_iq(&iq_id, status);
        self.send_iq(iq_node, Duration::from_secs(10)).await?;
        Ok(())
    }

    /// Queries the WhatsApp About / Bio status of a user.
    pub async fn fetch_status(
        &self,
        jid: &str,
    ) -> Result<Option<crate::privacy::ProfileStatus>, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let normalized = if !jid.contains('@') {
            format!("{}@s.whatsapp.net", jid.trim_start_matches('+'))
        } else {
            jid.to_string()
        };
        let iq_node = crate::privacy::build_fetch_status_iq(&iq_id, &normalized);
        let res = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        Ok(crate::privacy::parse_profile_status_response(&res))
    }

    /// Queries business profile metadata (address, description, websites, email, category) for a WhatsApp Business account.
    pub async fn get_business_profile(
        &self,
        jid: &str,
    ) -> Result<Option<crate::privacy::BusinessProfile>, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let normalized = if !jid.contains('@') {
            format!("{}@s.whatsapp.net", jid.trim_start_matches('+'))
        } else {
            jid.to_string()
        };
        let iq_node = crate::privacy::build_business_profile_iq(&iq_id, &normalized);
        let res = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        Ok(crate::privacy::parse_business_profile_response(&res))
    }

    /// Fetches the profile picture URL for a user or group.
    /// `picture_type` can be "preview" (low-res thumbnail) or "image" (high-res).
    pub async fn profile_picture_url(
        &self,
        jid: &str,
        picture_type: &str,
    ) -> Result<Option<String>, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let normalized = if !jid.contains('@') {
            format!("{}@s.whatsapp.net", jid)
        } else {
            jid.to_string()
        };
        let ptype = if picture_type == "image" {
            "image"
        } else {
            "preview"
        };
        let picture_node = BinaryNode::new("picture")
            .with_attr("type", ptype)
            .with_attr("query", "url");

        let iq_node = BinaryNode::new("iq")
            .with_attr("target", &normalized)
            .with_attr("to", S_WHATSAPP_NET)
            .with_attr("type", "get")
            .with_attr("xmlns", "w:profile:picture")
            .with_attr("id", &iq_id)
            .with_child(picture_node);

        let res = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        if let Some(pic) = res.get_child("picture") {
            Ok(pic.get_attr("url").map(|s| s.to_string()))
        } else {
            Ok(None)
        }
    }

    /// Updates the profile picture for the logged-in user or a group.
    pub async fn update_profile_picture(
        &self,
        jid_opt: Option<&str>,
        image_bytes: &[u8],
    ) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let mut iq_node = BinaryNode::new("iq")
            .with_attr("to", S_WHATSAPP_NET)
            .with_attr("type", "set")
            .with_attr("xmlns", "w:profile:picture")
            .with_attr("id", &iq_id);

        if let Some(target) = jid_opt {
            let normalized = if !target.contains('@') {
                format!("{}@s.whatsapp.net", target)
            } else {
                target.to_string()
            };
            iq_node = iq_node.with_attr("target", normalized);
        }

        let picture_node = BinaryNode::new("picture")
            .with_attr("type", "image")
            .with_bytes_content(image_bytes.to_vec());

        iq_node = iq_node.with_child(picture_node);
        self.send_iq(iq_node, Duration::from_secs(15)).await?;
        Ok(())
    }

    /// Removes the profile picture for self or a group.
    pub async fn remove_profile_picture(&self, jid_opt: Option<&str>) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let mut iq_node = BinaryNode::new("iq")
            .with_attr("to", S_WHATSAPP_NET)
            .with_attr("type", "set")
            .with_attr("xmlns", "w:profile:picture")
            .with_attr("id", &iq_id);

        if let Some(target) = jid_opt {
            let normalized = if !target.contains('@') {
                format!("{}@s.whatsapp.net", target)
            } else {
                target.to_string()
            };
            iq_node = iq_node.with_attr("target", normalized);
        }

        self.send_iq(iq_node, Duration::from_secs(10)).await?;
        Ok(())
    }

    /// Updates the push name / display name for the logged-in user.
    pub async fn update_profile_name(&self, name: &str) -> Result<(), CoreError> {
        let name_trimmed = name.trim();
        // 1. Update stored auth credentials
        if let Ok(Some(mut creds)) = self.store.get_creds().await {
            creds.me_name = Some(name_trimmed.to_string());
            let _ = self.store.save_creds(&creds).await;
        }

        // 2. Also persist in KV store for quick access
        let _ = self.store.set_kv("profile:name", name_trimmed).await;

        // 3. Send presence node with updated name over socket if connected
        let node = BinaryNode::new("presence")
            .with_attr("name", name_trimmed)
            .with_attr("type", "available");

        let socket_opt = {
            let lock = self.socket.lock().await;
            lock.clone()
        };
        if let Some(socket) = socket_opt {
            let _ = socket.send_node(node).await;
        }

        Ok(())
    }

    /// Subscribes to presence updates (online / offline / typing status) for a contact or group.
    pub async fn presence_subscribe(&self, to_jid: &str) -> Result<(), CoreError> {
        let socket_opt = {
            let lock = self.socket.lock().await;
            lock.clone()
        };
        let socket = socket_opt.ok_or_else(|| CoreError::Network("Client not connected".into()))?;

        let normalized = if !to_jid.contains('@') {
            format!("{}@s.whatsapp.net", to_jid)
        } else {
            to_jid.to_string()
        };

        let node = BinaryNode::new("presence")
            .with_attr("to", normalized)
            .with_attr("id", &format!("3EB0{:X}", rand::random::<u64>())[..12])
            .with_attr("type", "subscribe");

        socket.send_node(node).await
    }

    /// Checks whether phone numbers are registered on WhatsApp using the USync contact protocol.
    pub async fn check_on_whatsapp(
        &self,
        numbers: &[&str],
    ) -> Result<Vec<crate::usync::OnWhatsAppResult>, CoreError> {
        let iq_node = crate::usync::build_usync_contact_iq(numbers);
        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        crate::usync::parse_usync_contact_response(&res)
    }

    /// Synchronizes multi-device App State collections with WhatsApp Web Gateway.
    ///
    /// Requests snapshots or patches for collections (e.g. critical_block, regular_high, regular_low, regular),
    /// decrypts mutations, persists changes to the SQLite store (ChatStore, ContactStore, MessageStore),
    /// updates stored LTHash state, and emits real-time events.
    pub async fn resync_app_state(
        &self,
        collections: &[&str],
        force_snapshot: bool,
    ) -> Result<AppStateSyncSummary, CoreError> {
        if collections.is_empty() {
            return Ok(AppStateSyncSummary::default());
        }

        // 1. Load current LTHash state per collection
        let mut states: HashMap<String, crate::syncd::LTHashState> = HashMap::new();
        for col in collections {
            if let Ok(Some(v_bytes)) = self.store.get_key("app-state-sync-version", col).await {
                if let Ok(stored) =
                    serde_json::from_slice::<crate::syncd::StoredLTHashState>(&v_bytes)
                {
                    states.insert(col.to_string(), stored.into());
                } else if v_bytes.len() >= 8 {
                    let ver = u64::from_be_bytes(v_bytes[0..8].try_into().unwrap_or([0; 8]));
                    let h = if v_bytes.len() >= 136 {
                        v_bytes[8..136].to_vec()
                    } else {
                        vec![0u8; 128]
                    };
                    states.insert(
                        col.to_string(),
                        crate::syncd::LTHashState {
                            version: ver,
                            hash: h,
                            index_value_map: HashMap::new(),
                        },
                    );
                }
            }
        }

        // 2. Build wire resync IQ
        let iq = crate::syncd::build_resync_iq(collections, &states, force_snapshot);
        debug!(
            "Dispatching AppState resync IQ for {:?} (force_snapshot={})",
            collections, force_snapshot
        );

        let response = match self.send_iq(iq, Duration::from_secs(20)).await {
            Ok(resp) => resp,
            Err(e) => {
                warn!("AppState resync IQ failed: {}", e);
                return Err(e);
            }
        };

        // 3. Extract collections data from IQ response
        let collections_data = crate::syncd::extract_syncd_response(&response)?;

        // 4. Preload all app state keys into in-memory resolver map
        let mut key_map: HashMap<String, Vec<u8>> = HashMap::new();
        if let Ok(all_keys) = self.store.get_all_keys("app-state-sync-key").await {
            for (id, data) in all_keys {
                key_map.insert(id, data);
            }
        }
        if let Ok(Some(primary)) = self.store.get_key("app-state-sync-key", "primary").await {
            key_map.insert("primary".to_string(), primary);
        }

        let key_resolver = |key_id: &[u8]| -> Option<Vec<u8>> {
            let b64 = base64::engine::general_purpose::STANDARD.encode(key_id);
            if let Some(k) = key_map.get(&b64) {
                return Some(k.clone());
            }
            let hx = hex::encode(key_id);
            if let Some(k) = key_map.get(&hx) {
                return Some(k.clone());
            }
            if let Some(k) = key_map.get("primary") {
                return Some(k.clone());
            }
            None
        };

        let mut summary = AppStateSyncSummary::default();

        // 5. Process each collection
        for (col_name, col_data) in collections_data {
            let mut curr_state = states.remove(&col_name).unwrap_or_default();
            let mut all_mutations = Vec::new();

            // A. Decode snapshot if present
            if let Some(ref snapshot) = col_data.snapshot {
                match crate::syncd::decode_syncd_snapshot(snapshot, &col_name, key_resolver, false)
                {
                    Ok((new_state, mut muts)) => {
                        curr_state = new_state;
                        all_mutations.append(&mut muts);
                    }
                    Err(e) => {
                        warn!("Failed to decode Syncd snapshot for '{}': {}", col_name, e);
                    }
                }
            }

            // B. Decode patches
            for patch in &col_data.patches {
                match crate::syncd::decode_syncd_patch(
                    patch,
                    &col_name,
                    &curr_state,
                    key_resolver,
                    false,
                ) {
                    Ok((new_state, mut muts)) => {
                        curr_state = new_state;
                        all_mutations.append(&mut muts);
                    }
                    Err(e) => {
                        warn!("Failed to decode Syncd patch for '{}': {}", col_name, e);
                    }
                }
            }

            // C. Apply decoded mutations to SQLite stores
            for m in &all_mutations {
                let action = m.parse_action();
                match action {
                    crate::syncd::SyncdAction::Mute {
                        jid,
                        muted,
                        mute_end_timestamp,
                    } => {
                        let mute_until = if muted {
                            mute_end_timestamp.map(|t| t as u64).or(Some(u64::MAX))
                        } else {
                            None
                        };
                        let _ = self.store.update_chat_mute(&jid, mute_until).await;
                        self.events.emit(BaileysEvent::ChatUpdate {
                            jid,
                            pinned: None,
                            archived: None,
                            mute_until: Some(mute_until),
                            unread_count: None,
                        });
                        summary.mutations_applied += 1;
                    }
                    crate::syncd::SyncdAction::Pin { jid, pinned } => {
                        let _ = self.store.update_chat_pin(&jid, pinned).await;
                        self.events.emit(BaileysEvent::ChatUpdate {
                            jid,
                            pinned: Some(pinned),
                            archived: None,
                            mute_until: None,
                            unread_count: None,
                        });
                        summary.mutations_applied += 1;
                    }
                    crate::syncd::SyncdAction::Archive { jid, archived } => {
                        let _ = self.store.update_chat_archive(&jid, archived).await;
                        self.events.emit(BaileysEvent::ChatUpdate {
                            jid,
                            pinned: None,
                            archived: Some(archived),
                            mute_until: None,
                            unread_count: None,
                        });
                        summary.mutations_applied += 1;
                    }
                    crate::syncd::SyncdAction::Star {
                        jid,
                        msg_id,
                        starred,
                        ..
                    } => {
                        let _ = self.store.update_message_star(&jid, &msg_id, starred).await;
                        summary.mutations_applied += 1;
                    }
                    crate::syncd::SyncdAction::Contact { id, name, lid, .. } => {
                        let contact = baileys_store::StoredContact {
                            jid: id.clone(),
                            name: name.clone(),
                            push_name: name.clone(),
                            lid,
                        };
                        let _ = self.store.save_contact(&contact).await;
                        self.events.emit(BaileysEvent::ContactUpdate { contact });
                        summary.mutations_applied += 1;
                    }
                    crate::syncd::SyncdAction::MarkChatAsRead { jid, read } => {
                        let _ = self.store.update_chat_unread(&jid, !read).await;
                        let unread_count = if read { 0 } else { 1 };
                        self.events.emit(BaileysEvent::ChatUpdate {
                            jid,
                            pinned: None,
                            archived: None,
                            mute_until: None,
                            unread_count: Some(unread_count),
                        });
                        summary.mutations_applied += 1;
                    }
                    crate::syncd::SyncdAction::ClearChat { jid } => {
                        let _ = self.store.clear_chat_messages(&jid, true).await;
                        summary.mutations_applied += 1;
                    }
                    crate::syncd::SyncdAction::DeleteChat { jid } => {
                        let _ = self.store.delete_chat(&jid).await;
                        summary.mutations_applied += 1;
                    }
                    crate::syncd::SyncdAction::Other { .. } => {}
                }
            }

            // D. Persist updated LTHashState for this collection
            let stored = crate::syncd::StoredLTHashState::from(&curr_state);
            if let Ok(json_bytes) = serde_json::to_vec(&stored) {
                let _ = self
                    .store
                    .set_keys(vec![baileys_store::KeyEntry {
                        key_type: "app-state-sync-version".to_string(),
                        id: col_name.clone(),
                        data: json_bytes,
                    }])
                    .await;
            }

            summary.collections_synced += 1;
        }

        // 6. Emit AppStateSynced event
        self.events.emit(BaileysEvent::AppStateSynced {
            collections_synced: summary.collections_synced,
            mutations_applied: summary.mutations_applied,
        });

        info!(
            "AppState sync complete: {} collections synced, {} mutations applied",
            summary.collections_synced, summary.mutations_applied
        );

        Ok(summary)
    }

    /// Helper to dispatch AppState Syncd mutation patch to WhatsApp server.
    async fn dispatch_syncd_patch<F>(&self, collection: &str, patch_fn: F)
    where
        F: FnOnce(&[u8], &[u8], &[u8], u64) -> Result<crate::syncd::SyncdPatchResult, CoreError>,
    {
        let version_state = self
            .store
            .get_key("app-state-sync-version", collection)
            .await
            .unwrap_or(None);
        let (current_version, current_hash) = if let Some(v_bytes) = version_state {
            if let Ok(stored) = serde_json::from_slice::<crate::syncd::StoredLTHashState>(&v_bytes)
            {
                let state: crate::syncd::LTHashState = stored.into();
                (state.version, state.hash)
            } else if v_bytes.len() >= 8 {
                let ver = u64::from_be_bytes(v_bytes[0..8].try_into().unwrap_or([0; 8]));
                let h = if v_bytes.len() >= 136 {
                    v_bytes[8..136].to_vec()
                } else {
                    vec![0u8; 128]
                };
                (ver, h)
            } else {
                (0, vec![0u8; 128])
            }
        } else {
            (0, vec![0u8; 128])
        };

        let key_data_opt = self
            .store
            .get_key("app-state-sync-key", "primary")
            .await
            .unwrap_or(None);
        let key_id_opt = self
            .store
            .get_key("app-state-sync-key", "primary_id")
            .await
            .unwrap_or(None);

        if let (Some(key_data), Some(key_id)) = (key_data_opt, key_id_opt) {
            if let Ok(res) = patch_fn(&key_id, &key_data, &current_hash, current_version) {
                let iq = crate::syncd::build_syncd_iq(collection, current_version, &res.patch);
                match self.send_iq(iq, Duration::from_secs(10)).await {
                    Ok(_) => {
                        debug!(
                            "Syncd mutation for collection '{}' acknowledged by WhatsApp gateway",
                            collection
                        );
                        let updated_state = crate::syncd::LTHashState {
                            version: res.next_version,
                            hash: res.new_hash,
                            index_value_map: HashMap::new(),
                        };
                        let stored = crate::syncd::StoredLTHashState::from(&updated_state);
                        if let Ok(json_bytes) = serde_json::to_vec(&stored) {
                            let _ = self
                                .store
                                .set_keys(vec![baileys_store::KeyEntry {
                                    key_type: "app-state-sync-version".to_string(),
                                    id: collection.to_string(),
                                    data: json_bytes,
                                }])
                                .await;
                        }
                    }
                    Err(e) => {
                        debug!(
                            "Syncd mutation for '{}' gateway acknowledgement note: {}",
                            collection, e
                        );
                    }
                }
            }
        } else {
            debug!("No AppStateSyncKey available yet for '{}', wire sync skipped (local store updated)", collection);
        }
    }

    /// Modifies chat mute status (None = unmuted, Some(duration_seconds) = muted until now + duration, or Some(u64::MAX) for indefinite mute).
    pub async fn chat_modify_mute(
        &self,
        jid: &str,
        duration_secs: Option<u64>,
    ) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@s.whatsapp.net", jid.trim_start_matches('+'))
        } else {
            jid.to_string()
        };
        let mute_until = duration_secs.map(|dur| {
            if dur == u64::MAX {
                u64::MAX
            } else {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    + dur
            }
        });

        // 1. Update local database immediately
        self.store
            .update_chat_mute(&normalized, mute_until)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Dispatch wire-level Syncd patch
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_HIGH,
            |key_id, key_data, state_hash, ver| {
                crate::syncd::create_mute_patch(
                    &normalized,
                    mute_until,
                    key_id,
                    key_data,
                    state_hash,
                    ver,
                )
            },
        )
        .await;

        Ok(())
    }

    /// Pins or unpins a chat.
    pub async fn chat_modify_pin(&self, jid: &str, pin: bool) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@s.whatsapp.net", jid.trim_start_matches('+'))
        } else {
            jid.to_string()
        };

        // 1. Update local database immediately
        self.store
            .update_chat_pin(&normalized, pin)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Dispatch wire-level Syncd patch
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_LOW,
            |key_id, key_data, state_hash, ver| {
                crate::syncd::create_pin_patch(&normalized, pin, key_id, key_data, state_hash, ver)
            },
        )
        .await;

        Ok(())
    }

    /// Archives or unarchives a chat.
    pub async fn chat_modify_archive(&self, jid: &str, archive: bool) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@s.whatsapp.net", jid.trim_start_matches('+'))
        } else {
            jid.to_string()
        };

        // 1. Update local database immediately
        self.store
            .update_chat_archive(&normalized, archive)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Dispatch wire-level Syncd patch
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_LOW,
            |key_id, key_data, state_hash, ver| {
                crate::syncd::create_archive_patch(
                    &normalized,
                    archive,
                    key_id,
                    key_data,
                    state_hash,
                    ver,
                )
            },
        )
        .await;

        Ok(())
    }

    /// Stars or unstars an individual message.
    pub async fn chat_modify_star(
        &self,
        jid: &str,
        msg_id: &str,
        from_me: bool,
        star: bool,
    ) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@s.whatsapp.net", jid.trim_start_matches('+'))
        } else {
            jid.to_string()
        };

        // 1. Update local database immediately
        self.store
            .update_message_star(&normalized, msg_id, star)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Dispatch wire-level Syncd patch
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_LOW,
            |key_id, key_data, state_hash, ver| {
                crate::syncd::create_star_patch(
                    &normalized,
                    msg_id,
                    from_me,
                    star,
                    key_id,
                    key_data,
                    state_hash,
                    ver,
                )
            },
        )
        .await;

        Ok(())
    }

    /// Queries metadata for a WhatsApp Channel by JID or invite code/link.
    pub async fn newsletter_query_metadata(
        &self,
        jid_or_invite: &str,
    ) -> Result<NewsletterMetadata, CoreError> {
        let clean = jid_or_invite.trim();
        let (input_key, key_type, fetch_viewer) = if clean.contains("@newsletter") {
            (clean.to_string(), "JID", true)
        } else {
            let code = clean
                .trim_start_matches("https://whatsapp.com/channel/")
                .trim_start_matches("http://whatsapp.com/channel/")
                .trim_start_matches("whatsapp.com/channel/");
            (code.to_string(), "INVITE", false)
        };

        let vars = serde_json::json!({
            "input": {
                "key": input_key,
                "type": key_type
            },
            "fetch_viewer_metadata": fetch_viewer,
            "fetch_full_image": true,
            "fetch_creation_time": true
        });

        let iq = crate::newsletter::build_mex_iq(crate::newsletter::QUERY_FETCH_NEWSLETTER, vars);
        let res = self.send_iq(iq, Duration::from_secs(12)).await?;
        let data = crate::newsletter::parse_mex_response(&res)?;

        let nl_obj = data.get("xwa2_newsletter").ok_or_else(|| {
            CoreError::Protocol("Missing xwa2_newsletter in MEX response".to_string())
        })?;

        let meta = crate::newsletter::parse_single_newsletter(nl_obj).ok_or_else(|| {
            CoreError::Protocol("Failed to parse newsletter metadata".to_string())
        })?;

        // Cache in local SQLite store
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let stored = StoredNewsletter {
            jid: meta.jid.clone(),
            name: meta.name.clone(),
            description: meta.description.clone(),
            invite_code: meta.invite_code.clone(),
            picture_url: meta.picture_url.clone(),
            role: meta.role.clone(),
            muted: meta.muted,
            subscribers_count: meta.subscribers_count,
            updated_at: now,
        };
        let _ = self.store.save_newsletter(&stored).await;

        Ok(meta)
    }

    /// Retrieves all newsletters/channels the user is currently subscribed to.
    pub async fn newsletter_get_subscribed(&self) -> Result<Vec<NewsletterMetadata>, CoreError> {
        let vars = serde_json::json!({});
        let iq =
            crate::newsletter::build_mex_iq(crate::newsletter::QUERY_SUBSCRIBED_NEWSLETTERS, vars);
        let res = self.send_iq(iq, Duration::from_secs(12)).await?;
        let data = crate::newsletter::parse_mex_response(&res)?;

        let list = crate::newsletter::parse_subscribed_newsletters(&data);

        // Cache in local SQLite store
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        for meta in &list {
            let stored = StoredNewsletter {
                jid: meta.jid.clone(),
                name: meta.name.clone(),
                description: meta.description.clone(),
                invite_code: meta.invite_code.clone(),
                picture_url: meta.picture_url.clone(),
                role: meta.role.clone(),
                muted: meta.muted,
                subscribers_count: meta.subscribers_count,
                updated_at: now,
            };
            let _ = self.store.save_newsletter(&stored).await;
        }

        Ok(list)
    }

    /// Follows a WhatsApp Channel.
    pub async fn newsletter_follow(&self, jid: &str) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };

        let vars = serde_json::json!({
            "newsletter_id": normalized
        });
        let iq =
            crate::newsletter::build_mex_iq(crate::newsletter::MUTATION_FOLLOW_NEWSLETTER, vars);
        let res = self.send_iq(iq, Duration::from_secs(10)).await?;
        crate::newsletter::parse_mex_response(&res)?;

        // Update local store role
        if let Ok(Some(mut stored)) = self.store.get_newsletter(&normalized).await {
            stored.role = Some("SUBSCRIBER".to_string());
            let _ = self.store.save_newsletter(&stored).await;
        }

        Ok(())
    }

    /// Unfollows a WhatsApp Channel.
    pub async fn newsletter_unfollow(&self, jid: &str) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };

        let vars = serde_json::json!({
            "newsletter_id": normalized
        });
        let iq =
            crate::newsletter::build_mex_iq(crate::newsletter::MUTATION_UNFOLLOW_NEWSLETTER, vars);
        let res = self.send_iq(iq, Duration::from_secs(10)).await?;
        crate::newsletter::parse_mex_response(&res)?;

        // Update local store role
        if let Ok(Some(mut stored)) = self.store.get_newsletter(&normalized).await {
            stored.role = Some("GUEST".to_string());
            let _ = self.store.save_newsletter(&stored).await;
        }

        Ok(())
    }

    /// Toggles mute status for a WhatsApp Channel.
    pub async fn newsletter_mute(&self, jid: &str, mute: bool) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };

        let query_id = if mute {
            crate::newsletter::MUTATION_MUTE_NEWSLETTER
        } else {
            crate::newsletter::MUTATION_UNMUTE_NEWSLETTER
        };

        let vars = serde_json::json!({
            "newsletter_id": normalized
        });
        let iq = crate::newsletter::build_mex_iq(query_id, vars);
        let res = self.send_iq(iq, Duration::from_secs(10)).await?;
        crate::newsletter::parse_mex_response(&res)?;

        // Update local store
        if let Ok(Some(mut stored)) = self.store.get_newsletter(&normalized).await {
            stored.muted = mute;
            let _ = self.store.save_newsletter(&stored).await;
        }

        Ok(())
    }

    /// Creates a new WhatsApp Channel.
    pub async fn newsletter_create(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> Result<NewsletterMetadata, CoreError> {
        let mut input_obj = serde_json::json!({
            "name": name
        });
        if let Some(desc) = description {
            input_obj["description"] = serde_json::Value::String(desc.to_string());
        }

        let vars = serde_json::json!({
            "newsletter_input": input_obj
        });

        let iq =
            crate::newsletter::build_mex_iq(crate::newsletter::MUTATION_CREATE_NEWSLETTER, vars);
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        let data = crate::newsletter::parse_mex_response(&res)?;

        let nl_obj = data.get("xwa2_newsletter_create").ok_or_else(|| {
            CoreError::Protocol("Missing xwa2_newsletter_create in MEX response".to_string())
        })?;

        let meta = crate::newsletter::parse_single_newsletter(nl_obj).ok_or_else(|| {
            CoreError::Protocol("Failed to parse created newsletter metadata".to_string())
        })?;

        // Cache in local SQLite store
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let stored = StoredNewsletter {
            jid: meta.jid.clone(),
            name: meta.name.clone(),
            description: meta.description.clone(),
            invite_code: meta.invite_code.clone(),
            picture_url: meta.picture_url.clone(),
            role: Some("OWNER".to_string()),
            muted: false,
            subscribers_count: 1,
            updated_at: now,
        };
        let _ = self.store.save_newsletter(&stored).await;

        Ok(meta)
    }

    /// Fetches message history for a WhatsApp Channel.
    pub async fn newsletter_fetch_messages(
        &self,
        jid: &str,
        count: u32,
        before: Option<u64>,
    ) -> Result<Vec<NewsletterMessage>, CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };

        let iq = crate::newsletter::build_newsletter_messages_iq(&normalized, count, before);
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;

        let mut messages = Vec::new();
        if let Some(msgs_parent) = res.get_child("messages") {
            for child in msgs_parent.get_children() {
                if child.tag == "message" {
                    let server_id = child
                        .get_attr("server_id")
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(0);
                    let ts = child
                        .get_attr("t")
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(0);

                    let views_count = child
                        .get_child("views")
                        .and_then(|v| v.get_attr("count"))
                        .and_then(|c| c.parse::<u64>().ok());

                    let mut reactions = HashMap::new();
                    if let Some(reacts_node) = child.get_child("reactions") {
                        for r in reacts_node.get_children() {
                            if r.tag == "reaction" {
                                if let (Some(code), Some(cnt_str)) =
                                    (r.get_attr("code"), r.get_attr("count"))
                                {
                                    if let Ok(cnt) = cnt_str.parse::<u32>() {
                                        reactions.insert(code.to_string(), cnt);
                                    }
                                }
                            }
                        }
                    }

                    let raw_bytes = child
                        .get_child("plaintext")
                        .and_then(|p| p.content_as_bytes())
                        .map(|b| b.to_vec())
                        .unwrap_or_default();

                    let (summary, has_media) = if !raw_bytes.is_empty() {
                        if let Ok(m) = <baileys_proto::proto::Message as prost::Message>::decode(
                            &raw_bytes[..],
                        ) {
                            let s = crate::messages::extract_message_summary(&m);
                            let h_m = crate::messages::message_has_media(&m);
                            (s, h_m)
                        } else {
                            ("<channel post>".to_string(), false)
                        }
                    } else {
                        ("<channel post>".to_string(), false)
                    };

                    messages.push(NewsletterMessage {
                        server_id,
                        views_count,
                        reactions,
                        timestamp: ts,
                        summary,
                        has_media,
                        raw_proto: raw_bytes,
                    });
                }
            }
        }

        Ok(messages)
    }

    /// Sends an emoji reaction to a message in a WhatsApp Channel.
    pub async fn newsletter_react(
        &self,
        jid: &str,
        server_id: u64,
        reaction: &str,
    ) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };

        let node = crate::newsletter::build_newsletter_reaction_node(
            &normalized,
            server_id,
            reaction,
            None,
        );
        let socket_opt = self.socket.lock().await;
        if let Some(ref socket) = *socket_opt {
            socket.send_node(node).await
        } else {
            Err(CoreError::Network("Socket is disconnected".to_string()))
        }
    }

    /// Sends a view receipt for channel message server IDs.
    pub async fn newsletter_view_receipt(
        &self,
        jid: &str,
        server_ids: &[u64],
    ) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };

        let node = crate::newsletter::build_newsletter_view_receipt(&normalized, server_ids);
        let socket_opt = self.socket.lock().await;
        if let Some(ref socket) = *socket_opt {
            socket.send_node(node).await
        } else {
            Err(CoreError::Network("Socket is disconnected".to_string()))
        }
    }

    /// Sends a plaintext broadcast post to an owned WhatsApp Channel.
    pub async fn newsletter_send_post(&self, jid: &str, text: &str) -> Result<String, CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };

        let proto_msg = Message {
            extended_text_message: Some(Box::new(
                baileys_proto::proto::message::ExtendedTextMessage {
                    text: Some(text.to_string()),
                    ..Default::default()
                },
            )),
            ..Default::default()
        };

        self.send_message(&normalized, proto_msg).await
    }

    /// Sends an image broadcast post to an owned WhatsApp Channel using unencrypted media CDN flow.
    pub async fn newsletter_send_image(
        &self,
        jid: &str,
        image_bytes: &[u8],
        caption: Option<&str>,
    ) -> Result<String, CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };

        let file_sha256 = baileys_crypto::sha256(image_bytes);

        let media_conn = self.refresh_media_conn(false).await?;
        let upload_res = baileys_media::upload_newsletter_media(
            image_bytes,
            baileys_media::MediaType::Image,
            &media_conn,
        )
        .await
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let proto_msg = Message {
            image_message: Some(Box::new(baileys_proto::proto::message::ImageMessage {
                url: upload_res.url,
                mimetype: Some("image/jpeg".to_string()),
                caption: caption.map(|s| s.to_string()),
                file_sha256: Some(file_sha256.to_vec()),
                file_length: Some(image_bytes.len() as u64),
                direct_path: upload_res.direct_path,
                ..Default::default()
            })),
            ..Default::default()
        };

        self.send_message_with_options(
            &normalized,
            proto_msg,
            MessageOptions {
                media_handle: upload_res.handle,
                ..Default::default()
            },
        )
        .await
    }

    /// Sends a status story to `status@broadcast` forwarding/referencing a WhatsApp Channel post.
    /// In the WhatsApp UI, this creates an interactive status story with a rich preview card and
    /// a direct "View Channel" ("Lihat Saluran") navigation banner.
    pub async fn newsletter_send_status_story(
        &self,
        newsletter_jid: &str,
        server_msg_id: i32,
        newsletter_name: &str,
        caption_or_text: &str,
        status_recipients: Option<Vec<String>>,
    ) -> Result<String, CoreError> {
        let normalized_njid = if !newsletter_jid.contains('@') {
            format!("{}@newsletter", newsletter_jid)
        } else {
            newsletter_jid.to_string()
        };

        let forwarded_info = baileys_proto::proto::context_info::ForwardedNewsletterMessageInfo {
            newsletter_jid: Some(normalized_njid.clone()),
            server_message_id: Some(server_msg_id),
            newsletter_name: Some(newsletter_name.to_string()),
            content_type: Some(baileys_proto::proto::context_info::forwarded_newsletter_message_info::ContentType::Update as i32),
            accessibility_text: None,
            profile_name: None,
        };

        let status_attribution = baileys_proto::proto::StatusAttribution {
            r#type: Some(baileys_proto::proto::status_attribution::Type::NewsletterStatus as i32),
            action_url: None,
            attribution_data: Some(baileys_proto::proto::status_attribution::AttributionData::StatusReshare(
                baileys_proto::proto::status_attribution::StatusReshare {
                    source: Some(baileys_proto::proto::status_attribution::status_reshare::Source::ChannelReshare as i32),
                    metadata: Some(baileys_proto::proto::status_attribution::status_reshare::Metadata {
                        channel_jid: Some(normalized_njid.clone()),
                        channel_message_id: Some(server_msg_id),
                        duration: None,
                        has_multiple_reshares: None,
                    }),
                }
            )),
        };

        let context_info = ContextInfo {
            is_forwarded: Some(true),
            forwarding_score: Some(1),
            forwarded_newsletter_message_info: Some(forwarded_info),
            status_attributions: vec![status_attribution],
            ..Default::default()
        };

        let proto_msg = Message {
            extended_text_message: Some(Box::new(ExtendedTextMessage {
                text: Some(caption_or_text.to_string()),
                context_info: Some(Box::new(context_info)),
                background_argb: Some(0xFF1EBEA5), // WhatsApp teal color
                font: Some(1),
                ..Default::default()
            })),
            ..Default::default()
        };

        self.send_message_with_options(
            "status@broadcast",
            proto_msg,
            MessageOptions {
                status_jid_list: status_recipients,
                ..Default::default()
            },
        )
        .await
    }

    /// Publishes an interactive HD Image Story to `status@broadcast` that references/forwards a WhatsApp Channel post.
    /// In the WhatsApp UI, this appears in the Status tray as a full-screen HD photo story with a
    /// rich preview card and a direct "View Channel" ("Lihat Saluran") navigation banner at the bottom!
    pub async fn newsletter_send_image_status_story(
        &self,
        image_bytes: &[u8],
        caption: Option<&str>,
        newsletter_jid: &str,
        server_msg_id: i32,
        newsletter_name: &str,
        status_recipients: Option<Vec<String>>,
    ) -> Result<String, CoreError> {
        let normalized_njid = if !newsletter_jid.contains('@') {
            format!("{}@newsletter", newsletter_jid)
        } else {
            newsletter_jid.to_string()
        };

        let forwarded_info = baileys_proto::proto::context_info::ForwardedNewsletterMessageInfo {
            newsletter_jid: Some(normalized_njid.clone()),
            server_message_id: Some(server_msg_id),
            newsletter_name: Some(newsletter_name.to_string()),
            content_type: Some(baileys_proto::proto::context_info::forwarded_newsletter_message_info::ContentType::Update as i32),
            accessibility_text: None,
            profile_name: None,
        };

        let status_attribution = baileys_proto::proto::StatusAttribution {
            r#type: Some(baileys_proto::proto::status_attribution::Type::NewsletterStatus as i32),
            action_url: None,
            attribution_data: Some(baileys_proto::proto::status_attribution::AttributionData::StatusReshare(
                baileys_proto::proto::status_attribution::StatusReshare {
                    source: Some(baileys_proto::proto::status_attribution::status_reshare::Source::ChannelReshare as i32),
                    metadata: Some(baileys_proto::proto::status_attribution::status_reshare::Metadata {
                        channel_jid: Some(normalized_njid.clone()),
                        channel_message_id: Some(server_msg_id),
                        duration: None,
                        has_multiple_reshares: None,
                    }),
                }
            )),
        };

        let context_info = ContextInfo {
            is_forwarded: Some(true),
            forwarding_score: Some(1),
            forwarded_newsletter_message_info: Some(forwarded_info),
            status_attributions: vec![status_attribution],
            ..Default::default()
        };

        let processed_bytes = baileys_media::scale_image_for_status(image_bytes, true)
            .unwrap_or_else(|_| image_bytes.to_vec());

        let encrypted = baileys_media::encrypt_media_bytes(
            &processed_bytes,
            None,
            baileys_media::MediaType::Image,
        )
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let media_conn = self.refresh_media_conn(false).await?;

        let upload_res = baileys_media::upload_encrypted_media(
            &encrypted.encrypted_bytes,
            &encrypted.file_enc_sha256,
            baileys_media::MediaType::Image,
            &media_conn,
        )
        .await
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let dims = baileys_media::extract_image_dimensions(&processed_bytes);
        let micro_thumb = baileys_media::generate_micro_thumbnail(&processed_bytes, 48).ok();
        let now_sec = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let proto_msg = Message {
            image_message: Some(Box::new(baileys_proto::proto::message::ImageMessage {
                url: upload_res.url,
                direct_path: upload_res.direct_path,
                media_key: Some(encrypted.media_key.to_vec()),
                file_sha256: Some(encrypted.file_sha256.to_vec()),
                file_enc_sha256: Some(encrypted.file_enc_sha256.to_vec()),
                file_length: Some(image_bytes.len() as u64),
                mimetype: Some("image/jpeg".to_string()),
                caption: caption.map(|s| s.to_string()),
                width: dims.as_ref().map(|d| d.width),
                height: dims.as_ref().map(|d| d.height),
                jpeg_thumbnail: micro_thumb,
                media_key_timestamp: Some(now_sec),
                context_info: Some(Box::new(context_info)),
                ..Default::default()
            })),
            ..Default::default()
        };

        let options = MessageOptions {
            status_jid_list: status_recipients,
            ..Default::default()
        };
        let msg_id = self
            .send_message_with_options("status@broadcast", proto_msg, options)
            .await?;

        let creds = self.store.get_creds().await.ok().flatten();
        let me_jid = creds
            .and_then(|c| c.me_jid)
            .unwrap_or_else(|| "me".to_string());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let story = baileys_store::StoredStory {
            msg_id: msg_id.clone(),
            author_jid: me_jid,
            author_name: Some("Me".to_string()),
            timestamp: now,
            message_type: "image".to_string(),
            content: None,
            caption: caption.map(|s| s.to_string()),
            has_media: true,
            viewed: true,
        };
        let _ = self.store.save_story(&story).await;

        Ok(msg_id)
    }

    /// Deletes a WhatsApp Channel (must be channel owner).
    pub async fn newsletter_delete(&self, jid: &str) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };
        let vars = serde_json::json!({
            "newsletter_id": normalized
        });
        let iq =
            crate::newsletter::build_mex_iq(crate::newsletter::MUTATION_DELETE_NEWSLETTER, vars);
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        let _ = crate::newsletter::parse_mex_response(&res);
        let _ = self.store.delete_chat(&normalized).await;
        Ok(())
    }

    /// Updates the name of a WhatsApp Channel (must be channel admin or owner).
    pub async fn newsletter_update_name(&self, jid: &str, name: &str) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };
        let vars = serde_json::json!({
            "newsletter_id": normalized,
            "updates": {
                "name": name
            }
        });
        let iq =
            crate::newsletter::build_mex_iq(crate::newsletter::MUTATION_UPDATE_NEWSLETTER, vars);
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        let _ = crate::newsletter::parse_mex_response(&res);
        if let Ok(Some(mut stored)) = self.store.get_newsletter(&normalized).await {
            stored.name = name.to_string();
            let _ = self.store.save_newsletter(&stored).await;
        }
        Ok(())
    }

    /// Promotes a subscriber to admin in a WhatsApp Channel.
    pub async fn newsletter_promote(&self, jid: &str, user_jid: &str) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };
        let user_norm = if !user_jid.contains('@') {
            format!("{}@s.whatsapp.net", user_jid)
        } else {
            user_jid.to_string()
        };
        let vars = serde_json::json!({
            "newsletter_id": normalized,
            "user_id": user_norm,
            "role": "ADMIN"
        });
        let iq =
            crate::newsletter::build_mex_iq(crate::newsletter::MUTATION_UPDATE_NEWSLETTER, vars);
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        let _ = crate::newsletter::parse_mex_response(&res);
        Ok(())
    }

    /// Demotes an admin back to subscriber in a WhatsApp Channel.
    pub async fn newsletter_demote(&self, jid: &str, user_jid: &str) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };
        let user_norm = if !user_jid.contains('@') {
            format!("{}@s.whatsapp.net", user_jid)
        } else {
            user_jid.to_string()
        };
        let vars = serde_json::json!({
            "newsletter_id": normalized,
            "user_id": user_norm
        });
        let iq = crate::newsletter::build_mex_iq(
            crate::newsletter::MUTATION_DEMOTE_ADMIN_NEWSLETTER,
            vars,
        );
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        let _ = crate::newsletter::parse_mex_response(&res);
        Ok(())
    }

    /// Transfers ownership of a WhatsApp Channel to another user.
    pub async fn newsletter_change_owner(
        &self,
        jid: &str,
        new_owner_jid: &str,
    ) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };
        let user_norm = if !new_owner_jid.contains('@') {
            format!("{}@s.whatsapp.net", new_owner_jid)
        } else {
            new_owner_jid.to_string()
        };
        let vars = serde_json::json!({
            "newsletter_id": normalized,
            "new_owner_id": user_norm
        });
        let iq = crate::newsletter::build_mex_iq(
            crate::newsletter::MUTATION_CHANGE_OWNER_NEWSLETTER,
            vars,
        );
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        let _ = crate::newsletter::parse_mex_response(&res);
        Ok(())
    }

    /// Sends a poll creation message with options.
    pub async fn send_poll(
        &self,
        to_jid: &str,
        question: &str,
        options: &[String],
        selectable_count: u32,
    ) -> Result<String, CoreError> {
        let mut enc_key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut enc_key);

        let proto_msg = crate::polls::build_poll_creation_message(
            question,
            options,
            selectable_count,
            &enc_key,
        );

        let normalized_jid = if !to_jid.contains('@') {
            format!("{}@s.whatsapp.net", to_jid)
        } else {
            to_jid.to_string()
        };

        let me = self.me_jid().await?.unwrap_or_else(|| "me".to_string());
        let msg_id = self.send_message(&normalized_jid, proto_msg).await?;

        // Cache poll in local SQLite store
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let stored_poll = StoredPoll {
            id: msg_id.clone(),
            chat_jid: normalized_jid,
            sender_jid: me,
            question: question.to_string(),
            options: options.to_vec(),
            enc_key: enc_key.to_vec(),
            selectable_count: if selectable_count == 0 {
                1
            } else {
                selectable_count
            },
            created_at: now,
        };
        let _ = self.store.save_poll(&stored_poll).await;

        Ok(msg_id)
    }

    /// Casts an encrypted vote on a poll.
    pub async fn send_poll_vote(
        &self,
        to_jid: &str,
        poll_id: &str,
        selected_options: &[String],
    ) -> Result<String, CoreError> {
        let stored_poll = self
            .store
            .get_poll(poll_id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?
            .ok_or_else(|| CoreError::Protocol(format!("Poll '{}' not found in store", poll_id)))?;

        let me = self.me_jid().await?.unwrap_or_else(|| "me".to_string());
        let selected_hashes: Vec<Vec<u8>> = selected_options
            .iter()
            .map(|opt| crate::polls::hash_poll_option(opt).to_vec())
            .collect();

        let vote_enc = crate::polls::encrypt_poll_vote(
            &selected_hashes,
            &stored_poll.sender_jid,
            poll_id,
            &stored_poll.enc_key,
            &me,
        )?;

        let normalized_jid = if !to_jid.contains('@') {
            format!("{}@s.whatsapp.net", to_jid)
        } else {
            to_jid.to_string()
        };

        let poll_creation_key = MessageKey {
            remote_jid: Some(normalized_jid.clone()),
            from_me: Some(stored_poll.sender_jid == me),
            id: Some(poll_id.to_string()),
            participant: None,
        };

        let proto_msg = crate::polls::build_poll_update_message(poll_creation_key, vote_enc);
        let msg_id = self.send_message(&normalized_jid, proto_msg).await?;

        // Save own vote locally
        let _ = self
            .store
            .save_poll_vote(poll_id, &me, selected_options)
            .await;

        Ok(msg_id)
    }

    /// Fetches live vote tally for a poll from the SQLite store.
    pub async fn get_poll_tally(&self, poll_id: &str) -> Result<Option<PollTally>, CoreError> {
        self.store
            .get_poll_tally(poll_id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }

    /// Sends an interactive message with action buttons (URL, call, copy code, quick reply).
    pub async fn send_interactive_buttons(
        &self,
        to_jid: &str,
        body: &str,
        buttons: &[InteractiveButton],
        header: Option<&str>,
        footer: Option<&str>,
    ) -> Result<String, CoreError> {
        let proto_msg =
            crate::polls::build_interactive_button_message(body, buttons, header, footer);
        self.send_message(to_jid, proto_msg).await
    }

    /// Sends an interactive list message.
    pub async fn send_list_message(
        &self,
        to_jid: &str,
        title: &str,
        description: &str,
        button_text: &str,
        sections: &[ListSection],
        footer: Option<&str>,
    ) -> Result<String, CoreError> {
        let proto_msg =
            crate::polls::build_list_message(title, description, button_text, sections, footer);
        self.send_message(to_jid, proto_msg).await
    }

    /// Creates a new WhatsApp Community.
    pub async fn community_create(
        &self,
        subject: &str,
        description: Option<&str>,
    ) -> Result<CommunityMetadata, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let create_node = crate::community::build_create_community_node(subject, description);
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", "@g.us")
            .with_attr("type", "set")
            .with_attr("xmlns", "w:g2")
            .with_attr("id", &iq_id)
            .with_child(create_node);

        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        crate::community::parse_community_metadata(&res)
    }

    /// Creates a new subgroup linked under an existing parent Community.
    pub async fn community_create_subgroup(
        &self,
        parent_community_jid: &str,
        subject: &str,
        participants: &[String],
    ) -> Result<String, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let create_node = crate::community::build_create_community_subgroup_node(
            subject,
            participants,
            parent_community_jid,
        );
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", "@g.us")
            .with_attr("type", "set")
            .with_attr("xmlns", "w:g2")
            .with_attr("id", &iq_id)
            .with_child(create_node);

        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        if let Some(group) = res.get_child("group") {
            let gid = group.get_attr("id").unwrap_or("");
            let group_jid = if gid.contains('@') {
                gid.to_string()
            } else {
                format!("{}@g.us", gid)
            };
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let stored_sub = StoredCommunitySubgroup {
                group_jid: group_jid.clone(),
                subject: subject.to_string(),
                creation: Some(now),
                creator: self.me_jid().await.ok().flatten(),
                size: Some((participants.len() + 1) as u32),
                linked_at: now,
            };
            let _ = self
                .store
                .save_community_subgroup(parent_community_jid, &stored_sub)
                .await;
            Ok(group_jid)
        } else {
            Err(CoreError::Protocol(
                "Missing <group> in create subgroup response".to_string(),
            ))
        }
    }

    /// Links an existing WhatsApp group into a parent Community.
    pub async fn community_link_subgroup(
        &self,
        parent_community_jid: &str,
        group_jid: &str,
    ) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let link_node = crate::community::build_link_subgroup_node(group_jid);
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", parent_community_jid)
            .with_attr("type", "set")
            .with_attr("xmlns", "w:g2")
            .with_attr("id", &iq_id)
            .with_child(link_node);

        self.send_iq(iq_node, Duration::from_secs(15)).await?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let stored_sub = StoredCommunitySubgroup {
            group_jid: group_jid.to_string(),
            subject: String::new(),
            creation: None,
            creator: None,
            size: None,
            linked_at: now,
        };
        let _ = self
            .store
            .save_community_subgroup(parent_community_jid, &stored_sub)
            .await;
        Ok(())
    }

    /// Unlinks a subgroup from a parent Community.
    pub async fn community_unlink_subgroup(
        &self,
        parent_community_jid: &str,
        group_jid: &str,
    ) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let unlink_node = crate::community::build_unlink_subgroup_node(group_jid);
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", parent_community_jid)
            .with_attr("type", "set")
            .with_attr("xmlns", "w:g2")
            .with_attr("id", &iq_id)
            .with_child(unlink_node);

        self.send_iq(iq_node, Duration::from_secs(15)).await?;
        let _ = self
            .store
            .remove_community_subgroup(parent_community_jid, group_jid)
            .await;
        Ok(())
    }

    /// Fetches all linked subgroups under a Community and caches them locally in SQLite.
    pub async fn community_fetch_subgroups(
        &self,
        community_jid: &str,
    ) -> Result<Vec<CommunitySubgroup>, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let query_node = crate::community::build_fetch_subgroups_node();
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", community_jid)
            .with_attr("type", "get")
            .with_attr("xmlns", "w:g2")
            .with_attr("id", &iq_id)
            .with_child(query_node);

        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        let subgroups = crate::community::parse_linked_subgroups(&res)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        for sub in &subgroups {
            let stored = StoredCommunitySubgroup {
                group_jid: sub.id.clone(),
                subject: sub.subject.clone(),
                creation: sub.creation,
                creator: sub.creator.clone(),
                size: sub.size,
                linked_at: now,
            };
            let _ = self
                .store
                .save_community_subgroup(community_jid, &stored)
                .await;
        }
        Ok(subgroups)
    }

    /// Queries interactive metadata for a Community.
    pub async fn community_get_metadata(
        &self,
        community_jid: &str,
    ) -> Result<CommunityMetadata, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let query_node = crate::community::build_community_metadata_query_node();
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", community_jid)
            .with_attr("type", "get")
            .with_attr("xmlns", "w:g2")
            .with_attr("id", &iq_id)
            .with_child(query_node);

        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        crate::community::parse_community_metadata(&res)
    }

    /// Leaves an existing WhatsApp Community.
    pub async fn community_leave(&self, community_jid: &str) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let clean_id = community_jid.trim_end_matches("@g.us");
        let leave_node = crate::community::build_community_leave_node(clean_id);
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", "@g.us")
            .with_attr("type", "set")
            .with_attr("xmlns", "w:g2")
            .with_attr("id", &iq_id)
            .with_child(leave_node);

        self.send_iq(iq_node, Duration::from_secs(15)).await?;
        Ok(())
    }

    /// Gets an invite code for a Community.
    pub async fn community_get_invite_code(
        &self,
        community_jid: &str,
    ) -> Result<String, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let invite_node = crate::community::build_community_invite_query_node();
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", community_jid)
            .with_attr("type", "get")
            .with_attr("xmlns", "w:g2")
            .with_attr("id", &iq_id)
            .with_child(invite_node);

        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        if let Some(invite) = res.get_child("invite") {
            if let Some(code) = invite.get_attr("code") {
                return Ok(code.to_string());
            }
        }
        Err(CoreError::Protocol(
            "Missing invite code in response".to_string(),
        ))
    }

    /// Accepts an invite code to join a WhatsApp Community.
    pub async fn community_accept_invite(&self, code: &str) -> Result<String, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let invite_node = crate::community::build_community_accept_invite_node(code);
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", "@g.us")
            .with_attr("type", "set")
            .with_attr("xmlns", "w:g2")
            .with_attr("id", &iq_id)
            .with_child(invite_node);

        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        if let Some(comm) = res
            .get_child("community")
            .or_else(|| res.get_child("group"))
        {
            if let Some(jid) = comm.get_attr("jid").or_else(|| comm.get_attr("id")) {
                let full_jid = if jid.contains('@') {
                    jid.to_string()
                } else {
                    format!("{}@g.us", jid)
                };
                return Ok(full_jid);
            }
        }
        Ok(code.to_string())
    }

    /// Rejects an incoming WhatsApp voice or video call.
    pub async fn reject_call(&self, call_id: &str, call_from: &str) -> Result<(), CoreError> {
        let me = self
            .me_jid()
            .await?
            .unwrap_or_else(|| S_WHATSAPP_NET.to_string());
        let reject_node = crate::call::build_reject_call_node(&me, call_id, call_from);
        let socket_opt = {
            let lock = self.socket.lock().await;
            lock.clone()
        };
        let socket = socket_opt.ok_or_else(|| CoreError::Network("Client not connected".into()))?;
        socket.send_node(reject_node).await
    }

    /// Fetches past call logs stored in SQLite.
    pub async fn get_call_history(
        &self,
        limit: u32,
    ) -> Result<Vec<baileys_store::StoredCall>, CoreError> {
        self.store
            .get_call_history(limit)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }

    /// Publishes a text status story to `status@broadcast`.
    pub async fn send_text_status(
        &self,
        text: &str,
        background_color: Option<&str>,
        font: Option<i32>,
        audience: Option<Vec<String>>,
    ) -> Result<String, CoreError> {
        let msg = crate::status::build_text_status_message(text, background_color, font);
        let options = MessageOptions {
            status_jid_list: audience,
            ..Default::default()
        };
        let msg_id = self
            .send_message_with_options("status@broadcast", msg, options)
            .await?;

        // Save our own story in SQLite store
        let creds = self.store.get_creds().await.ok().flatten();
        let me_jid = creds
            .and_then(|c| c.me_jid)
            .unwrap_or_else(|| "me".to_string());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let story = baileys_store::StoredStory {
            msg_id: msg_id.clone(),
            author_jid: me_jid,
            author_name: Some("Me".to_string()),
            timestamp: now,
            message_type: "text".to_string(),
            content: Some(text.to_string()),
            caption: None,
            has_media: false,
            viewed: true,
        };
        let _ = self.store.save_story(&story).await;

        Ok(msg_id)
    }

    /// Publishes an HD Image status story to `status@broadcast` without heavy client compression.
    pub async fn send_status_image(
        &self,
        image_bytes: &[u8],
        caption: Option<&str>,
        mimetype: Option<&str>,
        audience: Option<Vec<String>>,
    ) -> Result<String, CoreError> {
        let processed_bytes = baileys_media::scale_image_for_status(image_bytes, true)
            .unwrap_or_else(|_| image_bytes.to_vec());

        let encrypted = baileys_media::encrypt_media_bytes(
            &processed_bytes,
            None,
            baileys_media::MediaType::Image,
        )
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let media_conn = self.refresh_media_conn(false).await?;

        let upload_res = baileys_media::upload_encrypted_media(
            &encrypted.encrypted_bytes,
            &encrypted.file_enc_sha256,
            baileys_media::MediaType::Image,
            &media_conn,
        )
        .await
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let dims = baileys_media::extract_image_dimensions(&processed_bytes);
        let micro_thumb = baileys_media::generate_micro_thumbnail(&processed_bytes, 48).ok();
        let now_sec = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let is_hd = dims.as_ref().map(|d| d.is_hd()).unwrap_or(false);

        let attrs = crate::status::StatusMediaAttributes {
            width: dims.as_ref().map(|d| d.width),
            height: dims.as_ref().map(|d| d.height),
            jpeg_thumbnail: micro_thumb,
            is_hd,
            media_key_timestamp: Some(now_sec as i64),
        };

        let msg = crate::status::build_image_status_message(
            upload_res.url,
            upload_res.direct_path,
            encrypted.media_key.to_vec(),
            encrypted.file_sha256.to_vec(),
            encrypted.file_enc_sha256.to_vec(),
            image_bytes.len() as u64,
            mimetype,
            caption,
            Some(attrs),
        );

        let options = MessageOptions {
            status_jid_list: audience,
            ..Default::default()
        };
        let msg_id = self
            .send_message_with_options("status@broadcast", msg, options)
            .await?;

        let creds = self.store.get_creds().await.ok().flatten();
        let me_jid = creds
            .and_then(|c| c.me_jid)
            .unwrap_or_else(|| "me".to_string());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let story = baileys_store::StoredStory {
            msg_id: msg_id.clone(),
            author_jid: me_jid,
            author_name: Some("Me".to_string()),
            timestamp: now,
            message_type: "image".to_string(),
            content: None,
            caption: caption.map(|s| s.to_string()),
            has_media: true,
            viewed: true,
        };
        let _ = self.store.save_story(&story).await;

        Ok(msg_id)
    }

    /// Publishes an HD Video status story to `status@broadcast` without heavy client compression.
    pub async fn send_status_video(
        &self,
        video_bytes: &[u8],
        caption: Option<&str>,
        mimetype: Option<&str>,
        audience: Option<Vec<String>>,
    ) -> Result<String, CoreError> {
        let encrypted =
            baileys_media::encrypt_media_bytes(video_bytes, None, baileys_media::MediaType::Video)
                .map_err(|e| CoreError::Media(e.to_string()))?;

        let media_conn = self.refresh_media_conn(false).await?;

        let upload_res = baileys_media::upload_encrypted_media(
            &encrypted.encrypted_bytes,
            &encrypted.file_enc_sha256,
            baileys_media::MediaType::Video,
            &media_conn,
        )
        .await
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let now_sec = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let attrs = crate::status::StatusMediaAttributes {
            width: None,
            height: None,
            jpeg_thumbnail: None,
            is_hd: true,
            media_key_timestamp: Some(now_sec as i64),
        };

        let msg = crate::status::build_video_status_message(
            upload_res.url,
            upload_res.direct_path,
            encrypted.media_key.to_vec(),
            encrypted.file_sha256.to_vec(),
            encrypted.file_enc_sha256.to_vec(),
            video_bytes.len() as u64,
            30,
            mimetype,
            caption,
            Some(attrs),
        );

        let options = MessageOptions {
            status_jid_list: audience,
            ..Default::default()
        };
        let msg_id = self
            .send_message_with_options("status@broadcast", msg, options)
            .await?;

        let creds = self.store.get_creds().await.ok().flatten();
        let me_jid = creds
            .and_then(|c| c.me_jid)
            .unwrap_or_else(|| "me".to_string());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let story = baileys_store::StoredStory {
            msg_id: msg_id.clone(),
            author_jid: me_jid,
            author_name: Some("Me".to_string()),
            timestamp: now,
            message_type: "video".to_string(),
            content: None,
            caption: caption.map(|s| s.to_string()),
            has_media: true,
            viewed: true,
        };
        let _ = self.store.save_story(&story).await;

        Ok(msg_id)
    }

    /// Publishes a rich URL link preview status story to `status@broadcast`.
    pub async fn send_link_status(
        &self,
        url: &str,
        title: Option<&str>,
        description: Option<&str>,
        custom_text: Option<&str>,
        background_color: Option<&str>,
        font: Option<i32>,
        thumbnail_bytes: Option<Vec<u8>>,
        audience: Option<Vec<String>>,
    ) -> Result<String, CoreError> {
        let msg = crate::status::build_link_status_message(
            url,
            title,
            description,
            custom_text,
            background_color,
            font,
            thumbnail_bytes,
        );
        let options = MessageOptions {
            status_jid_list: audience,
            ..Default::default()
        };
        let msg_id = self
            .send_message_with_options("status@broadcast", msg, options)
            .await?;

        // Save our own story in SQLite store
        let creds = self.store.get_creds().await.ok().flatten();
        let me_jid = creds
            .and_then(|c| c.me_jid)
            .unwrap_or_else(|| "me".to_string());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let story = baileys_store::StoredStory {
            msg_id: msg_id.clone(),
            author_jid: me_jid,
            author_name: Some("Me".to_string()),
            timestamp: now,
            message_type: "text".to_string(),
            content: Some(custom_text.unwrap_or(url).to_string()),
            caption: None,
            has_media: false,
            viewed: true,
        };
        let _ = self.store.save_story(&story).await;

        Ok(msg_id)
    }

    /// Publishes an HD Audio / Voice Note status story to `status@broadcast`.
    pub async fn send_status_audio(
        &self,
        audio_bytes: &[u8],
        seconds: u32,
        mimetype: Option<&str>,
        audience: Option<Vec<String>>,
    ) -> Result<String, CoreError> {
        let encrypted =
            baileys_media::encrypt_media_bytes(audio_bytes, None, baileys_media::MediaType::Audio)
                .map_err(|e| CoreError::Media(e.to_string()))?;

        let media_conn = self.refresh_media_conn(false).await?;

        let upload_res = baileys_media::upload_encrypted_media(
            &encrypted.encrypted_bytes,
            &encrypted.file_enc_sha256,
            baileys_media::MediaType::Audio,
            &media_conn,
        )
        .await
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let msg = crate::status::build_audio_status_message(
            upload_res.url,
            upload_res.direct_path,
            encrypted.media_key.to_vec(),
            encrypted.file_sha256.to_vec(),
            encrypted.file_enc_sha256.to_vec(),
            audio_bytes.len() as u64,
            seconds,
            mimetype,
            None,
        );

        let options = MessageOptions {
            status_jid_list: audience,
            ..Default::default()
        };
        let msg_id = self
            .send_message_with_options("status@broadcast", msg, options)
            .await?;

        let creds = self.store.get_creds().await.ok().flatten();
        let me_jid = creds
            .and_then(|c| c.me_jid)
            .unwrap_or_else(|| "me".to_string());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let story = baileys_store::StoredStory {
            msg_id: msg_id.clone(),
            author_jid: me_jid,
            author_name: Some("Me".to_string()),
            timestamp: now,
            message_type: "audio".to_string(),
            content: None,
            caption: None,
            has_media: true,
            viewed: true,
        };
        let _ = self.store.save_story(&story).await;

        Ok(msg_id)
    }

    /// Sends a read receipt for a status story, marking it as viewed both on WhatsApp and locally.
    pub async fn send_status_read_receipt(
        &self,
        msg_id: &str,
        author_jid: &str,
    ) -> Result<(), CoreError> {
        if self.is_ghost_mode() {
            debug!(
                "[Ghost Mode] Suppressed status read receipt for {} to {}",
                msg_id, author_jid
            );
            self.store
                .mark_story_viewed(msg_id)
                .await
                .map_err(|e| CoreError::Store(e.to_string()))?;
            return Ok(());
        }

        let _creds = self
            .store
            .get_creds()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?
            .ok_or_else(|| CoreError::Auth("No active session credentials found".into()))?;

        let socket_opt = {
            let lock = self.socket.lock().await;
            lock.clone()
        };

        if socket_opt.is_none() && !self.config.wa_websocket_url.starts_with("mock://") {
            return Err(CoreError::Network("Client not connected".into()));
        }

        let receipt_node = crate::status::build_status_read_receipt(msg_id, author_jid);
        if let Some(socket) = socket_opt {
            socket.send_node(receipt_node).await?;
        }

        // Mark as viewed in local SQLite store
        self.store
            .mark_story_viewed(msg_id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;
        Ok(())
    }

    /// Fetches recent status updates from the SQLite cache.
    pub async fn get_recent_stories(
        &self,
        limit: u32,
        unviewed_only: bool,
    ) -> Result<Vec<baileys_store::StoredStory>, CoreError> {
        self.store
            .get_stories(limit, unviewed_only)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }

    /// Fetches all active status stories posted by a specific contact.
    pub async fn get_stories_by_author(
        &self,
        author_jid: &str,
    ) -> Result<Vec<baileys_store::StoredStory>, CoreError> {
        self.store
            .get_stories_by_author(author_jid)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }

    /// Edits a previously sent message by sending a ProtocolMessage of type MESSAGE_EDIT.
    pub async fn edit_message(
        &self,
        chat_jid: &str,
        target_message_id: &str,
        new_text: &str,
    ) -> Result<String, CoreError> {
        let normalized_jid = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid)
        } else {
            chat_jid.to_string()
        };
        let edit_proto = crate::ephemeral::build_edit_message(
            &normalized_jid,
            target_message_id,
            true,
            new_text,
        );

        let env_id = self
            .send_message(&normalized_jid, edit_proto.clone())
            .await?;

        // Update local database & emit MessageEdited
        let mut raw_bytes = Vec::new();
        let _ = edit_proto.encode(&mut raw_bytes);
        let _ = self
            .store
            .update_message_content(
                &normalized_jid,
                target_message_id,
                new_text,
                Some(&raw_bytes),
            )
            .await;

        self.events.emit(BaileysEvent::MessageEdited {
            chat_jid: normalized_jid,
            message_id: target_message_id.to_string(),
            new_text: new_text.to_string(),
        });

        Ok(env_id)
    }

    /// Revokes (deletes for everyone) a previously sent message by sending a ProtocolMessage of type REVOKE.
    pub async fn revoke_message(
        &self,
        chat_jid: &str,
        target_message_id: &str,
    ) -> Result<String, CoreError> {
        let normalized_jid = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid)
        } else {
            chat_jid.to_string()
        };
        let creds = self
            .store
            .get_creds()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?
            .unwrap_or_else(AuthCreds::init);

        let participant = if normalized_jid.ends_with("@g.us") {
            creds.me_jid.clone()
        } else {
            None
        };

        let revoke_proto = crate::ephemeral::build_revoke_message(
            &normalized_jid,
            target_message_id,
            true,
            participant.as_deref(),
        );

        let env_id = self.send_message(&normalized_jid, revoke_proto).await?;

        // Update local database & emit MessageRevoked
        let _ = self
            .store
            .revoke_message(&normalized_jid, target_message_id)
            .await;

        self.events.emit(BaileysEvent::MessageRevoked {
            chat_jid: normalized_jid,
            message_id: target_message_id.to_string(),
            participant,
        });

        Ok(env_id)
    }

    /// Configures disappearing messages ephemeral timer for a 1:1 or group chat.
    /// Duration in seconds: 0 (Off), 86400 (24h), 604800 (7d), 7776000 (90d).
    pub async fn set_chat_ephemeral(
        &self,
        chat_jid: &str,
        expiration_seconds: u32,
    ) -> Result<(), CoreError> {
        let normalized_jid = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid)
        } else {
            chat_jid.to_string()
        };
        if normalized_jid.ends_with("@g.us") {
            let iq_node =
                crate::ephemeral::build_group_ephemeral_iq(&normalized_jid, expiration_seconds);
            self.send_iq(iq_node, Duration::from_secs(10)).await?;
        } else {
            let msg = crate::ephemeral::build_ephemeral_setting_message(
                &normalized_jid,
                expiration_seconds,
            );
            self.send_message(&normalized_jid, msg).await?;
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let _ = self
            .store
            .update_chat_ephemeral(&normalized_jid, expiration_seconds, Some(now))
            .await;

        self.events.emit(BaileysEvent::ChatEphemeralUpdate {
            chat_jid: normalized_jid,
            ephemeral_expiration: expiration_seconds,
        });

        Ok(())
    }

    /// Retrieves current ephemeral disappearing timer settings for a chat (seconds, timestamp).
    pub async fn get_chat_ephemeral(
        &self,
        chat_jid: &str,
    ) -> Result<Option<(u32, Option<i64>)>, CoreError> {
        let normalized_jid = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid)
        } else {
            chat_jid.to_string()
        };
        self.store
            .get_chat_ephemeral(&normalized_jid)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }

    // ==========================================
    // Track 9: Labels & WhatsApp Business Protocol
    // ==========================================

    /// Returns all WhatsApp Business labels from local store.
    pub async fn get_labels(&self) -> Result<Vec<StoredLabel>, CoreError> {
        self.store
            .get_labels()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }

    /// Returns a specific label by ID from local store.
    pub async fn get_label(&self, id: &str) -> Result<Option<StoredLabel>, CoreError> {
        self.store
            .get_label(id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }

    /// Creates or updates a WhatsApp Business label, saves to SQLite, and dispatches AppState Syncd patch.
    pub async fn upsert_label(
        &self,
        id: Option<&str>,
        name: &str,
        color: i32,
        predefined_id: Option<i32>,
    ) -> Result<String, CoreError> {
        let label_id = match id {
            Some(existing_id) => existing_id.to_string(),
            None => format!("{}", rand::random::<u32>()),
        };

        let label = StoredLabel {
            id: label_id.clone(),
            name: name.to_string(),
            color,
            predefined_id,
            deleted: false,
        };

        // 1. Update local database immediately
        self.store
            .upsert_label(&label)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Emit event
        self.events.emit(BaileysEvent::LabelEdit {
            label: label.clone(),
        });

        // 3. Dispatch wire-level Syncd patch
        let id_clone = label_id.clone();
        let name_clone = name.to_string();
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_LOW,
            move |key_id, key_data, state_hash, ver| {
                crate::labels::create_label_edit_patch(
                    &id_clone,
                    &name_clone,
                    color,
                    predefined_id,
                    false,
                    key_id,
                    key_data,
                    state_hash,
                    ver,
                )
            },
        )
        .await;

        Ok(label_id)
    }

    /// Deletes a WhatsApp Business label, removes associations in SQLite, and dispatches AppState Syncd patch.
    pub async fn delete_label(&self, id: &str) -> Result<(), CoreError> {
        // 1. Fetch label or create tombstone
        let existing = self.store.get_label(id).await.unwrap_or(None);
        let name = existing.as_ref().map(|l| l.name.as_str()).unwrap_or("");
        let color = existing.as_ref().map(|l| l.color).unwrap_or(0);
        let predefined_id = existing.as_ref().and_then(|l| l.predefined_id);

        // 2. Update local database immediately
        self.store
            .delete_label(id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 3. Emit event
        self.events.emit(BaileysEvent::LabelEdit {
            label: StoredLabel {
                id: id.to_string(),
                name: name.to_string(),
                color,
                predefined_id,
                deleted: true,
            },
        });

        // 4. Dispatch wire-level Syncd patch
        let id_str = id.to_string();
        let name_str = name.to_string();
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_LOW,
            move |key_id, key_data, state_hash, ver| {
                crate::labels::create_label_edit_patch(
                    &id_str,
                    &name_str,
                    color,
                    predefined_id,
                    true,
                    key_id,
                    key_data,
                    state_hash,
                    ver,
                )
            },
        )
        .await;

        Ok(())
    }

    /// Associates a label with a chat (group or 1-on-1).
    pub async fn add_chat_label(&self, chat_jid: &str, label_id: &str) -> Result<(), CoreError> {
        let normalized = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid.trim_start_matches('+'))
        } else {
            chat_jid.to_string()
        };

        // 1. Update local database
        self.store
            .add_chat_label(&normalized, label_id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Emit event
        self.events.emit(BaileysEvent::ChatLabelUpdate {
            chat_jid: normalized.clone(),
            label_id: label_id.to_string(),
            labeled: true,
        });

        // 3. Dispatch wire-level Syncd patch
        let lid = label_id.to_string();
        let jid = normalized.clone();
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_LOW,
            move |key_id, key_data, state_hash, ver| {
                crate::labels::create_chat_label_patch(
                    &lid, &jid, true, key_id, key_data, state_hash, ver,
                )
            },
        )
        .await;

        Ok(())
    }

    /// Removes a label association from a chat.
    pub async fn remove_chat_label(&self, chat_jid: &str, label_id: &str) -> Result<(), CoreError> {
        let normalized = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid.trim_start_matches('+'))
        } else {
            chat_jid.to_string()
        };

        // 1. Update local database
        self.store
            .remove_chat_label(&normalized, label_id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Emit event
        self.events.emit(BaileysEvent::ChatLabelUpdate {
            chat_jid: normalized.clone(),
            label_id: label_id.to_string(),
            labeled: false,
        });

        // 3. Dispatch wire-level Syncd patch
        let lid = label_id.to_string();
        let jid = normalized.clone();
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_LOW,
            move |key_id, key_data, state_hash, ver| {
                crate::labels::create_chat_label_patch(
                    &lid, &jid, false, key_id, key_data, state_hash, ver,
                )
            },
        )
        .await;

        Ok(())
    }

    /// Gets all label IDs associated with a chat.
    pub async fn get_chat_labels(&self, chat_jid: &str) -> Result<Vec<String>, CoreError> {
        let normalized = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid.trim_start_matches('+'))
        } else {
            chat_jid.to_string()
        };
        self.store
            .get_chat_labels(&normalized)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }

    /// Gets all chat JIDs associated with a label.
    pub async fn get_chats_by_label(&self, label_id: &str) -> Result<Vec<String>, CoreError> {
        self.store
            .get_chats_by_label(label_id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }

    /// Associates a label with an individual message.
    pub async fn add_message_label(
        &self,
        chat_jid: &str,
        message_id: &str,
        label_id: &str,
    ) -> Result<(), CoreError> {
        let normalized = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid.trim_start_matches('+'))
        } else {
            chat_jid.to_string()
        };

        // 1. Update local database
        self.store
            .add_message_label(&normalized, message_id, label_id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Emit event
        self.events.emit(BaileysEvent::MessageLabelUpdate {
            chat_jid: normalized.clone(),
            message_id: message_id.to_string(),
            label_id: label_id.to_string(),
            labeled: true,
        });

        // 3. Dispatch wire-level Syncd patch
        let lid = label_id.to_string();
        let jid = normalized.clone();
        let mid = message_id.to_string();
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_LOW,
            move |key_id, key_data, state_hash, ver| {
                crate::labels::create_message_label_patch(
                    &lid, &jid, &mid, true, key_id, key_data, state_hash, ver,
                )
            },
        )
        .await;

        Ok(())
    }

    /// Removes a label association from an individual message.
    pub async fn remove_message_label(
        &self,
        chat_jid: &str,
        message_id: &str,
        label_id: &str,
    ) -> Result<(), CoreError> {
        let normalized = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid.trim_start_matches('+'))
        } else {
            chat_jid.to_string()
        };

        // 1. Update local database
        self.store
            .remove_message_label(&normalized, message_id, label_id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Emit event
        self.events.emit(BaileysEvent::MessageLabelUpdate {
            chat_jid: normalized.clone(),
            message_id: message_id.to_string(),
            label_id: label_id.to_string(),
            labeled: false,
        });

        // 3. Dispatch wire-level Syncd patch
        let lid = label_id.to_string();
        let jid = normalized.clone();
        let mid = message_id.to_string();
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_LOW,
            move |key_id, key_data, state_hash, ver| {
                crate::labels::create_message_label_patch(
                    &lid, &jid, &mid, false, key_id, key_data, state_hash, ver,
                )
            },
        )
        .await;

        Ok(())
    }

    /// Gets all label IDs associated with a message.
    pub async fn get_message_labels(
        &self,
        chat_jid: &str,
        message_id: &str,
    ) -> Result<Vec<String>, CoreError> {
        let normalized = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid.trim_start_matches('+'))
        } else {
            chat_jid.to_string()
        };
        self.store
            .get_message_labels(&normalized, message_id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }

    /// Gets all message IDs associated with a label.
    pub async fn get_messages_by_label(&self, label_id: &str) -> Result<Vec<String>, CoreError> {
        self.store
            .get_messages_by_label(label_id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }

    // ==========================================
    // Track 10: Rich Messages & Attachments
    // ==========================================

    /// Sends a static location message to a contact or group.
    pub async fn send_location(
        &self,
        to_jid: &str,
        latitude: f64,
        longitude: f64,
        name: Option<&str>,
        address: Option<&str>,
        url: Option<&str>,
    ) -> Result<String, CoreError> {
        let proto = crate::rich_messages::build_location_message(
            latitude, longitude, name, address, url, None,
        );
        self.send_message(to_jid, proto).await
    }

    /// Sends a real-time live location message to a contact or group.
    pub async fn send_live_location(
        &self,
        to_jid: &str,
        latitude: f64,
        longitude: f64,
        accuracy_in_meters: Option<u32>,
        caption: Option<&str>,
    ) -> Result<String, CoreError> {
        let proto = crate::rich_messages::build_live_location_message(
            latitude,
            longitude,
            accuracy_in_meters,
            None,
            None,
            caption,
            Some(1),
            Some(0),
            None,
        );
        self.send_message(to_jid, proto).await
    }

    /// Sends a single contact card (vCard 3.0) to a contact or group.
    pub async fn send_contact(
        &self,
        to_jid: &str,
        display_name: &str,
        phone_number: &str,
    ) -> Result<String, CoreError> {
        let vcard = crate::rich_messages::build_vcard_string(display_name, phone_number, None);
        let proto = crate::rich_messages::build_contact_message(display_name, &vcard);
        self.send_message(to_jid, proto).await
    }

    /// Sends a contact card with custom raw vCard text to a contact or group.
    pub async fn send_vcard(
        &self,
        to_jid: &str,
        display_name: &str,
        vcard: &str,
    ) -> Result<String, CoreError> {
        let proto = crate::rich_messages::build_contact_message(display_name, vcard);
        self.send_message(to_jid, proto).await
    }

    /// Sends an interactive message (Buttons, List menus, Native Flows, Carousel cards) to a contact or group.
    pub async fn send_interactive_message(
        &self,
        to_jid: &str,
        builder: crate::interactive::InteractiveMessageBuilder,
    ) -> Result<String, CoreError> {
        let msg = builder.build();
        self.send_message(to_jid, msg).await
    }

    /// Sends a multi-contact card to a contact or group.
    pub async fn send_contacts_array(
        &self,
        to_jid: &str,
        display_name: &str,
        contacts: Vec<(String, String)>,
    ) -> Result<String, CoreError> {
        let proto_contacts: Vec<baileys_proto::proto::message::ContactMessage> = contacts
            .into_iter()
            .map(|(name, phone)| {
                let vcard = crate::rich_messages::build_vcard_string(&name, &phone, None);
                baileys_proto::proto::message::ContactMessage {
                    display_name: Some(name),
                    vcard: Some(vcard),
                    ..Default::default()
                }
            })
            .collect();
        let proto =
            crate::rich_messages::build_contacts_array_message(display_name, proto_contacts);
        self.send_message(to_jid, proto).await
    }

    /// Sends a group invitation preview message to a contact or group.
    pub async fn send_group_invite(
        &self,
        to_jid: &str,
        group_jid: &str,
        invite_code: &str,
        invite_expiration: i64,
        group_name: &str,
        caption: Option<&str>,
    ) -> Result<String, CoreError> {
        let proto = crate::rich_messages::build_group_invite_message(
            group_jid,
            invite_code,
            invite_expiration,
            group_name,
            caption,
            None,
        );
        self.send_message(to_jid, proto).await
    }

    /// Sends a view-once wrapped message (e.g. image, video) to a contact or group.
    pub async fn send_view_once(
        &self,
        to_jid: &str,
        inner_proto: Message,
        v2: bool,
    ) -> Result<String, CoreError> {
        let proto = crate::rich_messages::wrap_view_once(inner_proto, v2);
        self.send_message(to_jid, proto).await
    }

    /// Sends a calendar event message (`EventMessage`) to a contact or group.
    pub async fn send_event(
        &self,
        to_jid: &str,
        name: &str,
        description: Option<&str>,
        start_time: i64,
        end_time: Option<i64>,
        location_name: Option<&str>,
    ) -> Result<String, CoreError> {
        let proto = crate::proto_builders::build_event_message(
            name,
            description,
            start_time,
            end_time,
            location_name,
            None,
            false,
        );
        self.send_message(to_jid, proto).await
    }

    /// Sends an album metadata message for grouping multiple media items.
    pub async fn send_album(
        &self,
        to_jid: &str,
        expected_image_count: u32,
        expected_video_count: u32,
    ) -> Result<String, CoreError> {
        let proto =
            crate::proto_builders::build_album_message(expected_image_count, expected_video_count);
        self.send_message(to_jid, proto).await
    }

    // ==========================================
    // Track 11: Advanced Group Administration
    // ==========================================

    /// Fetches parsed typed metadata and participant list for a group.
    pub async fn group_fetch_metadata(
        &self,
        group_jid: &str,
    ) -> Result<crate::groups::GroupMetadata, CoreError> {
        let node = self.group_metadata(group_jid).await?;
        let meta = crate::groups::parse_group_metadata(&node)?;
        let chat = baileys_store::StoredChat {
            jid: meta.id.clone(),
            name: Some(meta.subject.clone()),
            unread_count: 0,
            last_message_timestamp: meta.creation as u64,
            last_message_summary: None,
            pinned: false,
            archived: false,
            mute_until: None,
            ephemeral_expiration: 0,
            ephemeral_setting_timestamp: None,
        };
        let _ = self.store.save_chat(&chat).await;
        Ok(meta)
    }

    /// Fetches all participating groups the authenticated account belongs to.
    /// Also caches their basic conversation records into SQLite.
    pub async fn group_fetch_all_participating(
        &self,
    ) -> Result<Vec<crate::groups::GroupMetadata>, CoreError> {
        let iq_node = crate::groups::build_fetch_participating_groups_iq();
        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        let groups = crate::groups::parse_participating_groups(&res)?;

        // Cache into local database
        for g in &groups {
            let chat = baileys_store::StoredChat {
                jid: g.id.clone(),
                name: Some(g.subject.clone()),
                unread_count: 0,
                last_message_timestamp: g.creation as u64,
                last_message_summary: None,
                pinned: false,
                archived: false,
                mute_until: None,
                ephemeral_expiration: g.ephemeral_duration.unwrap_or(0),
                ephemeral_setting_timestamp: None,
            };
            let _ = self.store.save_chat(&chat).await;
        }

        Ok(groups)
    }

    /// Configures membership approval mode for a group ("on" or "off").
    pub async fn group_join_approval_mode(
        &self,
        group_jid: &str,
        mode: &str,
    ) -> Result<(), CoreError> {
        let iq_node = crate::groups::build_group_join_approval_mode_iq(group_jid, mode);
        self.send_iq(iq_node, Duration::from_secs(10)).await?;
        Ok(())
    }

    /// Queries all pending membership approval requests for a group.
    pub async fn group_request_participants_list(
        &self,
        group_jid: &str,
    ) -> Result<Vec<crate::groups::GroupJoinRequest>, CoreError> {
        let iq_node = crate::groups::build_group_request_participants_list_iq(group_jid);
        let res = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        crate::groups::parse_group_join_requests(&res)
    }

    /// Approves or rejects pending membership approval requests for a group.
    pub async fn group_request_participants_update(
        &self,
        group_jid: &str,
        participants: &[String],
        action: &str,
    ) -> Result<Vec<(String, String)>, CoreError> {
        let iq_node = crate::groups::build_group_request_participants_update_iq(
            group_jid,
            participants,
            action,
        );
        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        let mut results = Vec::new();
        if let Some(act_node) = res
            .get_child("membership_requests_action")
            .and_then(|m| m.get_child(action))
        {
            for child in act_node.get_children() {
                if child.tag == "participant" {
                    let jid = child.get_attr("jid").unwrap_or("").to_string();
                    let status = child
                        .get_attr("error")
                        .or_else(|| child.get_attr("code"))
                        .unwrap_or("200")
                        .to_string();
                    results.push((jid, status));
                }
            }
        }
        Ok(results)
    }

    /// Configures member add mode for a group ("admin_add" or "all_member_add").
    pub async fn group_member_add_mode(
        &self,
        group_jid: &str,
        mode: &str,
    ) -> Result<(), CoreError> {
        let iq_node = crate::groups::build_group_member_add_mode_iq(group_jid, mode);
        self.send_iq(iq_node, Duration::from_secs(10)).await?;
        Ok(())
    }

    /// Revokes a participant's v4 group invite.
    pub async fn group_revoke_invite_v4(
        &self,
        group_jid: &str,
        invited_jid: &str,
    ) -> Result<(), CoreError> {
        let iq_node = crate::groups::build_group_revoke_invite_v4_iq(group_jid, invited_jid);
        self.send_iq(iq_node, Duration::from_secs(10)).await?;
        Ok(())
    }

    /// Accepts a v4 group invite directly via IQ stanza. Returns the joined group JID.
    pub async fn group_accept_invite_v4(
        &self,
        group_jid: &str,
        invite_code: &str,
        invite_expiration: i64,
        admin_jid: &str,
    ) -> Result<String, CoreError> {
        let iq_node = crate::groups::build_group_accept_invite_v4_iq(
            group_jid,
            invite_code,
            invite_expiration,
            admin_jid,
        );
        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        let joined_jid = res.get_attr("from").unwrap_or(group_jid).to_string();
        Ok(joined_jid)
    }

    // ==========================================
    // Track 12: Business Catalog, Products & Orders
    // ==========================================

    /// Queries the WhatsApp Business product catalog for a merchant JID.
    /// Also automatically caches all returned products into local SQLite storage.
    pub async fn get_catalog(
        &self,
        biz_jid: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<crate::commerce::CatalogResult, CoreError> {
        let iq_node = crate::commerce::build_get_catalog_iq(biz_jid, limit, cursor);
        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        let catalog_res = crate::commerce::parse_catalog_response(&res, biz_jid)?;

        // Cache into local database
        for prod in &catalog_res.products {
            let stored: baileys_store::StoredCatalogProduct = prod.clone().into();
            let _ = self.store.save_catalog_product(&stored).await;
        }

        Ok(catalog_res)
    }

    /// Queries a single product by product ID for a merchant.
    /// Also caches the product into local SQLite storage.
    pub async fn get_product(
        &self,
        biz_jid: &str,
        product_id: &str,
    ) -> Result<crate::commerce::CatalogProduct, CoreError> {
        let iq_node = crate::commerce::build_get_product_iq(biz_jid, product_id);
        let res = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        let prod_node = res
            .get_child("response")
            .and_then(|r| r.get_child("product"))
            .or_else(|| res.get_child("product"))
            .ok_or_else(|| CoreError::Protocol("Missing product node in response".to_string()))?;

        let prod = crate::commerce::parse_product_node(prod_node, biz_jid)?;
        let stored: baileys_store::StoredCatalogProduct = prod.clone().into();
        let _ = self.store.save_catalog_product(&stored).await;

        Ok(prod)
    }

    /// Queries product collections/categories for a merchant.
    /// Also caches the collections into local SQLite storage.
    pub async fn get_collections(
        &self,
        biz_jid: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<crate::commerce::CollectionsResult, CoreError> {
        let iq_node = crate::commerce::build_get_collections_iq(biz_jid, limit, cursor);
        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        let col_res = crate::commerce::parse_collections_response(&res, biz_jid)?;

        for col in &col_res.collections {
            let stored: baileys_store::StoredCatalogCollection = col.clone().into();
            let _ = self.store.save_catalog_collection(&stored).await;
        }

        Ok(col_res)
    }

    /// Sends a product card message to a recipient.
    pub async fn send_product_message(
        &self,
        to_jid: &str,
        biz_owner_jid: &str,
        product: &crate::commerce::CatalogProduct,
        caption: Option<&str>,
        footer: Option<&str>,
    ) -> Result<String, CoreError> {
        let proto = crate::commerce::build_product_message(biz_owner_jid, product, caption, footer);
        self.send_message(to_jid, proto).await
    }

    /// Sends an order card message to a recipient and saves it in SQLite.
    pub async fn send_order_message(
        &self,
        to_jid: &str,
        seller_jid: &str,
        order_id: &str,
        title: &str,
        item_count: i32,
        total_amount_1000: i64,
        currency: &str,
        message: Option<&str>,
        thumbnail: Option<Vec<u8>>,
    ) -> Result<String, CoreError> {
        let proto = crate::commerce::build_order_message(
            order_id,
            seller_jid,
            title,
            item_count,
            total_amount_1000,
            currency,
            message,
            thumbnail,
        );

        let msg_id = self.send_message(to_jid, proto).await?;

        // Cache order in local database
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let stored_order = baileys_store::StoredOrder {
            order_id: order_id.to_string(),
            chat_jid: to_jid.to_string(),
            seller_jid: seller_jid.to_string(),
            item_count,
            status: "INQUIRY".to_string(),
            surface: Some("CATALOG".to_string()),
            order_title: Some(title.to_string()),
            message: message.map(|s| s.to_string()),
            total_amount_1000,
            total_currency_code: currency.to_string(),
            token: None,
            created_at: now,
        };
        let _ = self.store.save_order(&stored_order).await;

        Ok(msg_id)
    }

    /// Retrieves cached products from local SQLite store for a business JID.
    pub async fn get_cached_catalog(
        &self,
        biz_jid: &str,
    ) -> Result<Vec<baileys_store::StoredCatalogProduct>, CoreError> {
        self.store
            .get_catalog_products(biz_jid)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }

    /// Retrieves cached collections from local SQLite store for a business JID.
    pub async fn get_cached_collections(
        &self,
        biz_jid: &str,
    ) -> Result<Vec<baileys_store::StoredCatalogCollection>, CoreError> {
        self.store
            .get_catalog_collections(biz_jid)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }

    /// Retrieves cached orders for a chat from local SQLite store.
    pub async fn get_cached_orders(
        &self,
        chat_jid: &str,
        limit: u32,
    ) -> Result<Vec<baileys_store::StoredOrder>, CoreError> {
        self.store
            .get_orders(chat_jid, limit)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }

    // ==========================================
    // Track 13: Multi-Device Companions & USync
    // ==========================================

    /// Queries linked companion devices for a user JID via USync protocol.
    /// Also updates local SQLite storage cache.
    pub async fn get_user_devices(&self, user_jid: &str) -> Result<Vec<u32>, CoreError> {
        let normalized = if !user_jid.contains('@') {
            format!("{}@{}", user_jid, S_WHATSAPP_NET)
        } else {
            user_jid.to_string()
        };
        let norm_user = baileys_binary::jid::jid_normalized_user(&normalized);

        // Baileys parity: Check SQLite persistent cache first!
        if let Ok(cached) = self.store.get_user_devices(&normalized).await {
            if !cached.is_empty() {
                return Ok(cached);
            }
        }
        if let Ok(cached) = self.store.get_user_devices(&norm_user).await {
            if !cached.is_empty() {
                return Ok(cached);
            }
        }

        // Cache miss: Query WhatsApp USync gateway
        let iq_node = crate::usync::build_usync_devices_iq(&[&normalized]);
        match self.send_iq(iq_node, Duration::from_secs(10)).await {
            Ok(res) => {
                if let Ok(user_devs) = crate::usync::parse_usync_devices_response(&res) {
                    if let Some(ud) = user_devs.into_iter().find(|u| {
                        baileys_binary::jid::jid_normalized_user(&u.user_jid) == norm_user
                            || u.user_jid == normalized
                    }) {
                        let mut ids: Vec<u32> = ud.devices.iter().map(|d| d.id).collect();
                        if !ids.contains(&0) {
                            ids.insert(0, 0);
                        }
                        let _ = self.store.save_user_devices(&normalized, &ids).await;
                        let _ = self.store.save_user_devices(&norm_user, &ids).await;
                        return Ok(ids);
                    }
                }
            }
            Err(e) => {
                debug!(
                    "USync device query failed for {}: {}, falling back to local cache",
                    normalized, e
                );
            }
        }

        let cached = self
            .store
            .get_user_devices(&normalized)
            .await
            .unwrap_or_default();
        if cached.is_empty() {
            Ok(vec![0])
        } else {
            Ok(cached)
        }
    }

    /// Queries all active companion devices linked to our current account.
    pub async fn get_paired_devices(&self) -> Result<Vec<crate::usync::DeviceRecord>, CoreError> {
        let creds = self
            .store
            .get_creds()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?
            .ok_or_else(|| CoreError::Auth("No active session credentials found".into()))?;

        let me_jid = creds.me_jid.as_deref().unwrap_or("");
        let user_jid = if let Some((u, _)) = me_jid.split_once(':') {
            format!("{}@{}", u, S_WHATSAPP_NET)
        } else if let Some((u, _)) = me_jid.split_once('@') {
            format!("{}@{}", u, S_WHATSAPP_NET)
        } else {
            return Err(CoreError::Auth("Invalid me_jid format".into()));
        };

        let iq_node = crate::usync::build_usync_devices_iq(&[&user_jid]);
        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        let user_devs = crate::usync::parse_usync_devices_response(&res)?;

        if let Some(ud) = user_devs.into_iter().find(|u| u.user_jid == user_jid) {
            let ids: Vec<u32> = ud.devices.iter().map(|d| d.id).collect();
            let _ = self.store.save_user_devices(&user_jid, &ids).await;
            Ok(ud.devices)
        } else {
            let cached_ids = self
                .store
                .get_user_devices(&user_jid)
                .await
                .map_err(|e| CoreError::Store(e.to_string()))?;
            Ok(cached_ids
                .into_iter()
                .map(|id| crate::usync::DeviceRecord {
                    id,
                    key_index: None,
                })
                .collect())
        }
    }

    /// Revokes/unlinks a companion device from our account by device ID.
    pub async fn remove_companion_device(&self, device_id: u32) -> Result<(), CoreError> {
        let creds = self
            .store
            .get_creds()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?
            .ok_or_else(|| CoreError::Auth("No active session credentials found".into()))?;

        let me_jid = creds.me_jid.as_deref().unwrap_or("");
        let user_phone = if let Some((u, _)) = me_jid.split_once(':') {
            u
        } else if let Some((u, _)) = me_jid.split_once('@') {
            u
        } else {
            return Err(CoreError::Auth("Invalid me_jid format".into()));
        };

        let dev_jid = format!("{}:{}@{}", user_phone, device_id, S_WHATSAPP_NET);
        let user_jid = format!("{}@{}", user_phone, S_WHATSAPP_NET);

        let iq_node = crate::usync::build_remove_device_iq(&dev_jid);
        self.send_iq(iq_node, Duration::from_secs(10)).await?;

        let _ = self.store.remove_user_device(&user_jid, device_id).await;
        Ok(())
    }

    // ==========================================
    // Track 14: History Fetch, Chat Actions & Media Notes
    // ==========================================

    /// Requests on-demand message history for a chat via w:sync:history IQ.
    pub async fn fetch_message_history(
        &self,
        chat_jid: &str,
        count: u32,
        oldest_msg_id: &str,
        oldest_from_me: bool,
        oldest_timestamp: u64,
    ) -> Result<String, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let req_node = BinaryNode::new("history_sync_request")
            .with_attr("chat_jid", chat_jid)
            .with_attr("count", count.to_string())
            .with_attr("oldest_msg_id", oldest_msg_id)
            .with_attr(
                "oldest_msg_from_me",
                if oldest_from_me { "true" } else { "false" },
            )
            .with_attr("oldest_msg_timestamp", oldest_timestamp.to_string());

        let iq_node = BinaryNode::new("iq")
            .with_attr("to", S_WHATSAPP_NET)
            .with_attr("type", "set")
            .with_attr("xmlns", "w:sync:history")
            .with_attr("id", &iq_id)
            .with_child(req_node);

        self.send_iq(iq_node, Duration::from_secs(15)).await?;
        Ok(iq_id)
    }

    /// Clears message history for a chat while optionally keeping starred messages.
    pub async fn chat_modify_clear(
        &self,
        chat_jid: &str,
        keep_starred: bool,
    ) -> Result<(), CoreError> {
        let normalized = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid.trim_start_matches('+'))
        } else {
            chat_jid.to_string()
        };

        // 1. Update local database immediately
        self.store
            .clear_chat_messages(&normalized, keep_starred)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Dispatch wire-level Syncd patch
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_HIGH,
            |key_id, key_data, state_hash, ver| {
                crate::syncd::create_clear_chat_patch(
                    &normalized,
                    key_id,
                    key_data,
                    state_hash,
                    ver,
                )
            },
        )
        .await;

        Ok(())
    }

    /// Deletes a chat conversation thread from WhatsApp and local SQLite store.
    pub async fn chat_modify_delete(&self, chat_jid: &str) -> Result<(), CoreError> {
        let normalized = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid.trim_start_matches('+'))
        } else {
            chat_jid.to_string()
        };

        // 1. Update local database immediately
        self.store
            .delete_chat(&normalized)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Dispatch wire-level Syncd patch
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_HIGH,
            |key_id, key_data, state_hash, ver| {
                crate::syncd::create_delete_chat_patch(
                    &normalized,
                    key_id,
                    key_data,
                    state_hash,
                    ver,
                )
            },
        )
        .await;

        Ok(())
    }

    /// Marks a chat as read or unread across devices.
    pub async fn chat_modify_mark_unread(
        &self,
        chat_jid: &str,
        unread: bool,
    ) -> Result<(), CoreError> {
        let normalized = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid.trim_start_matches('+'))
        } else {
            chat_jid.to_string()
        };

        // 1. Update local database immediately
        self.store
            .update_chat_unread(&normalized, unread)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Dispatch wire-level Syncd patch
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_LOW,
            |key_id, key_data, state_hash, ver| {
                crate::syncd::create_mark_chat_unread_patch(
                    &normalized,
                    unread,
                    key_id,
                    key_data,
                    state_hash,
                    ver,
                )
            },
        )
        .await;

        Ok(())
    }

    /// Sends a document file attachment message.
    pub async fn send_document(
        &self,
        to_jid: &str,
        doc_bytes: &[u8],
        filename: &str,
        mimetype: &str,
        title: Option<&str>,
        page_count: Option<u32>,
        caption: Option<&str>,
    ) -> Result<String, CoreError> {
        let enc =
            baileys_media::encrypt_media_bytes(doc_bytes, None, baileys_media::MediaType::Document)
                .map_err(|e| CoreError::Crypto(e.to_string()))?;

        let msg = crate::rich_messages::build_document_message(
            None,
            None,
            Some(&enc.media_key),
            Some(&enc.file_sha256),
            Some(&enc.file_enc_sha256),
            doc_bytes.len() as u64,
            filename,
            mimetype,
            title,
            page_count,
            caption,
            None,
        );
        self.send_message(to_jid, msg).await
    }

    /// Sends a video message (or GIF animation).
    pub async fn send_video(
        &self,
        to_jid: &str,
        video_bytes: &[u8],
        caption: Option<&str>,
        is_gif: bool,
        seconds: Option<u32>,
    ) -> Result<String, CoreError> {
        let enc =
            baileys_media::encrypt_media_bytes(video_bytes, None, baileys_media::MediaType::Video)
                .map_err(|e| CoreError::Crypto(e.to_string()))?;

        let msg = crate::rich_messages::build_video_message(
            None,
            None,
            Some(&enc.media_key),
            Some(&enc.file_sha256),
            Some(&enc.file_enc_sha256),
            video_bytes.len() as u64,
            caption,
            is_gif,
            seconds,
            None,
        );
        self.send_message(to_jid, msg).await
    }

    /// Sends a sticker message (WebP format).
    pub async fn send_sticker(
        &self,
        to_jid: &str,
        sticker_bytes: &[u8],
        is_animated: bool,
    ) -> Result<String, CoreError> {
        let enc = baileys_media::encrypt_media_bytes(
            sticker_bytes,
            None,
            baileys_media::MediaType::Sticker,
        )
        .map_err(|e| CoreError::Crypto(e.to_string()))?;

        let msg = crate::rich_messages::build_sticker_message(
            None,
            None,
            Some(&enc.media_key),
            Some(&enc.file_sha256),
            Some(&enc.file_enc_sha256),
            sticker_bytes.len() as u64,
            is_animated,
            false,
        );
        self.send_message(to_jid, msg).await
    }

    /// Dynamically requests an 8-character phone number pairing code if not authenticated.
    pub async fn request_pairing_code(&self, phone: &str) -> Result<String, CoreError> {
        let creds = match self
            .store
            .get_creds()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?
        {
            Some(c) => c,
            None => {
                let fresh = AuthCreds::init();
                self.store
                    .save_creds(&fresh)
                    .await
                    .map_err(|e| CoreError::Store(e.to_string()))?;
                fresh
            }
        };
        if creds.me_jid.is_some() {
            return Err(CoreError::Protocol(
                "Client is already authenticated".into(),
            ));
        }

        let clean_phone = phone.replace(['+', ' ', '-'], "");
        let state = Arc::new(CompanionPairingState::new(&clean_phone));
        let hello_node = state.build_companion_hello_iq(&creds, &self.config)?;

        // Ensure connection is running
        if !self.is_running.load(Ordering::SeqCst) {
            let _ = self.connect().await;
        }

        // Wait up to 10 seconds for the socket to become ready
        let mut socket_opt = {
            let lock = self.socket.lock().await;
            lock.clone()
        };
        if socket_opt.is_none() {
            let start = std::time::Instant::now();
            while socket_opt.is_none() && start.elapsed() < Duration::from_secs(10) {
                tokio::time::sleep(Duration::from_millis(250)).await;
                let lock = self.socket.lock().await;
                socket_opt = lock.clone();
            }
        }
        let socket = socket_opt
            .ok_or_else(|| CoreError::Network("Client not connected to WhatsApp gateway".into()))?;
        socket.send_node(hello_node).await?;

        let display_code = if state.pairing_code.len() == 8 {
            format!(
                "{}-{}",
                &state.pairing_code[0..4],
                &state.pairing_code[4..8]
            )
        } else {
            state.pairing_code.clone()
        };

        {
            let mut lock = self.companion_state.lock().await;
            *lock = Some(Arc::clone(&state));
        }

        self.events.emit(BaileysEvent::PairingCode {
            code: display_code.clone(),
        });

        Ok(display_code)
    }

    /// Fetches the account reachout timelock timestamp via WhatsApp GraphQL MEX query.
    pub async fn fetch_account_reachout_timelock(
        &self,
    ) -> Result<Option<(bool, Option<String>, Option<u64>)>, CoreError> {
        let vars = serde_json::json!({});
        let iq = crate::newsletter::build_mex_iq(crate::newsletter::QUERY_REACHOUT_TIMELOCK, vars);
        let res = self.send_iq(iq, Duration::from_secs(12)).await?;
        let data = crate::newsletter::parse_mex_response(&res)?;
        Ok(crate::newsletter::parse_reachout_timelock_response(&data))
    }

    /// Fetches the new chat message cap limit via WhatsApp GraphQL MEX query.
    pub async fn fetch_new_chat_message_cap(&self) -> Result<Option<serde_json::Value>, CoreError> {
        let vars = serde_json::json!({});
        let iq =
            crate::newsletter::build_mex_iq(crate::newsletter::QUERY_MESSAGE_CAPPING_INFO, vars);
        let res = self.send_iq(iq, Duration::from_secs(12)).await?;
        let data = crate::newsletter::parse_mex_response(&res)?;
        Ok(crate::newsletter::parse_message_capping_response(&data))
    }

    /// Creates a WhatsApp call link for an audio or video call.
    pub async fn create_call_link(&self, is_video: bool) -> Result<Option<String>, CoreError> {
        let media_type = if is_video { "video" } else { "audio" };
        let iq = crate::call::build_create_call_link_node(media_type, None);
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        Ok(crate::call::parse_call_link_response(&res))
    }

    /// Cleans dirty bits on WhatsApp server (e.g. "account_sync" or "groups").
    pub async fn clean_dirty_bits(
        &self,
        dirty_type: &str,
        from_timestamp: Option<u64>,
    ) -> Result<(), CoreError> {
        let msg_id = format!("clean_{:X}", rand::random::<u64>());
        let mut clean_node = BinaryNode::new("clean").with_attr("type", dirty_type);
        if let Some(ts) = from_timestamp {
            clean_node = clean_node.with_attr("timestamp", ts.to_string());
        }
        let iq = BinaryNode::new("iq")
            .with_attr("id", msg_id)
            .with_attr("to", S_WHATSAPP_NET)
            .with_attr("type", "set")
            .with_attr("xmlns", "urn:xmpp:whatsapp:dirty")
            .with_child(clean_node);
        self.send_iq(iq, Duration::from_secs(10)).await?;
        Ok(())
    }

    /// Fetches the WhatsApp AI Bot list v2.
    pub async fn get_bot_list_v2(&self) -> Result<Vec<(String, String)>, CoreError> {
        let msg_id = format!("bot_{:X}", rand::random::<u64>());
        let iq = BinaryNode::new("iq")
            .with_attr("id", msg_id)
            .with_attr("to", S_WHATSAPP_NET)
            .with_attr("type", "get")
            .with_attr("xmlns", "bot")
            .with_child(BinaryNode::new("bot").with_attr("v", "2"));
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        let mut bots = Vec::new();
        if let Some(bot_node) = res.get_child("bot") {
            for section in bot_node
                .get_children()
                .iter()
                .filter(|n| n.tag == "section")
            {
                for bot in section.get_children().iter().filter(|n| n.tag == "bot") {
                    if let (Some(jid), Some(persona_id)) =
                        (bot.get_attr("jid"), bot.get_attr("persona_id"))
                    {
                        bots.push((jid.to_string(), persona_id.to_string()));
                    }
                }
            }
        }
        Ok(bots)
    }

    /// Queries the administrator count for a newsletter via GraphQL MEX.
    pub async fn newsletter_admin_count(&self, newsletter_jid: &str) -> Result<u32, CoreError> {
        let vars = serde_json::json!({
            "input": { "key": newsletter_jid }
        });
        let iq = crate::newsletter::build_mex_iq(crate::newsletter::QUERY_ADMIN_COUNT, vars);
        let res = self.send_iq(iq, Duration::from_secs(12)).await?;
        let data = crate::newsletter::parse_mex_response(&res)?;
        crate::newsletter::parse_newsletter_admin_count(&data)
            .ok_or_else(|| CoreError::Protocol("Admin count not found in response".into()))
    }

    /// Subscribes to live updates (edits, reactions) for a newsletter.
    pub async fn subscribe_newsletter_updates(
        &self,
        newsletter_jid: &str,
    ) -> Result<Option<String>, CoreError> {
        let iq = crate::newsletter::build_newsletter_live_updates_iq(newsletter_jid);
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        Ok(crate::newsletter::parse_newsletter_live_updates_response(
            &res,
        ))
    }

    /// Revokes an existing community invite code and generates a new one.
    pub async fn community_revoke_invite(&self, community_jid: &str) -> Result<String, CoreError> {
        let iq = crate::community::build_community_revoke_invite_iq(community_jid);
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        res.get_child("invitation")
            .and_then(|inv| inv.get_attr("code"))
            .map(|s| s.to_string())
            .ok_or_else(|| CoreError::Protocol("Missing code in revoke invite response".into()))
    }

    /// Gets invite info for a community invite link code.
    pub async fn community_get_invite_info(
        &self,
        invite_code: &str,
    ) -> Result<crate::community::CommunityInviteInfo, CoreError> {
        let iq = crate::community::build_community_get_invite_info_iq(invite_code);
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        let group_node = res
            .get_child("group")
            .ok_or_else(|| CoreError::Protocol("Missing group in invite info response".into()))?;
        let id = group_node.get_attr("id").unwrap_or_default().to_string();
        let subject = group_node
            .get_attr("subject")
            .unwrap_or_default()
            .to_string();
        let creator = group_node.get_attr("creator").map(|s| s.to_string());
        let size = group_node.get_attr("size").and_then(|s| s.parse().ok());
        Ok(crate::community::CommunityInviteInfo {
            jid: format!("{}@g.us", id),
            subject,
            creator,
            size,
        })
    }

    /// Updates ephemeral disappearing messages setting for a community.
    pub async fn community_toggle_ephemeral(
        &self,
        community_jid: &str,
        duration_sec: u32,
    ) -> Result<(), CoreError> {
        let iq = crate::community::build_community_toggle_ephemeral_iq(community_jid, duration_sec);
        self.send_iq(iq, Duration::from_secs(15)).await?;
        Ok(())
    }

    /// Updates announcement or locked setting for a community.
    pub async fn community_setting_update(
        &self,
        community_jid: &str,
        setting: &str,
    ) -> Result<(), CoreError> {
        let iq = crate::community::build_community_setting_update_iq(community_jid, setting);
        self.send_iq(iq, Duration::from_secs(15)).await?;
        Ok(())
    }

    /// Sets member add mode for a community ("admin_add" or "all_member_add").
    pub async fn community_member_add_mode(
        &self,
        community_jid: &str,
        mode: &str,
    ) -> Result<(), CoreError> {
        let iq = crate::community::build_community_member_add_mode_iq(community_jid, mode);
        self.send_iq(iq, Duration::from_secs(15)).await?;
        Ok(())
    }

    /// Sets join approval mode for a community ("on" or "off").
    pub async fn community_join_approval_mode(
        &self,
        community_jid: &str,
        mode: &str,
    ) -> Result<(), CoreError> {
        let iq = crate::community::build_community_join_approval_mode_iq(community_jid, mode);
        self.send_iq(iq, Duration::from_secs(15)).await?;
        Ok(())
    }

    /// Fetches all participating community JIDs.
    pub async fn community_fetch_all_participating(&self) -> Result<Vec<String>, CoreError> {
        let iq = crate::community::build_community_fetch_all_participating_iq();
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        let mut jids = Vec::new();
        if let Some(comm_node) = res.get_child("communities") {
            for sub in comm_node
                .get_children()
                .iter()
                .filter(|n| n.tag == "community")
            {
                if let Some(jid) = sub.get_attr("jid") {
                    jids.push(jid.to_string());
                }
            }
        }
        Ok(jids)
    }

    /// Creates a new commerce catalog product.
    pub async fn product_create(
        &self,
        product: &crate::commerce::CatalogProduct,
    ) -> Result<String, CoreError> {
        let iq = crate::commerce::build_product_create_iq(product);
        let res = self.send_iq(iq, Duration::from_secs(20)).await?;
        res.get_child("product")
            .and_then(|p| p.get_child("id"))
            .and_then(|id| id.content_as_string())
            .map(|s| s.to_string())
            .ok_or_else(|| CoreError::Protocol("Missing product id in creation response".into()))
    }

    /// Updates an existing commerce catalog product.
    pub async fn product_update(
        &self,
        product_id: &str,
        product: &crate::commerce::CatalogProduct,
    ) -> Result<(), CoreError> {
        let iq = crate::commerce::build_product_update_iq(product_id, product);
        self.send_iq(iq, Duration::from_secs(20)).await?;
        Ok(())
    }

    /// Deletes one or more commerce catalog products.
    pub async fn product_delete(&self, product_ids: &[&str]) -> Result<(), CoreError> {
        let iq = crate::commerce::build_product_delete_iq(product_ids);
        self.send_iq(iq, Duration::from_secs(15)).await?;
        Ok(())
    }

    /// Sets business account cover photo.
    pub async fn update_cover_photo(
        &self,
        photo_id: &str,
        token: &str,
        ts: u64,
    ) -> Result<(), CoreError> {
        let iq = crate::commerce::build_update_cover_photo_iq(photo_id, token, ts);
        self.send_iq(iq, Duration::from_secs(20)).await?;
        Ok(())
    }

    /// Removes business account cover photo.
    pub async fn remove_cover_photo(&self, photo_id: &str) -> Result<(), CoreError> {
        let iq = crate::commerce::build_remove_cover_photo_iq(photo_id);
        self.send_iq(iq, Duration::from_secs(15)).await?;
        Ok(())
    }

    /// Resolves LIDs from Phone Number JIDs via USync protocol.
    pub async fn get_lid_for_pn(
        &self,
        pn_jids: &[&str],
    ) -> Result<Vec<(String, String)>, CoreError> {
        let iq = crate::usync::build_usync_lid_iq(pn_jids);
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        crate::usync::parse_usync_lid_response(&res)
    }

    /// Resolves Phone Number JIDs from LIDs via USync protocol.
    pub async fn get_pn_for_lid(
        &self,
        lid_jids: &[&str],
    ) -> Result<Vec<(String, String)>, CoreError> {
        let iq = crate::usync::build_usync_lid_iq(lid_jids);
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        crate::usync::parse_usync_lid_response(&res)
    }

    /// Fetches contact status text and timestamp via USync protocol.
    pub async fn get_user_status(
        &self,
        jids: &[&str],
    ) -> Result<Vec<(String, String, u64)>, CoreError> {
        let iq = crate::usync::build_usync_status_iq(jids);
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        crate::usync::parse_usync_status_response(&res)
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
        Ok(())
    }

    /// Requests the sender phone to re-upload expired media and update the direct_path.
    /// Matches Baileys `updateMediaMessage(message)`.
    pub async fn update_media_message(&self, message_id: &str) -> Result<(), CoreError> {
        let msg = self
            .store
            .get_message(message_id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?
            .ok_or_else(|| CoreError::Protocol(format!("Message not found: {}", message_id)))?;

        let proto_info = baileys_proto::proto::WebMessageInfo::decode(&msg.proto_bytes[..])
            .map_err(|e| CoreError::Protocol(format!("Failed to decode message proto: {}", e)))?;

        let m = proto_info
            .message
            .ok_or_else(|| CoreError::Protocol("Message has no content".into()))?;

        let media_key = m
            .image_message
            .as_ref()
            .and_then(|img| img.media_key.clone())
            .or_else(|| m.video_message.as_ref().and_then(|v| v.media_key.clone()))
            .or_else(|| m.audio_message.as_ref().and_then(|a| a.media_key.clone()))
            .or_else(|| {
                m.document_message
                    .as_ref()
                    .and_then(|d| d.media_key.clone())
            })
            .or_else(|| m.sticker_message.as_ref().and_then(|s| s.media_key.clone()))
            .ok_or_else(|| CoreError::Protocol("No mediaKey in message".into()))?;

        let (ciphertext, iv) = baileys_media::encrypt_media_retry_request(message_id, &media_key)
            .map_err(|e| CoreError::Crypto(e.to_string()))?;

        let me_jid = self.me_jid().await?.unwrap_or_default();
        let mut receipt = BinaryNode::new("receipt")
            .with_attr("id", message_id)
            .with_attr("to", me_jid)
            .with_attr("type", "server-error");

        let enc_p = BinaryNode::new("enc_p").with_bytes_content(ciphertext);
        let enc_iv = BinaryNode::new("enc_iv").with_bytes_content(iv.to_vec());
        let encrypt_node = BinaryNode::new("encrypt")
            .with_child(enc_p)
            .with_child(enc_iv);

        let mut rmr_node = BinaryNode::new("rmr")
            .with_attr("jid", &msg.chat_jid)
            .with_attr("from_me", if msg.from_me { "true" } else { "false" });

        if !msg.sender_jid.is_empty() && msg.sender_jid != msg.chat_jid {
            rmr_node = rmr_node.with_attr("participant", &msg.sender_jid);
        }

        receipt = receipt.with_child(encrypt_node).with_child(rmr_node);
        let socket = {
            let lock = self.socket.lock().await;
            lock.clone()
                .ok_or_else(|| CoreError::Network("Not connected".into()))?
        };
        socket.send_node(receipt).await
    }

    /// Fetches rich OpenGraph metadata and thumbnail for a URL string.
    pub async fn fetch_url_preview(
        &self,
        text: &str,
    ) -> Result<Option<baileys_media::UrlPreview>, CoreError> {
        baileys_media::fetch_url_preview(text, None)
            .await
            .map_err(|e| CoreError::Protocol(format!("Link preview error: {}", e)))
    }

    /// Sends a text message with an automatically generated OpenGraph preview card if a URL is present.
    pub async fn send_text_with_preview(
        &self,
        to_jid: &str,
        text: &str,
    ) -> Result<String, CoreError> {
        if let Ok(Some(preview)) = baileys_media::fetch_url_preview(text, None).await {
            let ext = baileys_proto::proto::message::ExtendedTextMessage {
                text: Some(text.to_string()),
                matched_text: Some(preview.matched_text),
                title: preview.title,
                description: preview.description,
                jpeg_thumbnail: preview.jpeg_thumbnail,
                ..Default::default()
            };
            let proto_msg = Message {
                extended_text_message: Some(Box::new(ext)),
                ..Default::default()
            };
            self.send_message(to_jid, proto_msg).await
        } else {
            let proto_msg = Message {
                conversation: Some(text.to_string()),
                ..Default::default()
            };
            self.send_message(to_jid, proto_msg).await
        }
    }
}

/// Returns the user portion of a JID (`628123:12@s.whatsapp.net` -> `628123`).
fn jid_user_part(jid: &str) -> &str {
    let user = jid.split('@').next().unwrap_or(jid);
    user.split(':').next().unwrap_or(user)
}

/// Strips the device suffix from a JID (`628123:12@s.whatsapp.net` -> `628123@s.whatsapp.net`).
fn normalize_user_jid(jid: &str) -> String {
    match jid.split_once('@') {
        Some((user, server)) => format!("{}@{}", user.split(':').next().unwrap_or(user), server),
        None => jid.to_string(),
    }
}
