#![allow(clippy::too_many_arguments)]

use baileys_core::{BaileysClient, BaileysConfigBuilder, BaileysEvent};
use baileys_store::SqliteStore;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::runtime::Runtime;

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    #[error("Initialization error: {0}")]
    InitError(String),
    #[error("Operation error: {0}")]
    OperationError(String),
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysClientConfig {
    pub database_path: String,
    pub browser_name: String,
    pub platform: String,
    pub phone_number: Option<String>,
    pub auto_download_media: bool,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysMessage {
    pub id: String,
    pub remote_jid: String,
    pub participant_jid: Option<String>,
    pub from_me: bool,
    pub timestamp: u64,
    pub summary: String,
    pub has_media: bool,
    pub is_starred: bool,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysChat {
    pub jid: String,
    pub name: Option<String>,
    pub unread_count: u32,
    pub last_message_timestamp: u64,
    pub last_message_summary: Option<String>,
    pub pinned: bool,
    pub archived: bool,
    pub mute_until: Option<u64>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysTelemetrySnapshot {
    pub messages_in: u64,
    pub messages_out: u64,
    pub last_ping_latency_ms: u64,
    pub signal_cache_hit_rate_pct: f64,
    pub reconnect_count: u64,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysContact {
    pub jid: String,
    pub name: Option<String>,
    pub push_name: Option<String>,
    pub lid: Option<String>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysParticipantUpdateResult {
    pub jid: String,
    pub status: String,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysGroupInfo {
    pub jid: String,
    pub subject: String,
    pub owner: Option<String>,
    pub participant_count: u32,
    pub participants: Vec<String>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysGroupParticipant {
    pub jid: String,
    pub phone_number: Option<String>,
    pub lid: Option<String>,
    pub admin: Option<String>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysGroupMetadata {
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
    pub participants: Vec<RolleysGroupParticipant>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysGroupJoinRequest {
    pub jid: String,
    pub requested_time: i64,
    pub request_method: Option<String>,
}

impl From<baileys_core::groups::GroupParticipant> for RolleysGroupParticipant {
    fn from(p: baileys_core::groups::GroupParticipant) -> Self {
        Self {
            jid: p.jid,
            phone_number: p.phone_number,
            lid: p.lid,
            admin: p.admin,
        }
    }
}

impl From<baileys_core::groups::GroupMetadata> for RolleysGroupMetadata {
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

impl From<baileys_core::groups::GroupJoinRequest> for RolleysGroupJoinRequest {
    fn from(r: baileys_core::groups::GroupJoinRequest) -> Self {
        Self {
            jid: r.jid,
            requested_time: r.requested_time,
            request_method: r.request_method,
        }
    }
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysNewsletter {
    pub jid: String,
    pub name: String,
    pub description: Option<String>,
    pub invite_code: Option<String>,
    pub picture_url: Option<String>,
    pub subscribers_count: u64,
    pub role: Option<String>,
    pub muted: bool,
    pub verified: bool,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysNewsletterMessage {
    pub server_id: u64,
    pub views_count: Option<u64>,
    pub timestamp: u64,
    pub summary: String,
    pub has_media: bool,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysPoll {
    pub id: String,
    pub chat_jid: String,
    pub sender_jid: String,
    pub question: String,
    pub options: Vec<String>,
    pub selectable_count: u32,
    pub created_at: u64,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysPollTally {
    pub poll_id: String,
    pub question: String,
    pub total_voters: u32,
    pub option_votes: HashMap<String, u32>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysInteractiveButton {
    pub name: String,
    pub button_params_json: String,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysListRow {
    pub title: String,
    pub description: Option<String>,
    pub row_id: String,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysListSection {
    pub title: String,
    pub rows: Vec<RolleysListRow>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysCommunityParticipant {
    pub jid: String,
    pub admin: Option<String>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysCommunityMetadata {
    pub id: String,
    pub subject: String,
    pub subject_owner: Option<String>,
    pub subject_time: u64,
    pub size: u32,
    pub creation: u64,
    pub owner: Option<String>,
    pub description: Option<String>,
    pub description_id: Option<String>,
    pub linked_parent: Option<String>,
    pub is_community: bool,
    pub is_community_announce: bool,
    pub join_approval_mode: bool,
    pub member_add_mode: bool,
    pub participants: Vec<RolleysCommunityParticipant>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysCommunitySubgroup {
    pub id: String,
    pub subject: String,
    pub creation: Option<u64>,
    pub creator: Option<String>,
    pub size: Option<u32>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysCallEvent {
    pub id: String,
    pub chat_jid: String,
    pub from: String,
    pub caller_pn: Option<String>,
    pub is_group: bool,
    pub group_jid: Option<String>,
    pub is_video: bool,
    pub status: String,
    pub timestamp: u64,
    pub offline: bool,
    pub latency_ms: Option<u32>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysStoredCall {
    pub call_id: String,
    pub chat_jid: String,
    pub caller_jid: String,
    pub is_group: bool,
    pub group_jid: Option<String>,
    pub is_video: bool,
    pub status: String,
    pub timestamp: u64,
    pub duration: Option<u32>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysStoredStory {
    pub msg_id: String,
    pub author_jid: String,
    pub author_name: Option<String>,
    pub timestamp: u64,
    pub message_type: String,
    pub content: Option<String>,
    pub caption: Option<String>,
    pub has_media: bool,
    pub viewed: bool,
    pub background_argb: Option<u32>,
    pub text_argb: Option<u32>,
    pub font: Option<i32>,
    pub mimetype: Option<String>,
    pub seconds: Option<u32>,
    pub is_ptt: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl From<baileys_store::StoredStory> for RolleysStoredStory {
    fn from(s: baileys_store::StoredStory) -> Self {
        Self {
            msg_id: s.msg_id,
            author_jid: s.author_jid,
            author_name: s.author_name,
            timestamp: s.timestamp,
            message_type: s.message_type,
            content: s.content,
            caption: s.caption,
            has_media: s.has_media,
            viewed: s.viewed,
            background_argb: s.background_argb,
            text_argb: s.text_argb,
            font: s.font,
            mimetype: s.mimetype,
            seconds: s.seconds,
            is_ptt: s.is_ptt,
            width: s.width,
            height: s.height,
        }
    }
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysPrivacySettings {
    pub readreceipts: Option<String>,
    pub profile: Option<String>,
    pub status: Option<String>,
    pub online: Option<String>,
    pub last: Option<String>,
    pub groupadd: Option<String>,
    pub calladd: Option<String>,
    pub raw: HashMap<String, String>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysProfileStatus {
    pub status: String,
    pub set_at: Option<u64>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysBusinessProfile {
    pub jid: String,
    pub address: Option<String>,
    pub description: Option<String>,
    pub websites: Vec<String>,
    pub email: Option<String>,
    pub category: Option<String>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysLabel {
    pub id: String,
    pub name: String,
    pub color: i32,
    pub predefined_id: Option<i32>,
    pub deleted: bool,
}

impl From<baileys_store::StoredLabel> for RolleysLabel {
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

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysCatalogProduct {
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

impl From<baileys_core::CatalogProduct> for RolleysCatalogProduct {
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

impl From<baileys_store::StoredCatalogProduct> for RolleysCatalogProduct {
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

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysCatalogCollection {
    pub id: String,
    pub biz_jid: String,
    pub name: String,
    pub products_count: u32,
    pub status: String,
}

impl From<baileys_core::CatalogCollection> for RolleysCatalogCollection {
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

impl From<baileys_store::StoredCatalogCollection> for RolleysCatalogCollection {
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

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysCatalogResult {
    pub products: Vec<RolleysCatalogProduct>,
    pub cursor: Option<String>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysCollectionsResult {
    pub collections: Vec<RolleysCatalogCollection>,
    pub cursor: Option<String>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysOrder {
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
    pub created_at: u64,
}

impl From<baileys_store::StoredOrder> for RolleysOrder {
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
            created_at: o.created_at,
        }
    }
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysDeviceRecord {
    pub id: u32,
    pub key_index: Option<u32>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysOnWhatsAppResult {
    pub jid: String,
    pub exists: bool,
    pub phone_number: String,
}

#[derive(uniffi::Enum, Clone, Debug)]
pub enum RolleysEvent {
    Connecting,
    Connected {
        me_jid: String,
    },
    Disconnected {
        reason: String,
    },
    QrCode {
        code: String,
    },
    PairingCode {
        code: String,
    },
    MessageReceived {
        message: RolleysMessage,
    },
    ReceiptUpdate {
        id: String,
        remote_jid: String,
        status: String,
    },
    PresenceUpdate {
        jid: String,
        state: String,
    },
    HistorySync {
        sync_type: i32,
        progress: u32,
        conversation_count: u32,
    },
    PollVoteReceived {
        poll_id: String,
        voter_jid: String,
        selected_options: Vec<String>,
        tally: RolleysPollTally,
    },
    CallReceived {
        call: RolleysCallEvent,
    },
    StoryReceived {
        story: RolleysStoredStory,
    },
    BlocklistUpdate {
        added: Vec<String>,
        removed: Vec<String>,
    },
    MessageEdited {
        chat_jid: String,
        message_id: String,
        new_text: String,
    },
    MessageRevoked {
        chat_jid: String,
        message_id: String,
        participant: Option<String>,
    },
    ChatEphemeralUpdate {
        chat_jid: String,
        ephemeral_expiration: u32,
    },
    LabelEdit {
        label: RolleysLabel,
    },
    ChatLabelUpdate {
        chat_jid: String,
        label_id: String,
        labeled: bool,
    },
    MessageLabelUpdate {
        chat_jid: String,
        message_id: String,
        label_id: String,
        labeled: bool,
    },
    DeviceUpdate {
        user_jid: String,
        devices: Vec<u32>,
    },
    IdentityChanged {
        jid: String,
        timestamp: u64,
    },
    MediaUpdated {
        message_id: String,
        direct_path: String,
    },
    ChatUpdate {
        jid: String,
        pinned: Option<bool>,
        archived: Option<bool>,
        mute_until: Option<u64>,
        unread_count: Option<u32>,
    },
    ContactUpdate {
        jid: String,
        name: Option<String>,
        push_name: Option<String>,
    },
    AppStateSynced {
        collections_synced: u32,
        mutations_applied: u32,
    },
    MediaProgress {
        task_id: String,
        direction: String,
        bytes_transferred: u64,
        total_bytes: u64,
        percentage: f32,
    },
    /// Credentials were persisted (Baileys `creds.update`).
    CredsUpdate,
    /// Group created or joined (Baileys `groups.upsert`).
    GroupsUpsert {
        groups: Vec<RolleysGroupMetadata>,
    },
    /// Partial group change (Baileys `groups.update`).
    GroupsUpdate {
        update: RolleysGroupUpdate,
    },
    /// Baileys `group-participants.update`; `action` is add/remove/leave/promote/demote/modify.
    GroupParticipantsUpdate {
        group_jid: String,
        author: Option<String>,
        participants: Vec<RolleysGroupParticipant>,
        action: String,
    },
    /// Baileys `group.join-request`.
    GroupJoinRequest {
        group_jid: String,
        author: Option<String>,
        participant: String,
        participant_pn: Option<String>,
        action: String,
        method: Option<String>,
    },
    /// Baileys `group.member-tag.update`.
    GroupMemberTagUpdate {
        group_jid: String,
        participant: String,
        participant_alt: Option<String>,
        label: String,
        message_timestamp: u64,
    },
    /// Baileys `newsletter.reaction`.
    NewsletterReaction {
        newsletter_jid: String,
        server_id: String,
        code: Option<String>,
        count: Option<u32>,
        removed: bool,
    },
    /// Baileys `newsletter.view`.
    NewsletterView {
        newsletter_jid: String,
        server_id: String,
        count: u32,
    },
    /// Baileys `newsletter-participants.update`.
    NewsletterParticipantsUpdate {
        newsletter_jid: String,
        author: Option<String>,
        user: String,
        action: String,
        new_role: Option<String>,
    },
    /// Baileys `newsletter-settings.update`.
    NewsletterSettingsUpdate {
        newsletter_jid: String,
        name: Option<String>,
        description: Option<String>,
    },
    /// Baileys `chats.upsert`.
    ChatsUpsert {
        chats: Vec<RolleysChatUpsert>,
    },
    /// Baileys `chats.delete`.
    ChatsDelete {
        jids: Vec<String>,
    },
    /// Baileys `chats.lock`.
    ChatsLock {
        jid: String,
        locked: bool,
    },
    /// Baileys `settings.update`.
    SettingsUpdate {
        setting: String,
        value: String,
    },
    /// Baileys `lid-mapping.update`.
    LidMappingUpdate {
        lid: String,
        pn: String,
    },
    /// Baileys `contacts.upsert`.
    ContactsUpsert {
        contacts: Vec<RolleysContact>,
    },
    /// Baileys `messages.delete` (deleted for me).
    MessagesDelete {
        chat_jid: String,
        message_id: String,
        from_me: bool,
    },
    /// Baileys `messages.update` with `starred`.
    MessageStarred {
        chat_jid: String,
        message_id: String,
        from_me: bool,
        starred: bool,
    },
    /// Baileys `messages.reaction`.
    MessagesReaction {
        reaction: RolleysMessageReaction,
    },
    /// Baileys `messaging-history.status`: `status` is "complete" or "paused".
    MessagingHistoryStatus {
        sync_type: i32,
        status: String,
        explicit: bool,
    },
    /// Baileys `message-capping.update`.
    MessageCappingUpdate {
        info: RolleysMessageCapInfo,
    },
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysGroupUpdate {
    pub id: String,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub subject_owner: Option<String>,
    pub subject_time: Option<i64>,
    pub description: Option<String>,
    pub description_deleted: bool,
    pub description_id: Option<String>,
    pub announce: Option<bool>,
    pub restrict: Option<bool>,
    pub join_approval_mode: Option<bool>,
    pub member_add_mode: Option<String>,
    pub ephemeral_duration: Option<u32>,
    pub invite_code: Option<String>,
}

impl From<baileys_core::events::GroupUpdate> for RolleysGroupUpdate {
    fn from(u: baileys_core::events::GroupUpdate) -> Self {
        Self {
            id: u.id,
            author: u.author,
            subject: u.subject,
            subject_owner: u.subject_owner,
            subject_time: u.subject_time,
            description: u.description,
            description_deleted: u.description_deleted,
            description_id: u.description_id,
            announce: u.announce,
            restrict: u.restrict,
            join_approval_mode: u.join_approval_mode,
            member_add_mode: u.member_add_mode,
            ephemeral_duration: u.ephemeral_duration,
            invite_code: u.invite_code,
        }
    }
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysChatUpsert {
    pub jid: String,
    pub name: Option<String>,
    pub conversation_timestamp: Option<i64>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysMessageReaction {
    pub chat_jid: String,
    pub message_id: String,
    pub target_from_me: bool,
    pub target_participant: Option<String>,
    pub sender: String,
    pub sender_from_me: bool,
    /// Emoji; empty when the reaction was removed.
    pub text: String,
    pub removed: bool,
    pub sender_timestamp_ms: Option<i64>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysMessageCapInfo {
    pub total_quota: Option<i64>,
    pub used_quota: Option<i64>,
    pub cycle_start_timestamp: Option<String>,
    pub cycle_end_timestamp: Option<String>,
    pub server_sent_timestamp: Option<String>,
    pub ote_status: Option<String>,
    pub mv_status: Option<String>,
    pub capping_status: Option<String>,
}

impl From<baileys_store::StoredContact> for RolleysContact {
    fn from(c: baileys_store::StoredContact) -> Self {
        Self {
            jid: c.jid,
            name: c.name,
            push_name: c.push_name,
            lid: c.lid,
        }
    }
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysUrlPreview {
    pub matched_text: String,
    pub canonical_url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub jpeg_thumbnail: Option<Vec<u8>>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysUserStatus {
    pub jid: String,
    pub status: String,
    pub timestamp: u64,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysCommunityInviteInfo {
    pub jid: String,
    pub subject: String,
    pub creator: Option<String>,
    pub size: Option<u32>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysBotEntry {
    pub jid: String,
    pub persona_id: String,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysAccountReachoutTimelock {
    pub is_active: bool,
    pub enforcement_type: Option<String>,
    pub time_enforcement_ends: Option<u64>,
}

#[uniffi::export(callback_interface)]
pub trait RolleysEventListener: Send + Sync + 'static {
    fn on_event(&self, event: RolleysEvent);
}

#[derive(uniffi::Object)]
pub struct RolleysClient {
    client: Arc<BaileysClient>,
    runtime: Arc<Runtime>,
}

#[uniffi::export]
impl RolleysClient {
    #[uniffi::constructor]
    pub fn new(config: RolleysClientConfig) -> Result<Arc<Self>, FfiError> {
        let runtime = Runtime::new().map_err(|e| FfiError::InitError(e.to_string()))?;

        let store = SqliteStore::open(&config.database_path)
            .map_err(|e| FfiError::InitError(e.to_string()))?;

        // Keep the default browser version (Baileys' Browsers.macOS: 14.4.1) instead of a made-up one.
        let browser_version = baileys_core::BaileysConfig::default().browser_version;
        let mut config_builder = BaileysConfigBuilder::new().with_browser(
            config.browser_name,
            config.platform,
            browser_version,
        );

        if let Some(ref phone) = config.phone_number {
            config_builder = config_builder.with_phone_pairing(phone);
        }

        let core_config = config_builder.build();
        let client = Arc::new(BaileysClient::new(Arc::new(store), core_config));

        if config.auto_download_media {
            client.set_auto_download(true);
        }

        Ok(Arc::new(Self {
            client,
            runtime: Arc::new(runtime),
        }))
    }

    /// Starts the client connection and forwards real-time events to the native Kotlin listener.
    pub fn start(&self, listener: Box<dyn RolleysEventListener>) -> Result<(), FfiError> {
        let mut rx = self.client.events();

        // Spawn event forwarding background task
        self.runtime.spawn(async move {
            while let Ok(event) = rx.recv().await {
                let ffi_event = match event {
                    BaileysEvent::Connecting => RolleysEvent::Connecting,
                    BaileysEvent::Connected { me_jid } => RolleysEvent::Connected { me_jid },
                    BaileysEvent::Disconnected { reason } => RolleysEvent::Disconnected { reason },
                    BaileysEvent::QrCode { code } => RolleysEvent::QrCode { code },
                    BaileysEvent::PairingCode { code } => RolleysEvent::PairingCode { code },
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
                        let participant_jid =
                            message.key.as_ref().and_then(|k| k.participant.clone());
                        let from_me = message
                            .key
                            .as_ref()
                            .and_then(|k| k.from_me)
                            .unwrap_or(false);
                        let timestamp = message.message_timestamp.unwrap_or(0);

                        let (summary, has_media) = if let Some(ref m) = message.message {
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

                        RolleysEvent::MessageReceived {
                            message: RolleysMessage {
                                id,
                                remote_jid,
                                participant_jid,
                                from_me,
                                timestamp,
                                summary,
                                has_media,
                                is_starred: message.starred.unwrap_or(false),
                            },
                        }
                    }
                    BaileysEvent::Receipt {
                        key_id,
                        remote_jid,
                        status,
                    } => RolleysEvent::ReceiptUpdate {
                        id: key_id,
                        remote_jid,
                        status,
                    },
                    BaileysEvent::Presence { jid, presence } => RolleysEvent::PresenceUpdate {
                        jid,
                        state: presence,
                    },
                    BaileysEvent::HistorySync {
                        sync_type,
                        progress,
                        conversations,
                        ..
                    } => RolleysEvent::HistorySync {
                        sync_type,
                        progress,
                        conversation_count: conversations.len() as u32,
                    },
                    BaileysEvent::PollVote {
                        poll_id,
                        voter_jid,
                        selected_options,
                        tally,
                    } => RolleysEvent::PollVoteReceived {
                        poll_id,
                        voter_jid,
                        selected_options,
                        tally: RolleysPollTally {
                            poll_id: tally.poll_id,
                            question: tally.question,
                            total_voters: tally.total_voters as u32,
                            option_votes: tally.option_votes,
                        },
                    },
                    BaileysEvent::Call { call } => RolleysEvent::CallReceived {
                        call: RolleysCallEvent {
                            id: call.id,
                            chat_jid: call.chat_jid,
                            from: call.from,
                            caller_pn: call.caller_pn,
                            is_group: call.is_group,
                            group_jid: call.group_jid,
                            is_video: call.is_video,
                            status: call.status,
                            timestamp: call.timestamp,
                            offline: call.offline,
                            latency_ms: call.latency_ms,
                        },
                    },
                    BaileysEvent::StoryReceived { story } => RolleysEvent::StoryReceived {
                        story: RolleysStoredStory::from(story),
                    },
                    BaileysEvent::BlocklistUpdate { added, removed } => {
                        RolleysEvent::BlocklistUpdate { added, removed }
                    }
                    BaileysEvent::MessageEdited {
                        chat_jid,
                        message_id,
                        new_text,
                    } => RolleysEvent::MessageEdited {
                        chat_jid,
                        message_id,
                        new_text,
                    },
                    BaileysEvent::MessageRevoked {
                        chat_jid,
                        message_id,
                        participant,
                    } => RolleysEvent::MessageRevoked {
                        chat_jid,
                        message_id,
                        participant,
                    },
                    BaileysEvent::ChatEphemeralUpdate {
                        chat_jid,
                        ephemeral_expiration,
                    } => RolleysEvent::ChatEphemeralUpdate {
                        chat_jid,
                        ephemeral_expiration,
                    },
                    BaileysEvent::LabelEdit { label } => RolleysEvent::LabelEdit {
                        label: label.into(),
                    },
                    BaileysEvent::ChatLabelUpdate {
                        chat_jid,
                        label_id,
                        labeled,
                    } => RolleysEvent::ChatLabelUpdate {
                        chat_jid,
                        label_id,
                        labeled,
                    },
                    BaileysEvent::MessageLabelUpdate {
                        chat_jid,
                        message_id,
                        label_id,
                        labeled,
                    } => RolleysEvent::MessageLabelUpdate {
                        chat_jid,
                        message_id,
                        label_id,
                        labeled,
                    },
                    BaileysEvent::DeviceUpdate { user_jid, devices } => {
                        RolleysEvent::DeviceUpdate { user_jid, devices }
                    }
                    BaileysEvent::IdentityChanged { jid, timestamp } => {
                        RolleysEvent::IdentityChanged { jid, timestamp }
                    }
                    BaileysEvent::MediaUpdated {
                        message_id,
                        direct_path,
                    } => RolleysEvent::MediaUpdated {
                        message_id,
                        direct_path,
                    },
                    BaileysEvent::ChatUpdate {
                        jid,
                        pinned,
                        archived,
                        mute_until,
                        unread_count,
                    } => RolleysEvent::ChatUpdate {
                        jid,
                        pinned,
                        archived,
                        mute_until: mute_until.flatten(),
                        unread_count,
                    },
                    BaileysEvent::ContactUpdate { contact } => RolleysEvent::ContactUpdate {
                        jid: contact.jid,
                        name: contact.name,
                        push_name: contact.push_name,
                    },
                    BaileysEvent::AppStateSynced {
                        collections_synced,
                        mutations_applied,
                    } => RolleysEvent::AppStateSynced {
                        collections_synced: collections_synced as u32,
                        mutations_applied: mutations_applied as u32,
                    },
                    BaileysEvent::MediaProgress {
                        task_id,
                        direction,
                        bytes_transferred,
                        total_bytes,
                        percentage,
                    } => RolleysEvent::MediaProgress {
                        task_id,
                        direction,
                        bytes_transferred,
                        total_bytes,
                        percentage,
                    },
                    BaileysEvent::CredsUpdate => RolleysEvent::CredsUpdate,
                    BaileysEvent::GroupsUpsert { groups } => RolleysEvent::GroupsUpsert {
                        groups: groups.into_iter().map(Into::into).collect(),
                    },
                    BaileysEvent::GroupsUpdate { update } => RolleysEvent::GroupsUpdate {
                        update: update.into(),
                    },
                    BaileysEvent::GroupParticipantsUpdate {
                        group_jid,
                        author,
                        participants,
                        action,
                    } => RolleysEvent::GroupParticipantsUpdate {
                        group_jid,
                        author,
                        participants: participants.into_iter().map(Into::into).collect(),
                        action,
                    },
                    BaileysEvent::GroupJoinRequest {
                        group_jid,
                        author,
                        participant,
                        participant_pn,
                        action,
                        method,
                    } => RolleysEvent::GroupJoinRequest {
                        group_jid,
                        author,
                        participant,
                        participant_pn,
                        action,
                        method,
                    },
                    BaileysEvent::GroupMemberTagUpdate {
                        group_jid,
                        participant,
                        participant_alt,
                        label,
                        message_timestamp,
                    } => RolleysEvent::GroupMemberTagUpdate {
                        group_jid,
                        participant,
                        participant_alt,
                        label,
                        message_timestamp,
                    },
                    BaileysEvent::NewsletterReaction {
                        newsletter_jid,
                        server_id,
                        code,
                        count,
                        removed,
                    } => RolleysEvent::NewsletterReaction {
                        newsletter_jid,
                        server_id,
                        code,
                        count,
                        removed,
                    },
                    BaileysEvent::NewsletterView {
                        newsletter_jid,
                        server_id,
                        count,
                    } => RolleysEvent::NewsletterView {
                        newsletter_jid,
                        server_id,
                        count,
                    },
                    BaileysEvent::NewsletterParticipantsUpdate {
                        newsletter_jid,
                        author,
                        user,
                        action,
                        new_role,
                    } => RolleysEvent::NewsletterParticipantsUpdate {
                        newsletter_jid,
                        author,
                        user,
                        action,
                        new_role,
                    },
                    BaileysEvent::NewsletterSettingsUpdate {
                        newsletter_jid,
                        name,
                        description,
                    } => RolleysEvent::NewsletterSettingsUpdate {
                        newsletter_jid,
                        name,
                        description,
                    },
                    BaileysEvent::ChatsUpsert { chats } => RolleysEvent::ChatsUpsert {
                        chats: chats
                            .into_iter()
                            .map(|c| RolleysChatUpsert {
                                jid: c.jid,
                                name: c.name,
                                conversation_timestamp: c.conversation_timestamp,
                            })
                            .collect(),
                    },
                    BaileysEvent::ChatsDelete { jids } => RolleysEvent::ChatsDelete { jids },
                    BaileysEvent::ChatsLock { jid, locked } => {
                        RolleysEvent::ChatsLock { jid, locked }
                    }
                    BaileysEvent::SettingsUpdate { setting, value } => {
                        RolleysEvent::SettingsUpdate { setting, value }
                    }
                    BaileysEvent::LidMappingUpdate { lid, pn } => {
                        RolleysEvent::LidMappingUpdate { lid, pn }
                    }
                    BaileysEvent::ContactsUpsert { contacts } => RolleysEvent::ContactsUpsert {
                        contacts: contacts.into_iter().map(Into::into).collect(),
                    },
                    BaileysEvent::MessagesDelete {
                        chat_jid,
                        message_id,
                        from_me,
                    } => RolleysEvent::MessagesDelete {
                        chat_jid,
                        message_id,
                        from_me,
                    },
                    BaileysEvent::MessageStarred {
                        chat_jid,
                        message_id,
                        from_me,
                        starred,
                    } => RolleysEvent::MessageStarred {
                        chat_jid,
                        message_id,
                        from_me,
                        starred,
                    },
                    BaileysEvent::MessagesReaction { reaction: r } => {
                        RolleysEvent::MessagesReaction {
                            reaction: RolleysMessageReaction {
                                chat_jid: r.chat_jid,
                                message_id: r.message_id,
                                target_from_me: r.target_from_me,
                                target_participant: r.target_participant,
                                sender: r.sender,
                                sender_from_me: r.sender_from_me,
                                text: r.text,
                                removed: r.removed,
                                sender_timestamp_ms: r.sender_timestamp_ms,
                            },
                        }
                    }
                    BaileysEvent::MessagingHistoryStatus {
                        sync_type,
                        status,
                        explicit,
                    } => RolleysEvent::MessagingHistoryStatus {
                        sync_type,
                        status,
                        explicit,
                    },
                    BaileysEvent::MessageCappingUpdate { info: i } => {
                        RolleysEvent::MessageCappingUpdate {
                            info: RolleysMessageCapInfo {
                                total_quota: i.total_quota,
                                used_quota: i.used_quota,
                                cycle_start_timestamp: i.cycle_start_timestamp,
                                cycle_end_timestamp: i.cycle_end_timestamp,
                                server_sent_timestamp: i.server_sent_timestamp,
                                ote_status: i.ote_status,
                                mv_status: i.mv_status,
                                capping_status: i.capping_status,
                            },
                        }
                    }
                };

                listener.on_event(ffi_event);
            }
        });

        // Trigger connection
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.connect().await })
            .map_err(|e| FfiError::OperationError(e.to_string()))?;

        Ok(())
    }

    /// Sends a text message to a WhatsApp user or group JID.
    pub fn send_text(&self, to_jid: String, text: String) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.send_text_message(&to_jid, &text).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Sends an emoji reaction to a message.
    pub fn send_reaction(
        &self,
        to_jid: String,
        target_msg_id: String,
        emoji: String,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.send_reaction(&to_jid, &target_msg_id, &emoji).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Revokes (deletes for everyone) a sent message.
    pub fn send_revoke(&self, to_jid: String, target_msg_id: String) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.send_revoke_message(&to_jid, &target_msg_id).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Sends a read receipt for a message.
    pub fn send_read(
        &self,
        remote_jid: String,
        msg_id: String,
        participant: Option<String>,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_read_receipt(&remote_jid, &msg_id, participant.as_deref())
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Updates client presence status (e.g. "available", "unavailable", "composing", "paused").
    pub fn send_presence(&self, state: String, to_jid: Option<String>) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.send_presence_update(&state, to_jid.as_deref()).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Encrypts, uploads, and sends an image message.
    pub fn send_image(
        &self,
        to_jid: String,
        image_bytes: Vec<u8>,
        caption: Option<String>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_image_message(&to_jid, &image_bytes, caption.as_deref())
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Sends a text message quoting/replying to a previous message.
    pub fn send_text_with_quote(
        &self,
        to_jid: String,
        text: String,
        quoted_id: String,
        quoted_participant: Option<String>,
        quoted_text: Option<String>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_text_message_with_quote(
                        &to_jid,
                        &text,
                        &quoted_id,
                        quoted_participant.as_deref(),
                        quoted_text.as_deref(),
                    )
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Encrypts, uploads, and sends a video message.
    pub fn send_video(
        &self,
        to_jid: String,
        video_bytes: Vec<u8>,
        caption: Option<String>,
        seconds: Option<u32>,
        gif_playback: bool,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_video_message(
                        &to_jid,
                        &video_bytes,
                        caption.as_deref(),
                        seconds,
                        gif_playback,
                    )
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Encrypts, uploads, and sends an audio message or voice note (PTT).
    pub fn send_audio(
        &self,
        to_jid: String,
        audio_bytes: Vec<u8>,
        ptt: bool,
        seconds: Option<u32>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_audio_message(&to_jid, &audio_bytes, ptt, seconds)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Encrypts, uploads, and sends a document/file message.
    pub fn send_document(
        &self,
        to_jid: String,
        document_bytes: Vec<u8>,
        file_name: String,
        mimetype: String,
        title: Option<String>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_document_message(
                        &to_jid,
                        &document_bytes,
                        &file_name,
                        &mimetype,
                        title.as_deref(),
                    )
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Encrypts, uploads, and sends a sticker message.
    pub fn send_sticker(
        &self,
        to_jid: String,
        sticker_bytes: Vec<u8>,
        is_animated: bool,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_sticker_message(&to_jid, &sticker_bytes, is_animated)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Sends a voice note / PTT audio message (Opus format, waveform).
    pub fn send_voice_note(
        &self,
        to_jid: String,
        audio_bytes: Vec<u8>,
        seconds: u32,
        waveform: Option<Vec<u8>>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_voice_note(&to_jid, &audio_bytes, seconds, waveform.as_deref())
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Sends a document attachment with title, page count, and caption.
    pub fn send_document_attachment(
        &self,
        to_jid: String,
        doc_bytes: Vec<u8>,
        filename: String,
        mimetype: String,
        title: Option<String>,
        page_count: Option<u32>,
        caption: Option<String>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_document(
                        &to_jid,
                        &doc_bytes,
                        &filename,
                        &mimetype,
                        title.as_deref(),
                        page_count,
                        caption.as_deref(),
                    )
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Downloads and decrypts media from a message ID into dest_dir, returning the saved path.
    pub fn download_media(&self, msg_id: String, dest_dir: String) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                let path = client
                    .download_media_for_message(&msg_id, Path::new(&dest_dir))
                    .await?;
                Ok(path.to_string_lossy().to_string())
            })
            .map_err(|e: baileys_core::CoreError| FfiError::OperationError(e.to_string()))
    }

    /// Downloads and decrypts media from a message ID using the streaming pipeline (~64KB RAM),
    /// saving it to dest_path (or a default path if not specified) and returning the resulting path.
    pub fn download_media_streaming(
        &self,
        msg_id: String,
        dest_path: Option<String>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                let target = dest_path.as_deref().map(Path::new);
                let path = client
                    .download_message_media_to_path(&msg_id, target)
                    .await?;
                Ok(path.to_string_lossy().to_string())
            })
            .map_err(|e: baileys_core::CoreError| FfiError::OperationError(e.to_string()))
    }

    /// Sends a local media file using the streaming upload pipeline (zero-OOM spooling for >= 10MB)
    /// and emits real-time RolleysEvent::MediaProgress events.
    pub fn send_media_file(
        &self,
        chat_jid: String,
        file_path: String,
        media_type: String,
        caption: Option<String>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        let parsed_type: baileys_media::MediaType = media_type
            .parse()
            .map_err(|e: baileys_media::MediaError| FfiError::OperationError(e.to_string()))?;
        self.runtime
            .block_on(async move {
                client
                    .send_media_file_streaming(
                        &chat_jid,
                        Path::new(&file_path),
                        parsed_type,
                        caption.as_deref(),
                    )
                    .await
            })
            .map_err(|e: baileys_core::CoreError| FfiError::OperationError(e.to_string()))
    }

    /// Toggles auto-downloading of incoming media.
    pub fn set_auto_download(&self, enabled: bool) {
        self.client.set_auto_download(enabled);
    }

    /// Returns whether auto-downloading is currently enabled.
    pub fn is_auto_download(&self) -> bool {
        self.client.is_auto_download()
    }

    /// Queries group info (subject, participants) from WhatsApp server.
    pub fn get_group_info(&self, group_jid: String) -> Result<RolleysGroupInfo, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                let node = client.group_metadata(&group_jid).await?;
                let group_node = node.get_child("group").ok_or_else(|| {
                    baileys_core::CoreError::Protocol("Missing <group> child".into())
                })?;

                let subject = group_node.get_attr("subject").unwrap_or("").to_string();
                let owner = group_node.get_attr("owner").map(|s| s.to_string());
                let participants: Vec<String> = group_node
                    .get_children()
                    .iter()
                    .filter(|c| c.tag == "participant")
                    .filter_map(|p| p.get_attr("jid").map(|s| s.to_string()))
                    .collect();
                let participant_count = participants.len() as u32;

                Ok(RolleysGroupInfo {
                    jid: group_jid,
                    subject,
                    owner,
                    participant_count,
                    participants,
                })
            })
            .map_err(|e: baileys_core::CoreError| FfiError::OperationError(e.to_string()))
    }

    /// Returns round-trip gateway ping latency in milliseconds.
    pub fn ping(&self) -> Result<u64, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                let dur = client.ping().await?;
                Ok(dur.as_millis() as u64)
            })
            .map_err(|e: baileys_core::CoreError| FfiError::OperationError(e.to_string()))
    }

    /// Returns whether the client is currently connected.
    pub fn is_connected(&self) -> bool {
        self.client.is_connected()
    }

    /// Returns the currently authenticated JID if paired/connected.
    pub fn get_me_jid(&self) -> Option<String> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.me_jid().await.unwrap_or(None) })
    }

    /// Returns all saved chats ordered by recent activity.
    pub fn get_chats(&self) -> Result<Vec<RolleysChat>, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let chats = client
                .get_chats()
                .await
                .map_err(|e| FfiError::OperationError(e.to_string()))?;
            Ok(chats
                .into_iter()
                .map(|c| RolleysChat {
                    jid: c.jid,
                    name: c.name,
                    unread_count: c.unread_count,
                    last_message_timestamp: c.last_message_timestamp,
                    last_message_summary: c.last_message_summary,
                    pinned: c.pinned,
                    archived: c.archived,
                    mute_until: c.mute_until,
                })
                .collect())
        })
    }

    /// Returns historical messages for a chat from SQLite.
    pub fn get_messages(
        &self,
        chat_jid: String,
        limit: u32,
        before_timestamp: Option<u64>,
    ) -> Result<Vec<RolleysMessage>, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let msgs = client
                .get_messages(&chat_jid, limit, before_timestamp)
                .await
                .map_err(|e| FfiError::OperationError(e.to_string()))?;
            Ok(msgs
                .into_iter()
                .map(|m| RolleysMessage {
                    id: m.id,
                    remote_jid: m.chat_jid,
                    participant_jid: Some(m.sender_jid),
                    from_me: m.from_me,
                    timestamp: m.timestamp,
                    summary: m.summary,
                    has_media: m.has_media,
                    is_starred: m.is_starred,
                })
                .collect())
        })
    }

    /// Queries contact info by JID from SQLite.
    pub fn get_contact(&self, jid: String) -> Result<Option<RolleysContact>, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let contact = client
                .get_contact(&jid)
                .await
                .map_err(|e| FfiError::OperationError(e.to_string()))?;
            Ok(contact.map(|c| RolleysContact {
                jid: c.jid,
                name: c.name,
                push_name: c.push_name,
                lid: c.lid,
            }))
        })
    }

    /// Creates a new WhatsApp group with subject and initial participants.
    pub fn group_create(
        &self,
        subject: String,
        participants: Vec<String>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.group_create(&subject, &participants).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Updates participants in a group ("add", "remove", "promote", "demote").
    pub fn group_participants_update(
        &self,
        group_jid: String,
        participants: Vec<String>,
        action: String,
    ) -> Result<Vec<RolleysParticipantUpdateResult>, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                let res = client
                    .group_participants_update(&group_jid, &participants, &action)
                    .await?;
                Ok(res
                    .into_iter()
                    .map(|(jid, status)| RolleysParticipantUpdateResult { jid, status })
                    .collect())
            })
            .map_err(|e: baileys_core::CoreError| FfiError::OperationError(e.to_string()))
    }

    /// Updates the subject/title of a group.
    pub fn group_update_subject(&self, group_jid: String, subject: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.group_update_subject(&group_jid, &subject).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Updates the description of a group.
    pub fn group_update_description(
        &self,
        group_jid: String,
        description: String,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .group_update_description(&group_jid, &description)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Gets the invite code for a group.
    pub fn group_get_invite_code(&self, group_jid: String) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.group_get_invite_code(&group_jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Joins a group using an invite code.
    pub fn group_accept_invite(&self, code: String) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.group_accept_invite(&code).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Leaves a WhatsApp group.
    pub fn group_leave(&self, group_jid: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.group_leave(&group_jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Updates group settings ("announcement", "not_announcement", "locked", "unlocked").
    pub fn group_setting_update(&self, group_jid: String, setting: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.group_setting_update(&group_jid, &setting).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Revokes the current invite code of a group and generates a new one.
    pub fn group_revoke_invite(&self, group_jid: String) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.group_revoke_invite(&group_jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Fetches comprehensive group metadata including participants, roles, and configuration.
    pub fn group_fetch_metadata(
        &self,
        group_jid: String,
    ) -> Result<RolleysGroupMetadata, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                let meta = client.group_fetch_metadata(&group_jid).await?;
                Ok(meta.into())
            })
            .map_err(|e: baileys_core::CoreError| FfiError::OperationError(e.to_string()))
    }

    /// Fetches all participating groups for the authenticated account and caches them locally.
    pub fn group_fetch_all_participating(&self) -> Result<Vec<RolleysGroupMetadata>, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                let groups = client.group_fetch_all_participating().await?;
                Ok(groups.into_iter().map(Into::into).collect())
            })
            .map_err(|e: baileys_core::CoreError| FfiError::OperationError(e.to_string()))
    }

    /// Configures membership approval mode for a group ("on" or "off").
    pub fn group_join_approval_mode(
        &self,
        group_jid: String,
        mode: String,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.group_join_approval_mode(&group_jid, &mode).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Fetches pending membership join requests for a group.
    pub fn group_request_participants_list(
        &self,
        group_jid: String,
    ) -> Result<Vec<RolleysGroupJoinRequest>, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                let reqs = client.group_request_participants_list(&group_jid).await?;
                Ok(reqs.into_iter().map(Into::into).collect())
            })
            .map_err(|e: baileys_core::CoreError| FfiError::OperationError(e.to_string()))
    }

    /// Approves or rejects pending participant join requests ("approve" or "reject").
    pub fn group_request_participants_update(
        &self,
        group_jid: String,
        participants: Vec<String>,
        action: String,
    ) -> Result<Vec<RolleysParticipantUpdateResult>, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                let res = client
                    .group_request_participants_update(&group_jid, &participants, &action)
                    .await?;
                Ok(res
                    .into_iter()
                    .map(|(jid, status)| RolleysParticipantUpdateResult { jid, status })
                    .collect())
            })
            .map_err(|e: baileys_core::CoreError| FfiError::OperationError(e.to_string()))
    }

    /// Configures member add mode for a group ("admin_add" or "all_member_add").
    pub fn group_member_add_mode(&self, group_jid: String, mode: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.group_member_add_mode(&group_jid, &mode).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Revokes a participant's v4 group invite.
    pub fn group_revoke_invite_v4(
        &self,
        group_jid: String,
        invited_jid: String,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .group_revoke_invite_v4(&group_jid, &invited_jid)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Accepts a v4 group invite directly via IQ stanza. Returns joined group JID.
    pub fn group_accept_invite_v4(
        &self,
        group_jid: String,
        invite_code: String,
        invite_expiration: i64,
        admin_jid: String,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .group_accept_invite_v4(&group_jid, &invite_code, invite_expiration, &admin_jid)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Updates your WhatsApp About / Bio status string.
    pub fn update_profile_status(&self, status: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.update_profile_status(&status).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Fetches the user's current privacy settings from WhatsApp gateway.
    pub fn fetch_privacy_settings(&self) -> Result<RolleysPrivacySettings, FfiError> {
        let client = self.client.clone();
        let s = self
            .runtime
            .block_on(async move { client.fetch_privacy_settings().await })
            .map_err(|e| FfiError::OperationError(e.to_string()))?;

        Ok(RolleysPrivacySettings {
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

    /// Updates a privacy setting (e.g. category="last", value="all"|"contacts"|"none").
    pub fn update_privacy_setting(&self, category: String, value: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.update_privacy_setting(&category, &value).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Fetches the full list of blocked contact JIDs from WhatsApp server and syncs to SQLite.
    pub fn fetch_blocklist(&self) -> Result<Vec<String>, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.fetch_blocklist().await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Reads all locally stored blocked contacts from SQLite.
    pub fn get_blocked_contacts(&self) -> Result<Vec<String>, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.get_blocked_contacts().await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Blocks or unblocks a contact ("block" or "unblock").
    pub fn update_block_status(&self, jid: String, action: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.update_block_status(&jid, &action).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Fetches the About / Bio status string of a user.
    pub fn fetch_status(&self, jid: String) -> Result<Option<RolleysProfileStatus>, FfiError> {
        let client = self.client.clone();
        let st = self
            .runtime
            .block_on(async move { client.fetch_status(&jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))?;

        Ok(st.map(|s| RolleysProfileStatus {
            status: s.status,
            set_at: s.set_at,
        }))
    }

    /// Queries business profile metadata for a WhatsApp Business account.
    pub fn get_business_profile(
        &self,
        jid: String,
    ) -> Result<Option<RolleysBusinessProfile>, FfiError> {
        let client = self.client.clone();
        let bp = self
            .runtime
            .block_on(async move { client.get_business_profile(&jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))?;

        Ok(bp.map(|b| RolleysBusinessProfile {
            jid: b.jid,
            address: b.address,
            description: b.description,
            websites: b.websites,
            email: b.email,
            category: b.category,
        }))
    }

    /// Fetches profile picture URL for a user or group.
    pub fn profile_picture_url(
        &self,
        jid: String,
        picture_type: String,
    ) -> Result<Option<String>, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.profile_picture_url(&jid, &picture_type).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Updates profile picture for self or a group using image bytes.
    pub fn update_profile_picture(
        &self,
        jid_opt: Option<String>,
        image_bytes: Vec<u8>,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .update_profile_picture(jid_opt.as_deref(), &image_bytes)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Removes profile picture for self or a group.
    pub fn remove_profile_picture(&self, jid_opt: Option<String>) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.remove_profile_picture(jid_opt.as_deref()).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Subscribes to presence updates for a contact or group.
    pub fn presence_subscribe(&self, to_jid: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.presence_subscribe(&to_jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Disconnects the WhatsApp session.
    pub fn disconnect(&self) {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            client.disconnect().await;
        });
    }

    /// Modifies chat mute status (None = unmuted, Some(duration_seconds) = muted until now + duration, or Some(u64::MAX) for indefinite mute).
    pub fn chat_modify_mute(
        &self,
        jid: String,
        duration_secs: Option<u64>,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.chat_modify_mute(&jid, duration_secs).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Pins or unpins a chat.
    pub fn chat_modify_pin(&self, jid: String, pin: bool) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.chat_modify_pin(&jid, pin).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Archives or unarchives a chat.
    pub fn chat_modify_archive(&self, jid: String, archive: bool) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.chat_modify_archive(&jid, archive).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Stars or unstars an individual message.
    pub fn chat_modify_star(
        &self,
        jid: String,
        msg_id: String,
        from_me: bool,
        star: bool,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.chat_modify_star(&jid, &msg_id, from_me, star).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Clears message history for a chat while optionally keeping starred messages.
    pub fn chat_modify_clear(&self, chat_jid: String, keep_starred: bool) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.chat_modify_clear(&chat_jid, keep_starred).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Deletes a chat conversation thread from WhatsApp and local SQLite store.
    pub fn chat_modify_delete(&self, chat_jid: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.chat_modify_delete(&chat_jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Marks a chat as read or unread across devices.
    pub fn chat_modify_mark_unread(&self, chat_jid: String, unread: bool) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.chat_modify_mark_unread(&chat_jid, unread).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Requests on-demand message history for a chat via w:sync:history IQ.
    pub fn fetch_message_history(
        &self,
        chat_jid: String,
        count: u32,
        oldest_msg_id: String,
        oldest_from_me: bool,
        oldest_timestamp: u64,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .fetch_message_history(
                        &chat_jid,
                        count,
                        &oldest_msg_id,
                        oldest_from_me,
                        oldest_timestamp,
                    )
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Queries metadata for a WhatsApp Channel by JID or invite link/code.
    pub fn newsletter_query_metadata(
        &self,
        jid_or_invite: String,
    ) -> Result<RolleysNewsletter, FfiError> {
        let client = self.client.clone();
        let meta = self
            .runtime
            .block_on(async move { client.newsletter_query_metadata(&jid_or_invite).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))?;

        Ok(RolleysNewsletter {
            jid: meta.jid,
            name: meta.name,
            description: meta.description,
            invite_code: meta.invite_code,
            picture_url: meta.picture_url,
            subscribers_count: meta.subscribers_count,
            role: meta.role,
            muted: meta.muted,
            verified: meta.verified,
        })
    }

    /// Fetches all WhatsApp Channels the user is subscribed to.
    pub fn newsletter_get_subscribed(&self) -> Result<Vec<RolleysNewsletter>, FfiError> {
        let client = self.client.clone();
        let list = self
            .runtime
            .block_on(async move { client.newsletter_get_subscribed().await })
            .map_err(|e| FfiError::OperationError(e.to_string()))?;

        Ok(list
            .into_iter()
            .map(|meta| RolleysNewsletter {
                jid: meta.jid,
                name: meta.name,
                description: meta.description,
                invite_code: meta.invite_code,
                picture_url: meta.picture_url,
                subscribers_count: meta.subscribers_count,
                role: meta.role,
                muted: meta.muted,
                verified: meta.verified,
            })
            .collect())
    }

    /// Follows a WhatsApp Channel.
    pub fn newsletter_follow(&self, jid: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.newsletter_follow(&jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Unfollows a WhatsApp Channel.
    pub fn newsletter_unfollow(&self, jid: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.newsletter_unfollow(&jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Toggles mute status for a WhatsApp Channel.
    pub fn newsletter_mute(&self, jid: String, mute: bool) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.newsletter_mute(&jid, mute).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Creates a new WhatsApp Channel.
    pub fn newsletter_create(
        &self,
        name: String,
        description: Option<String>,
    ) -> Result<RolleysNewsletter, FfiError> {
        let client = self.client.clone();
        let meta = self
            .runtime
            .block_on(async move {
                client
                    .newsletter_create(&name, description.as_deref())
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))?;

        Ok(RolleysNewsletter {
            jid: meta.jid,
            name: meta.name,
            description: meta.description,
            invite_code: meta.invite_code,
            picture_url: meta.picture_url,
            subscribers_count: meta.subscribers_count,
            role: meta.role,
            muted: meta.muted,
            verified: meta.verified,
        })
    }

    /// Fetches message history from a WhatsApp Channel.
    pub fn newsletter_fetch_messages(
        &self,
        jid: String,
        count: u32,
        before: Option<u64>,
    ) -> Result<Vec<RolleysNewsletterMessage>, FfiError> {
        let client = self.client.clone();
        let msgs = self
            .runtime
            .block_on(async move { client.newsletter_fetch_messages(&jid, count, before).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))?;

        Ok(msgs
            .into_iter()
            .map(|m| RolleysNewsletterMessage {
                server_id: m.server_id,
                views_count: m.views_count,
                timestamp: m.timestamp,
                summary: m.summary,
                has_media: m.has_media,
            })
            .collect())
    }

    /// Sends an emoji reaction to a channel message post.
    pub fn newsletter_react(
        &self,
        jid: String,
        server_id: u64,
        reaction: String,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.newsletter_react(&jid, server_id, &reaction).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Broadcasts a text message to an owned WhatsApp Channel.
    pub fn newsletter_send_post(&self, jid: String, text: String) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.newsletter_send_post(&jid, &text).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Promotes a channel subscriber to admin role.
    pub fn newsletter_promote(&self, jid: String, user_jid: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.newsletter_promote(&jid, &user_jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Demotes a channel admin back to subscriber.
    pub fn newsletter_demote(&self, jid: String, user_jid: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.newsletter_demote(&jid, &user_jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Transfers channel ownership to another admin.
    pub fn newsletter_change_owner(
        &self,
        jid: String,
        new_owner_jid: String,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.newsletter_change_owner(&jid, &new_owner_jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Returns current health telemetry snapshot (message throughput, ping latency, Signal hit rate).
    pub fn get_telemetry_snapshot(&self) -> RolleysTelemetrySnapshot {
        let snap = self.client.telemetry_snapshot();
        RolleysTelemetrySnapshot {
            messages_in: snap.messages_in,
            messages_out: snap.messages_out,
            last_ping_latency_ms: snap.last_ping_latency_ms,
            signal_cache_hit_rate_pct: snap.signal_cache_hit_rate_pct,
            reconnect_count: snap.reconnect_count,
        }
    }

    /// Returns the current formal connection state string.
    pub fn get_connection_state(&self) -> String {
        format!("{:?}", self.client.get_connection_state())
    }

    /// Sends an encrypted WhatsApp poll.
    pub fn send_poll(
        &self,
        target_jid: String,
        question: String,
        options: Vec<String>,
        selectable_count: u32,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_poll(&target_jid, &question, &options, selectable_count)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Casts an encrypted AES-256-GCM vote on an existing poll.
    pub fn send_poll_vote(
        &self,
        target_jid: String,
        poll_id: String,
        selected_options: Vec<String>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_poll_vote(&target_jid, &poll_id, &selected_options)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Fetches the aggregated vote tally for a poll from the local SQLite database.
    pub fn get_poll_tally(&self, poll_id: String) -> Result<Option<RolleysPollTally>, FfiError> {
        let client = self.client.clone();
        let tally_opt = self
            .runtime
            .block_on(async move { client.get_poll_tally(&poll_id).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))?;

        Ok(tally_opt.map(|t| RolleysPollTally {
            poll_id: t.poll_id,
            question: t.question,
            total_voters: t.total_voters as u32,
            option_votes: t.option_votes,
        }))
    }

    /// Sends native flow interactive action buttons (quick reply, cta_url, etc.).
    pub fn send_interactive_buttons(
        &self,
        target_jid: String,
        body: String,
        buttons: Vec<RolleysInteractiveButton>,
        header: Option<String>,
        footer: Option<String>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        let core_buttons: Vec<baileys_core::InteractiveButton> = buttons
            .into_iter()
            .map(|b| baileys_core::InteractiveButton {
                name: b.name,
                button_params_json: b.button_params_json,
            })
            .collect();

        self.runtime
            .block_on(async move {
                client
                    .send_interactive_buttons(
                        &target_jid,
                        &body,
                        &core_buttons,
                        header.as_deref(),
                        footer.as_deref(),
                    )
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Sends an interactive single-selection list message.
    pub fn send_list_message(
        &self,
        target_jid: String,
        title: String,
        description: String,
        button_text: String,
        sections: Vec<RolleysListSection>,
        footer: Option<String>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
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

        self.runtime
            .block_on(async move {
                client
                    .send_list_message(
                        &target_jid,
                        &title,
                        &description,
                        &button_text,
                        &core_sections,
                        footer.as_deref(),
                    )
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Creates a new WhatsApp Community.
    pub fn community_create(
        &self,
        subject: String,
        description: Option<String>,
    ) -> Result<RolleysCommunityMetadata, FfiError> {
        let client = self.client.clone();
        let meta = self
            .runtime
            .block_on(async move {
                client
                    .community_create(&subject, description.as_deref())
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))?;

        Ok(map_core_community_metadata(meta))
    }

    /// Creates a new subgroup linked under an existing parent Community.
    pub fn community_create_subgroup(
        &self,
        parent_community_jid: String,
        subject: String,
        participants: Vec<String>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .community_create_subgroup(&parent_community_jid, &subject, &participants)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Links an existing WhatsApp group into a parent Community.
    pub fn community_link_subgroup(
        &self,
        parent_community_jid: String,
        group_jid: String,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .community_link_subgroup(&parent_community_jid, &group_jid)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Unlinks a subgroup from a parent Community.
    pub fn community_unlink_subgroup(
        &self,
        parent_community_jid: String,
        group_jid: String,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .community_unlink_subgroup(&parent_community_jid, &group_jid)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Fetches all linked subgroups under a Community.
    pub fn community_fetch_subgroups(
        &self,
        community_jid: String,
    ) -> Result<Vec<RolleysCommunitySubgroup>, FfiError> {
        let client = self.client.clone();
        let list = self
            .runtime
            .block_on(async move { client.community_fetch_subgroups(&community_jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))?;

        Ok(list
            .into_iter()
            .map(|s| RolleysCommunitySubgroup {
                id: s.id,
                subject: s.subject,
                creation: s.creation,
                creator: s.creator,
                size: s.size,
            })
            .collect())
    }

    /// Queries interactive metadata for a Community.
    pub fn community_get_metadata(
        &self,
        community_jid: String,
    ) -> Result<RolleysCommunityMetadata, FfiError> {
        let client = self.client.clone();
        let meta = self
            .runtime
            .block_on(async move { client.community_get_metadata(&community_jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))?;

        Ok(map_core_community_metadata(meta))
    }

    /// Leaves an existing WhatsApp Community.
    pub fn community_leave(&self, community_jid: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.community_leave(&community_jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Gets an invite code for a Community.
    pub fn community_get_invite_code(&self, community_jid: String) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.community_get_invite_code(&community_jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Accepts an invite code to join a WhatsApp Community.
    pub fn community_accept_invite(&self, code: String) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.community_accept_invite(&code).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Rejects an incoming WhatsApp voice or video call.
    pub fn reject_call(&self, call_id: String, call_from: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.reject_call(&call_id, &call_from).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Fetches VoIP call history from SQLite.
    pub fn get_call_history(&self, limit: u32) -> Result<Vec<RolleysStoredCall>, FfiError> {
        let client = self.client.clone();
        let list = self
            .runtime
            .block_on(async move { client.get_call_history(limit).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))?;

        Ok(list
            .into_iter()
            .map(|c| RolleysStoredCall {
                call_id: c.call_id,
                chat_jid: c.chat_jid,
                caller_jid: c.caller_jid,
                is_group: c.is_group,
                group_jid: c.group_jid,
                is_video: c.is_video,
                status: c.status,
                timestamp: c.timestamp,
                duration: c.duration,
            })
            .collect())
    }

    /// Publishes a text status story to `status@broadcast`.
    pub fn send_text_status(
        &self,
        text: String,
        background_color: Option<String>,
        font: Option<i32>,
        audience: Option<Vec<String>>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_text_status(&text, background_color.as_deref(), font, audience)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Sends a status read receipt to mark a story as viewed.
    pub fn send_status_read_receipt(
        &self,
        msg_id: String,
        author_jid: String,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.send_status_read_receipt(&msg_id, &author_jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    /// Fetches recent status stories from SQLite cache.
    pub fn get_recent_stories(
        &self,
        limit: u32,
        unviewed_only: bool,
    ) -> Result<Vec<RolleysStoredStory>, FfiError> {
        let client = self.client.clone();
        let list = self
            .runtime
            .block_on(async move { client.get_recent_stories(limit, unviewed_only).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))?;

        Ok(list.into_iter().map(RolleysStoredStory::from).collect())
    }

    /// Fetches all active status stories posted by a specific contact.
    pub fn get_stories_by_author(
        &self,
        author_jid: String,
    ) -> Result<Vec<RolleysStoredStory>, FfiError> {
        let client = self.client.clone();
        let list = self
            .runtime
            .block_on(async move { client.get_stories_by_author(&author_jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))?;

        Ok(list.into_iter().map(RolleysStoredStory::from).collect())
    }

    pub fn edit_message(
        &self,
        chat_jid: String,
        target_message_id: String,
        new_text: String,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .edit_message(&chat_jid, &target_message_id, &new_text)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn revoke_message(
        &self,
        chat_jid: String,
        target_message_id: String,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.revoke_message(&chat_jid, &target_message_id).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn set_chat_ephemeral(
        &self,
        chat_jid: String,
        expiration_seconds: u32,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .set_chat_ephemeral(&chat_jid, expiration_seconds)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn get_chat_ephemeral(&self, chat_jid: String) -> Result<Option<u32>, FfiError> {
        let client = self.client.clone();
        let res = self
            .runtime
            .block_on(async move { client.get_chat_ephemeral(&chat_jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))?;
        Ok(res.map(|(exp, _)| exp))
    }

    pub fn get_labels(&self) -> Result<Vec<RolleysLabel>, FfiError> {
        let client = self.client.clone();
        let list = self
            .runtime
            .block_on(async move { client.get_labels().await })
            .map_err(|e| FfiError::OperationError(e.to_string()))?;
        Ok(list.into_iter().map(Into::into).collect())
    }

    pub fn get_label(&self, id: String) -> Result<Option<RolleysLabel>, FfiError> {
        let client = self.client.clone();
        let l = self
            .runtime
            .block_on(async move { client.get_label(&id).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))?;
        Ok(l.map(Into::into))
    }

    pub fn upsert_label(
        &self,
        id: Option<String>,
        name: String,
        color: i32,
        predefined_id: Option<i32>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .upsert_label(id.as_deref(), &name, color, predefined_id)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn delete_label(&self, id: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.delete_label(&id).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn add_chat_label(&self, chat_jid: String, label_id: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.add_chat_label(&chat_jid, &label_id).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn remove_chat_label(&self, chat_jid: String, label_id: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.remove_chat_label(&chat_jid, &label_id).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn get_chat_labels(&self, chat_jid: String) -> Result<Vec<String>, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.get_chat_labels(&chat_jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn get_chats_by_label(&self, label_id: String) -> Result<Vec<String>, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.get_chats_by_label(&label_id).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn add_message_label(
        &self,
        chat_jid: String,
        message_id: String,
        label_id: String,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .add_message_label(&chat_jid, &message_id, &label_id)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn remove_message_label(
        &self,
        chat_jid: String,
        message_id: String,
        label_id: String,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .remove_message_label(&chat_jid, &message_id, &label_id)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn get_message_labels(
        &self,
        chat_jid: String,
        message_id: String,
    ) -> Result<Vec<String>, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.get_message_labels(&chat_jid, &message_id).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn get_messages_by_label(&self, label_id: String) -> Result<Vec<String>, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.get_messages_by_label(&label_id).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    // ==========================================
    // Track 10: Rich Messages & Attachments
    // ==========================================

    pub fn send_location(
        &self,
        to_jid: String,
        latitude: f64,
        longitude: f64,
        name: Option<String>,
        address: Option<String>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_location(
                        &to_jid,
                        latitude,
                        longitude,
                        name.as_deref(),
                        address.as_deref(),
                        None,
                    )
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn send_live_location(
        &self,
        to_jid: String,
        latitude: f64,
        longitude: f64,
        accuracy_in_meters: Option<u32>,
        caption: Option<String>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_live_location(
                        &to_jid,
                        latitude,
                        longitude,
                        accuracy_in_meters,
                        caption.as_deref(),
                    )
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn send_contact(
        &self,
        to_jid: String,
        display_name: String,
        phone_number: String,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_contact(&to_jid, &display_name, &phone_number)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn send_group_invite(
        &self,
        to_jid: String,
        group_jid: String,
        invite_code: String,
        invite_expiration: i64,
        group_name: String,
        caption: Option<String>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_group_invite(
                        &to_jid,
                        &group_jid,
                        &invite_code,
                        invite_expiration,
                        &group_name,
                        caption.as_deref(),
                    )
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn get_catalog(
        &self,
        biz_jid: String,
        limit: u32,
        cursor: Option<String>,
    ) -> Result<RolleysCatalogResult, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let res = client
                .get_catalog(&biz_jid, limit, cursor.as_deref())
                .await
                .map_err(|e| FfiError::OperationError(e.to_string()))?;
            Ok(RolleysCatalogResult {
                products: res.products.into_iter().map(Into::into).collect(),
                cursor: res.cursor,
            })
        })
    }

    pub fn get_product(
        &self,
        biz_jid: String,
        product_id: String,
    ) -> Result<RolleysCatalogProduct, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let p = client
                .get_product(&biz_jid, &product_id)
                .await
                .map_err(|e| FfiError::OperationError(e.to_string()))?;
            Ok(p.into())
        })
    }

    pub fn get_collections(
        &self,
        biz_jid: String,
        limit: u32,
        cursor: Option<String>,
    ) -> Result<RolleysCollectionsResult, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let res = client
                .get_collections(&biz_jid, limit, cursor.as_deref())
                .await
                .map_err(|e| FfiError::OperationError(e.to_string()))?;
            Ok(RolleysCollectionsResult {
                collections: res.collections.into_iter().map(Into::into).collect(),
                cursor: res.cursor,
            })
        })
    }

    pub fn send_product_message(
        &self,
        to_jid: String,
        biz_owner_jid: String,
        product_id: String,
        title: String,
        price_amount_1000: i64,
        currency: String,
        caption: Option<String>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
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
            client
                .send_product_message(&to_jid, &biz_owner_jid, &product, caption.as_deref(), None)
                .await
                .map_err(|e| FfiError::OperationError(e.to_string()))
        })
    }

    pub fn send_order_message(
        &self,
        to_jid: String,
        seller_jid: String,
        order_id: String,
        title: String,
        item_count: i32,
        total_amount_1000: i64,
        currency: String,
        message: Option<String>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            client
                .send_order_message(
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
                .map_err(|e| FfiError::OperationError(e.to_string()))
        })
    }

    pub fn get_cached_orders(
        &self,
        chat_jid: String,
        limit: u32,
    ) -> Result<Vec<RolleysOrder>, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let orders = client
                .get_cached_orders(&chat_jid, limit)
                .await
                .map_err(|e| FfiError::OperationError(e.to_string()))?;
            Ok(orders.into_iter().map(Into::into).collect())
        })
    }

    pub fn get_cached_catalog(
        &self,
        biz_jid: String,
    ) -> Result<Vec<RolleysCatalogProduct>, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let prods = client
                .get_cached_catalog(&biz_jid)
                .await
                .map_err(|e| FfiError::OperationError(e.to_string()))?;
            Ok(prods.into_iter().map(Into::into).collect())
        })
    }

    pub fn get_cached_collections(
        &self,
        biz_jid: String,
    ) -> Result<Vec<RolleysCatalogCollection>, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let cols = client
                .get_cached_collections(&biz_jid)
                .await
                .map_err(|e| FfiError::OperationError(e.to_string()))?;
            Ok(cols.into_iter().map(Into::into).collect())
        })
    }

    pub fn get_user_devices(&self, user_jid: String) -> Result<Vec<u32>, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            client
                .get_user_devices(&user_jid)
                .await
                .map_err(|e| FfiError::OperationError(e.to_string()))
        })
    }

    pub fn get_paired_devices(&self) -> Result<Vec<RolleysDeviceRecord>, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let devs = client
                .get_paired_devices()
                .await
                .map_err(|e| FfiError::OperationError(e.to_string()))?;
            Ok(devs
                .into_iter()
                .map(|d| RolleysDeviceRecord {
                    id: d.id,
                    key_index: d.key_index,
                })
                .collect())
        })
    }

    pub fn remove_companion_device(&self, device_id: u32) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            client
                .remove_companion_device(device_id)
                .await
                .map_err(|e| FfiError::OperationError(e.to_string()))
        })
    }

    pub fn check_on_whatsapp(
        &self,
        phone_numbers: Vec<String>,
    ) -> Result<Vec<RolleysOnWhatsAppResult>, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let num_refs: Vec<&str> = phone_numbers.iter().map(|s| s.as_str()).collect();
            let results = client
                .check_on_whatsapp(&num_refs)
                .await
                .map_err(|e| FfiError::OperationError(e.to_string()))?;
            Ok(results
                .into_iter()
                .map(|r| RolleysOnWhatsAppResult {
                    jid: r.jid,
                    exists: r.exists,
                    phone_number: r.phone_number,
                })
                .collect())
        })
    }

    pub fn update_media_message(&self, message_id: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.update_media_message(&message_id).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn fetch_url_preview(&self, text: String) -> Result<Option<RolleysUrlPreview>, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let res = client
                .fetch_url_preview(&text)
                .await
                .map_err(|e| FfiError::OperationError(e.to_string()))?;
            Ok(res.map(|p| RolleysUrlPreview {
                matched_text: p.matched_text,
                canonical_url: p.canonical_url,
                title: p.title,
                description: p.description,
                jpeg_thumbnail: p.jpeg_thumbnail,
            }))
        })
    }

    pub fn send_text_with_preview(&self, to_jid: String, text: String) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.send_text_with_preview(&to_jid, &text).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn get_lid_for_pn(&self, pn_jid: String) -> Result<Option<String>, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let res = client
                .get_lid_for_pn(&[&pn_jid])
                .await
                .map_err(|e| FfiError::OperationError(e.to_string()))?;
            Ok(res.into_iter().next().map(|(_, lid)| lid))
        })
    }

    pub fn get_pn_for_lid(&self, lid_jid: String) -> Result<Option<String>, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let res = client
                .get_pn_for_lid(&[&lid_jid])
                .await
                .map_err(|e| FfiError::OperationError(e.to_string()))?;
            Ok(res.into_iter().next().map(|(_, pn)| pn))
        })
    }

    pub fn get_user_status(&self, jid: String) -> Result<Option<RolleysUserStatus>, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let res = client
                .get_user_status(&[&jid])
                .await
                .map_err(|e| FfiError::OperationError(e.to_string()))?;
            Ok(res.into_iter().next().map(|(j, s, t)| RolleysUserStatus {
                jid: j,
                status: s,
                timestamp: t,
            }))
        })
    }

    pub fn create_call_link(&self, is_video: bool) -> Result<Option<String>, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.create_call_link(is_video).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn clean_dirty_bits(
        &self,
        clean_type: String,
        timestamp: Option<u64>,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.clean_dirty_bits(&clean_type, timestamp).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn fetch_account_reachout_timelock(
        &self,
    ) -> Result<Option<RolleysAccountReachoutTimelock>, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let res = client
                .fetch_account_reachout_timelock()
                .await
                .map_err(|e| FfiError::OperationError(e.to_string()))?;
            Ok(
                res.map(|(is_active, enforcement_type, time_enforcement_ends)| {
                    RolleysAccountReachoutTimelock {
                        is_active,
                        enforcement_type,
                        time_enforcement_ends,
                    }
                }),
            )
        })
    }

    pub fn fetch_new_chat_message_cap(&self) -> Result<Option<String>, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let val = client
                .fetch_new_chat_message_cap()
                .await
                .map_err(|e| FfiError::OperationError(e.to_string()))?;
            Ok(val.map(|v| v.to_string()))
        })
    }

    pub fn get_bot_list_v2(&self) -> Result<Vec<RolleysBotEntry>, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let list = client
                .get_bot_list_v2()
                .await
                .map_err(|e| FfiError::OperationError(e.to_string()))?;
            Ok(list
                .into_iter()
                .map(|(j, p)| RolleysBotEntry {
                    jid: j,
                    persona_id: p,
                })
                .collect())
        })
    }

    pub fn newsletter_admin_count(&self, channel_jid: String) -> Result<u32, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.newsletter_admin_count(&channel_jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn subscribe_newsletter_updates(
        &self,
        channel_jid: String,
    ) -> Result<Option<String>, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.subscribe_newsletter_updates(&channel_jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn community_revoke_invite(&self, community_jid: String) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.community_revoke_invite(&community_jid).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn community_get_invite_info(
        &self,
        code: String,
    ) -> Result<RolleysCommunityInviteInfo, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let info = client
                .community_get_invite_info(&code)
                .await
                .map_err(|e| FfiError::OperationError(e.to_string()))?;
            Ok(RolleysCommunityInviteInfo {
                jid: info.jid,
                subject: info.subject,
                creator: info.creator,
                size: info.size.map(|s| s as u32),
            })
        })
    }

    pub fn community_toggle_ephemeral(
        &self,
        community_jid: String,
        duration: u32,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .community_toggle_ephemeral(&community_jid, duration)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn community_setting_update(
        &self,
        community_jid: String,
        setting: String,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .community_setting_update(&community_jid, &setting)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn community_member_add_mode(
        &self,
        community_jid: String,
        mode: String,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .community_member_add_mode(&community_jid, &mode)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn community_join_approval_mode(
        &self,
        community_jid: String,
        mode: String,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .community_join_approval_mode(&community_jid, &mode)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn community_fetch_all_participating(&self) -> Result<Vec<String>, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.community_fetch_all_participating().await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn product_delete(&self, product_ids: Vec<String>) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                let ids: Vec<&str> = product_ids.iter().map(|s| s.as_str()).collect();
                client.product_delete(&ids).await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn update_cover_photo(
        &self,
        photo_id: String,
        token: String,
        ts: u64,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.update_cover_photo(&photo_id, &token, ts).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn remove_cover_photo(&self, photo_id: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.remove_cover_photo(&photo_id).await })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }

    pub fn update_disable_link_previews_privacy(&self, is_disabled: bool) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                let val = if is_disabled { "true" } else { "false" };
                client
                    .update_privacy_setting("disable_link_previews", val)
                    .await
            })
            .map_err(|e| FfiError::OperationError(e.to_string()))
    }
}

fn map_core_community_metadata(meta: baileys_core::CommunityMetadata) -> RolleysCommunityMetadata {
    RolleysCommunityMetadata {
        id: meta.id,
        subject: meta.subject,
        subject_owner: meta.subject_owner,
        subject_time: meta.subject_time,
        size: meta.size,
        creation: meta.creation,
        owner: meta.owner,
        description: meta.description,
        description_id: meta.description_id,
        linked_parent: meta.linked_parent,
        is_community: meta.is_community,
        is_community_announce: meta.is_community_announce,
        join_approval_mode: meta.join_approval_mode,
        member_add_mode: meta.member_add_mode,
        participants: meta
            .participants
            .into_iter()
            .map(|p| RolleysCommunityParticipant {
                jid: p.jid,
                admin: p.admin,
            })
            .collect(),
    }
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysTosNotice {
    pub id: String,
    pub stage: Option<String>,
    pub state: Option<String>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysResolvedUsername {
    pub username: String,
    pub jid: String,
    pub phone_jid: Option<String>,
    pub verified_name: Option<String>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysAppStateSyncSummary {
    pub collections_synced: u32,
    pub mutations_applied: u32,
    pub collections_failed: u32,
}

/// One entry of a multi-contact card message.
#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysContactCard {
    pub display_name: String,
    pub phone_number: String,
}

impl From<RolleysCatalogProduct> for baileys_core::CatalogProduct {
    fn from(p: RolleysCatalogProduct) -> Self {
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

fn op_err(e: impl std::fmt::Display) -> FfiError {
    FfiError::OperationError(e.to_string())
}

fn parse_message_json(json: &str) -> Result<baileys_core::baileys_proto::proto::Message, FfiError> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(op_err)?;
    baileys_core::messages::parse_message_from_json(&value).map_err(op_err)
}

/// Methods added to reach parity with the core client (and Baileys' socket API).
#[uniffi::export]
impl RolleysClient {
    /// Sends any message described as JSON (same shorthands as the core `send_message_json`).
    pub fn send_message_json(
        &self,
        to_jid: String,
        message_json: String,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        let value: serde_json::Value = serde_json::from_str(&message_json).map_err(op_err)?;
        self.runtime
            .block_on(async move { client.send_message_json(&to_jid, &value).await })
            .map_err(op_err)
    }

    /// Wraps a JSON-described message in view-once (`v2` uses viewOnceMessageV2).
    pub fn send_view_once(
        &self,
        to_jid: String,
        message_json: String,
        v2: bool,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        let inner = parse_message_json(&message_json)?;
        self.runtime
            .block_on(async move { client.send_view_once(&to_jid, inner, v2).await })
            .map_err(op_err)
    }

    /// Forwards a message this client has seen (recent cache or store) by its id.
    pub fn forward_message(&self, to_jid: String, message_id: String) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let message = match client
                .get_recent_message(&message_id)
                .await
                .and_then(|m| m.message)
            {
                Some(m) => m,
                None => {
                    use prost::Message as _;
                    let stored = client
                        .store
                        .get_message(&message_id)
                        .await
                        .map_err(op_err)?
                        .ok_or_else(|| op_err(format!("message {message_id} not found")))?;
                    baileys_core::baileys_proto::proto::Message::decode(
                        stored.proto_bytes.as_slice(),
                    )
                    .map_err(op_err)?
                }
            };
            client
                .forward_message(&to_jid, message, true, 1)
                .await
                .map_err(op_err)
        })
    }

    /// Pins (`pin = true`) or unpins a message in a chat.
    pub fn pin_message(
        &self,
        chat_jid: String,
        message_id: String,
        pin: bool,
        duration_secs: u32,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .pin_message(&chat_jid, &message_id, pin, duration_secs)
                    .await
            })
            .map_err(op_err)
    }

    /// Comments on a message (e.g. a channel or community post).
    pub fn send_comment(
        &self,
        to_jid: String,
        target_message_id: String,
        target_from_me: bool,
        target_participant: Option<String>,
        text: String,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        let key = baileys_core::baileys_proto::proto::MessageKey {
            remote_jid: Some(to_jid.clone()),
            from_me: Some(target_from_me),
            id: Some(target_message_id),
            participant: target_participant,
        };
        self.runtime
            .block_on(async move { client.send_comment(&to_jid, &key, &text).await })
            .map_err(op_err)
    }

    pub fn send_album(
        &self,
        to_jid: String,
        expected_image_count: u32,
        expected_video_count: u32,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_album(&to_jid, expected_image_count, expected_video_count)
                    .await
            })
            .map_err(op_err)
    }

    pub fn send_event(
        &self,
        to_jid: String,
        name: String,
        description: Option<String>,
        start_time: i64,
        end_time: Option<i64>,
        location_name: Option<String>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_event(
                        &to_jid,
                        &name,
                        description.as_deref(),
                        start_time,
                        end_time,
                        location_name.as_deref(),
                    )
                    .await
            })
            .map_err(op_err)
    }

    pub fn send_vcard(
        &self,
        to_jid: String,
        display_name: String,
        vcard: String,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.send_vcard(&to_jid, &display_name, &vcard).await })
            .map_err(op_err)
    }

    pub fn send_contacts_array(
        &self,
        to_jid: String,
        display_name: String,
        contacts: Vec<RolleysContactCard>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        let contacts = contacts
            .into_iter()
            .map(|c| (c.display_name, c.phone_number))
            .collect();
        self.runtime
            .block_on(async move {
                client
                    .send_contacts_array(&to_jid, &display_name, contacts)
                    .await
            })
            .map_err(op_err)
    }

    pub fn send_fake_quoted_message(
        &self,
        to_jid: String,
        reply_text: String,
        fake_sender_jid: String,
        fake_quoted_text: String,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_fake_quoted_message(
                        &to_jid,
                        &reply_text,
                        &fake_sender_jid,
                        &fake_quoted_text,
                    )
                    .await
            })
            .map_err(op_err)
    }

    /// Sends a prompt to Meta AI, optionally inside an existing chat.
    pub fn send_bot_prompt(
        &self,
        prompt: String,
        target_chat: Option<String>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_bot_prompt(&prompt, target_chat.as_deref())
                    .await
            })
            .map_err(op_err)
    }

    pub fn send_status_image(
        &self,
        image_bytes: Vec<u8>,
        caption: Option<String>,
        mimetype: Option<String>,
        audience: Option<Vec<String>>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_status_image(
                        &image_bytes,
                        caption.as_deref(),
                        mimetype.as_deref(),
                        audience,
                    )
                    .await
            })
            .map_err(op_err)
    }

    pub fn send_status_video(
        &self,
        video_bytes: Vec<u8>,
        caption: Option<String>,
        mimetype: Option<String>,
        audience: Option<Vec<String>>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_status_video(
                        &video_bytes,
                        caption.as_deref(),
                        mimetype.as_deref(),
                        audience,
                    )
                    .await
            })
            .map_err(op_err)
    }

    /// Audio status as a voice note (`ptt`) or as a music/audio file, with its real mimetype.
    pub fn send_status_audio_as(
        &self,
        audio_bytes: Vec<u8>,
        seconds: u32,
        mimetype: Option<String>,
        ptt: bool,
        audience: Option<Vec<String>>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_status_audio_as(&audio_bytes, seconds, mimetype.as_deref(), ptt, audience)
                    .await
            })
            .map_err(op_err)
    }

    /// Video status with its length in seconds and an optional JPEG preview.
    pub fn send_status_video_clip(
        &self,
        video_bytes: Vec<u8>,
        caption: Option<String>,
        mimetype: Option<String>,
        seconds: u32,
        thumbnail: Option<Vec<u8>>,
        audience: Option<Vec<String>>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_status_video_clip(
                        &video_bytes,
                        caption.as_deref(),
                        mimetype.as_deref(),
                        seconds,
                        thumbnail,
                        audience,
                    )
                    .await
            })
            .map_err(op_err)
    }

    /// The JPEG preview embedded in a status story, if it carried one.
    pub fn get_story_thumbnail(&self, msg_id: String) -> Result<Option<Vec<u8>>, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.get_story_thumbnail(&msg_id).await })
            .map_err(op_err)
    }

    pub fn send_status_audio(
        &self,
        audio_bytes: Vec<u8>,
        seconds: u32,
        mimetype: Option<String>,
        audience: Option<Vec<String>>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_status_audio(&audio_bytes, seconds, mimetype.as_deref(), audience)
                    .await
            })
            .map_err(op_err)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn send_link_status(
        &self,
        url: String,
        title: Option<String>,
        description: Option<String>,
        custom_text: Option<String>,
        background_color: Option<String>,
        font: Option<i32>,
        thumbnail_bytes: Option<Vec<u8>>,
        audience: Option<Vec<String>>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .send_link_status(
                        &url,
                        title.as_deref(),
                        description.as_deref(),
                        custom_text.as_deref(),
                        background_color.as_deref(),
                        font,
                        thumbnail_bytes,
                        audience,
                    )
                    .await
            })
            .map_err(op_err)
    }

    pub fn newsletter_delete(&self, jid: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.newsletter_delete(&jid).await })
            .map_err(op_err)
    }

    pub fn newsletter_update_name(&self, jid: String, name: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.newsletter_update_name(&jid, &name).await })
            .map_err(op_err)
    }

    pub fn newsletter_view_receipt(
        &self,
        jid: String,
        server_ids: Vec<u64>,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.newsletter_view_receipt(&jid, &server_ids).await })
            .map_err(op_err)
    }

    pub fn newsletter_send_image(
        &self,
        jid: String,
        image_bytes: Vec<u8>,
        caption: Option<String>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .newsletter_send_image(&jid, &image_bytes, caption.as_deref())
                    .await
            })
            .map_err(op_err)
    }

    pub fn newsletter_send_status_story(
        &self,
        newsletter_jid: String,
        server_msg_id: i32,
        newsletter_name: String,
        caption_or_text: String,
        status_recipients: Option<Vec<String>>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .newsletter_send_status_story(
                        &newsletter_jid,
                        server_msg_id,
                        &newsletter_name,
                        &caption_or_text,
                        status_recipients,
                    )
                    .await
            })
            .map_err(op_err)
    }

    pub fn newsletter_send_image_status_story(
        &self,
        image_bytes: Vec<u8>,
        caption: Option<String>,
        newsletter_jid: String,
        server_msg_id: i32,
        newsletter_name: String,
        status_recipients: Option<Vec<String>>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .newsletter_send_image_status_story(
                        &image_bytes,
                        caption.as_deref(),
                        &newsletter_jid,
                        server_msg_id,
                        &newsletter_name,
                        status_recipients,
                    )
                    .await
            })
            .map_err(op_err)
    }

    /// Metadata of the group behind an invite code, without joining it.
    pub fn group_get_invite_info(&self, code: String) -> Result<RolleysGroupMetadata, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let node = client.group_get_invite_info(&code).await.map_err(op_err)?;
            baileys_core::groups::parse_group_metadata(&node)
                .map(Into::into)
                .map_err(op_err)
        })
    }

    pub fn product_create(&self, product: RolleysCatalogProduct) -> Result<String, FfiError> {
        let client = self.client.clone();
        let product: baileys_core::CatalogProduct = product.into();
        self.runtime
            .block_on(async move { client.product_create(&product).await })
            .map_err(op_err)
    }

    pub fn product_update(
        &self,
        product_id: String,
        product: RolleysCatalogProduct,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        let product: baileys_core::CatalogProduct = product.into();
        self.runtime
            .block_on(async move { client.product_update(&product_id, &product).await })
            .map_err(op_err)
    }

    pub fn update_profile_name(&self, name: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.update_profile_name(&name).await })
            .map_err(op_err)
    }

    /// Asks the server for an 8-character pairing code for `phone` (link with phone number).
    pub fn request_pairing_code(&self, phone: String) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.request_pairing_code(&phone).await })
            .map_err(op_err)
    }

    /// The most recent QR string, if one was issued.
    pub fn get_last_qr(&self) -> Option<String> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.get_last_qr().await })
    }

    /// Logs this device out and clears the session.
    pub fn logout(&self) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.logout().await })
            .map_err(op_err)
    }

    /// The account's own display name, if known.
    pub fn me_name(&self) -> Result<Option<String>, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.me_name().await })
            .map_err(op_err)
    }

    pub fn me_lid(&self) -> Result<Option<String>, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.me_lid().await })
            .map_err(op_err)
    }

    pub fn is_ghost_mode(&self) -> bool {
        self.client.is_ghost_mode()
    }

    /// When enabled, read receipts and presence are not sent.
    pub fn set_ghost_mode(&self, enabled: bool) {
        self.client.set_ghost_mode(enabled)
    }

    /// Resolves a WhatsApp username (with its PIN when it has one).
    pub fn lookup_username(
        &self,
        username: String,
        pin: Option<String>,
    ) -> Result<Option<RolleysResolvedUsername>, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let found = client
                .lookup_username(&username, pin.as_deref())
                .await
                .map_err(op_err)?;
            Ok(found.map(|u| RolleysResolvedUsername {
                username: u.username,
                jid: u.jid,
                phone_jid: u.phone_jid,
                verified_name: u.verified_name,
            }))
        })
    }

    /// Re-synchronises app-state collections (`force_snapshot` rebuilds them from scratch).
    pub fn resync_app_state(
        &self,
        collections: Vec<String>,
        force_snapshot: bool,
    ) -> Result<RolleysAppStateSyncSummary, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let refs: Vec<&str> = collections.iter().map(String::as_str).collect();
            let s = client
                .resync_app_state(&refs, force_snapshot)
                .await
                .map_err(op_err)?;
            Ok(RolleysAppStateSyncSummary {
                collections_synced: s.collections_synced as u32,
                mutations_applied: s.mutations_applied as u32,
                collections_failed: s.collections_failed as u32,
            })
        })
    }

    pub fn query_tos(&self, notice_ids: Vec<String>) -> Result<Vec<RolleysTosNotice>, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let refs: Vec<&str> = notice_ids.iter().map(String::as_str).collect();
            let notices = client.query_tos(&refs).await.map_err(op_err)?;
            Ok(notices
                .into_iter()
                .map(|n| RolleysTosNotice {
                    id: n.id,
                    stage: n.stage,
                    state: n.state,
                })
                .collect())
        })
    }

    pub fn accept_tos(
        &self,
        notice_ids: Vec<String>,
        stage: Option<String>,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let refs: Vec<&str> = notice_ids.iter().map(String::as_str).collect();
            client
                .accept_tos(&refs, stage.as_deref())
                .await
                .map_err(op_err)
        })
    }

    pub fn get_linked_email(&self) -> Result<Option<String>, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.get_linked_email().await })
            .map_err(op_err)
    }

    pub fn set_linked_email(&self, email: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.set_linked_email(&email).await })
            .map_err(op_err)
    }

    pub fn request_email_code(&self, lang: String, locale: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.request_email_code(&lang, &locale).await })
            .map_err(op_err)
    }

    pub fn verify_email_code(&self, code: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.verify_email_code(&code).await })
            .map_err(op_err)
    }
}

/// A contact saved from the phone's address book (Baileys `ContactAction`).
#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysContactEdit {
    pub full_name: Option<String>,
    pub first_name: Option<String>,
    pub lid_jid: Option<String>,
    pub pn_jid: Option<String>,
    pub username: Option<String>,
    pub save_on_primary_addressbook: Option<bool>,
}

/// A message to mark as read (Baileys `WAMessageKey`).
#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysReadKey {
    pub remote_jid: String,
    pub id: String,
    pub participant: Option<String>,
    pub from_me: bool,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysDisappearingDuration {
    pub jid: String,
    pub duration_secs: u32,
    pub set_at: u64,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysOrderProduct {
    pub id: String,
    pub name: String,
    pub image_url: String,
    pub price: i64,
    pub currency: String,
    pub quantity: i64,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysOrderDetails {
    pub total: i64,
    pub currency: String,
    pub products: Vec<RolleysOrderProduct>,
}

/// One day of business hours; `mode` is specific_hours, open_24h or appointment_only.
#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysBusinessHoursDay {
    pub day: String,
    pub mode: String,
    pub open_time_in_minutes: Option<String>,
    pub close_time_in_minutes: Option<String>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysBusinessHours {
    pub timezone: String,
    pub days: Vec<RolleysBusinessHoursDay>,
}

/// Business profile fields to change; unset fields stay as they are.
#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysBusinessProfileUpdate {
    pub address: Option<String>,
    pub email: Option<String>,
    pub description: Option<String>,
    pub websites: Option<Vec<String>>,
    pub hours: Option<RolleysBusinessHours>,
}

/// Channel fields to change; `picture` is base64 JPEG, an empty string removes it.
#[derive(uniffi::Record, Clone, Debug)]
pub struct RolleysNewsletterUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub picture: Option<String>,
}

#[uniffi::export]
impl RolleysClient {
    /// Adds or edits a contact (Baileys `addOrEditContact`).
    pub fn add_or_edit_contact(
        &self,
        jid: String,
        contact: RolleysContactEdit,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        let contact = baileys_core::syncd::ContactEdit {
            full_name: contact.full_name,
            first_name: contact.first_name,
            lid_jid: contact.lid_jid,
            pn_jid: contact.pn_jid,
            username: contact.username,
            save_on_primary_addressbook: contact.save_on_primary_addressbook,
        };
        self.runtime
            .block_on(async move { client.add_or_edit_contact(&jid, &contact).await })
            .map_err(op_err)
    }

    /// Removes a contact; it must exist in the synced app state.
    pub fn remove_contact(&self, jid: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.remove_contact(&jid).await })
            .map_err(op_err)
    }

    /// Adds or edits a quick reply (WhatsApp Business); returns its id.
    pub fn add_or_edit_quick_reply(
        &self,
        timestamp: Option<String>,
        shortcut: String,
        message: String,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .add_or_edit_quick_reply(timestamp.as_deref(), &shortcut, &message)
                    .await
            })
            .map_err(op_err)
    }

    pub fn remove_quick_reply(&self, timestamp: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.remove_quick_reply(&timestamp).await })
            .map_err(op_err)
    }

    /// Changes name, description and/or picture of a channel in one request.
    pub fn newsletter_update(
        &self,
        jid: String,
        update: RolleysNewsletterUpdate,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        let update = baileys_core::newsletter::NewsletterUpdate {
            name: update.name,
            description: update.description,
            picture: update.picture,
        };
        self.runtime
            .block_on(async move { client.newsletter_update(&jid, &update).await })
            .map_err(op_err)
    }

    pub fn newsletter_update_description(
        &self,
        jid: String,
        description: String,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move {
                client
                    .newsletter_update_description(&jid, &description)
                    .await
            })
            .map_err(op_err)
    }

    /// Sets the channel picture from any decodable image (cropped square, 640 px JPEG).
    pub fn newsletter_update_picture(
        &self,
        jid: String,
        image_bytes: Vec<u8>,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.newsletter_update_picture(&jid, &image_bytes).await })
            .map_err(op_err)
    }

    pub fn newsletter_remove_picture(&self, jid: String) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.newsletter_remove_picture(&jid).await })
            .map_err(op_err)
    }

    pub fn newsletter_subscribers(&self, jid: String) -> Result<u64, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.newsletter_subscribers(&jid).await })
            .map_err(op_err)
    }

    /// Marks many messages as read at once; uses read-self when read receipts are off.
    pub fn read_messages(&self, keys: Vec<RolleysReadKey>) -> Result<(), FfiError> {
        let client = self.client.clone();
        let keys: Vec<baileys_core::receipts::ReadKey> = keys
            .into_iter()
            .map(|k| baileys_core::receipts::ReadKey {
                remote_jid: k.remote_jid,
                id: k.id,
                participant: k.participant,
                from_me: k.from_me,
            })
            .collect();
        self.runtime
            .block_on(async move { client.read_messages(&keys).await })
            .map_err(op_err)
    }

    /// Sets our label in a group (30 characters at most).
    pub fn update_member_label(
        &self,
        group_jid: String,
        label: String,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.update_member_label(&group_jid, &label).await })
            .map_err(op_err)
    }

    /// Asks our phone to resend a message that arrived undecryptable.
    pub fn request_placeholder_resend(
        &self,
        chat_jid: String,
        message_id: String,
        from_me: bool,
        participant: Option<String>,
    ) -> Result<String, FfiError> {
        let client = self.client.clone();
        let key = baileys_core::baileys_proto::proto::MessageKey {
            remote_jid: Some(chat_jid),
            from_me: Some(from_me),
            id: Some(message_id),
            participant,
        };
        self.runtime
            .block_on(async move { client.request_placeholder_resend(&key).await })
            .map_err(op_err)
    }

    /// Default disappearing timer for new chats (0 = off).
    pub fn update_default_disappearing_mode(&self, duration_secs: u32) -> Result<(), FfiError> {
        let client = self.client.clone();
        self.runtime
            .block_on(async move { client.update_default_disappearing_mode(duration_secs).await })
            .map_err(op_err)
    }

    pub fn fetch_disappearing_duration(
        &self,
        jids: Vec<String>,
    ) -> Result<Vec<RolleysDisappearingDuration>, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let refs: Vec<&str> = jids.iter().map(String::as_str).collect();
            let rows = client
                .fetch_disappearing_duration(&refs)
                .await
                .map_err(op_err)?;
            Ok(rows
                .into_iter()
                .map(|(jid, duration_secs, set_at)| RolleysDisappearingDuration {
                    jid,
                    duration_secs,
                    set_at,
                })
                .collect())
        })
    }

    /// Details of an order, using the token carried by the order message.
    pub fn get_order_details(
        &self,
        order_id: String,
        token_base64: String,
    ) -> Result<RolleysOrderDetails, FfiError> {
        let client = self.client.clone();
        self.runtime.block_on(async move {
            let d = client
                .get_order_details(&order_id, &token_base64)
                .await
                .map_err(op_err)?;
            Ok(RolleysOrderDetails {
                total: d.total,
                currency: d.currency,
                products: d
                    .products
                    .into_iter()
                    .map(|p| RolleysOrderProduct {
                        id: p.id,
                        name: p.name,
                        image_url: p.image_url,
                        price: p.price,
                        currency: p.currency,
                        quantity: p.quantity,
                    })
                    .collect(),
            })
        })
    }

    /// Updates our business profile; unset fields stay as they are.
    pub fn update_business_profile(
        &self,
        update: RolleysBusinessProfileUpdate,
    ) -> Result<(), FfiError> {
        let client = self.client.clone();
        let update = baileys_core::commerce::BusinessProfileUpdate {
            address: update.address,
            email: update.email,
            description: update.description,
            websites: update.websites,
            hours: update.hours.map(|h| baileys_core::commerce::BusinessHours {
                timezone: h.timezone,
                days: h
                    .days
                    .into_iter()
                    .map(|d| baileys_core::commerce::BusinessHoursDay {
                        day: d.day,
                        mode: d.mode,
                        open_time_in_minutes: d.open_time_in_minutes,
                        close_time_in_minutes: d.close_time_in_minutes,
                    })
                    .collect(),
            }),
        };
        self.runtime
            .block_on(async move { client.update_business_profile(&update).await })
            .map_err(op_err)
    }
}

#[cfg(test)]
mod parity_tests {
    /// Core client methods deliberately not exposed through FFI.
    const NOT_EXPOSED: &[(&str, &str)] = &[
        ("assert_session", "internal Signal session setup"),
        ("inner", "internal state accessor"),
        ("metrics", "exposed as telemetry_snapshot"),
        ("refresh_media_conn", "internal media host refresh"),
        (
            "subscribe_connection_state",
            "Rust stream; connection state arrives as events",
        ),
        (
            "send_message",
            "takes a protobuf Message; use send_message_json",
        ),
        (
            "send_message_with_options",
            "takes a protobuf Message; use send_message_json",
        ),
        (
            "relay_message",
            "takes a protobuf Message; use send_message_json",
        ),
        (
            "send_peer_message",
            "takes a protobuf Message; used internally",
        ),
        ("send_node", "raw binary node"),
        ("send_iq", "raw binary node"),
        (
            "send_interactive_message",
            "takes the Rust builder; use send_interactive_buttons / send_list_message",
        ),
        (
            "with_additional_node",
            "MessageOptions builder, not a client method",
        ),
        (
            "with_additional_nodes",
            "MessageOptions builder, not a client method",
        ),
        ("commerce", "Rust service facade"),
        ("companion", "Rust service facade"),
        ("groups", "Rust service facade"),
        ("messages", "Rust service facade"),
        ("newsletters", "Rust service facade"),
        ("privacy", "Rust service facade"),
        ("status", "Rust service facade"),
    ];

    const CORE_SOURCES: &[&str] = &[
        include_str!("../../../crates/baileys-core/src/client/business.rs"),
        include_str!("../../../crates/baileys-core/src/client/channels.rs"),
        include_str!("../../../crates/baileys-core/src/client/chat_actions.rs"),
        include_str!("../../../crates/baileys-core/src/client/community.rs"),
        include_str!("../../../crates/baileys-core/src/client/companions.rs"),
        include_str!("../../../crates/baileys-core/src/client/connection.rs"),
        include_str!("../../../crates/baileys-core/src/client/groups.rs"),
        include_str!("../../../crates/baileys-core/src/client/media.rs"),
        include_str!("../../../crates/baileys-core/src/client/messaging.rs"),
        include_str!("../../../crates/baileys-core/src/client/mod.rs"),
        include_str!("../../../crates/baileys-core/src/client/privacy.rs"),
        include_str!("../../../crates/baileys-core/src/client/status.rs"),
    ];

    fn pub_fns(src: &str) -> impl Iterator<Item = &str> {
        src.lines().filter_map(|line| {
            let rest = line
                .trim_start()
                .strip_prefix("pub async fn ")
                .or_else(|| line.trim_start().strip_prefix("pub fn "))?;
            rest.split(['(', '<']).next()
        })
    }

    #[test]
    fn every_core_method_is_reachable_from_ffi() {
        let ffi = include_str!("lib.rs");
        let ffi_impl = &ffi[ffi.find("impl RolleysClient").expect("RolleysClient impl")..];
        let core: Vec<&str> = CORE_SOURCES.iter().flat_map(|s| pub_fns(s)).collect();

        let missing: Vec<&str> = core
            .iter()
            .copied()
            .filter(|name| !NOT_EXPOSED.iter().any(|(n, _)| n == name))
            .filter(|name| {
                !ffi_impl.contains(&format!(".{name}("))
                    && !ffi_impl.contains(&format!("pub fn {name}("))
            })
            .collect();
        assert!(
            missing.is_empty(),
            "core methods not reachable from ffi; expose them or list them in NOT_EXPOSED with a reason: {missing:?}"
        );

        let stale: Vec<&str> = NOT_EXPOSED
            .iter()
            .map(|(n, _)| *n)
            .filter(|n| !core.contains(n))
            .collect();
        assert!(
            stale.is_empty(),
            "NOT_EXPOSED lists methods core no longer has: {stale:?}"
        );
    }
}
