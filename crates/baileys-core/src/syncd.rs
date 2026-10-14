use baileys_binary::jid::S_WHATSAPP_NET;
use baileys_binary::BinaryNode;
use baileys_crypto::{
    aes_256_cbc_decrypt_prefixed_iv, aes_256_cbc_encrypt_prefixed_iv, expand_app_state_keys,
    hmac_sha256, hmac_sha512, LTHashAntiTampering,
};
use baileys_proto::proto::{
    sync_action_value, ExternalBlobReference, KeyId, SyncActionData, SyncActionValue, SyncdIndex,
    SyncdMutation, SyncdPatch, SyncdRecord, SyncdSnapshot, SyncdValue, SyncdVersion,
};
use prost::Message;
use std::collections::HashMap;
use std::time::SystemTime;

use crate::CoreError;

pub const CRITICAL_BLOCK: &str = "critical_block";
pub const CRITICAL_UNBLOCK_LOW: &str = "critical_unblock_low";
pub const REGULAR_HIGH: &str = "regular_high";
pub const REGULAR_LOW: &str = "regular_low";
pub const REGULAR: &str = "regular";
pub const ALL_COLLECTIONS: &[&str] = &[
    CRITICAL_BLOCK,
    CRITICAL_UNBLOCK_LOW,
    REGULAR_HIGH,
    REGULAR_LOW,
    REGULAR,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LTHashState {
    pub version: u64,
    pub hash: Vec<u8>,
    pub index_value_map: HashMap<Vec<u8>, Vec<u8>>,
}

impl Default for LTHashState {
    fn default() -> Self {
        Self {
            version: 0,
            hash: vec![0u8; 128],
            index_value_map: HashMap::new(),
        }
    }
}

/// JSON-serializable representation of LTHashState for keystore persistence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoredLTHashState {
    pub version: u64,
    pub hash_hex: String,
    pub index_value_hex: HashMap<String, String>,
}

impl From<&LTHashState> for StoredLTHashState {
    fn from(s: &LTHashState) -> Self {
        let mut index_value_hex = HashMap::new();
        for (k, v) in &s.index_value_map {
            index_value_hex.insert(hex::encode(k), hex::encode(v));
        }
        Self {
            version: s.version,
            hash_hex: hex::encode(&s.hash),
            index_value_hex,
        }
    }
}

impl From<StoredLTHashState> for LTHashState {
    fn from(s: StoredLTHashState) -> Self {
        let hash = hex::decode(&s.hash_hex).unwrap_or_else(|_| vec![0u8; 128]);
        let mut index_value_map = HashMap::new();
        for (k_hex, v_hex) in s.index_value_hex {
            if let (Ok(k), Ok(v)) = (hex::decode(k_hex), hex::decode(v_hex)) {
                index_value_map.insert(k, v);
            }
        }
        Self {
            version: s.version,
            hash,
            index_value_map,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DecodedSyncdMutation {
    pub operation: i32,
    pub index: Vec<String>,
    pub sync_action: SyncActionData,
    pub index_mac: Vec<u8>,
    pub value_mac: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncdAction {
    Mute {
        jid: String,
        muted: bool,
        mute_end_timestamp: Option<i64>,
    },
    Pin {
        jid: String,
        pinned: bool,
    },
    Archive {
        jid: String,
        archived: bool,
    },
    Star {
        jid: String,
        msg_id: String,
        from_me: bool,
        starred: bool,
    },
    ClearChat {
        jid: String,
    },
    DeleteChat {
        jid: String,
    },
    MarkChatAsRead {
        jid: String,
        read: bool,
    },
    Contact {
        id: String,
        name: Option<String>,
        username: Option<String>,
        lid: Option<String>,
        phone_number: Option<String>,
    },
    Other {
        name: String,
        index: Vec<String>,
    },
}

impl DecodedSyncdMutation {
    pub fn parse_action(&self) -> SyncdAction {
        let action_name = self.index.first().map(|s| s.as_str()).unwrap_or("");
        let jid = self.index.get(1).cloned().unwrap_or_default();
        let val = self.sync_action.value.as_ref();

        match action_name {
            "mute" => {
                let mute_act = val.and_then(|v| v.mute_action.as_ref());
                SyncdAction::Mute {
                    jid,
                    muted: mute_act.and_then(|m| m.muted).unwrap_or(false),
                    mute_end_timestamp: mute_act.and_then(|m| m.mute_end_timestamp),
                }
            }
            "pin_v1" | "pin" => {
                let pin_act = val.and_then(|v| v.pin_action.as_ref());
                SyncdAction::Pin {
                    jid,
                    pinned: pin_act.and_then(|p| p.pinned).unwrap_or(false),
                }
            }
            "archive" => {
                let arch_act = val.and_then(|v| v.archive_chat_action.as_ref());
                SyncdAction::Archive {
                    jid,
                    archived: arch_act.and_then(|a| a.archived).unwrap_or(false),
                }
            }
            "star" => {
                let msg_id = self.index.get(2).cloned().unwrap_or_default();
                let from_me = self.index.get(3).map(|s| s == "1").unwrap_or(false);
                let star_act = val.and_then(|v| v.star_action.as_ref());
                SyncdAction::Star {
                    jid,
                    msg_id,
                    from_me,
                    starred: star_act.and_then(|s| s.starred).unwrap_or(false),
                }
            }
            "clearChat" => SyncdAction::ClearChat { jid },
            "deleteChat" => SyncdAction::DeleteChat { jid },
            "markChatAsRead" => {
                let read_act = val.and_then(|v| v.mark_chat_as_read_action.as_ref());
                SyncdAction::MarkChatAsRead {
                    jid,
                    read: read_act.and_then(|r| r.read).unwrap_or(true),
                }
            }
            "contact" => {
                let contact_act = val.and_then(|v| v.contact_action.as_ref());
                let name = contact_act.and_then(|c| {
                    c.full_name
                        .clone()
                        .or_else(|| c.first_name.clone())
                        .or_else(|| c.username.clone())
                });
                let username = contact_act.and_then(|c| c.username.clone());
                let lid = contact_act.and_then(|c| c.lid_jid.clone());
                let phone_number = if jid.contains('@') && !jid.ends_with("@lid") {
                    Some(jid.clone())
                } else {
                    contact_act.and_then(|c| c.pn_jid.clone())
                };
                SyncdAction::Contact {
                    id: jid,
                    name,
                    username,
                    lid,
                    phone_number,
                }
            }
            _ => SyncdAction::Other {
                name: action_name.to_string(),
                index: self.index.clone(),
            },
        }
    }
}

pub struct SyncdPatchResult {
    pub patch: SyncdPatch,
    pub new_hash: Vec<u8>,
    pub next_version: u64,
}

/// Encodes an AppState Syncd patch following the WhatsApp Syncd v2 protocol.
///
/// 1. Serializes `SyncActionData` with JSON index and protobuf action value.
/// 2. Derives mutation keys via HKDF-SHA256 info="WhatsApp Mutation Keys".
/// 3. Encrypts with AES-256-CBC with prefixed random 16-byte IV.
/// 4. Generates HMAC-SHA512 value MAC, HMAC-SHA256 index MAC, and updates LTHash160.
/// 5. Derives snapshot MAC and patch MAC.
pub fn encode_syncd_patch(
    collection: &str,
    index_parts: &[&str],
    sync_action: SyncActionValue,
    api_version: i32,
    operation: i32, // 0 = SET, 1 = REMOVE
    key_id: &[u8],
    key_data: &[u8],
    state_hash: &[u8],
    current_version: u64,
) -> Result<SyncdPatchResult, CoreError> {
    let index_json = serde_json::to_vec(index_parts)
        .map_err(|e| CoreError::Protocol(format!("Syncd index serialization error: {}", e)))?;

    let data_proto = SyncActionData {
        index: Some(index_json.clone()),
        value: Some(sync_action),
        padding: Some(Vec::new()),
        version: Some(api_version),
    };

    let mut encoded_data = Vec::new();
    data_proto
        .encode(&mut encoded_data)
        .map_err(|e| CoreError::Protocol(format!("SyncActionData encode error: {}", e)))?;

    let keys = expand_app_state_keys(key_data)
        .map_err(|e| CoreError::Crypto(format!("Failed to expand app state keys: {}", e)))?;

    let enc_value = aes_256_cbc_encrypt_prefixed_iv(&encoded_data, &keys.value_encryption_key)
        .map_err(|e| CoreError::Crypto(format!("Syncd value CBC encryption error: {}", e)))?;

    // generateMac:
    // opByte: 0x01 for SET, 0x02 for REMOVE
    // total: [opByte, key_id, enc_value, [0,0,0,0,0,0,0, (1 + key_id.len()) as u8]]
    let mut mac_buf = Vec::with_capacity(1 + key_id.len() + enc_value.len() + 8);
    mac_buf.push(if operation == 0 { 0x01 } else { 0x02 });
    mac_buf.extend_from_slice(key_id);
    mac_buf.extend_from_slice(&enc_value);
    let key_data_len = (1 + key_id.len()) as u64;
    mac_buf.extend_from_slice(&key_data_len.to_be_bytes());

    let val_hmac_512 = hmac_sha512(&mac_buf, &keys.value_mac_key)
        .map_err(|e| CoreError::Crypto(format!("Value MAC HMAC error: {}", e)))?;
    let value_mac = &val_hmac_512[0..32];

    // indexMac: HMAC-SHA256(index_json, keys.index_key)
    let index_mac = hmac_sha256(&index_json, &keys.index_key)
        .map_err(|e| CoreError::Crypto(format!("Index MAC HMAC error: {}", e)))?;

    // Update LTHash160
    let base_hash = if state_hash.len() == 128 {
        state_hash.to_vec()
    } else {
        vec![0u8; 128]
    };

    let new_hash = if operation == 0 {
        LTHashAntiTampering::subtract_then_add(&base_hash, &[], &[value_mac])
            .map_err(|e| CoreError::Crypto(format!("LTHash error: {}", e)))?
    } else {
        LTHashAntiTampering::subtract_then_add(&base_hash, &[value_mac], &[])
            .map_err(|e| CoreError::Crypto(format!("LTHash error: {}", e)))?
    };

    let next_version = current_version + 1;

    // generateSnapshotMac:
    // total = [new_hash, next_version.to_be_bytes(), collection.as_bytes()]
    let mut snap_buf = Vec::with_capacity(128 + 8 + collection.len());
    snap_buf.extend_from_slice(&new_hash);
    snap_buf.extend_from_slice(&next_version.to_be_bytes());
    snap_buf.extend_from_slice(collection.as_bytes());
    let snapshot_mac = hmac_sha256(&snap_buf, &keys.snapshot_mac_key)
        .map_err(|e| CoreError::Crypto(format!("Snapshot MAC error: {}", e)))?;

    // generatePatchMac:
    // total = [snapshot_mac, value_mac, next_version.to_be_bytes(), collection.as_bytes()]
    let mut patch_buf = Vec::with_capacity(32 + 32 + 8 + collection.len());
    patch_buf.extend_from_slice(&snapshot_mac);
    patch_buf.extend_from_slice(value_mac);
    patch_buf.extend_from_slice(&next_version.to_be_bytes());
    patch_buf.extend_from_slice(collection.as_bytes());
    let patch_mac = hmac_sha256(&patch_buf, &keys.patch_mac_key)
        .map_err(|e| CoreError::Crypto(format!("Patch MAC error: {}", e)))?;

    let mut value_blob = enc_value;
    value_blob.extend_from_slice(value_mac);

    let patch = SyncdPatch {
        version: Some(SyncdVersion {
            version: Some(next_version),
        }),
        mutations: vec![SyncdMutation {
            operation: Some(operation),
            record: Some(SyncdRecord {
                index: Some(SyncdIndex {
                    blob: Some(index_mac.to_vec()),
                }),
                value: Some(SyncdValue {
                    blob: Some(value_blob),
                }),
                key_id: Some(KeyId {
                    id: Some(key_id.to_vec()),
                }),
            }),
        }],
        snapshot_mac: Some(snapshot_mac.to_vec()),
        patch_mac: Some(patch_mac.to_vec()),
        key_id: Some(KeyId {
            id: Some(key_id.to_vec()),
        }),
        ..Default::default()
    };

    Ok(SyncdPatchResult {
        patch,
        new_hash,
        next_version,
    })
}

/// Builds the outgoing WhatsApp wire `<iq>` stanza to submit an AppState patch.
pub fn build_syncd_iq(collection: &str, current_version: u64, patch: &SyncdPatch) -> BinaryNode {
    let mut patch_bytes = Vec::new();
    let _ = patch.encode(&mut patch_bytes);

    let msg_id = format!("syncd_{:X}", rand::random::<u64>());

    BinaryNode::new("iq")
        .with_attr("id", msg_id)
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("type", "set")
        .with_attr("xmlns", "w:sync:app:state")
        .with_child(
            BinaryNode::new("sync").with_child(
                BinaryNode::new("collection")
                    .with_attr("name", collection)
                    .with_attr("version", current_version.to_string())
                    .with_attr("return_snapshot", "false")
                    .with_child(BinaryNode::new("patch").with_content(patch_bytes)),
            ),
        )
}

/// Parsed collection payload from an incoming AppState sync `<iq>` response.
#[derive(Debug, Clone, Default)]
pub struct SyncdCollectionData {
    pub name: String,
    pub version: u64,
    pub has_more_patches: bool,
    pub snapshot: Option<SyncdSnapshot>,
    pub snapshot_blob_ref: Option<ExternalBlobReference>,
    pub patches: Vec<SyncdPatch>,
}

/// Builds an outgoing `<iq>` stanza to request/resync AppState collections.
pub fn build_resync_iq(
    collections: &[&str],
    states: &HashMap<String, LTHashState>,
    force_snapshot: bool,
) -> BinaryNode {
    let msg_id = format!("syncd_req_{:X}", rand::random::<u64>());
    let mut collection_nodes = Vec::new();

    for &name in collections {
        let current_version = states.get(name).map(|s| s.version).unwrap_or(0);
        let return_snapshot = force_snapshot || current_version == 0;

        collection_nodes.push(
            BinaryNode::new("collection")
                .with_attr("name", name)
                .with_attr("version", current_version.to_string())
                .with_attr(
                    "return_snapshot",
                    if return_snapshot { "true" } else { "false" },
                ),
        );
    }

    BinaryNode::new("iq")
        .with_attr("id", msg_id)
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("type", "set")
        .with_attr("xmlns", "w:sync:app:state")
        .with_child(BinaryNode::new("sync").with_children(collection_nodes))
}

/// Extracts snapshots, patches, and blob references from an AppState sync `<iq>` response.
pub fn extract_syncd_response(
    response: &BinaryNode,
) -> Result<HashMap<String, SyncdCollectionData>, CoreError> {
    let sync_node = response
        .get_child("sync")
        .ok_or_else(|| CoreError::Protocol("Syncd IQ response missing <sync> child".to_string()))?;

    let mut result = HashMap::new();

    for col_node in sync_node.get_children_by_tag("collection") {
        let name = col_node.get_attr("name").unwrap_or_default().to_string();
        if name.is_empty() {
            continue;
        }

        let version = col_node
            .get_attr("version")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);

        let has_more_patches = col_node
            .get_attr("has_more_patches")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        let mut snapshot = None;
        let mut snapshot_blob_ref = None;

        if let Some(snap_node) = col_node.get_child("snapshot") {
            if let Some(content) = snap_node.as_bytes() {
                if let Ok(snap) = SyncdSnapshot::decode(content) {
                    snapshot = Some(snap);
                } else if let Ok(blob) = ExternalBlobReference::decode(content) {
                    snapshot_blob_ref = Some(blob);
                }
            }
        }

        let mut patches = Vec::new();
        let patches_parent = col_node.get_child("patches").unwrap_or(col_node);
        for p_node in patches_parent.get_children_by_tag("patch") {
            if let Some(content) = p_node.as_bytes() {
                if let Ok(patch) = SyncdPatch::decode(content) {
                    patches.push(patch);
                }
            }
        }

        result.insert(
            name.clone(),
            SyncdCollectionData {
                name,
                version,
                has_more_patches,
                snapshot,
                snapshot_blob_ref,
                patches,
            },
        );
    }

    Ok(result)
}

/// Constructs a Mute/Unmute mutation patch for a chat.
pub fn create_mute_patch(
    jid: &str,
    mute_until_sec: Option<u64>,
    key_id: &[u8],
    key_data: &[u8],
    state_hash: &[u8],
    current_version: u64,
) -> Result<SyncdPatchResult, CoreError> {
    let is_muted = mute_until_sec.is_some();
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let action_value = SyncActionValue {
        timestamp: Some(now as i64),
        mute_action: Some(sync_action_value::MuteAction {
            muted: Some(is_muted),
            mute_end_timestamp: mute_until_sec.map(|ts| ts as i64),
            ..Default::default()
        }),
        ..Default::default()
    };

    encode_syncd_patch(
        REGULAR_HIGH,
        &["mute", jid],
        action_value,
        2, // apiVersion 2
        0, // SET
        key_id,
        key_data,
        state_hash,
        current_version,
    )
}

/// Constructs a Pin/Unpin mutation patch for a chat.
pub fn create_pin_patch(
    jid: &str,
    pin: bool,
    key_id: &[u8],
    key_data: &[u8],
    state_hash: &[u8],
    current_version: u64,
) -> Result<SyncdPatchResult, CoreError> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let action_value = SyncActionValue {
        timestamp: Some(now as i64),
        pin_action: Some(sync_action_value::PinAction { pinned: Some(pin) }),
        ..Default::default()
    };

    encode_syncd_patch(
        REGULAR_LOW,
        &["pin_v1", jid],
        action_value,
        5, // apiVersion 5
        0, // SET
        key_id,
        key_data,
        state_hash,
        current_version,
    )
}

/// Constructs an Archive/Unarchive mutation patch for a chat.
pub fn create_archive_patch(
    jid: &str,
    archive: bool,
    key_id: &[u8],
    key_data: &[u8],
    state_hash: &[u8],
    current_version: u64,
) -> Result<SyncdPatchResult, CoreError> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let action_value = SyncActionValue {
        timestamp: Some(now as i64),
        archive_chat_action: Some(sync_action_value::ArchiveChatAction {
            archived: Some(archive),
            message_range: None,
        }),
        ..Default::default()
    };

    encode_syncd_patch(
        REGULAR_LOW,
        &["archive", jid],
        action_value,
        3, // apiVersion 3
        0, // SET
        key_id,
        key_data,
        state_hash,
        current_version,
    )
}

/// Constructs a Star/Unstar mutation patch for an individual message.
pub fn create_star_patch(
    jid: &str,
    msg_id: &str,
    from_me: bool,
    star: bool,
    key_id: &[u8],
    key_data: &[u8],
    state_hash: &[u8],
    current_version: u64,
) -> Result<SyncdPatchResult, CoreError> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let action_value = SyncActionValue {
        timestamp: Some(now as i64),
        star_action: Some(sync_action_value::StarAction {
            starred: Some(star),
        }),
        ..Default::default()
    };

    let from_me_str = if from_me { "1" } else { "0" };
    encode_syncd_patch(
        REGULAR_LOW,
        &["star", jid, msg_id, from_me_str, "0"],
        action_value,
        2, // apiVersion 2
        0, // SET
        key_id,
        key_data,
        state_hash,
        current_version,
    )
}

/// Constructs a ClearChat mutation patch for a chat thread.
pub fn create_clear_chat_patch(
    jid: &str,
    key_id: &[u8],
    key_data: &[u8],
    state_hash: &[u8],
    current_version: u64,
) -> Result<SyncdPatchResult, CoreError> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let action_value = SyncActionValue {
        timestamp: Some(now as i64),
        clear_chat_action: Some(sync_action_value::ClearChatAction {
            message_range: None,
        }),
        ..Default::default()
    };

    encode_syncd_patch(
        REGULAR_HIGH,
        &["clearChat", jid],
        action_value,
        6, // apiVersion 6
        0, // SET
        key_id,
        key_data,
        state_hash,
        current_version,
    )
}

/// Constructs a DeleteChat mutation patch for a chat thread.
pub fn create_delete_chat_patch(
    jid: &str,
    key_id: &[u8],
    key_data: &[u8],
    state_hash: &[u8],
    current_version: u64,
) -> Result<SyncdPatchResult, CoreError> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let action_value = SyncActionValue {
        timestamp: Some(now as i64),
        delete_chat_action: Some(sync_action_value::DeleteChatAction {
            message_range: None,
        }),
        ..Default::default()
    };

    encode_syncd_patch(
        REGULAR_HIGH,
        &["deleteChat", jid],
        action_value,
        6, // apiVersion 6
        0, // SET
        key_id,
        key_data,
        state_hash,
        current_version,
    )
}

/// Constructs a MarkChatAsRead/Unread mutation patch for a chat.
pub fn create_mark_chat_unread_patch(
    jid: &str,
    unread: bool,
    key_id: &[u8],
    key_data: &[u8],
    state_hash: &[u8],
    current_version: u64,
) -> Result<SyncdPatchResult, CoreError> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let action_value = SyncActionValue {
        timestamp: Some(now as i64),
        mark_chat_as_read_action: Some(sync_action_value::MarkChatAsReadAction {
            read: Some(!unread),
            message_range: None,
        }),
        ..Default::default()
    };

    encode_syncd_patch(
        REGULAR_LOW,
        &["markChatAsRead", jid],
        action_value,
        3, // apiVersion 3
        0, // SET
        key_id,
        key_data,
        state_hash,
        current_version,
    )
}

/// Computes the HMAC-SHA512 value MAC for a Syncd mutation record.
pub fn generate_syncd_mac(
    operation: i32,
    data: &[u8],
    key_id: &[u8],
    key: &[u8],
) -> Result<Vec<u8>, CoreError> {
    let op_byte = if operation == 0 { 0x01 } else { 0x02 };
    let mut mac_buf = Vec::with_capacity(1 + key_id.len() + data.len() + 8);
    mac_buf.push(op_byte);
    mac_buf.extend_from_slice(key_id);
    mac_buf.extend_from_slice(data);
    let key_data_len = (1 + key_id.len()) as u64;
    mac_buf.extend_from_slice(&key_data_len.to_be_bytes());

    let val_hmac_512 = hmac_sha512(&mac_buf, key)
        .map_err(|e| CoreError::Crypto(format!("Value MAC HMAC error: {}", e)))?;
    Ok(val_hmac_512[0..32].to_vec())
}

/// Computes the HMAC-SHA256 snapshot MAC for an AppState collection state.
pub fn generate_snapshot_mac(
    lthash: &[u8],
    version: u64,
    name: &str,
    key: &[u8],
) -> Result<Vec<u8>, CoreError> {
    let mut snap_buf = Vec::with_capacity(lthash.len() + 8 + name.len());
    snap_buf.extend_from_slice(lthash);
    snap_buf.extend_from_slice(&version.to_be_bytes());
    snap_buf.extend_from_slice(name.as_bytes());
    hmac_sha256(&snap_buf, key)
        .map(|h| h.to_vec())
        .map_err(|e| CoreError::Crypto(format!("Snapshot MAC error: {}", e)))
}

/// Computes the HMAC-SHA256 patch MAC for an AppState Syncd patch.
pub fn generate_patch_mac(
    snapshot_mac: &[u8],
    value_macs: &[&[u8]],
    version: u64,
    patch_name: &str,
    key: &[u8],
) -> Result<Vec<u8>, CoreError> {
    let total_len = snapshot_mac.len()
        + value_macs.iter().map(|v| v.len()).sum::<usize>()
        + 8
        + patch_name.len();
    let mut patch_buf = Vec::with_capacity(total_len);
    patch_buf.extend_from_slice(snapshot_mac);
    for vm in value_macs {
        patch_buf.extend_from_slice(vm);
    }
    patch_buf.extend_from_slice(&version.to_be_bytes());
    patch_buf.extend_from_slice(patch_name.as_bytes());
    hmac_sha256(&patch_buf, key)
        .map(|h| h.to_vec())
        .map_err(|e| CoreError::Crypto(format!("Patch MAC error: {}", e)))
}

/// Decodes a list of Syncd mutations, verifying MACs, decrypting records, and updating LTHash160.
pub fn decode_syncd_mutations<F>(
    mutations: &[SyncdMutation],
    initial_state: &LTHashState,
    key_resolver: F,
    validate_macs: bool,
) -> Result<(LTHashState, Vec<DecodedSyncdMutation>), CoreError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let mut index_value_map = initial_state.index_value_map.clone();
    let mut add_buffs: Vec<Vec<u8>> = Vec::new();
    let mut sub_buffs: Vec<Vec<u8>> = Vec::new();
    let mut decoded_mutations = Vec::new();

    for mutation in mutations {
        let operation = mutation.operation.unwrap_or(0);
        let record = match mutation.record.as_ref() {
            Some(r) => r,
            None => continue,
        };

        let key_id_bytes = match record.key_id.as_ref().and_then(|k| k.id.as_deref()) {
            Some(id) => id,
            None => continue,
        };

        let key_data = match key_resolver(key_id_bytes) {
            Some(kd) => kd,
            None => continue,
        };

        let keys = expand_app_state_keys(&key_data)
            .map_err(|e| CoreError::Crypto(format!("Failed to expand app state keys: {}", e)))?;

        let blob = match record.value.as_ref().and_then(|v| v.blob.as_deref()) {
            Some(b) if b.len() >= 32 => b,
            _ => continue,
        };

        let enc_content = &blob[..blob.len() - 32];
        let og_value_mac = &blob[blob.len() - 32..];

        if validate_macs {
            let content_hmac =
                generate_syncd_mac(operation, enc_content, key_id_bytes, &keys.value_mac_key)?;
            if content_hmac.as_slice() != og_value_mac {
                continue;
            }
        }

        let decrypted =
            match aes_256_cbc_decrypt_prefixed_iv(enc_content, &keys.value_encryption_key) {
                Ok(d) => d,
                Err(_) => continue,
            };

        let sync_action = match SyncActionData::decode(&decrypted[..]) {
            Ok(sa) => sa,
            Err(_) => continue,
        };

        let index_bytes = sync_action.index.as_deref().unwrap_or(&[]);
        if validate_macs {
            let index_hmac = hmac_sha256(index_bytes, &keys.index_key)
                .map_err(|e| CoreError::Crypto(format!("Index HMAC error: {}", e)))?;
            if let Some(record_index_mac) = record.index.as_ref().and_then(|i| i.blob.as_deref()) {
                if index_hmac.as_slice() != record_index_mac {
                    continue;
                }
            }
        }

        let index_strings: Vec<String> = serde_json::from_slice(index_bytes).unwrap_or_default();
        let index_mac = record
            .index
            .as_ref()
            .and_then(|i| i.blob.clone())
            .unwrap_or_default();

        if operation == 1 {
            // REMOVE
            if let Some(prev) = index_value_map.remove(&index_mac) {
                sub_buffs.push(prev);
            }
        } else {
            // SET
            if let Some(prev) = index_value_map.insert(index_mac.clone(), og_value_mac.to_vec()) {
                sub_buffs.push(prev);
            }
            add_buffs.push(og_value_mac.to_vec());
        }

        decoded_mutations.push(DecodedSyncdMutation {
            operation,
            index: index_strings,
            sync_action,
            index_mac,
            value_mac: og_value_mac.to_vec(),
        });
    }

    let sub_refs: Vec<&[u8]> = sub_buffs.iter().map(|b| b.as_slice()).collect();
    let add_refs: Vec<&[u8]> = add_buffs.iter().map(|b| b.as_slice()).collect();

    let base_hash = if initial_state.hash.len() == 128 {
        initial_state.hash.as_slice()
    } else {
        &[0u8; 128]
    };

    let new_hash = LTHashAntiTampering::subtract_then_add(base_hash, &sub_refs, &add_refs)
        .map_err(|e| CoreError::Crypto(format!("LTHash calculation error: {}", e)))?;

    let new_state = LTHashState {
        version: initial_state.version,
        hash: new_hash,
        index_value_map,
    };

    Ok((new_state, decoded_mutations))
}

/// Decodes an AppState Syncd patch.
pub fn decode_syncd_patch<F>(
    patch: &SyncdPatch,
    patch_name: &str,
    initial_state: &LTHashState,
    key_resolver: F,
    validate_macs: bool,
) -> Result<(LTHashState, Vec<DecodedSyncdMutation>), CoreError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let version = patch
        .version
        .as_ref()
        .and_then(|v| v.version)
        .unwrap_or(initial_state.version + 1);

    if validate_macs {
        if let (Some(key_id), Some(snapshot_mac), Some(patch_mac)) = (
            patch.key_id.as_ref().and_then(|k| k.id.as_deref()),
            patch.snapshot_mac.as_deref(),
            patch.patch_mac.as_deref(),
        ) {
            if let Some(key_data) = key_resolver(key_id) {
                let keys = expand_app_state_keys(&key_data)
                    .map_err(|e| CoreError::Crypto(format!("Key expansion error: {}", e)))?;

                let value_macs: Vec<&[u8]> = patch
                    .mutations
                    .iter()
                    .filter_map(|m| {
                        m.record
                            .as_ref()
                            .and_then(|r| r.value.as_ref())
                            .and_then(|v| v.blob.as_deref())
                            .filter(|b| b.len() >= 32)
                            .map(|b| &b[b.len() - 32..])
                    })
                    .collect();

                let computed_patch_mac = generate_patch_mac(
                    snapshot_mac,
                    &value_macs,
                    version,
                    patch_name,
                    &keys.patch_mac_key,
                )?;

                if computed_patch_mac.as_slice() != patch_mac {
                    return Err(CoreError::Crypto("Invalid patch MAC".into()));
                }
            }
        }
    }

    let (mut state, mutations) = decode_syncd_mutations(
        &patch.mutations,
        initial_state,
        &key_resolver,
        validate_macs,
    )?;
    state.version = version;
    Ok((state, mutations))
}

/// Decodes an AppState Syncd snapshot.
pub fn decode_syncd_snapshot<F>(
    snapshot: &SyncdSnapshot,
    collection_name: &str,
    key_resolver: F,
    validate_macs: bool,
) -> Result<(LTHashState, Vec<DecodedSyncdMutation>), CoreError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let version = snapshot
        .version
        .as_ref()
        .and_then(|v| v.version)
        .unwrap_or(0);
    let initial_state = LTHashState {
        version,
        hash: vec![0u8; 128],
        index_value_map: HashMap::new(),
    };

    let syncd_mutations: Vec<SyncdMutation> = snapshot
        .records
        .iter()
        .map(|rec| SyncdMutation {
            operation: Some(0), // SET
            record: Some(rec.clone()),
        })
        .collect();

    let (mut state, mutations) = decode_syncd_mutations(
        &syncd_mutations,
        &initial_state,
        &key_resolver,
        validate_macs,
    )?;
    state.version = version;

    if validate_macs {
        if let (Some(key_id), Some(snapshot_mac)) = (
            snapshot.key_id.as_ref().and_then(|k| k.id.as_deref()),
            snapshot.mac.as_deref(),
        ) {
            if let Some(key_data) = key_resolver(key_id) {
                let keys = expand_app_state_keys(&key_data)
                    .map_err(|e| CoreError::Crypto(format!("Key expansion error: {}", e)))?;
                let computed = generate_snapshot_mac(
                    &state.hash,
                    state.version,
                    collection_name,
                    &keys.snapshot_mac_key,
                )?;
                if computed.as_slice() != snapshot_mac {
                    tracing::warn!(
                        "LTHash verification failed on snapshot for {}, continuing with partial state",
                        collection_name
                    );
                }
            }
        }
    }

    Ok((state, mutations))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_mute_patch() {
        let key_id = b"test_key_id_1234";
        let key_data = [0x42u8; 32];
        let state_hash = [0u8; 128];

        let res = create_mute_patch(
            "6283162498175@s.whatsapp.net",
            Some(1800000000),
            key_id,
            &key_data,
            &state_hash,
            0,
        )
        .expect("create_mute_patch failed");

        assert_eq!(res.next_version, 1);
        assert_eq!(res.new_hash.len(), 128);
        assert_ne!(res.new_hash, vec![0u8; 128]);

        let iq = build_syncd_iq(REGULAR_HIGH, 0, &res.patch);
        assert_eq!(iq.tag, "iq");
        assert_eq!(iq.get_attr("xmlns"), Some("w:sync:app:state"));
        assert!(iq.get_child("sync").is_some());
    }

    #[test]
    fn test_encode_pin_patch() {
        let key_id = b"test_key_id_5678";
        let key_data = [0x33u8; 32];
        let state_hash = [0u8; 128];

        let res = create_pin_patch(
            "120363391057464982@g.us",
            true,
            key_id,
            &key_data,
            &state_hash,
            5,
        )
        .expect("create_pin_patch failed");

        assert_eq!(res.next_version, 6);
        assert_eq!(res.new_hash.len(), 128);
    }

    #[test]
    fn test_encode_chat_actions_patches() {
        let key_id = b"test_key_id_9999";
        let key_data = [0x55u8; 32];
        let state_hash = [0u8; 128];

        // 1. Clear chat patch
        let clear_res = create_clear_chat_patch(
            "628123456789@s.whatsapp.net",
            key_id,
            &key_data,
            &state_hash,
            10,
        )
        .expect("create_clear_chat_patch failed");
        assert_eq!(clear_res.next_version, 11);
        assert_eq!(clear_res.new_hash.len(), 128);

        // 2. Delete chat patch
        let del_res = create_delete_chat_patch(
            "628123456789@s.whatsapp.net",
            key_id,
            &key_data,
            &state_hash,
            11,
        )
        .expect("create_delete_chat_patch failed");
        assert_eq!(del_res.next_version, 12);

        // 3. Mark unread patch
        let unread_res = create_mark_chat_unread_patch(
            "628123456789@s.whatsapp.net",
            true,
            key_id,
            &key_data,
            &state_hash,
            12,
        )
        .expect("create_mark_chat_unread_patch failed");
        assert_eq!(unread_res.next_version, 13);
    }

    #[test]
    fn test_decode_mute_patch() {
        let key_id = b"test_key_id_1234";
        let key_data = [0x42u8; 32];
        let state_hash = [0u8; 128];

        let res = create_mute_patch(
            "6283162498175@s.whatsapp.net",
            Some(1800000000),
            key_id,
            &key_data,
            &state_hash,
            0,
        )
        .expect("create_mute_patch failed");

        let initial_state = LTHashState::default();
        let key_resolver = |id: &[u8]| {
            if id == key_id {
                Some(key_data.to_vec())
            } else {
                None
            }
        };

        let (new_state, mutations) =
            decode_syncd_patch(&res.patch, REGULAR_HIGH, &initial_state, key_resolver, true)
                .expect("decode_syncd_patch failed");

        assert_eq!(new_state.version, 1);
        assert_eq!(new_state.hash, res.new_hash);
        assert_eq!(mutations.len(), 1);

        let action = mutations[0].parse_action();
        assert_eq!(
            action,
            SyncdAction::Mute {
                jid: "6283162498175@s.whatsapp.net".to_string(),
                muted: true,
                mute_end_timestamp: Some(1800000000),
            }
        );
    }

    #[test]
    fn test_decode_pin_and_star_patch() {
        let key_id = b"test_key_id_5678";
        let key_data = [0x33u8; 32];
        let state_hash = [0u8; 128];

        let pin_res = create_pin_patch(
            "120363391057464982@g.us",
            true,
            key_id,
            &key_data,
            &state_hash,
            5,
        )
        .expect("create_pin_patch failed");

        let key_resolver = |id: &[u8]| {
            if id == key_id {
                Some(key_data.to_vec())
            } else {
                None
            }
        };

        let (pin_state, pin_mutations) = decode_syncd_patch(
            &pin_res.patch,
            REGULAR_LOW,
            &LTHashState {
                version: 5,
                hash: vec![0u8; 128],
                index_value_map: HashMap::new(),
            },
            key_resolver,
            true,
        )
        .expect("decode pin patch failed");

        assert_eq!(pin_state.version, 6);
        assert_eq!(pin_mutations.len(), 1);
        assert_eq!(
            pin_mutations[0].parse_action(),
            SyncdAction::Pin {
                jid: "120363391057464982@g.us".to_string(),
                pinned: true,
            }
        );

        let star_res = create_star_patch(
            "628111111@s.whatsapp.net",
            "MSG12345",
            false,
            true,
            key_id,
            &key_data,
            &state_hash,
            6,
        )
        .expect("create_star_patch failed");

        let (star_state, star_mutations) =
            decode_syncd_patch(&star_res.patch, REGULAR_LOW, &pin_state, key_resolver, true)
                .expect("decode star patch failed");

        assert_eq!(star_state.version, 7);
        assert_eq!(star_mutations.len(), 1);
        assert_eq!(
            star_mutations[0].parse_action(),
            SyncdAction::Star {
                jid: "628111111@s.whatsapp.net".to_string(),
                msg_id: "MSG12345".to_string(),
                from_me: false,
                starred: true,
            }
        );
    }

    #[test]
    fn test_stored_lthash_state_conversion() {
        let mut index_value_map = std::collections::HashMap::new();
        index_value_map.insert(vec![1, 2, 3], vec![4, 5, 6]);
        let state = LTHashState {
            version: 5,
            hash: vec![42u8; 128],
            index_value_map,
        };

        let stored: StoredLTHashState = (&state).into();
        assert_eq!(stored.version, 5);
        assert_eq!(stored.hash_hex, hex::encode(vec![42u8; 128]));
        assert_eq!(
            stored.index_value_hex.get(&hex::encode([1, 2, 3])),
            Some(&hex::encode([4, 5, 6]))
        );

        let restored: LTHashState = stored.into();
        assert_eq!(restored.version, state.version);
        assert_eq!(restored.hash, state.hash);
        assert_eq!(restored.index_value_map, state.index_value_map);
    }

    #[test]
    fn test_build_and_extract_resync_iq() {
        let mut states = HashMap::new();
        states.insert(
            CRITICAL_BLOCK.to_string(),
            LTHashState {
                version: 3,
                ..Default::default()
            },
        );

        let iq = build_resync_iq(ALL_COLLECTIONS, &states, false);
        assert_eq!(iq.tag, "iq");
        assert_eq!(iq.get_attr("type"), Some("set"));
        assert_eq!(iq.get_attr("xmlns"), Some("w:sync:app:state"));

        let sync_node = iq.get_child("sync").expect("missing sync");
        let collections: Vec<&BinaryNode> = sync_node.get_children_by_tag("collection").collect();
        assert_eq!(collections.len(), 5);

        // critical_block has version 3 and return_snapshot=false
        let cb = collections
            .iter()
            .find(|c| c.get_attr("name") == Some(CRITICAL_BLOCK))
            .unwrap();
        assert_eq!(cb.get_attr("version"), Some("3"));
        assert_eq!(cb.get_attr("return_snapshot"), Some("false"));

        // regular_high has version 0 and return_snapshot=true
        let rh = collections
            .iter()
            .find(|c| c.get_attr("name") == Some(REGULAR_HIGH))
            .unwrap();
        assert_eq!(rh.get_attr("version"), Some("0"));
        assert_eq!(rh.get_attr("return_snapshot"), Some("true"));

        // Test extracting response
        let patch = SyncdPatch {
            version: Some(SyncdVersion { version: Some(1) }),
            ..Default::default()
        };
        let mut patch_bytes = Vec::new();
        patch.encode(&mut patch_bytes).unwrap();

        let response = BinaryNode::new("iq")
            .with_attr("type", "result")
            .with_child(
                BinaryNode::new("sync").with_child(
                    BinaryNode::new("collection")
                        .with_attr("name", REGULAR_HIGH)
                        .with_attr("version", "1")
                        .with_attr("has_more_patches", "false")
                        .with_child(
                            BinaryNode::new("patches")
                                .with_child(BinaryNode::new("patch").with_content(patch_bytes)),
                        ),
                ),
            );

        let extracted = extract_syncd_response(&response).expect("extract failed");
        assert!(extracted.contains_key(REGULAR_HIGH));
        let col = &extracted[REGULAR_HIGH];
        assert_eq!(col.version, 1);
        assert_eq!(col.patches.len(), 1);
        assert!(!col.has_more_patches);
    }
}
