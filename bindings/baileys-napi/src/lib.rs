#![allow(clippy::too_many_arguments)]

use napi::bindgen_prelude::*;
use napi::threadsafe_function::ThreadsafeFunctionCallMode;
use napi_derive::napi;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use baileys_core::{BaileysClient, BaileysConfigBuilder, BaileysEvent};
use baileys_store::SqliteStore;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "event", content = "data")]
pub enum NapiEventPayload {
    #[serde(rename = "connecting")]
    Connecting,
    #[serde(rename = "qr")]
    QrCode { code: String },
    #[serde(rename = "pairing_code")]
    PairingCode { code: String },
    #[serde(rename = "connected")]
    Connected { me: String },
    #[serde(rename = "message")]
    Message {
        id: String,
        remote_jid: String,
        participant: Option<String>,
        from_me: bool,
        timestamp: u64,
        summary: String,
        has_media: bool,
        is_starred: bool,
    },
    #[serde(rename = "receipt")]
    Receipt {
        id: String,
        remote_jid: String,
        status: String,
    },
    #[serde(rename = "presence")]
    Presence { jid: String, presence: String },
    #[serde(rename = "history_sync")]
    HistorySync {
        sync_type: i32,
        progress: u32,
        conversation_count: u32,
    },
    #[serde(rename = "poll_vote")]
    PollVote {
        poll_id: String,
        voter_jid: String,
        selected_options: Vec<String>,
        tally: NapiPollTally,
    },
    #[serde(rename = "call")]
    Call { call: NapiCallEvent },
    #[serde(rename = "story")]
    StoryReceived { story: NapiStoredStory },
    #[serde(rename = "blocklist_update")]
    BlocklistUpdate {
        added: Vec<String>,
        removed: Vec<String>,
    },
    #[serde(rename = "message_edited")]
    MessageEdited {
        chat_jid: String,
        message_id: String,
        new_text: String,
    },
    #[serde(rename = "message_revoked")]
    MessageRevoked {
        chat_jid: String,
        message_id: String,
        participant: Option<String>,
    },
    #[serde(rename = "chat_ephemeral_update")]
    ChatEphemeralUpdate {
        chat_jid: String,
        ephemeral_expiration: u32,
    },
    #[serde(rename = "label_edit")]
    LabelEdit { label: NapiLabel },
    #[serde(rename = "chat_label_update")]
    ChatLabelUpdate {
        chat_jid: String,
        label_id: String,
        labeled: bool,
    },
    #[serde(rename = "message_label_update")]
    MessageLabelUpdate {
        chat_jid: String,
        message_id: String,
        label_id: String,
        labeled: bool,
    },
    #[serde(rename = "device_update")]
    DeviceUpdate { user_jid: String, devices: Vec<u32> },
    #[serde(rename = "identity_changed")]
    IdentityChanged { jid: String, timestamp: u64 },
    #[serde(rename = "media_updated")]
    MediaUpdated {
        message_id: String,
        direct_path: String,
    },
    #[serde(rename = "chat_update")]
    ChatUpdate {
        jid: String,
        pinned: Option<bool>,
        archived: Option<bool>,
        mute_until: Option<Option<u64>>,
        unread_count: Option<u32>,
    },
    #[serde(rename = "contact_update")]
    ContactUpdate {
        contact: baileys_store::StoredContact,
    },
    #[serde(rename = "app_state_synced")]
    AppStateSynced {
        collections_synced: usize,
        mutations_applied: usize,
    },
    #[serde(rename = "media_progress")]
    MediaProgress {
        task_id: String,
        direction: String,
        bytes_transferred: i64,
        total_bytes: i64,
        percentage: f64,
    },
    #[serde(rename = "disconnected")]
    Disconnected { reason: String },
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiPrivacySettings {
    pub readreceipts: Option<String>,
    pub profile: Option<String>,
    pub status: Option<String>,
    pub online: Option<String>,
    pub last: Option<String>,
    pub groupadd: Option<String>,
    pub calladd: Option<String>,
    pub raw: HashMap<String, String>,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiProfileStatus {
    pub status: String,
    pub set_at: Option<i64>,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiBusinessProfile {
    pub jid: String,
    pub address: Option<String>,
    pub description: Option<String>,
    pub websites: Vec<String>,
    pub email: Option<String>,
    pub category: Option<String>,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiCallEvent {
    pub id: String,
    pub chat_jid: String,
    pub from: String,
    pub caller_pn: Option<String>,
    pub is_group: bool,
    pub group_jid: Option<String>,
    pub is_video: bool,
    pub status: String,
    pub timestamp: i64,
    pub offline: bool,
    pub latency_ms: Option<u32>,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiStoredCall {
    pub call_id: String,
    pub chat_jid: String,
    pub caller_jid: String,
    pub is_group: bool,
    pub group_jid: Option<String>,
    pub is_video: bool,
    pub status: String,
    pub timestamp: i64,
    pub duration: Option<u32>,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiStoredStory {
    pub msg_id: String,
    pub author_jid: String,
    pub author_name: Option<String>,
    pub timestamp: i64,
    pub message_type: String,
    pub content: Option<String>,
    pub caption: Option<String>,
    pub has_media: bool,
    pub viewed: bool,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiPollTally {
    pub poll_id: String,
    pub question: String,
    pub total_voters: u32,
    pub option_votes: HashMap<String, u32>,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiInteractiveButton {
    pub name: String,
    pub button_params_json: String,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiListRow {
    pub title: String,
    pub description: Option<String>,
    pub row_id: String,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiListSection {
    pub title: String,
    pub rows: Vec<NapiListRow>,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiNewsletterMetadata {
    pub jid: String,
    pub name: String,
    pub description: Option<String>,
    pub invite_code: Option<String>,
    pub picture_url: Option<String>,
    pub subscribers_count: i64,
    pub role: Option<String>,
    pub muted: bool,
    pub verified: bool,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiNewsletterMessage {
    pub server_id: i64,
    pub views_count: Option<i64>,
    pub timestamp: i64,
    pub summary: String,
    pub has_media: bool,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiCommunityParticipant {
    pub jid: String,
    pub admin: Option<String>,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiCommunityMetadata {
    pub id: String,
    pub subject: String,
    pub subject_owner: Option<String>,
    pub subject_time: i64,
    pub size: u32,
    pub creation: i64,
    pub owner: Option<String>,
    pub description: Option<String>,
    pub description_id: Option<String>,
    pub linked_parent: Option<String>,
    pub is_community: bool,
    pub is_community_announce: bool,
    pub join_approval_mode: bool,
    pub member_add_mode: bool,
    pub participants: Vec<NapiCommunityParticipant>,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiCommunitySubgroup {
    pub id: String,
    pub subject: String,
    pub creation: Option<i64>,
    pub creator: Option<String>,
    pub size: Option<u32>,
}

impl From<baileys_core::community::CommunityParticipant> for NapiCommunityParticipant {
    fn from(p: baileys_core::community::CommunityParticipant) -> Self {
        Self {
            jid: p.jid,
            admin: p.admin,
        }
    }
}

impl From<baileys_core::community::CommunityMetadata> for NapiCommunityMetadata {
    fn from(meta: baileys_core::community::CommunityMetadata) -> Self {
        Self {
            id: meta.id,
            subject: meta.subject,
            subject_owner: meta.subject_owner,
            subject_time: meta.subject_time as i64,
            size: meta.size,
            creation: meta.creation as i64,
            owner: meta.owner,
            description: meta.description,
            description_id: meta.description_id,
            linked_parent: meta.linked_parent,
            is_community: meta.is_community,
            is_community_announce: meta.is_community_announce,
            join_approval_mode: meta.join_approval_mode,
            member_add_mode: meta.member_add_mode,
            participants: meta.participants.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<baileys_core::community::CommunitySubgroup> for NapiCommunitySubgroup {
    fn from(sg: baileys_core::community::CommunitySubgroup) -> Self {
        Self {
            id: sg.id,
            subject: sg.subject,
            creation: sg.creation.map(|c| c as i64),
            creator: sg.creator,
            size: sg.size,
        }
    }
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiGroupParticipant {
    pub jid: String,
    pub phone_number: Option<String>,
    pub lid: Option<String>,
    pub admin: Option<String>,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiGroupMetadata {
    pub id: String,
    pub subject: String,
    pub subject_owner: Option<String>,
    pub subject_time: Option<i64>,
    pub size: u32,
    pub creation: i64,
    pub owner: Option<String>,
    pub description: Option<String>,
    pub description_id: Option<String>,
    pub restrict: bool,
    pub announce: bool,
    pub is_community: bool,
    pub is_community_announce: bool,
    pub join_approval_mode: bool,
    pub member_add_mode: String,
    pub ephemeral_duration: Option<u32>,
    pub participants: Vec<NapiGroupParticipant>,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiGroupJoinRequest {
    pub jid: String,
    pub requested_time: i64,
    pub request_method: Option<String>,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiGroupParticipantUpdateResult {
    pub jid: String,
    pub status: String,
}

impl From<baileys_core::groups::GroupParticipant> for NapiGroupParticipant {
    fn from(p: baileys_core::groups::GroupParticipant) -> Self {
        Self {
            jid: p.jid,
            phone_number: p.phone_number,
            lid: p.lid,
            admin: p.admin,
        }
    }
}

impl From<baileys_core::groups::GroupMetadata> for NapiGroupMetadata {
    fn from(g: baileys_core::groups::GroupMetadata) -> Self {
        Self {
            id: g.id,
            subject: g.subject,
            subject_owner: g.subject_owner,
            subject_time: g.subject_time,
            size: g.size,
            creation: g.creation,
            owner: g.owner,
            description: g.description,
            description_id: g.description_id,
            restrict: g.restrict,
            announce: g.announce,
            is_community: g.is_community,
            is_community_announce: g.is_community_announce,
            join_approval_mode: g.join_approval_mode,
            member_add_mode: g.member_add_mode,
            ephemeral_duration: g.ephemeral_duration,
            participants: g.participants.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<baileys_core::groups::GroupJoinRequest> for NapiGroupJoinRequest {
    fn from(r: baileys_core::groups::GroupJoinRequest) -> Self {
        Self {
            jid: r.jid,
            requested_time: r.requested_time,
            request_method: r.request_method,
        }
    }
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiLabel {
    pub id: String,
    pub name: String,
    pub color: i32,
    pub predefined_id: Option<i32>,
    pub deleted: bool,
}

impl From<baileys_store::StoredLabel> for NapiLabel {
    fn from(l: baileys_store::StoredLabel) -> Self {
        Self {
            id: l.id,
            name: l.name,
            color: l.color,
            predefined_id: l.predefined_id,
            deleted: l.deleted,
        }
    }
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiCatalogProduct {
    pub id: String,
    pub biz_jid: String,
    pub title: String,
    pub description: Option<String>,
    pub currency: String,
    pub price_amount_1000: i64,
    pub sale_price_amount_1000: Option<i64>,
    pub retailer_id: Option<String>,
    pub url: Option<String>,
    pub image_urls: Vec<String>,
    pub is_hidden: bool,
    pub availability: String,
}

impl From<baileys_core::CatalogProduct> for NapiCatalogProduct {
    fn from(p: baileys_core::CatalogProduct) -> Self {
        Self {
            id: p.id,
            biz_jid: p.biz_jid,
            title: p.title,
            description: p.description,
            currency: p.currency,
            price_amount_1000: p.price_amount_1000,
            sale_price_amount_1000: p.sale_price_amount_1000,
            retailer_id: p.retailer_id,
            url: p.url,
            image_urls: p.image_urls,
            is_hidden: p.is_hidden,
            availability: p.availability,
        }
    }
}

impl From<baileys_store::StoredCatalogProduct> for NapiCatalogProduct {
    fn from(p: baileys_store::StoredCatalogProduct) -> Self {
        Self {
            id: p.product_id,
            biz_jid: p.business_owner_jid,
            title: p.title,
            description: p.description,
            currency: p.currency,
            price_amount_1000: p.price_amount_1000,
            sale_price_amount_1000: p.sale_price_amount_1000,
            retailer_id: p.retailer_id,
            url: p.url,
            image_urls: p.image_urls,
            is_hidden: p.is_hidden,
            availability: p.availability,
        }
    }
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiCatalogCollection {
    pub id: String,
    pub biz_jid: String,
    pub name: String,
    pub products_count: u32,
    pub status: String,
}

impl From<baileys_core::CatalogCollection> for NapiCatalogCollection {
    fn from(c: baileys_core::CatalogCollection) -> Self {
        Self {
            id: c.id,
            biz_jid: c.biz_jid,
            name: c.name,
            products_count: c.products_count,
            status: c.status,
        }
    }
}

impl From<baileys_store::StoredCatalogCollection> for NapiCatalogCollection {
    fn from(c: baileys_store::StoredCatalogCollection) -> Self {
        Self {
            id: c.collection_id,
            biz_jid: c.business_owner_jid,
            name: c.name,
            products_count: c.products_count,
            status: c.status,
        }
    }
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiCatalogResult {
    pub products: Vec<NapiCatalogProduct>,
    pub cursor: Option<String>,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiCollectionsResult {
    pub collections: Vec<NapiCatalogCollection>,
    pub cursor: Option<String>,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiOrder {
    pub order_id: String,
    pub chat_jid: String,
    pub seller_jid: String,
    pub item_count: i32,
    pub status: String,
    pub surface: Option<String>,
    pub order_title: Option<String>,
    pub message: Option<String>,
    pub total_amount_1000: i64,
    pub total_currency_code: String,
    pub token: Option<String>,
    pub created_at: i64,
}

impl From<baileys_store::StoredOrder> for NapiOrder {
    fn from(o: baileys_store::StoredOrder) -> Self {
        Self {
            order_id: o.order_id,
            chat_jid: o.chat_jid,
            seller_jid: o.seller_jid,
            item_count: o.item_count,
            status: o.status,
            surface: o.surface,
            order_title: o.order_title,
            message: o.message,
            total_amount_1000: o.total_amount_1000,
            total_currency_code: o.total_currency_code,
            token: o.token,
            created_at: o.created_at as i64,
        }
    }
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiDeviceRecord {
    pub id: u32,
    pub key_index: Option<u32>,
}

impl From<baileys_core::DeviceRecord> for NapiDeviceRecord {
    fn from(d: baileys_core::DeviceRecord) -> Self {
        Self {
            id: d.id,
            key_index: d.key_index,
        }
    }
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiOnWhatsAppResult {
    pub jid: String,
    pub exists: bool,
    pub phone_number: String,
}

impl From<baileys_core::OnWhatsAppResult> for NapiOnWhatsAppResult {
    fn from(r: baileys_core::OnWhatsAppResult) -> Self {
        Self {
            jid: r.jid,
            exists: r.exists,
            phone_number: r.phone_number,
        }
    }
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiUrlPreview {
    pub matched_text: String,
    pub canonical_url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub jpeg_thumbnail: Option<Vec<u8>>,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiUserStatus {
    pub jid: String,
    pub status: String,
    pub timestamp: i64,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiCommunityInviteInfo {
    pub jid: String,
    pub subject: String,
    pub creator: Option<String>,
    pub size: Option<u32>,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiBotEntry {
    pub jid: String,
    pub persona_id: String,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiAccountReachoutTimelock {
    pub is_active: bool,
    pub enforcement_type: Option<String>,
    pub time_enforcement_ends: Option<i64>,
}

#[napi(object)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NapiTelemetrySnapshot {
    pub messages_in: i64,
    pub messages_out: i64,
    pub last_ping_latency_ms: i64,
    pub signal_cache_hit_rate_pct: f64,
    pub reconnect_count: i64,
}

#[napi]
pub struct RoseBaileysClient {
    client: Arc<BaileysClient>,
    runtime: Arc<tokio::runtime::Runtime>,
    is_running: Arc<AtomicBool>,
}

impl RoseBaileysClient {
    async fn run_async<T, F, Fut>(&self, f: F) -> Result<T>
    where
        F: FnOnce(Arc<BaileysClient>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = std::result::Result<T, baileys_core::CoreError>>
            + Send
            + 'static,
        T: Send + 'static,
    {
        let client = Arc::clone(&self.client);
        let handle = self.runtime.spawn(async move { f(client).await });
        handle
            .await
            .map_err(|e| Error::from_reason(e.to_string()))?
            .map_err(|e| Error::from_reason(e.to_string()))
    }
}

#[napi]
impl RoseBaileysClient {
    #[napi(constructor)]
    pub fn new(database_path: Option<String>, phone_number: Option<String>) -> Result<Self> {
        let db_path = database_path.unwrap_or_else(|| "aurora_session.db".to_string());
        let store =
            Arc::new(SqliteStore::open(&db_path).map_err(|e| Error::from_reason(e.to_string()))?);

        let mut config_builder =
            BaileysConfigBuilder::new().with_browser("Chrome", "Mac OS", "14.4.1");

        if let Some(phone) = phone_number {
            config_builder = config_builder.with_phone_pairing(&phone);
        }

        let config = config_builder.build();
        let client = Arc::new(BaileysClient::new(store, config));
        let runtime =
            Arc::new(tokio::runtime::Runtime::new().map_err(|e| {
                Error::from_reason(format!("Failed to start Tokio runtime: {}", e))
            })?);

        Ok(Self {
            client,
            runtime,
            is_running: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Registers a thread-safe callback that receives JSON-serialized events from the engine.
    #[napi(ts_args_type = "callback: (eventJson: string) => void")]
    pub fn on_event(&self, callback: Function<'_, String, ()>) -> Result<()> {
        let tsfn = callback.build_threadsafe_function().build()?;
        let mut event_rx = self.client.events();

        self.runtime.spawn(async move {
            while let Ok(event) = event_rx.recv().await {
                let payload = match event {
                    BaileysEvent::Connecting => NapiEventPayload::Connecting,
                    BaileysEvent::QrCode { code } => NapiEventPayload::QrCode { code },
                    BaileysEvent::PairingCode { code } => NapiEventPayload::PairingCode { code },
                    BaileysEvent::Connected { me_jid } => {
                        NapiEventPayload::Connected { me: me_jid }
                    }
                    BaileysEvent::Message { message } => {
                        let id = message
                            .key
                            .as_ref()
                            .and_then(|k| k.id.clone())
                            .unwrap_or_default();
                        let remote_jid = message
                            .key
                            .as_ref()
                            .and_then(|k| k.remote_jid.clone())
                            .unwrap_or_default();
                        let participant = message.key.as_ref().and_then(|k| k.participant.clone());
                        let from_me = message
                            .key
                            .as_ref()
                            .and_then(|k| k.from_me)
                            .unwrap_or(false);
                        let timestamp = message.message_timestamp.unwrap_or(0);
                        let (summary, has_media) = if let Some(m) = message.message.as_ref() {
                            let text = baileys_core::extract_message_summary(m);
                            let media = m.image_message.is_some()
                                || m.video_message.is_some()
                                || m.audio_message.is_some()
                                || m.document_message.is_some()
                                || m.sticker_message.is_some();
                            (text, media)
                        } else {
                            ("<empty>".to_string(), false)
                        };

                        let is_starred = message.starred.unwrap_or(false);

                        NapiEventPayload::Message {
                            id,
                            remote_jid,
                            participant,
                            from_me,
                            timestamp,
                            summary,
                            has_media,
                            is_starred,
                        }
                    }
                    BaileysEvent::Receipt {
                        key_id,
                        remote_jid,
                        status,
                    } => NapiEventPayload::Receipt {
                        id: key_id,
                        remote_jid,
                        status,
                    },
                    BaileysEvent::Presence { jid, presence } => {
                        NapiEventPayload::Presence { jid, presence }
                    }
                    BaileysEvent::HistorySync {
                        sync_type,
                        progress,
                        conversations,
                        ..
                    } => NapiEventPayload::HistorySync {
                        sync_type,
                        progress,
                        conversation_count: conversations.len() as u32,
                    },
                    BaileysEvent::PollVote {
                        poll_id,
                        voter_jid,
                        selected_options,
                        tally,
                    } => NapiEventPayload::PollVote {
                        poll_id,
                        voter_jid,
                        selected_options,
                        tally: NapiPollTally {
                            poll_id: tally.poll_id,
                            question: tally.question,
                            total_voters: tally.total_voters as u32,
                            option_votes: tally.option_votes,
                        },
                    },
                    BaileysEvent::Call { call } => NapiEventPayload::Call {
                        call: NapiCallEvent {
                            id: call.id,
                            chat_jid: call.chat_jid,
                            from: call.from,
                            caller_pn: call.caller_pn,
                            is_group: call.is_group,
                            group_jid: call.group_jid,
                            is_video: call.is_video,
                            status: call.status,
                            timestamp: call.timestamp as i64,
                            offline: call.offline,
                            latency_ms: call.latency_ms,
                        },
                    },
                    BaileysEvent::StoryReceived { story } => NapiEventPayload::StoryReceived {
                        story: NapiStoredStory {
                            msg_id: story.msg_id,
                            author_jid: story.author_jid,
                            author_name: story.author_name,
                            timestamp: story.timestamp as i64,
                            message_type: story.message_type,
                            content: story.content,
                            caption: story.caption,
                            has_media: story.has_media,
                            viewed: story.viewed,
                        },
                    },
                    BaileysEvent::BlocklistUpdate { added, removed } => {
                        NapiEventPayload::BlocklistUpdate { added, removed }
                    }
                    BaileysEvent::MessageEdited {
                        chat_jid,
                        message_id,
                        new_text,
                    } => NapiEventPayload::MessageEdited {
                        chat_jid,
                        message_id,
                        new_text,
                    },
                    BaileysEvent::MessageRevoked {
                        chat_jid,
                        message_id,
                        participant,
                    } => NapiEventPayload::MessageRevoked {
                        chat_jid,
                        message_id,
                        participant,
                    },
                    BaileysEvent::ChatEphemeralUpdate {
                        chat_jid,
                        ephemeral_expiration,
                    } => NapiEventPayload::ChatEphemeralUpdate {
                        chat_jid,
                        ephemeral_expiration,
                    },
                    BaileysEvent::LabelEdit { label } => NapiEventPayload::LabelEdit {
                        label: label.into(),
                    },
                    BaileysEvent::ChatLabelUpdate {
                        chat_jid,
                        label_id,
                        labeled,
                    } => NapiEventPayload::ChatLabelUpdate {
                        chat_jid,
                        label_id,
                        labeled,
                    },
                    BaileysEvent::MessageLabelUpdate {
                        chat_jid,
                        message_id,
                        label_id,
                        labeled,
                    } => NapiEventPayload::MessageLabelUpdate {
                        chat_jid,
                        message_id,
                        label_id,
                        labeled,
                    },
                    BaileysEvent::DeviceUpdate { user_jid, devices } => {
                        NapiEventPayload::DeviceUpdate { user_jid, devices }
                    }
                    BaileysEvent::IdentityChanged { jid, timestamp } => {
                        NapiEventPayload::IdentityChanged { jid, timestamp }
                    }
                    BaileysEvent::MediaUpdated {
                        message_id,
                        direct_path,
                    } => NapiEventPayload::MediaUpdated {
                        message_id,
                        direct_path,
                    },
                    BaileysEvent::ChatUpdate {
                        jid,
                        pinned,
                        archived,
                        mute_until,
                        unread_count,
                    } => NapiEventPayload::ChatUpdate {
                        jid,
                        pinned,
                        archived,
                        mute_until,
                        unread_count,
                    },
                    BaileysEvent::ContactUpdate { contact } => {
                        NapiEventPayload::ContactUpdate { contact }
                    }
                    BaileysEvent::AppStateSynced {
                        collections_synced,
                        mutations_applied,
                    } => NapiEventPayload::AppStateSynced {
                        collections_synced,
                        mutations_applied,
                    },
                    BaileysEvent::MediaProgress {
                        task_id,
                        direction,
                        bytes_transferred,
                        total_bytes,
                        percentage,
                    } => NapiEventPayload::MediaProgress {
                        task_id,
                        direction,
                        bytes_transferred: bytes_transferred as i64,
                        total_bytes: total_bytes as i64,
                        percentage: percentage as f64,
                    },
                    BaileysEvent::Disconnected { reason } => {
                        NapiEventPayload::Disconnected { reason }
                    }
                };

                if let Ok(json_str) = serde_json::to_string(&payload) {
                    tsfn.call(json_str, ThreadsafeFunctionCallMode::NonBlocking);
                }
            }
        });

        Ok(())
    }

    /// Connects to WhatsApp Web Gateway.
    #[napi]
    pub async fn connect(&self) -> Result<()> {
        self.is_running.store(true, Ordering::SeqCst);
        self.run_async(|c| async move { c.connect().await }).await
    }

    /// Disconnects from WhatsApp Web Gateway.
    #[napi]
    pub async fn disconnect(&self) -> Result<()> {
        self.is_running.store(false, Ordering::SeqCst);
        self.run_async(|c| async move {
            c.disconnect().await;
            Ok(())
        })
        .await
    }

    /// Returns true if currently authenticated and connected.
    #[napi]
    pub fn is_connected(&self) -> bool {
        self.client.is_connected()
    }

    /// Returns authenticated JID.
    #[napi]
    pub async fn me_jid(&self) -> Result<Option<String>> {
        self.run_async(|c| async move { c.me_jid().await }).await
    }

    /// Measures WhatsApp gateway round-trip latency in milliseconds.
    #[napi]
    pub async fn ping(&self) -> Result<f64> {
        let dur = self.run_async(|c| async move { c.ping().await }).await?;
        Ok(dur.as_secs_f64() * 1000.0)
    }

    /// Sends a plain text message.
    #[napi]
    pub async fn send_text_message(&self, to_jid: String, text: String) -> Result<String> {
        self.run_async(move |c| async move { c.send_text_message(&to_jid, &text).await })
            .await
    }

    /// Sends a text message quoting a previous message.
    #[napi]
    pub async fn send_text_message_with_quote(
        &self,
        to_jid: String,
        text: String,
        quoted_id: String,
        quoted_participant: Option<String>,
        quoted_text: Option<String>,
    ) -> Result<String> {
        self.run_async(move |c| async move {
            c.send_text_message_with_quote(
                &to_jid,
                &text,
                &quoted_id,
                quoted_participant.as_deref(),
                quoted_text.as_deref(),
            )
            .await
        })
        .await
    }

    /// Sends an emoji reaction.
    #[napi]
    pub async fn send_reaction(
        &self,
        to_jid: String,
        target_msg_id: String,
        emoji: String,
    ) -> Result<String> {
        self.run_async(
            move |c| async move { c.send_reaction(&to_jid, &target_msg_id, &emoji).await },
        )
        .await
    }

    /// Revokes (deletes for everyone) a sent message.
    #[napi]
    pub async fn send_revoke_message(
        &self,
        to_jid: String,
        target_msg_id: String,
    ) -> Result<String> {
        self.run_async(move |c| async move { c.send_revoke_message(&to_jid, &target_msg_id).await })
            .await
    }

    /// Sends a read receipt.
    #[napi]
    pub async fn send_read_receipt(
        &self,
        remote_jid: String,
        msg_id: String,
        participant: Option<String>,
    ) -> Result<()> {
        self.run_async(move |c| async move {
            c.send_read_receipt(&remote_jid, &msg_id, participant.as_deref())
                .await
        })
        .await
    }

    /// Updates user presence status.
    #[napi]
    pub async fn send_presence_update(
        &self,
        presence: String,
        to_jid: Option<String>,
    ) -> Result<()> {
        self.run_async(move |c| async move {
            c.send_presence_update(&presence, to_jid.as_deref()).await
        })
        .await
    }

    /// Sends media (image, video, audio, ptt, document, sticker).
    #[napi]
    pub async fn send_media(
        &self,
        to_jid: String,
        media_type: String,
        data: Buffer,
        caption: Option<String>,
        file_name: Option<String>,
        mimetype: Option<String>,
        ptt: Option<bool>,
        seconds: Option<u32>,
        gif_playback: Option<bool>,
    ) -> Result<String> {
        let bytes: Vec<u8> = data.as_ref().to_vec();
        self.run_async(move |c| async move {
            match media_type.to_lowercase().as_str() {
                "image" => {
                    c.send_image_message(&to_jid, &bytes, caption.as_deref())
                        .await
                }
                "video" => {
                    c.send_video_message(
                        &to_jid,
                        &bytes,
                        caption.as_deref(),
                        seconds,
                        gif_playback.unwrap_or(false),
                    )
                    .await
                }
                "audio" => {
                    c.send_audio_message(&to_jid, &bytes, ptt.unwrap_or(false), seconds)
                        .await
                }
                "ptt" => c.send_audio_message(&to_jid, &bytes, true, seconds).await,
                "document" => {
                    let fname = file_name.as_deref().unwrap_or("document.bin");
                    let mime = mimetype.as_deref().unwrap_or("application/octet-stream");
                    c.send_document_message(&to_jid, &bytes, fname, mime, caption.as_deref())
                        .await
                }
                "sticker" => {
                    c.send_sticker_message(&to_jid, &bytes, gif_playback.unwrap_or(false))
                        .await
                }
                _ => Err(baileys_core::CoreError::Protocol(format!(
                    "Unknown media type: '{}'",
                    media_type
                ))),
            }
        })
        .await
    }

    /// Sends a voice note / PTT audio message (Opus format, waveform).
    #[napi]
    pub async fn send_voice_note(
        &self,
        to_jid: String,
        audio_data: Buffer,
        seconds: Option<u32>,
        waveform: Option<Buffer>,
    ) -> Result<String> {
        let bytes: Vec<u8> = audio_data.as_ref().to_vec();
        let wf: Option<Vec<u8>> = waveform.map(|w| w.as_ref().to_vec());
        let sec = seconds.unwrap_or(0);
        self.run_async(move |c| async move {
            c.send_voice_note(&to_jid, &bytes, sec, wf.as_deref()).await
        })
        .await
    }

    /// Sends a document file attachment message.
    #[napi]
    pub async fn send_document(
        &self,
        to_jid: String,
        doc_data: Buffer,
        filename: String,
        mimetype: Option<String>,
        caption: Option<String>,
        page_count: Option<u32>,
    ) -> Result<String> {
        let bytes: Vec<u8> = doc_data.as_ref().to_vec();
        let mime = mimetype.unwrap_or_else(|| "application/octet-stream".to_string());
        self.run_async(move |c| async move {
            c.send_document(
                &to_jid,
                &bytes,
                &filename,
                &mime,
                None,
                page_count,
                caption.as_deref(),
            )
            .await
        })
        .await
    }

    /// Sends a video message (or GIF animation).
    #[napi]
    pub async fn send_video(
        &self,
        to_jid: String,
        video_data: Buffer,
        caption: Option<String>,
        is_gif: Option<bool>,
        seconds: Option<u32>,
    ) -> Result<String> {
        let bytes: Vec<u8> = video_data.as_ref().to_vec();
        let gif = is_gif.unwrap_or(false);
        self.run_async(move |c| async move {
            c.send_video(&to_jid, &bytes, caption.as_deref(), gif, seconds)
                .await
        })
        .await
    }

    /// Sends a sticker message (WebP format).
    #[napi]
    pub async fn send_sticker(
        &self,
        to_jid: String,
        sticker_data: Buffer,
        is_animated: Option<bool>,
    ) -> Result<String> {
        let bytes: Vec<u8> = sticker_data.as_ref().to_vec();
        let anim = is_animated.unwrap_or(false);
        self.run_async(move |c| async move { c.send_sticker(&to_jid, &bytes, anim).await })
            .await
    }

    /// Downloads and decrypts media from message ID.
    #[napi]
    pub async fn download_media_for_message(
        &self,
        msg_id: String,
        dest_dir: String,
    ) -> Result<String> {
        let path = self
            .run_async(move |c| async move {
                c.download_media_for_message(&msg_id, Path::new(&dest_dir))
                    .await
            })
            .await?;
        Ok(path.to_string_lossy().to_string())
    }

    /// Downloads and decrypts media from message ID using the streaming pipeline (~64KB RAM).
    #[napi]
    pub async fn download_media_streaming(
        &self,
        msg_id: String,
        dest_path: Option<String>,
    ) -> Result<String> {
        let path = self
            .run_async(move |c| async move {
                let target_path = dest_path.as_deref().map(Path::new);
                c.download_message_media_to_path(&msg_id, target_path).await
            })
            .await?;
        Ok(path.to_string_lossy().to_string())
    }

    /// Sends a local media file using the streaming upload pipeline (zero-OOM spooling for >= 10MB)
    /// and emits real-time 'media_progress' events.
    #[napi]
    pub async fn send_media_file(
        &self,
        chat_jid: String,
        file_path: String,
        media_type: String,
        caption: Option<String>,
    ) -> Result<String> {
        let parsed_type: baileys_media::MediaType = media_type
            .parse()
            .map_err(|e: baileys_media::MediaError| Error::from_reason(e.to_string()))?;
        self.run_async(move |c| async move {
            c.send_media_file_streaming(
                &chat_jid,
                Path::new(&file_path),
                parsed_type,
                caption.as_deref(),
            )
            .await
        })
        .await
    }

    /// Toggles auto-downloading of incoming media.
    #[napi]
    pub fn set_auto_download(&self, enabled: bool) {
        self.client.set_auto_download(enabled);
    }

    /// Returns JSON string of all saved conversations.
    #[napi]
    pub async fn get_chats(&self) -> Result<String> {
        let chats = self
            .run_async(|c| async move { c.get_chats().await })
            .await?;
        serde_json::to_string(&chats).map_err(|e| Error::from_reason(e.to_string()))
    }

    /// Returns JSON string of message history for a chat.
    #[napi]
    pub async fn get_messages(
        &self,
        chat_jid: String,
        limit: Option<u32>,
        before_timestamp: Option<i64>,
    ) -> Result<String> {
        let before_u64 = before_timestamp.map(|t| t as u64);
        let msgs = self
            .run_async(move |c| async move {
                c.get_messages(&chat_jid, limit.unwrap_or(50), before_u64)
                    .await
            })
            .await?;
        serde_json::to_string(&msgs).map_err(|e| Error::from_reason(e.to_string()))
    }

    /// Returns JSON string of contact info or null.
    #[napi]
    pub async fn get_contact(&self, jid: String) -> Result<Option<String>> {
        let contact = self
            .run_async(move |c| async move { c.get_contact(&jid).await })
            .await?;
        match contact {
            Some(c) => serde_json::to_string(&c)
                .map(Some)
                .map_err(|e| Error::from_reason(e.to_string())),
            None => Ok(None),
        }
    }

    /// Queries group metadata, returning JSON string of the group node.
    #[napi]
    pub async fn group_metadata(&self, group_jid: String) -> Result<String> {
        let node = self
            .run_async(move |c| async move { c.group_metadata(&group_jid).await })
            .await?;
        serde_json::to_string(&node).map_err(|e| Error::from_reason(e.to_string()))
    }

    /// Creates a new WhatsApp group.
    #[napi]
    pub async fn group_create(&self, subject: String, participants: Vec<String>) -> Result<String> {
        self.run_async(move |c| async move { c.group_create(&subject, &participants).await })
            .await
    }

    /// Updates participants in a group ("add", "remove", "promote", "demote").
    #[napi]
    pub async fn group_participants_update(
        &self,
        group_jid: String,
        participants: Vec<String>,
        action: String,
    ) -> Result<String> {
        let results = self
            .run_async(move |c| async move {
                c.group_participants_update(&group_jid, &participants, &action)
                    .await
            })
            .await?;
        serde_json::to_string(&results).map_err(|e| Error::from_reason(e.to_string()))
    }

    /// Updates group subject/title.
    #[napi]
    pub async fn group_update_subject(&self, group_jid: String, subject: String) -> Result<()> {
        self.run_async(move |c| async move { c.group_update_subject(&group_jid, &subject).await })
            .await
    }

    /// Updates group description.
    #[napi]
    pub async fn group_update_description(
        &self,
        group_jid: String,
        description: String,
    ) -> Result<()> {
        self.run_async(move |c| async move {
            c.group_update_description(&group_jid, &description).await
        })
        .await
    }

    /// Retrieves group invite code.
    #[napi]
    pub async fn group_get_invite_code(&self, group_jid: String) -> Result<String> {
        self.run_async(move |c| async move { c.group_get_invite_code(&group_jid).await })
            .await
    }

    /// Joins a group using an invite code.
    #[napi]
    pub async fn group_accept_invite(&self, code: String) -> Result<String> {
        self.run_async(move |c| async move { c.group_accept_invite(&code).await })
            .await
    }

    /// Leaves a group.
    #[napi]
    pub async fn group_leave(&self, group_jid: String) -> Result<()> {
        self.run_async(move |c| async move { c.group_leave(&group_jid).await })
            .await
    }

    /// Updates group setting ("announcement", "not_announcement", "locked", "unlocked").
    #[napi]
    pub async fn group_setting_update(&self, group_jid: String, setting: String) -> Result<()> {
        self.run_async(move |c| async move { c.group_setting_update(&group_jid, &setting).await })
            .await
    }

    /// Revokes current group invite code.
    #[napi]
    pub async fn group_revoke_invite(&self, group_jid: String) -> Result<String> {
        self.run_async(move |c| async move { c.group_revoke_invite(&group_jid).await })
            .await
    }

    /// Fetches comprehensive group metadata including participants, roles, and configuration.
    #[napi]
    pub async fn group_fetch_metadata(&self, group_jid: String) -> Result<NapiGroupMetadata> {
        let meta = self
            .run_async(move |c| async move { c.group_fetch_metadata(&group_jid).await })
            .await?;
        Ok(meta.into())
    }

    /// Fetches all participating groups for the authenticated account and caches them locally.
    #[napi]
    pub async fn group_fetch_all_participating(&self) -> Result<Vec<NapiGroupMetadata>> {
        let groups = self
            .run_async(move |c| async move { c.group_fetch_all_participating().await })
            .await?;
        Ok(groups.into_iter().map(Into::into).collect())
    }

    /// Configures membership approval mode for a group ("on" or "off").
    #[napi]
    pub async fn group_join_approval_mode(&self, group_jid: String, mode: String) -> Result<()> {
        self.run_async(move |c| async move { c.group_join_approval_mode(&group_jid, &mode).await })
            .await
    }

    /// Fetches pending membership join requests for a group.
    #[napi]
    pub async fn group_request_participants_list(
        &self,
        group_jid: String,
    ) -> Result<Vec<NapiGroupJoinRequest>> {
        let reqs = self
            .run_async(move |c| async move { c.group_request_participants_list(&group_jid).await })
            .await?;
        Ok(reqs.into_iter().map(Into::into).collect())
    }

    /// Approves or rejects pending participant join requests ("approve" or "reject").
    #[napi]
    pub async fn group_request_participants_update(
        &self,
        group_jid: String,
        participants: Vec<String>,
        action: String,
    ) -> Result<Vec<NapiGroupParticipantUpdateResult>> {
        let res = self
            .run_async(move |c| async move {
                c.group_request_participants_update(&group_jid, &participants, &action)
                    .await
            })
            .await?;
        Ok(res
            .into_iter()
            .map(|(jid, status)| NapiGroupParticipantUpdateResult { jid, status })
            .collect())
    }

    /// Configures member add mode for a group ("admin_add" or "all_member_add").
    #[napi]
    pub async fn group_member_add_mode(&self, group_jid: String, mode: String) -> Result<()> {
        self.run_async(move |c| async move { c.group_member_add_mode(&group_jid, &mode).await })
            .await
    }

    /// Revokes a participant's v4 group invite.
    #[napi]
    pub async fn group_revoke_invite_v4(
        &self,
        group_jid: String,
        invited_jid: String,
    ) -> Result<()> {
        self.run_async(
            move |c| async move { c.group_revoke_invite_v4(&group_jid, &invited_jid).await },
        )
        .await
    }

    /// Accepts a v4 group invite directly via IQ stanza. Returns joined group JID.
    #[napi]
    pub async fn group_accept_invite_v4(
        &self,
        group_jid: String,
        invite_code: String,
        invite_expiration: i64,
        admin_jid: String,
    ) -> Result<String> {
        self.run_async(move |c| async move {
            c.group_accept_invite_v4(&group_jid, &invite_code, invite_expiration, &admin_jid)
                .await
        })
        .await
    }

    /// Updates bio/about status.
    #[napi]
    pub async fn update_profile_status(&self, status: String) -> Result<()> {
        self.run_async(move |c| async move { c.update_profile_status(&status).await })
            .await
    }

    /// Queries the WhatsApp About / Bio status of a user.
    #[napi]
    pub async fn fetch_status(&self, jid: String) -> Result<Option<NapiProfileStatus>> {
        let st = self
            .run_async(move |c| async move { c.fetch_status(&jid).await })
            .await?;
        Ok(st.map(|s| NapiProfileStatus {
            status: s.status,
            set_at: s.set_at.map(|t| t as i64),
        }))
    }

    /// Queries business profile metadata for a WhatsApp Business account.
    #[napi]
    pub async fn get_business_profile(&self, jid: String) -> Result<Option<NapiBusinessProfile>> {
        let bp = self
            .run_async(move |c| async move { c.get_business_profile(&jid).await })
            .await?;
        Ok(bp.map(|b| NapiBusinessProfile {
            jid: b.jid,
            address: b.address,
            description: b.description,
            websites: b.websites,
            email: b.email,
            category: b.category,
        }))
    }

    /// Fetches user privacy settings.
    #[napi]
    pub async fn fetch_privacy_settings(&self) -> Result<NapiPrivacySettings> {
        let s = self
            .run_async(|c| async move { c.fetch_privacy_settings().await })
            .await?;
        Ok(NapiPrivacySettings {
            readreceipts: s.readreceipts,
            profile: s.profile,
            status: s.status,
            online: s.online,
            last: s.last,
            groupadd: s.groupadd,
            calladd: s.calladd,
            raw: s.raw,
        })
    }

    /// Updates a privacy setting.
    #[napi]
    pub async fn update_privacy_setting(&self, category: String, value: String) -> Result<()> {
        self.run_async(move |c| async move { c.update_privacy_setting(&category, &value).await })
            .await
    }

    /// Fetches blocklist of JIDs from WhatsApp server and syncs to SQLite.
    #[napi]
    pub async fn fetch_blocklist(&self) -> Result<Vec<String>> {
        self.run_async(|c| async move { c.fetch_blocklist().await })
            .await
    }

    /// Reads all locally stored blocked contacts from SQLite.
    #[napi]
    pub async fn get_blocked_contacts(&self) -> Result<Vec<String>> {
        self.run_async(|c| async move { c.get_blocked_contacts().await })
            .await
    }

    /// Blocks or unblocks a contact ("block" or "unblock").
    #[napi]
    pub async fn update_block_status(&self, jid: String, action: String) -> Result<()> {
        self.run_async(move |c| async move { c.update_block_status(&jid, &action).await })
            .await
    }

    /// Fetches profile picture URL for a user or group.
    #[napi]
    pub async fn profile_picture_url(
        &self,
        jid: String,
        picture_type: Option<String>,
    ) -> Result<Option<String>> {
        let ptype = picture_type.unwrap_or_else(|| "preview".to_string());
        self.run_async(move |c| async move { c.profile_picture_url(&jid, &ptype).await })
            .await
    }

    /// Updates profile picture for self or a group using image buffer.
    #[napi]
    pub async fn update_profile_picture(
        &self,
        jid_opt: Option<String>,
        image_bytes: Buffer,
    ) -> Result<()> {
        let bytes = image_bytes.to_vec();
        self.run_async(move |c| async move {
            c.update_profile_picture(jid_opt.as_deref(), &bytes).await
        })
        .await
    }

    /// Removes profile picture for self or a group.
    #[napi]
    pub async fn remove_profile_picture(&self, jid_opt: Option<String>) -> Result<()> {
        self.run_async(move |c| async move { c.remove_profile_picture(jid_opt.as_deref()).await })
            .await
    }

    /// Subscribes to presence updates for a contact or group.
    #[napi]
    pub async fn presence_subscribe(&self, to_jid: String) -> Result<()> {
        self.run_async(move |c| async move { c.presence_subscribe(&to_jid).await })
            .await
    }

    /// Modifies chat mute status (pass duration in seconds, or null/undefined to unmute; pass -1 for indefinite).
    #[napi]
    pub async fn chat_modify_mute(&self, jid: String, duration_secs: Option<i64>) -> Result<()> {
        let dur = duration_secs.and_then(|d| {
            if d < 0 {
                Some(u64::MAX)
            } else if d == 0 {
                None
            } else {
                Some(d as u64)
            }
        });
        self.run_async(move |c| async move { c.chat_modify_mute(&jid, dur).await })
            .await
    }

    /// Pins or unpins a chat.
    #[napi]
    pub async fn chat_modify_pin(&self, jid: String, pin: bool) -> Result<()> {
        self.run_async(move |c| async move { c.chat_modify_pin(&jid, pin).await })
            .await
    }

    /// Archives or unarchives a chat.
    #[napi]
    pub async fn chat_modify_archive(&self, jid: String, archive: bool) -> Result<()> {
        self.run_async(move |c| async move { c.chat_modify_archive(&jid, archive).await })
            .await
    }

    /// Stars or unstars an individual message.
    #[napi]
    pub async fn chat_modify_star(
        &self,
        jid: String,
        msg_id: String,
        from_me: Option<bool>,
        star: bool,
    ) -> Result<()> {
        let from_me_val = from_me.unwrap_or(false);
        self.run_async(move |c| async move {
            c.chat_modify_star(&jid, &msg_id, from_me_val, star).await
        })
        .await
    }

    /// Clears message history for a chat while optionally keeping starred messages.
    #[napi]
    pub async fn chat_modify_clear(
        &self,
        chat_jid: String,
        keep_starred: Option<bool>,
    ) -> Result<()> {
        let keep = keep_starred.unwrap_or(false);
        self.run_async(move |c| async move { c.chat_modify_clear(&chat_jid, keep).await })
            .await
    }

    /// Deletes a chat conversation thread from WhatsApp and local SQLite store.
    #[napi]
    pub async fn chat_modify_delete(&self, chat_jid: String) -> Result<()> {
        self.run_async(move |c| async move { c.chat_modify_delete(&chat_jid).await })
            .await
    }

    /// Marks a chat as read or unread across devices.
    #[napi]
    pub async fn chat_modify_mark_unread(&self, chat_jid: String, unread: bool) -> Result<()> {
        self.run_async(move |c| async move { c.chat_modify_mark_unread(&chat_jid, unread).await })
            .await
    }

    /// Requests on-demand message history for a chat via w:sync:history IQ.
    #[napi]
    pub async fn fetch_message_history(
        &self,
        chat_jid: String,
        count: u32,
        oldest_msg_id: Option<String>,
        oldest_from_me: Option<bool>,
        oldest_timestamp: Option<i64>,
    ) -> Result<String> {
        let msg_id = oldest_msg_id.unwrap_or_default();
        let from_me = oldest_from_me.unwrap_or(false);
        let ts = oldest_timestamp.unwrap_or(0) as u64;
        self.run_async(move |c| async move {
            c.fetch_message_history(&chat_jid, count, &msg_id, from_me, ts)
                .await
        })
        .await
    }

    /// Queries metadata for a WhatsApp Channel by JID or invite code/link.
    #[napi]
    pub async fn newsletter_query_metadata(
        &self,
        jid_or_invite: String,
    ) -> Result<NapiNewsletterMetadata> {
        let meta = self
            .run_async(move |c| async move { c.newsletter_query_metadata(&jid_or_invite).await })
            .await?;
        Ok(NapiNewsletterMetadata {
            jid: meta.jid,
            name: meta.name,
            description: meta.description,
            invite_code: meta.invite_code,
            picture_url: meta.picture_url,
            subscribers_count: meta.subscribers_count as i64,
            role: meta.role,
            muted: meta.muted,
            verified: meta.verified,
        })
    }

    /// Fetches all subscribed WhatsApp Channels.
    #[napi]
    pub async fn newsletter_get_subscribed(&self) -> Result<Vec<NapiNewsletterMetadata>> {
        let list = self
            .run_async(|c| async move { c.newsletter_get_subscribed().await })
            .await?;
        Ok(list
            .into_iter()
            .map(|meta| NapiNewsletterMetadata {
                jid: meta.jid,
                name: meta.name,
                description: meta.description,
                invite_code: meta.invite_code,
                picture_url: meta.picture_url,
                subscribers_count: meta.subscribers_count as i64,
                role: meta.role,
                muted: meta.muted,
                verified: meta.verified,
            })
            .collect())
    }

    /// Follows a WhatsApp Channel.
    #[napi]
    pub async fn newsletter_follow(&self, jid: String) -> Result<()> {
        self.run_async(move |c| async move { c.newsletter_follow(&jid).await })
            .await
    }

    /// Unfollows a WhatsApp Channel.
    #[napi]
    pub async fn newsletter_unfollow(&self, jid: String) -> Result<()> {
        self.run_async(move |c| async move { c.newsletter_unfollow(&jid).await })
            .await
    }

    /// Toggles mute status for a WhatsApp Channel.
    #[napi]
    pub async fn newsletter_mute(&self, jid: String, mute: bool) -> Result<()> {
        self.run_async(move |c| async move { c.newsletter_mute(&jid, mute).await })
            .await
    }

    /// Creates a new WhatsApp Channel.
    #[napi]
    pub async fn newsletter_create(
        &self,
        name: String,
        description: Option<String>,
    ) -> Result<NapiNewsletterMetadata> {
        let meta = self
            .run_async(
                move |c| async move { c.newsletter_create(&name, description.as_deref()).await },
            )
            .await?;
        Ok(NapiNewsletterMetadata {
            jid: meta.jid,
            name: meta.name,
            description: meta.description,
            invite_code: meta.invite_code,
            picture_url: meta.picture_url,
            subscribers_count: meta.subscribers_count as i64,
            role: meta.role,
            muted: meta.muted,
            verified: meta.verified,
        })
    }

    /// Fetches message history from a WhatsApp Channel.
    #[napi]
    pub async fn newsletter_fetch_messages(
        &self,
        jid: String,
        count: Option<u32>,
        before: Option<i64>,
    ) -> Result<Vec<NapiNewsletterMessage>> {
        let cnt = count.unwrap_or(20);
        let bef = before.map(|b| b as u64);
        let msgs = self
            .run_async(move |c| async move { c.newsletter_fetch_messages(&jid, cnt, bef).await })
            .await?;
        Ok(msgs
            .into_iter()
            .map(|m| NapiNewsletterMessage {
                server_id: m.server_id as i64,
                views_count: m.views_count.map(|v| v as i64),
                timestamp: m.timestamp as i64,
                summary: m.summary,
                has_media: m.has_media,
            })
            .collect())
    }

    /// Sends an emoji reaction to a channel message post.
    #[napi]
    pub async fn newsletter_react(
        &self,
        jid: String,
        server_id: i64,
        reaction: String,
    ) -> Result<()> {
        let sid = server_id as u64;
        self.run_async(move |c| async move { c.newsletter_react(&jid, sid, &reaction).await })
            .await
    }

    /// Broadcasts a text message to an owned WhatsApp Channel.
    #[napi]
    pub async fn newsletter_send_post(&self, jid: String, text: String) -> Result<String> {
        self.run_async(move |c| async move { c.newsletter_send_post(&jid, &text).await })
            .await
    }

    /// Sends an encrypted WhatsApp poll.
    #[napi]
    pub async fn send_poll(
        &self,
        target_jid: String,
        question: String,
        options: Vec<String>,
        selectable_count: Option<u32>,
    ) -> Result<String> {
        let count = selectable_count.unwrap_or(1);
        self.run_async(move |c| async move {
            c.send_poll(&target_jid, &question, &options, count).await
        })
        .await
    }

    /// Casts an encrypted AES-256-GCM vote on an existing poll.
    #[napi]
    pub async fn send_poll_vote(
        &self,
        target_jid: String,
        poll_id: String,
        selected_options: Vec<String>,
    ) -> Result<String> {
        self.run_async(move |c| async move {
            c.send_poll_vote(&target_jid, &poll_id, &selected_options)
                .await
        })
        .await
    }

    /// Fetches the aggregated vote tally for a poll from the local SQLite database.
    #[napi]
    pub async fn get_poll_tally(&self, poll_id: String) -> Result<Option<NapiPollTally>> {
        let tally = self
            .run_async(move |c| async move { c.get_poll_tally(&poll_id).await })
            .await?;

        Ok(tally.map(|t| NapiPollTally {
            poll_id: t.poll_id,
            question: t.question,
            total_voters: t.total_voters as u32,
            option_votes: t.option_votes,
        }))
    }

    /// Sends native flow interactive action buttons (quick reply, cta_url, etc.).
    #[napi]
    pub async fn send_interactive_buttons(
        &self,
        target_jid: String,
        body: String,
        buttons: Vec<NapiInteractiveButton>,
        header: Option<String>,
        footer: Option<String>,
    ) -> Result<String> {
        let core_buttons: Vec<baileys_core::InteractiveButton> = buttons
            .into_iter()
            .map(|b| baileys_core::InteractiveButton {
                name: b.name,
                button_params_json: b.button_params_json,
            })
            .collect();

        self.run_async(move |c| async move {
            c.send_interactive_buttons(
                &target_jid,
                &body,
                &core_buttons,
                header.as_deref(),
                footer.as_deref(),
            )
            .await
        })
        .await
    }

    /// Sends an interactive single-selection list message.
    #[napi]
    pub async fn send_list_message(
        &self,
        target_jid: String,
        title: String,
        description: String,
        button_text: String,
        sections: Vec<NapiListSection>,
        footer: Option<String>,
    ) -> Result<String> {
        let core_sections: Vec<baileys_core::ListSection> = sections
            .into_iter()
            .map(|s| baileys_core::ListSection {
                title: s.title,
                rows: s
                    .rows
                    .into_iter()
                    .map(|r| baileys_core::ListRow {
                        title: r.title,
                        description: r.description,
                        row_id: r.row_id,
                    })
                    .collect(),
            })
            .collect();

        self.run_async(move |c| async move {
            c.send_list_message(
                &target_jid,
                &title,
                &description,
                &button_text,
                &core_sections,
                footer.as_deref(),
            )
            .await
        })
        .await
    }

    /// Creates a new WhatsApp Community.
    #[napi]
    pub async fn community_create(
        &self,
        subject: String,
        description: Option<String>,
    ) -> Result<NapiCommunityMetadata> {
        let meta = self
            .run_async(move |c| async move {
                c.community_create(&subject, description.as_deref()).await
            })
            .await?;
        Ok(meta.into())
    }

    /// Creates a new linked subgroup inside an existing WhatsApp Community.
    #[napi]
    pub async fn community_create_subgroup(
        &self,
        community_jid: String,
        subject: String,
        participants: Option<Vec<String>>,
    ) -> Result<String> {
        let parts = participants.unwrap_or_default();
        self.run_async(move |c| async move {
            c.community_create_subgroup(&community_jid, &subject, &parts)
                .await
        })
        .await
    }

    /// Links an existing standalone group into a WhatsApp Community.
    #[napi]
    pub async fn community_link_subgroup(
        &self,
        community_jid: String,
        subgroup_jid: String,
    ) -> Result<()> {
        self.run_async(move |c| async move {
            c.community_link_subgroup(&community_jid, &subgroup_jid)
                .await
        })
        .await
    }

    /// Unlinks a subgroup from a WhatsApp Community.
    #[napi]
    pub async fn community_unlink_subgroup(
        &self,
        community_jid: String,
        subgroup_jid: String,
    ) -> Result<()> {
        self.run_async(move |c| async move {
            c.community_unlink_subgroup(&community_jid, &subgroup_jid)
                .await
        })
        .await
    }

    /// Fetches all linked subgroups belonging to a WhatsApp Community.
    #[napi]
    pub async fn community_fetch_subgroups(
        &self,
        community_jid: String,
    ) -> Result<Vec<NapiCommunitySubgroup>> {
        let list = self
            .run_async(move |c| async move { c.community_fetch_subgroups(&community_jid).await })
            .await?;
        Ok(list.into_iter().map(Into::into).collect())
    }

    /// Fetches complete community metadata and settings.
    #[napi]
    pub async fn community_get_metadata(
        &self,
        community_jid: String,
    ) -> Result<NapiCommunityMetadata> {
        let meta = self
            .run_async(move |c| async move { c.community_get_metadata(&community_jid).await })
            .await?;
        Ok(meta.into())
    }

    /// Leaves a WhatsApp Community parent group.
    #[napi]
    pub async fn community_leave(&self, community_jid: String) -> Result<()> {
        self.run_async(move |c| async move { c.community_leave(&community_jid).await })
            .await
    }

    /// Generates/queries an invite code for a WhatsApp Community.
    #[napi]
    pub async fn community_get_invite_code(&self, community_jid: String) -> Result<String> {
        self.run_async(move |c| async move { c.community_get_invite_code(&community_jid).await })
            .await
    }

    /// Accepts an invite code to join a WhatsApp Community.
    #[napi]
    pub async fn community_accept_invite(&self, invite_code: String) -> Result<String> {
        self.run_async(move |c| async move { c.community_accept_invite(&invite_code).await })
            .await
    }

    /// Rejects an incoming WhatsApp voice or video call.
    #[napi]
    pub async fn reject_call(&self, call_id: String, call_from: String) -> Result<()> {
        self.run_async(move |c| async move { c.reject_call(&call_id, &call_from).await })
            .await
    }

    /// Fetches VoIP call history from SQLite.
    #[napi]
    pub async fn get_call_history(&self, limit: Option<u32>) -> Result<Vec<NapiStoredCall>> {
        let lim = limit.unwrap_or(20);
        let list = self
            .run_async(move |c| async move { c.get_call_history(lim).await })
            .await?;
        Ok(list
            .into_iter()
            .map(|c| NapiStoredCall {
                call_id: c.call_id,
                chat_jid: c.chat_jid,
                caller_jid: c.caller_jid,
                is_group: c.is_group,
                group_jid: c.group_jid,
                is_video: c.is_video,
                status: c.status,
                timestamp: c.timestamp as i64,
                duration: c.duration,
            })
            .collect())
    }

    /// Publishes a text status story to `status@broadcast`.
    #[napi]
    pub async fn send_text_status(
        &self,
        text: String,
        background_color: Option<String>,
        font: Option<i32>,
        audience: Option<Vec<String>>,
    ) -> Result<String> {
        self.run_async(move |c| async move {
            c.send_text_status(&text, background_color.as_deref(), font, audience)
                .await
        })
        .await
    }

    /// Sends a status read receipt to mark a story as viewed.
    #[napi]
    pub async fn send_status_read_receipt(&self, msg_id: String, author_jid: String) -> Result<()> {
        self.run_async(
            move |c| async move { c.send_status_read_receipt(&msg_id, &author_jid).await },
        )
        .await
    }

    /// Fetches recent status stories from SQLite cache.
    #[napi]
    pub async fn get_recent_stories(
        &self,
        limit: Option<u32>,
        unviewed_only: Option<bool>,
    ) -> Result<Vec<NapiStoredStory>> {
        let lim = limit.unwrap_or(20);
        let unviewed = unviewed_only.unwrap_or(false);
        let list = self
            .run_async(move |c| async move { c.get_recent_stories(lim, unviewed).await })
            .await?;
        Ok(list
            .into_iter()
            .map(|s| NapiStoredStory {
                msg_id: s.msg_id,
                author_jid: s.author_jid,
                author_name: s.author_name,
                timestamp: s.timestamp as i64,
                message_type: s.message_type,
                content: s.content,
                caption: s.caption,
                has_media: s.has_media,
                viewed: s.viewed,
            })
            .collect())
    }

    /// Fetches all active status stories posted by a specific contact.
    #[napi]
    pub async fn get_stories_by_author(&self, author_jid: String) -> Result<Vec<NapiStoredStory>> {
        let list = self
            .run_async(move |c| async move { c.get_stories_by_author(&author_jid).await })
            .await?;
        Ok(list
            .into_iter()
            .map(|s| NapiStoredStory {
                msg_id: s.msg_id,
                author_jid: s.author_jid,
                author_name: s.author_name,
                timestamp: s.timestamp as i64,
                message_type: s.message_type,
                content: s.content,
                caption: s.caption,
                has_media: s.has_media,
                viewed: s.viewed,
            })
            .collect())
    }

    /// Edits a previously sent message.
    #[napi]
    pub async fn edit_message(
        &self,
        chat_jid: String,
        target_message_id: String,
        new_text: String,
    ) -> Result<String> {
        self.run_async(move |c| async move {
            c.edit_message(&chat_jid, &target_message_id, &new_text)
                .await
        })
        .await
    }

    /// Revokes (deletes for everyone) a previously sent message.
    #[napi]
    pub async fn revoke_message(
        &self,
        chat_jid: String,
        target_message_id: String,
    ) -> Result<String> {
        self.run_async(
            move |c| async move { c.revoke_message(&chat_jid, &target_message_id).await },
        )
        .await
    }

    /// Sets disappearing message expiration duration (in seconds, 0 = off) for a chat or group.
    #[napi]
    pub async fn set_chat_ephemeral(
        &self,
        chat_jid: String,
        ephemeral_expiration: u32,
    ) -> Result<()> {
        self.run_async(move |c| async move {
            c.set_chat_ephemeral(&chat_jid, ephemeral_expiration).await
        })
        .await
    }

    /// Gets current disappearing message expiration duration (in seconds, null if not set) for a chat.
    #[napi]
    pub async fn get_chat_ephemeral(&self, chat_jid: String) -> Result<Option<u32>> {
        let res = self
            .run_async(move |c| async move { c.get_chat_ephemeral(&chat_jid).await })
            .await?;
        Ok(res.map(|(exp, _)| exp))
    }

    /// Returns all WhatsApp Business labels from local store.
    #[napi]
    pub async fn get_labels(&self) -> Result<Vec<NapiLabel>> {
        let labels = self
            .run_async(|c| async move { c.get_labels().await })
            .await?;
        Ok(labels.into_iter().map(NapiLabel::from).collect())
    }

    /// Returns a specific WhatsApp Business label by ID.
    #[napi]
    pub async fn get_label(&self, id: String) -> Result<Option<NapiLabel>> {
        let label = self
            .run_async(move |c| async move { c.get_label(&id).await })
            .await?;
        Ok(label.map(NapiLabel::from))
    }

    /// Creates or updates a WhatsApp Business label.
    #[napi]
    pub async fn upsert_label(
        &self,
        id: Option<String>,
        name: String,
        color: Option<i32>,
        predefined_id: Option<i32>,
    ) -> Result<String> {
        let color_val = color.unwrap_or(0);
        self.run_async(move |c| async move {
            c.upsert_label(id.as_deref(), &name, color_val, predefined_id)
                .await
        })
        .await
    }

    /// Deletes a WhatsApp Business label by ID.
    #[napi]
    pub async fn delete_label(&self, id: String) -> Result<()> {
        self.run_async(move |c| async move { c.delete_label(&id).await })
            .await
    }

    /// Associates a label with a chat (group or 1-on-1).
    #[napi]
    pub async fn add_chat_label(&self, chat_jid: String, label_id: String) -> Result<()> {
        self.run_async(move |c| async move { c.add_chat_label(&chat_jid, &label_id).await })
            .await
    }

    /// Removes a label association from a chat.
    #[napi]
    pub async fn remove_chat_label(&self, chat_jid: String, label_id: String) -> Result<()> {
        self.run_async(move |c| async move { c.remove_chat_label(&chat_jid, &label_id).await })
            .await
    }

    /// Returns all label IDs associated with a chat.
    #[napi]
    pub async fn get_chat_labels(&self, chat_jid: String) -> Result<Vec<String>> {
        self.run_async(move |c| async move { c.get_chat_labels(&chat_jid).await })
            .await
    }

    /// Returns all chat JIDs associated with a label ID.
    #[napi]
    pub async fn get_chats_by_label(&self, label_id: String) -> Result<Vec<String>> {
        self.run_async(move |c| async move { c.get_chats_by_label(&label_id).await })
            .await
    }

    /// Associates a label with an individual message.
    #[napi]
    pub async fn add_message_label(
        &self,
        chat_jid: String,
        message_id: String,
        label_id: String,
    ) -> Result<()> {
        self.run_async(move |c| async move {
            c.add_message_label(&chat_jid, &message_id, &label_id).await
        })
        .await
    }

    /// Removes a label association from an individual message.
    #[napi]
    pub async fn remove_message_label(
        &self,
        chat_jid: String,
        message_id: String,
        label_id: String,
    ) -> Result<()> {
        self.run_async(move |c| async move {
            c.remove_message_label(&chat_jid, &message_id, &label_id)
                .await
        })
        .await
    }

    /// Returns all label IDs associated with a message.
    #[napi]
    pub async fn get_message_labels(
        &self,
        chat_jid: String,
        message_id: String,
    ) -> Result<Vec<String>> {
        self.run_async(move |c| async move { c.get_message_labels(&chat_jid, &message_id).await })
            .await
    }

    /// Returns all message IDs associated with a label ID.
    #[napi]
    pub async fn get_messages_by_label(&self, label_id: String) -> Result<Vec<String>> {
        self.run_async(move |c| async move { c.get_messages_by_label(&label_id).await })
            .await
    }

    // ==========================================
    // Track 10: Rich Messages & Attachments
    // ==========================================

    /// Sends a static location message to a contact or group.
    #[napi]
    pub async fn send_location(
        &self,
        to_jid: String,
        latitude: f64,
        longitude: f64,
        name: Option<String>,
        address: Option<String>,
        url: Option<String>,
    ) -> Result<String> {
        self.run_async(move |c| async move {
            c.send_location(
                &to_jid,
                latitude,
                longitude,
                name.as_deref(),
                address.as_deref(),
                url.as_deref(),
            )
            .await
        })
        .await
    }

    /// Sends a real-time live location message to a contact or group.
    #[napi]
    pub async fn send_live_location(
        &self,
        to_jid: String,
        latitude: f64,
        longitude: f64,
        accuracy_in_meters: Option<u32>,
        caption: Option<String>,
    ) -> Result<String> {
        self.run_async(move |c| async move {
            c.send_live_location(
                &to_jid,
                latitude,
                longitude,
                accuracy_in_meters,
                caption.as_deref(),
            )
            .await
        })
        .await
    }

    /// Sends a single contact card (vCard 3.0) to a contact or group.
    #[napi]
    pub async fn send_contact(
        &self,
        to_jid: String,
        display_name: String,
        phone_number: String,
    ) -> Result<String> {
        self.run_async(move |c| async move {
            c.send_contact(&to_jid, &display_name, &phone_number).await
        })
        .await
    }

    /// Sends a contact card with custom raw vCard text to a contact or group.
    #[napi]
    pub async fn send_vcard(
        &self,
        to_jid: String,
        display_name: String,
        vcard: String,
    ) -> Result<String> {
        self.run_async(move |c| async move { c.send_vcard(&to_jid, &display_name, &vcard).await })
            .await
    }

    /// Sends a group invitation preview card to a contact or group.
    #[napi]
    pub async fn send_group_invite(
        &self,
        to_jid: String,
        group_jid: String,
        invite_code: String,
        invite_expiration: i64,
        group_name: String,
        caption: Option<String>,
    ) -> Result<String> {
        self.run_async(move |c| async move {
            c.send_group_invite(
                &to_jid,
                &group_jid,
                &invite_code,
                invite_expiration,
                &group_name,
                caption.as_deref(),
            )
            .await
        })
        .await
    }

    /// Queries the WhatsApp Business catalog for a merchant JID.
    #[napi]
    pub async fn get_catalog(
        &self,
        biz_jid: String,
        limit: Option<u32>,
        cursor: Option<String>,
    ) -> Result<NapiCatalogResult> {
        let lim = limit.unwrap_or(10);
        let res = self
            .run_async(
                move |c| async move { c.get_catalog(&biz_jid, lim, cursor.as_deref()).await },
            )
            .await?;
        Ok(NapiCatalogResult {
            products: res.products.into_iter().map(Into::into).collect(),
            cursor: res.cursor,
        })
    }

    /// Queries a single product by product ID for a merchant JID.
    #[napi]
    pub async fn get_product(
        &self,
        biz_jid: String,
        product_id: String,
    ) -> Result<NapiCatalogProduct> {
        let p = self
            .run_async(move |c| async move { c.get_product(&biz_jid, &product_id).await })
            .await?;
        Ok(p.into())
    }

    /// Queries collections / categories for a merchant JID.
    #[napi]
    pub async fn get_collections(
        &self,
        biz_jid: String,
        limit: Option<u32>,
        cursor: Option<String>,
    ) -> Result<NapiCollectionsResult> {
        let lim = limit.unwrap_or(10);
        let res = self
            .run_async(
                move |c| async move { c.get_collections(&biz_jid, lim, cursor.as_deref()).await },
            )
            .await?;
        Ok(NapiCollectionsResult {
            collections: res.collections.into_iter().map(Into::into).collect(),
            cursor: res.cursor,
        })
    }

    /// Sends a product card message to a recipient.
    #[napi]
    pub async fn send_product_message(
        &self,
        to_jid: String,
        biz_owner_jid: String,
        product_id: String,
        title: String,
        price_amount_1000: i64,
        currency: String,
        caption: Option<String>,
    ) -> Result<String> {
        self.run_async(move |c| async move {
            let product = baileys_core::CatalogProduct {
                id: product_id,
                biz_jid: biz_owner_jid.clone(),
                title,
                description: caption.clone(),
                currency,
                price_amount_1000,
                sale_price_amount_1000: None,
                retailer_id: None,
                url: None,
                image_urls: Vec::new(),
                is_hidden: false,
                availability: "in_stock".to_string(),
            };
            c.send_product_message(&to_jid, &biz_owner_jid, &product, caption.as_deref(), None)
                .await
        })
        .await
    }

    /// Sends an order card message to a recipient and records it in SQLite.
    #[napi]
    pub async fn send_order_message(
        &self,
        to_jid: String,
        seller_jid: String,
        order_id: String,
        title: String,
        item_count: i32,
        total_amount_1000: i64,
        currency: String,
        message: Option<String>,
    ) -> Result<String> {
        self.run_async(move |c| async move {
            c.send_order_message(
                &to_jid,
                &seller_jid,
                &order_id,
                &title,
                item_count,
                total_amount_1000,
                &currency,
                message.as_deref(),
                None,
            )
            .await
        })
        .await
    }

    /// Retrieves cached commerce orders from SQLite for a chat.
    #[napi]
    pub async fn get_cached_orders(
        &self,
        chat_jid: String,
        limit: Option<u32>,
    ) -> Result<Vec<NapiOrder>> {
        let lim = limit.unwrap_or(20);
        let orders = self
            .run_async(move |c| async move { c.get_cached_orders(&chat_jid, lim).await })
            .await?;
        Ok(orders.into_iter().map(Into::into).collect())
    }

    /// Retrieves cached products from SQLite for a merchant JID.
    #[napi]
    pub async fn get_cached_catalog(&self, biz_jid: String) -> Result<Vec<NapiCatalogProduct>> {
        let prods = self
            .run_async(move |c| async move { c.get_cached_catalog(&biz_jid).await })
            .await?;
        Ok(prods.into_iter().map(Into::into).collect())
    }

    /// Retrieves cached collections from SQLite for a merchant JID.
    #[napi]
    pub async fn get_cached_collections(
        &self,
        biz_jid: String,
    ) -> Result<Vec<NapiCatalogCollection>> {
        let cols = self
            .run_async(move |c| async move { c.get_cached_collections(&biz_jid).await })
            .await?;
        Ok(cols.into_iter().map(Into::into).collect())
    }

    /// Queries active companion devices for a user JID via USync protocol.
    #[napi]
    pub async fn get_user_devices(&self, user_jid: String) -> Result<Vec<u32>> {
        self.run_async(move |c| async move { c.get_user_devices(&user_jid).await })
            .await
    }

    /// Queries all paired companion devices linked to our current account.
    #[napi]
    pub async fn get_paired_devices(&self) -> Result<Vec<NapiDeviceRecord>> {
        let devs = self
            .run_async(move |c| async move { c.get_paired_devices().await })
            .await?;
        Ok(devs.into_iter().map(Into::into).collect())
    }

    /// Revokes/unlinks a companion device from our account by device ID.
    #[napi]
    pub async fn remove_companion_device(&self, device_id: u32) -> Result<()> {
        self.run_async(move |c| async move { c.remove_companion_device(device_id).await })
            .await
    }

    /// Checks whether phone numbers are registered on WhatsApp using USync contact query.
    #[napi]
    pub async fn on_whatsapp(
        &self,
        phone_numbers: Vec<String>,
    ) -> Result<Vec<NapiOnWhatsAppResult>> {
        let results = self
            .run_async(move |c| async move {
                let refs: Vec<&str> = phone_numbers.iter().map(|s| s.as_str()).collect();
                c.check_on_whatsapp(&refs).await
            })
            .await?;
        Ok(results.into_iter().map(Into::into).collect())
    }

    /// Requests a media re-upload URL from WhatsApp servers for an expired media message.
    #[napi]
    pub async fn update_media_message(&self, message_id: String) -> Result<()> {
        self.run_async(move |c| async move { c.update_media_message(&message_id).await })
            .await
    }

    /// Fetches rich OpenGraph metadata preview for text containing a URL.
    #[napi]
    pub async fn fetch_url_preview(&self, text: String) -> Result<Option<NapiUrlPreview>> {
        let prev = self
            .run_async(move |c| async move { c.fetch_url_preview(&text).await })
            .await?;
        Ok(prev.map(|p| NapiUrlPreview {
            matched_text: p.matched_text,
            canonical_url: p.canonical_url,
            title: p.title,
            description: p.description,
            jpeg_thumbnail: p.jpeg_thumbnail,
        }))
    }

    /// Sends a text message with rich link preview metadata.
    #[napi]
    pub async fn send_text_with_preview(&self, to_jid: String, text: String) -> Result<String> {
        self.run_async(move |c| async move { c.send_text_with_preview(&to_jid, &text).await })
            .await
    }

    /// Resolves LID from Phone Number JID.
    #[napi]
    pub async fn get_lid_for_pn(&self, pn_jid: String) -> Result<Option<String>> {
        let res = self
            .run_async(move |c| async move {
                let arr = [pn_jid.as_str()];
                c.get_lid_for_pn(&arr).await
            })
            .await?;
        Ok(res.into_iter().next().map(|(_, lid)| lid))
    }

    /// Resolves Phone Number JID from LID.
    #[napi]
    pub async fn get_pn_for_lid(&self, lid_jid: String) -> Result<Option<String>> {
        let res = self
            .run_async(move |c| async move {
                let arr = [lid_jid.as_str()];
                c.get_pn_for_lid(&arr).await
            })
            .await?;
        Ok(res.into_iter().next().map(|(_, pn)| pn))
    }

    /// Fetches contact status text and timestamp.
    #[napi]
    pub async fn get_user_status(&self, jid: String) -> Result<Option<NapiUserStatus>> {
        let res = self
            .run_async(move |c| async move {
                let arr = [jid.as_str()];
                c.get_user_status(&arr).await
            })
            .await?;
        Ok(res.into_iter().next().map(|(j, s, t)| NapiUserStatus {
            jid: j,
            status: s,
            timestamp: t as i64,
        }))
    }

    /// Creates a WhatsApp call link for audio or video call.
    #[napi]
    pub async fn create_call_link(&self, is_video: bool) -> Result<Option<String>> {
        self.run_async(move |c| async move { c.create_call_link(is_video).await })
            .await
    }

    /// Cleans dirty bits on WhatsApp server.
    #[napi]
    pub async fn clean_dirty_bits(&self, dirty_type: String, timestamp: Option<i64>) -> Result<()> {
        let ts = timestamp.map(|t| t as u64);
        self.run_async(move |c| async move { c.clean_dirty_bits(&dirty_type, ts).await })
            .await
    }

    /// Fetches account reachout timelock / spam enforcement standing.
    #[napi]
    pub async fn fetch_account_reachout_timelock(
        &self,
    ) -> Result<Option<NapiAccountReachoutTimelock>> {
        let res = self
            .run_async(move |c| async move { c.fetch_account_reachout_timelock().await })
            .await?;
        Ok(
            res.map(|(is_active, enforcement_type, time_enforcement_ends)| {
                NapiAccountReachoutTimelock {
                    is_active,
                    enforcement_type,
                    time_enforcement_ends: time_enforcement_ends.map(|t| t as i64),
                }
            }),
        )
    }

    /// Fetches new chat message cap limit.
    #[napi]
    pub async fn fetch_new_chat_message_cap(&self) -> Result<Option<String>> {
        let res = self
            .run_async(move |c| async move { c.fetch_new_chat_message_cap().await })
            .await?;
        Ok(res.map(|v| v.to_string()))
    }

    /// Fetches the WhatsApp AI Bot list v2.
    #[napi]
    pub async fn get_bot_list_v2(&self) -> Result<Vec<NapiBotEntry>> {
        let bots = self
            .run_async(move |c| async move { c.get_bot_list_v2().await })
            .await?;
        Ok(bots
            .into_iter()
            .map(|(j, p)| NapiBotEntry {
                jid: j,
                persona_id: p,
            })
            .collect())
    }

    /// Queries administrator count for a newsletter via GraphQL MEX.
    #[napi]
    pub async fn newsletter_admin_count(&self, channel_jid: String) -> Result<u32> {
        self.run_async(move |c| async move { c.newsletter_admin_count(&channel_jid).await })
            .await
    }

    /// Subscribes to live updates (edits, reactions) for a newsletter.
    #[napi]
    pub async fn subscribe_newsletter_updates(
        &self,
        channel_jid: String,
    ) -> Result<Option<String>> {
        self.run_async(move |c| async move { c.subscribe_newsletter_updates(&channel_jid).await })
            .await
    }

    /// Revokes an existing community invite code and generates a new one.
    #[napi]
    pub async fn community_revoke_invite(&self, community_jid: String) -> Result<String> {
        self.run_async(move |c| async move { c.community_revoke_invite(&community_jid).await })
            .await
    }

    /// Gets invite info for a community invite link code.
    #[napi]
    pub async fn community_get_invite_info(
        &self,
        invite_code: String,
    ) -> Result<NapiCommunityInviteInfo> {
        let info = self
            .run_async(move |c| async move { c.community_get_invite_info(&invite_code).await })
            .await?;
        Ok(NapiCommunityInviteInfo {
            jid: info.jid,
            subject: info.subject,
            creator: info.creator,
            size: info.size.map(|s| s as u32),
        })
    }

    /// Updates ephemeral disappearing messages setting for a community.
    #[napi]
    pub async fn community_toggle_ephemeral(
        &self,
        community_jid: String,
        duration_sec: u32,
    ) -> Result<()> {
        self.run_async(move |c| async move {
            c.community_toggle_ephemeral(&community_jid, duration_sec)
                .await
        })
        .await
    }

    /// Updates announcement or locked setting for a community.
    #[napi]
    pub async fn community_setting_update(
        &self,
        community_jid: String,
        setting: String,
    ) -> Result<()> {
        self.run_async(move |c| async move {
            c.community_setting_update(&community_jid, &setting).await
        })
        .await
    }

    /// Sets member add mode for a community ("admin_add" or "all_member_add").
    #[napi]
    pub async fn community_member_add_mode(
        &self,
        community_jid: String,
        mode: String,
    ) -> Result<()> {
        self.run_async(
            move |c| async move { c.community_member_add_mode(&community_jid, &mode).await },
        )
        .await
    }

    /// Sets join approval mode for a community ("on" or "off").
    #[napi]
    pub async fn community_join_approval_mode(
        &self,
        community_jid: String,
        mode: String,
    ) -> Result<()> {
        self.run_async(move |c| async move {
            c.community_join_approval_mode(&community_jid, &mode).await
        })
        .await
    }

    /// Fetches all participating community JIDs.
    #[napi]
    pub async fn community_fetch_all_participating(&self) -> Result<Vec<String>> {
        self.run_async(move |c| async move { c.community_fetch_all_participating().await })
            .await
    }

    /// Deletes one or more commerce catalog products.
    #[napi]
    pub async fn product_delete(&self, product_ids: Vec<String>) -> Result<()> {
        self.run_async(move |c| async move {
            let ids: Vec<&str> = product_ids.iter().map(|s| s.as_str()).collect();
            c.product_delete(&ids).await
        })
        .await
    }

    /// Sets business account cover photo.
    #[napi]
    pub async fn update_cover_photo(&self, photo_id: String, token: String, ts: i64) -> Result<()> {
        self.run_async(
            move |c| async move { c.update_cover_photo(&photo_id, &token, ts as u64).await },
        )
        .await
    }

    /// Removes business account cover photo.
    #[napi]
    pub async fn remove_cover_photo(&self, photo_id: String) -> Result<()> {
        self.run_async(move |c| async move { c.remove_cover_photo(&photo_id).await })
            .await
    }

    /// Updates link preview privacy setting (disable_link_previews).
    #[napi]
    pub async fn update_disable_link_previews_privacy(&self, is_disabled: bool) -> Result<()> {
        let val = if is_disabled { "true" } else { "false" };
        self.run_async(move |c| async move {
            c.update_privacy_setting("disable_link_previews", val).await
        })
        .await
    }

    /// Returns current health telemetry snapshot (message throughput, ping latency, Signal hit rate).
    #[napi]
    pub fn get_telemetry_snapshot(&self) -> NapiTelemetrySnapshot {
        let snap = self.client.telemetry_snapshot();
        NapiTelemetrySnapshot {
            messages_in: snap.messages_in as i64,
            messages_out: snap.messages_out as i64,
            last_ping_latency_ms: snap.last_ping_latency_ms as i64,
            signal_cache_hit_rate_pct: snap.signal_cache_hit_rate_pct,
            reconnect_count: snap.reconnect_count as i64,
        }
    }

    /// Returns the current formal connection state string.
    #[napi]
    pub fn get_connection_state(&self) -> String {
        format!("{:?}", self.client.get_connection_state())
    }

    /// Promotes an existing newsletter follower to admin.
    #[napi]
    pub async fn newsletter_promote(&self, channel_jid: String, user_jid: String) -> Result<()> {
        self.run_async(move |c| async move { c.newsletter_promote(&channel_jid, &user_jid).await })
            .await
    }

    /// Demotes a newsletter admin back to subscriber.
    #[napi]
    pub async fn newsletter_demote(&self, channel_jid: String, user_jid: String) -> Result<()> {
        self.run_async(move |c| async move { c.newsletter_demote(&channel_jid, &user_jid).await })
            .await
    }

    /// Transfers ownership of a newsletter to another admin.
    #[napi]
    pub async fn newsletter_change_owner(
        &self,
        channel_jid: String,
        new_owner_jid: String,
    ) -> Result<()> {
        self.run_async(move |c| async move {
            c.newsletter_change_owner(&channel_jid, &new_owner_jid)
                .await
        })
        .await
    }
}
