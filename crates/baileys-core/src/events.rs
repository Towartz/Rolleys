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
        /// Chats in this chunk; each keeps only its most recent message (all are in `messages`).
        conversations: Vec<baileys_proto::proto::Conversation>,
        status_v3_messages: Vec<WebMessageInfo>,
        contacts: Vec<crate::history::HistoryContact>,
        messages: Vec<WebMessageInfo>,
        lid_pn_mappings: Vec<crate::history::LidPnMapping>,
        past_participants: Vec<baileys_proto::proto::PastParticipants>,
        /// True for the first non-on-demand history notification this session store has seen
        /// (Baileys `isLatest`); `None` for on-demand fetches.
        is_latest: Option<bool>,
        chunk_order: Option<u32>,
        peer_data_request_session_id: Option<String>,
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
    /// Chats deleted on another device (Baileys `chats.delete`); not emitted for initial snapshots.
    ChatsDelete { jids: Vec<String> },
    /// Chat locked or unlocked (Baileys `chats.lock`)
    ChatsLock { jid: String, locked: bool },
    /// Account setting changed (Baileys `settings.update`). `setting` uses Baileys' names
    /// (`locale`, `timeFormat`, `unarchiveChats`, `statusPrivacy`, ...); `value` is rendered as text.
    SettingsUpdate { setting: String, value: String },
    /// Phone number learned for a LID (Baileys `lid-mapping.update`)
    LidMappingUpdate { lid: String, pn: String },
    /// Contacts added or replaced (Baileys `contacts.upsert`)
    ContactsUpsert {
        contacts: Vec<baileys_store::StoredContact>,
    },
    /// Message deleted for this account only (Baileys `messages.delete`)
    MessagesDelete {
        chat_jid: String,
        message_id: String,
        from_me: bool,
    },
    /// Message starred or unstarred (Baileys `messages.update` with `starred`)
    MessageStarred {
        chat_jid: String,
        message_id: String,
        from_me: bool,
        starred: bool,
    },
    /// New chats (Baileys `chats.upsert`); emitted when a group is created or we are added to one.
    ChatsUpsert { chats: Vec<ChatUpsert> },
    /// Reaction added or removed on a message (Baileys `messages.reaction`)
    MessagesReaction { reaction: MessageReaction },
}

/// A chat carried by [`BaileysEvent::ChatsUpsert`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ChatUpsert {
    pub jid: String,
    pub name: Option<String>,
    pub conversation_timestamp: Option<i64>,
}

/// A reaction carried by [`BaileysEvent::MessagesReaction`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MessageReaction {
    /// Chat the reacted-to message lives in.
    pub chat_jid: String,
    /// Id of the reacted-to message.
    pub message_id: String,
    pub target_from_me: bool,
    pub target_participant: Option<String>,
    /// Who reacted (participant in groups, the chat JID in 1:1 chats).
    pub sender: String,
    pub sender_from_me: bool,
    /// Emoji; empty when the reaction was removed.
    pub text: String,
    pub removed: bool,
    pub sender_timestamp_ms: Option<i64>,
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

/// Events buffered per subscriber before the slowest one starts losing them.
///
/// A forced app-state snapshot emits one event per mutation (~1.8k on a real account), all at
/// once. Baileys' emitter is synchronous and never drops; a broadcast channel does once full, so
/// this must comfortably exceed a full resync.
pub const DEFAULT_EVENT_CAPACITY: usize = 16_384;

impl Default for EventEmitter {
    fn default() -> Self {
        Self::new(DEFAULT_EVENT_CAPACITY)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_capacity_survives_a_full_app_state_resync() {
        // A forced snapshot of a real account emitted ~1.8k events at once; with the old
        // capacity of 256 a consumer that was not polling lost ~1.5k of them.
        let emitter = EventEmitter::default();
        let mut rx = emitter.subscribe();
        for _ in 0..5_000 {
            emitter.emit(BaileysEvent::CredsUpdate);
        }
        let mut received = 0;
        loop {
            match rx.try_recv() {
                Ok(_) => received += 1,
                Err(broadcast::error::TryRecvError::Lagged(n)) => panic!("lagged by {n}"),
                Err(_) => break,
            }
        }
        assert_eq!(received, 5_000);
        eprintln!(
            "size_of::<BaileysEvent>() = {} bytes, capacity {}",
            std::mem::size_of::<BaileysEvent>(),
            DEFAULT_EVENT_CAPACITY
        );
    }
}
