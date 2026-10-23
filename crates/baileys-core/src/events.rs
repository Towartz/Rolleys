use baileys_proto::proto::WebMessageInfo;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// Real-time events emitted by the WhatsApp client engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BaileysEvent {
    /// Connecting to WhatsApp Web gateway
    Connecting,
    /// Connection established and authenticated
    Connected { me_jid: String },
    /// Disconnected from WhatsApp Web
    Disconnected { reason: String },
    /// QR code emitted for pairing scan
    QrCode { code: String },
    /// 8-character phone number pairing code emitted
    PairingCode { code: String },
    /// New message received or sent
    Message { message: Box<WebMessageInfo> },
    /// Delivery or read receipt received
    Receipt {
        key_id: String,
        remote_jid: String,
        status: String,
    },
    /// Presence status update (e.g. "available", "composing")
    Presence { jid: String, presence: String },
    /// History sync archive received (initial chats, contacts, past messages)
    HistorySync {
        sync_type: i32,
        progress: u32,
        conversations: Vec<baileys_proto::proto::Conversation>,
        status_v3_messages: Vec<WebMessageInfo>,
    },
    /// Poll vote update received and tallied
    PollVote {
        poll_id: String,
        voter_jid: String,
        selected_options: Vec<String>,
        tally: baileys_store::PollTally,
    },
    /// Call event received (offer, ringing, accept, reject, terminate, timeout)
    Call { call: CallEvent },
    /// Status or story update received
    StoryReceived { story: baileys_store::StoredStory },
    /// Blocklist update notification received from server
    BlocklistUpdate {
        added: Vec<String>,
        removed: Vec<String>,
    },
    /// Message edited by sender
    MessageEdited {
        chat_jid: String,
        message_id: String,
        new_text: String,
    },
    /// Message revoked / deleted for everyone
    MessageRevoked {
        chat_jid: String,
        message_id: String,
        participant: Option<String>,
    },
    /// Chat disappearing messages ephemeral timer updated
    ChatEphemeralUpdate {
        chat_jid: String,
        ephemeral_expiration: u32,
    },
    /// WhatsApp Business label created, updated, or deleted
    LabelEdit { label: baileys_store::StoredLabel },
    /// Chat tagged or untagged with a label
    ChatLabelUpdate {
        chat_jid: String,
        label_id: String,
        labeled: bool,
    },
    /// Message tagged or untagged with a label
    MessageLabelUpdate {
        chat_jid: String,
        message_id: String,
        label_id: String,
        labeled: bool,
    },
    /// Companion devices list updated for a user
    DeviceUpdate { user_jid: String, devices: Vec<u32> },
    /// Contact re-registered or security code changed
    IdentityChanged { jid: String, timestamp: u64 },
    /// Media direct_path refreshed via Media Retry protocol
    MediaUpdated {
        message_id: String,
        direct_path: String,
    },
    /// Chat state updated (e.g. pinned, archived, muted, unread count)
    ChatUpdate {
        jid: String,
        pinned: Option<bool>,
        archived: Option<bool>,
        mute_until: Option<Option<u64>>,
        unread_count: Option<u32>,
    },
    /// Contact details updated from Syncd or USync
    ContactUpdate {
        contact: baileys_store::StoredContact,
    },
    /// App state collections synchronized
    AppStateSynced {
        collections_synced: usize,
        mutations_applied: usize,
    },
    /// Streaming media transfer progress
    MediaProgress {
        task_id: String,
        direction: String,
        bytes_transferred: u64,
        total_bytes: u64,
        percentage: f32,
    },
    /// Authentication credentials were persisted (Baileys `creds.update`)
    CredsUpdate,
    /// Group created or we were added to a new group (Baileys `groups.upsert`)
    GroupsUpsert {
        groups: Vec<crate::groups::GroupMetadata>,
    },
    /// Partial group metadata change (Baileys `groups.update`)
    GroupsUpdate { update: GroupUpdate },
    /// Participants added, removed, promoted, demoted or modified
    /// (Baileys `group-participants.update`)
    GroupParticipantsUpdate {
        group_jid: String,
        author: Option<String>,
        participants: Vec<crate::groups::GroupParticipant>,
        /// "add" | "remove" | "leave" | "promote" | "demote" | "modify"
        action: String,
    },
    /// Membership approval request created or revoked (Baileys `group.join-request`)
    GroupJoinRequest {
        group_jid: String,
        author: Option<String>,
        participant: String,
        participant_pn: Option<String>,
        /// "created" | "revoked" | "rejected"
        action: String,
        method: Option<String>,
    },
    /// Reaction count changed on a channel post (Baileys `newsletter.reaction`)
    NewsletterReaction {
        newsletter_jid: String,
        server_id: String,
        code: Option<String>,
        count: Option<u32>,
        removed: bool,
    },
    /// View count changed on a channel post (Baileys `newsletter.view`)
    NewsletterView {
        newsletter_jid: String,
        server_id: String,
        count: u32,
    },
    /// Channel admin/subscriber role change (Baileys `newsletter-participants.update`)
    NewsletterParticipantsUpdate {
        newsletter_jid: String,
        author: Option<String>,
        user: String,
        action: String,
        new_role: Option<String>,
    },
    /// Channel name/description/settings change (Baileys `newsletter-settings.update`)
    NewsletterSettingsUpdate {
        newsletter_jid: String,
        name: Option<String>,
        description: Option<String>,
    },
}

/// Partial group metadata carried by [`BaileysEvent::GroupsUpdate`].
/// Only the fields the notification actually changed are `Some`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GroupUpdate {
    pub id: String,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub subject_owner: Option<String>,
    pub subject_time: Option<i64>,
    pub description: Option<String>,
    /// True when the description was removed (so `description` stays `None`).
    pub description_deleted: bool,
    pub description_id: Option<String>,
    pub announce: Option<bool>,
    pub restrict: Option<bool>,
    pub join_approval_mode: Option<bool>,
    pub member_add_mode: Option<String>,
    pub ephemeral_duration: Option<u32>,
    pub invite_code: Option<String>,
}

/// Details of an incoming or updated WhatsApp VoIP call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CallEvent {
    pub id: String,
    pub chat_jid: String,
    pub from: String,
    pub caller_pn: Option<String>,
    pub is_group: bool,
    pub group_jid: Option<String>,
    pub is_video: bool,
    pub status: String, // "offer", "ringing", "preaccept", "accept", "reject", "terminate", "timeout", "relaylatency"
    pub timestamp: u64,
    pub offline: bool,
    pub latency_ms: Option<u32>,
}

/// Reactive multi-consumer broadcast event emitter.
#[derive(Clone)]
pub struct EventEmitter {
    sender: broadcast::Sender<BaileysEvent>,
}

impl Default for EventEmitter {
    fn default() -> Self {
        Self::new(256)
    }
}

impl EventEmitter {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn emit(&self, event: BaileysEvent) {
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<BaileysEvent> {
        self.sender.subscribe()
    }
}
