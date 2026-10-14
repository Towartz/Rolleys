use async_trait::async_trait;
use baileys_crypto::{format_signal_pubkey, generate_key_pair, sign_ed25519, KeyPair};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::StoreError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedPreKey {
    pub key_id: u32,
    pub key_pair: SerializableKeyPair,
    #[serde(with = "crate::buffer_json")]
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableKeyPair {
    #[serde(with = "crate::buffer_json")]
    pub public: Vec<u8>,
    #[serde(with = "crate::buffer_json")]
    pub private: Vec<u8>,
}

impl From<&KeyPair> for SerializableKeyPair {
    fn from(kp: &KeyPair) -> Self {
        Self {
            public: kp.public_key.to_vec(),
            private: kp.private_key.to_vec(),
        }
    }
}

impl TryFrom<&SerializableKeyPair> for KeyPair {
    type Error = StoreError;
    fn try_from(skp: &SerializableKeyPair) -> Result<Self, Self::Error> {
        if skp.public.len() != 32 || skp.private.len() != 32 {
            return Err(StoreError::InvalidKeyFormat);
        }
        let mut public_key = [0u8; 32];
        let mut private_key = [0u8; 32];
        public_key.copy_from_slice(&skp.public);
        private_key.copy_from_slice(&skp.private);
        Ok(KeyPair {
            public_key,
            private_key,
        })
    }
}

/// WhatsApp client authentication credentials structure (matching Baileys `creds.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthCreds {
    pub noise_key: SerializableKeyPair,
    pub signed_identity_key: SerializableKeyPair,
    pub signed_pre_key: SignedPreKey,
    pub registration_id: u32,
    #[serde(with = "crate::buffer_json")]
    pub adv_secret_key: Vec<u8>,
    pub me_jid: Option<String>,
    pub me_name: Option<String>,
    #[serde(default)]
    pub me_lid: Option<String>,
    pub next_pre_key_id: u32,
    pub first_unuploaded_pre_key_id: u32,
    pub account_sync_counter: u32,
    #[serde(default)]
    pub account: Option<Vec<u8>>,
}

impl AuthCreds {
    /// Initializes brand new random credentials for pairing.
    pub fn init() -> Self {
        let noise_key = generate_key_pair();
        let identity_key = generate_key_pair();
        let pre_key = generate_key_pair();

        let prekey_signal_pub = format_signal_pubkey(&pre_key.public_key);
        let sig = sign_ed25519(&identity_key.private_key, &prekey_signal_pub);

        let mut adv_secret = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut adv_secret);

        let reg_id = (rand::random::<u16>() & 16383) as u32;

        Self {
            noise_key: (&noise_key).into(),
            signed_identity_key: (&identity_key).into(),
            signed_pre_key: SignedPreKey {
                key_id: 1,
                key_pair: (&pre_key).into(),
                signature: sig.to_vec(),
            },
            registration_id: reg_id,
            adv_secret_key: adv_secret.to_vec(),
            me_jid: None,
            me_name: None,
            me_lid: None,
            next_pre_key_id: 1,
            first_unuploaded_pre_key_id: 1,
            account_sync_counter: 0,
            account: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEntry {
    pub key_type: String,
    pub id: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredChat {
    pub jid: String,
    pub name: Option<String>,
    pub unread_count: u32,
    pub last_message_timestamp: u64,
    pub last_message_summary: Option<String>,
    pub pinned: bool,
    pub archived: bool,
    pub mute_until: Option<u64>,
    #[serde(default)]
    pub ephemeral_expiration: u32,
    #[serde(default)]
    pub ephemeral_setting_timestamp: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: String,
    pub chat_jid: String,
    pub sender_jid: String,
    pub from_me: bool,
    pub timestamp: u64,
    pub summary: String,
    pub status: String,
    pub has_media: bool,
    #[serde(with = "crate::buffer_json")]
    pub proto_bytes: Vec<u8>,
    pub is_starred: bool,
    #[serde(default)]
    pub is_revoked: bool,
    #[serde(default)]
    pub is_edited: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredContact {
    pub jid: String,
    pub name: Option<String>,
    pub push_name: Option<String>,
    pub lid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredNewsletter {
    pub jid: String,
    pub name: String,
    pub description: Option<String>,
    pub invite_code: Option<String>,
    pub picture_url: Option<String>,
    pub role: Option<String>,
    pub muted: bool,
    pub subscribers_count: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredPoll {
    pub id: String,
    pub chat_jid: String,
    pub sender_jid: String,
    pub question: String,
    pub options: Vec<String>,
    #[serde(with = "crate::buffer_json")]
    pub enc_key: Vec<u8>,
    pub selectable_count: u32,
    pub created_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredPollVote {
    pub poll_id: String,
    pub voter_jid: String,
    pub selected_options: Vec<String>,
    pub updated_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollTally {
    pub poll_id: String,
    pub question: String,
    pub total_voters: usize,
    pub option_votes: HashMap<String, u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCommunitySubgroup {
    pub group_jid: String,
    pub subject: String,
    pub creation: Option<u64>,
    pub creator: Option<String>,
    pub size: Option<u32>,
    pub linked_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCall {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredStory {
    pub msg_id: String,
    pub author_jid: String,
    pub author_name: Option<String>,
    pub timestamp: u64,
    pub message_type: String, // "text", "image", "video", "audio"
    pub content: Option<String>,
    pub caption: Option<String>,
    pub has_media: bool,
    pub viewed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredLabel {
    pub id: String,
    pub name: String,
    pub color: i32,
    #[serde(default)]
    pub predefined_id: Option<i32>,
    #[serde(default)]
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCatalogProduct {
    pub product_id: String,
    pub business_owner_jid: String,
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
    pub updated_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCatalogCollection {
    pub collection_id: String,
    pub business_owner_jid: String,
    pub name: String,
    pub products_count: u32,
    pub status: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredOrder {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredUserDevice {
    pub user_jid: String,
    pub device_id: u32,
    pub key_index: Option<u32>,
    pub updated_at: u64,
}

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn get_creds(&self) -> Result<Option<AuthCreds>, StoreError>;
    async fn save_creds(&self, creds: &AuthCreds) -> Result<(), StoreError>;
    async fn clear(&self) -> Result<(), StoreError>;
}

#[async_trait]
pub trait SignalKeysStore: Send + Sync {
    async fn get_key(&self, key_type: &str, id: &str) -> Result<Option<Vec<u8>>, StoreError>;
    async fn set_keys(&self, entries: Vec<KeyEntry>) -> Result<(), StoreError>;
    async fn remove_keys(&self, key_type: &str, ids: &[String]) -> Result<(), StoreError>;
    async fn get_key_count(&self, _key_type: &str) -> Result<usize, StoreError> {
        Ok(0)
    }
    async fn get_all_keys(&self, _key_type: &str) -> Result<Vec<(String, Vec<u8>)>, StoreError> {
        Ok(Vec::new())
    }
}

#[async_trait]
pub trait ChatStore: Send + Sync {
    // Chat history and message persistence
    async fn save_chat(&self, _chat: &StoredChat) -> Result<(), StoreError> {
        Ok(())
    }
    async fn get_chats(&self) -> Result<Vec<StoredChat>, StoreError> {
        Ok(Vec::new())
    }
    async fn get_chat(&self, _jid: &str) -> Result<Option<StoredChat>, StoreError> {
        Ok(None)
    }

    async fn save_message(&self, _message: &StoredMessage) -> Result<(), StoreError> {
        Ok(())
    }
    async fn save_messages_batch(&self, _messages: &[StoredMessage]) -> Result<(), StoreError> {
        Ok(())
    }
    async fn get_messages(
        &self,
        _chat_jid: &str,
        _limit: u32,
        _before_timestamp: Option<u64>,
    ) -> Result<Vec<StoredMessage>, StoreError> {
        Ok(Vec::new())
    }
    async fn get_message(&self, _id: &str) -> Result<Option<StoredMessage>, StoreError> {
        Ok(None)
    }
    async fn get_latest_message(&self) -> Result<Option<StoredMessage>, StoreError> {
        Ok(None)
    }

    // Chat and message mutations (AppState / Syncd support)
    async fn update_chat_pin(&self, _jid: &str, _pinned: bool) -> Result<(), StoreError> {
        Ok(())
    }
    async fn update_chat_archive(&self, _jid: &str, _archived: bool) -> Result<(), StoreError> {
        Ok(())
    }
    async fn update_chat_mute(
        &self,
        _jid: &str,
        _mute_until: Option<u64>,
    ) -> Result<(), StoreError> {
        Ok(())
    }
    async fn update_message_star(
        &self,
        _chat_jid: &str,
        _msg_id: &str,
        _is_starred: bool,
    ) -> Result<(), StoreError> {
        Ok(())
    }
    async fn update_message_status(&self, _msg_id: &str, _status: &str) -> Result<(), StoreError> {
        Ok(())
    }
    async fn clear_chat_messages(
        &self,
        _chat_jid: &str,
        _keep_starred: bool,
    ) -> Result<(), StoreError> {
        Ok(())
    }
    async fn delete_chat(&self, _chat_jid: &str) -> Result<(), StoreError> {
        Ok(())
    }
    async fn update_chat_unread(&self, _chat_jid: &str, _unread: bool) -> Result<(), StoreError> {
        Ok(())
    }

    async fn save_contact(&self, _contact: &StoredContact) -> Result<(), StoreError> {
        Ok(())
    }
    async fn get_contact(&self, _jid: &str) -> Result<Option<StoredContact>, StoreError> {
        Ok(None)
    }
    async fn get_contacts(&self) -> Result<Vec<StoredContact>, StoreError> {
        Ok(Vec::new())
    }
    async fn search_messages(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<StoredMessage>, StoreError> {
        Ok(Vec::new())
    }

    // Message Edit & Revoke persistence
    async fn update_message_content(
        &self,
        _chat_jid: &str,
        _message_id: &str,
        _new_content: &str,
        _new_proto_bytes: Option<&[u8]>,
    ) -> Result<bool, StoreError> {
        Ok(false)
    }
    async fn revoke_message(&self, chat_jid: &str, message_id: &str) -> Result<bool, StoreError> {
        self.revoke_message_ex(chat_jid, message_id, false).await
    }
    async fn revoke_message_ex(
        &self,
        _chat_jid: &str,
        _message_id: &str,
        _anti_revoke: bool,
    ) -> Result<bool, StoreError> {
        Ok(false)
    }

    // Ephemeral Disappearing Messages persistence
    async fn update_chat_ephemeral(
        &self,
        _chat_jid: &str,
        _ephemeral_expiration: u32,
        _setting_timestamp: Option<i64>,
    ) -> Result<(), StoreError> {
        Ok(())
    }
    async fn get_chat_ephemeral(
        &self,
        _chat_jid: &str,
    ) -> Result<Option<(u32, Option<i64>)>, StoreError> {
        Ok(None)
    }
}

#[async_trait]
pub trait NewsletterStore: Send + Sync {
    // Newsletter / Channel persistence
    async fn save_newsletter(&self, _newsletter: &StoredNewsletter) -> Result<(), StoreError> {
        Ok(())
    }
    async fn get_newsletters(&self) -> Result<Vec<StoredNewsletter>, StoreError> {
        Ok(Vec::new())
    }
    async fn get_newsletter(&self, _jid: &str) -> Result<Option<StoredNewsletter>, StoreError> {
        Ok(None)
    }
}

#[async_trait]
pub trait AppStateStore: Send + Sync {
    // Polls & Votes persistence
    async fn save_poll(&self, _poll: &StoredPoll) -> Result<(), StoreError> {
        Ok(())
    }
    async fn get_poll(&self, _poll_id: &str) -> Result<Option<StoredPoll>, StoreError> {
        Ok(None)
    }
    async fn save_poll_vote(
        &self,
        _poll_id: &str,
        _voter_jid: &str,
        _selected_options: &[String],
    ) -> Result<(), StoreError> {
        Ok(())
    }
    async fn get_poll_votes(&self, _poll_id: &str) -> Result<Vec<StoredPollVote>, StoreError> {
        Ok(Vec::new())
    }
    async fn get_poll_tally(&self, _poll_id: &str) -> Result<Option<PollTally>, StoreError> {
        Ok(None)
    }

    // Community parent-child relationships
    async fn save_community_subgroup(
        &self,
        _community_jid: &str,
        _subgroup: &StoredCommunitySubgroup,
    ) -> Result<(), StoreError> {
        Ok(())
    }
    async fn remove_community_subgroup(
        &self,
        _community_jid: &str,
        _group_jid: &str,
    ) -> Result<(), StoreError> {
        Ok(())
    }
    async fn get_community_subgroups(
        &self,
        _community_jid: &str,
    ) -> Result<Vec<StoredCommunitySubgroup>, StoreError> {
        Ok(Vec::new())
    }
    async fn get_parent_community(&self, _group_jid: &str) -> Result<Option<String>, StoreError> {
        Ok(None)
    }

    // Call logs persistence
    async fn save_call(&self, _call: &StoredCall) -> Result<(), StoreError> {
        Ok(())
    }
    async fn get_call(&self, _call_id: &str) -> Result<Option<StoredCall>, StoreError> {
        Ok(None)
    }
    async fn get_call_history(&self, _limit: u32) -> Result<Vec<StoredCall>, StoreError> {
        Ok(Vec::new())
    }

    // Status & Broadcast Stories persistence
    async fn save_story(&self, _story: &StoredStory) -> Result<(), StoreError> {
        Ok(())
    }
    async fn get_stories(
        &self,
        _limit: u32,
        _unviewed_only: bool,
    ) -> Result<Vec<StoredStory>, StoreError> {
        Ok(Vec::new())
    }
    async fn get_stories_by_author(
        &self,
        _author_jid: &str,
    ) -> Result<Vec<StoredStory>, StoreError> {
        Ok(Vec::new())
    }
    async fn mark_story_viewed(&self, _msg_id: &str) -> Result<(), StoreError> {
        Ok(())
    }

    // Blocklist persistence
    async fn save_blocked_contact(&self, _jid: &str) -> Result<(), StoreError> {
        Ok(())
    }
    async fn remove_blocked_contact(&self, _jid: &str) -> Result<(), StoreError> {
        Ok(())
    }
    async fn get_blocked_contacts(&self) -> Result<Vec<String>, StoreError> {
        Ok(Vec::new())
    }
    async fn set_blocked_contacts(&self, _jids: &[String]) -> Result<(), StoreError> {
        Ok(())
    }

    // ==========================================
    // Track 9: Labels & WhatsApp Business Protocol
    // ==========================================
    async fn upsert_label(&self, _label: &StoredLabel) -> Result<(), StoreError> {
        Ok(())
    }
    async fn get_labels(&self) -> Result<Vec<StoredLabel>, StoreError> {
        Ok(Vec::new())
    }
    async fn get_label(&self, _id: &str) -> Result<Option<StoredLabel>, StoreError> {
        Ok(None)
    }
    async fn delete_label(&self, _id: &str) -> Result<(), StoreError> {
        Ok(())
    }
    async fn add_chat_label(&self, _chat_jid: &str, _label_id: &str) -> Result<(), StoreError> {
        Ok(())
    }
    async fn remove_chat_label(&self, _chat_jid: &str, _label_id: &str) -> Result<(), StoreError> {
        Ok(())
    }
    async fn get_chat_labels(&self, _chat_jid: &str) -> Result<Vec<String>, StoreError> {
        Ok(Vec::new())
    }
    async fn get_chats_by_label(&self, _label_id: &str) -> Result<Vec<String>, StoreError> {
        Ok(Vec::new())
    }
    async fn add_message_label(
        &self,
        _chat_jid: &str,
        _message_id: &str,
        _label_id: &str,
    ) -> Result<(), StoreError> {
        Ok(())
    }
    async fn remove_message_label(
        &self,
        _chat_jid: &str,
        _message_id: &str,
        _label_id: &str,
    ) -> Result<(), StoreError> {
        Ok(())
    }
    async fn get_message_labels(
        &self,
        _chat_jid: &str,
        _message_id: &str,
    ) -> Result<Vec<String>, StoreError> {
        Ok(Vec::new())
    }
    async fn get_messages_by_label(&self, _label_id: &str) -> Result<Vec<String>, StoreError> {
        Ok(Vec::new())
    }

    // ==========================================
    // Track 12: Business Catalog, Products & Orders
    // ==========================================
    async fn save_catalog_product(
        &self,
        _product: &StoredCatalogProduct,
    ) -> Result<(), StoreError> {
        Ok(())
    }
    async fn get_catalog_products(
        &self,
        _biz_jid: &str,
    ) -> Result<Vec<StoredCatalogProduct>, StoreError> {
        Ok(Vec::new())
    }
    async fn get_catalog_product(
        &self,
        _product_id: &str,
    ) -> Result<Option<StoredCatalogProduct>, StoreError> {
        Ok(None)
    }
    async fn save_catalog_collection(
        &self,
        _collection: &StoredCatalogCollection,
    ) -> Result<(), StoreError> {
        Ok(())
    }
    async fn get_catalog_collections(
        &self,
        _biz_jid: &str,
    ) -> Result<Vec<StoredCatalogCollection>, StoreError> {
        Ok(Vec::new())
    }
    async fn save_order(&self, _order: &StoredOrder) -> Result<(), StoreError> {
        Ok(())
    }
    async fn get_order(&self, _order_id: &str) -> Result<Option<StoredOrder>, StoreError> {
        Ok(None)
    }
    async fn get_orders(
        &self,
        _chat_jid: &str,
        _limit: u32,
    ) -> Result<Vec<StoredOrder>, StoreError> {
        Ok(Vec::new())
    }

    // ==========================================
    // Track 13: Multi-Device Companions & Devices
    // ==========================================
    async fn save_user_devices(
        &self,
        _user_jid: &str,
        _device_ids: &[u32],
    ) -> Result<(), StoreError> {
        Ok(())
    }
    async fn get_user_devices(&self, _user_jid: &str) -> Result<Vec<u32>, StoreError> {
        Ok(Vec::new())
    }
    async fn remove_user_device(&self, _user_jid: &str, _device_id: u32) -> Result<(), StoreError> {
        Ok(())
    }

    // ==========================================
    // KV Store & Media Download Tracking
    // ==========================================
    async fn set_kv(&self, _key: &str, _value: &str) -> Result<(), StoreError> {
        Ok(())
    }
    async fn get_kv(&self, _key: &str) -> Result<Option<String>, StoreError> {
        Ok(None)
    }
    async fn delete_kv(&self, _key: &str) -> Result<(), StoreError> {
        Ok(())
    }
    async fn list_kv_prefix(&self, _prefix: &str) -> Result<HashMap<String, String>, StoreError> {
        Ok(HashMap::new())
    }
    async fn record_media_download(
        &self,
        _message_id: &str,
        _chat_jid: &str,
        _file_path: &str,
        _file_size: u64,
        _mimetype: Option<&str>,
    ) -> Result<(), StoreError> {
        Ok(())
    }
    async fn get_media_download(&self, _message_id: &str) -> Result<Option<String>, StoreError> {
        Ok(None)
    }
}

/// Unified storage supertrait combining all modular domain stores.
/// Any type implementing `SessionStore + SignalKeysStore + ChatStore + NewsletterStore + AppStateStore`
/// automatically implements `BaileysStore`.
pub trait BaileysStore:
    SessionStore + SignalKeysStore + ChatStore + NewsletterStore + AppStateStore + Send + Sync
{
}

impl<T> BaileysStore for T where
    T: SessionStore + SignalKeysStore + ChatStore + NewsletterStore + AppStateStore + Send + Sync
{
}

/// Adapter allowing any `Arc<dyn BaileysStore>` to be used as a Signal protocol `SignalStore`.
pub struct SignalStoreAdapter {
    store: std::sync::Arc<dyn BaileysStore>,
}

impl SignalStoreAdapter {
    pub fn new(store: std::sync::Arc<dyn BaileysStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl baileys_signal::SignalStore for SignalStoreAdapter {
    async fn get_session(
        &self,
        address: &str,
    ) -> Result<Option<Vec<u8>>, baileys_signal::SignalError> {
        self.store
            .get_key("session", address)
            .await
            .map_err(|e| baileys_signal::SignalError::Crypto(e.to_string()))
    }

    async fn set_session(
        &self,
        address: &str,
        session: &[u8],
    ) -> Result<(), baileys_signal::SignalError> {
        self.store
            .set_keys(vec![KeyEntry {
                key_type: "session".to_string(),
                id: address.to_string(),
                data: session.to_vec(),
            }])
            .await
            .map_err(|e| baileys_signal::SignalError::Crypto(e.to_string()))
    }

    async fn get_sender_key(
        &self,
        key_id: &str,
    ) -> Result<Option<Vec<u8>>, baileys_signal::SignalError> {
        self.store
            .get_key("sender-key", key_id)
            .await
            .map_err(|e| baileys_signal::SignalError::Crypto(e.to_string()))
    }

    async fn set_sender_key(
        &self,
        key_id: &str,
        sender_key: &[u8],
    ) -> Result<(), baileys_signal::SignalError> {
        self.store
            .set_keys(vec![KeyEntry {
                key_type: "sender-key".to_string(),
                id: key_id.to_string(),
                data: sender_key.to_vec(),
            }])
            .await
            .map_err(|e| baileys_signal::SignalError::Crypto(e.to_string()))
    }

    async fn get_pre_key(
        &self,
        pre_key_id: u32,
    ) -> Result<Option<KeyPair>, baileys_signal::SignalError> {
        match self.store.get_key("pre-key", &pre_key_id.to_string()).await {
            Ok(Some(bytes)) => match serde_json::from_slice::<SerializableKeyPair>(&bytes) {
                Ok(skp) => KeyPair::try_from(&skp)
                    .map(Some)
                    .map_err(|e| baileys_signal::SignalError::Crypto(e.to_string())),
                Err(e) => {
                    tracing::warn!("Corrupted pre-key {} in store: {}", pre_key_id, e);
                    Ok(None)
                }
            },
            Ok(None) => Ok(None),
            Err(e) => Err(baileys_signal::SignalError::Crypto(e.to_string())),
        }
    }

    async fn remove_pre_key(&self, pre_key_id: u32) -> Result<(), baileys_signal::SignalError> {
        self.store
            .remove_keys("pre-key", &[pre_key_id.to_string()])
            .await
            .map_err(|e| baileys_signal::SignalError::Crypto(e.to_string()))
    }
}
