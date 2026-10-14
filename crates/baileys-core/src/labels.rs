use baileys_proto::proto::{sync_action_value, SyncActionValue};
use std::time::SystemTime;

use crate::syncd::{encode_syncd_patch, SyncdPatchResult, REGULAR_LOW};
use crate::CoreError;

/// Standard WhatsApp Business label color palette (indices 0..19).
pub const LABEL_COLORS: [&str; 20] = [
    "#FF8000", "#FF4000", "#FF0000", "#D0004B", "#990080", "#7000B0", "#4B00D0", "#1C00E0",
    "#0028E0", "#0055D0", "#007CB0", "#00A080", "#00C040", "#28D000", "#70D000", "#A0D000",
    "#D0D000", "#D0A000", "#786850", "#483828",
];

/// Returns the hex color string for a given palette index (clamped to 0..19).
pub fn get_label_color_hex(color_index: i32) -> &'static str {
    let idx = color_index.clamp(0, (LABEL_COLORS.len() - 1) as i32) as usize;
    LABEL_COLORS[idx]
}

/// Constructs an AppState Syncd patch to create, edit, or delete a WhatsApp Business label.
pub fn create_label_edit_patch(
    label_id: &str,
    name: &str,
    color: i32,
    predefined_id: Option<i32>,
    deleted: bool,
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
        label_edit_action: Some(sync_action_value::LabelEditAction {
            name: Some(name.to_string()),
            color: Some(color),
            predefined_id,
            deleted: Some(deleted),
            order_index: None,
            is_active: Some(true),
            r#type: None,
            is_immutable: None,
            mute_end_time_ms: None,
        }),
        ..Default::default()
    };

    encode_syncd_patch(
        REGULAR_LOW,
        &["label_edit", label_id],
        action_value,
        3, // apiVersion 3
        0, // SET
        key_id,
        key_data,
        state_hash,
        current_version,
    )
}

/// Constructs an AppState Syncd patch to associate or disassociate a label with a chat.
pub fn create_chat_label_patch(
    label_id: &str,
    chat_jid: &str,
    labeled: bool,
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
        label_association_action: Some(sync_action_value::LabelAssociationAction {
            labeled: Some(labeled),
            ..Default::default()
        }),
        ..Default::default()
    };

    encode_syncd_patch(
        REGULAR_LOW,
        &["label_jid", label_id, chat_jid],
        action_value,
        3, // apiVersion 3
        0, // SET
        key_id,
        key_data,
        state_hash,
        current_version,
    )
}

/// Constructs an AppState Syncd patch to associate or disassociate a label with an individual message.
pub fn create_message_label_patch(
    label_id: &str,
    chat_jid: &str,
    message_id: &str,
    labeled: bool,
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
        label_association_action: Some(sync_action_value::LabelAssociationAction {
            labeled: Some(labeled),
            ..Default::default()
        }),
        ..Default::default()
    };

    encode_syncd_patch(
        REGULAR_LOW,
        &["label_message_jid", label_id, chat_jid, message_id],
        action_value,
        3, // apiVersion 3
        0, // SET
        key_id,
        key_data,
        state_hash,
        current_version,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_label_palette_hex() {
        assert_eq!(get_label_color_hex(0), "#FF8000");
        assert_eq!(get_label_color_hex(2), "#FF0000");
        assert_eq!(get_label_color_hex(19), "#483828");
        // Out-of-bounds clamped
        assert_eq!(get_label_color_hex(99), "#483828");
        assert_eq!(get_label_color_hex(-1), "#FF8000");
    }

    #[test]
    fn test_create_label_edit_patch() {
        let key_id = b"test_key_id_5678";
        let key_data = [0x55u8; 32];
        let state_hash = [0u8; 128];

        let res = create_label_edit_patch(
            "1",
            "VIP Customer",
            3,
            Some(1),
            false,
            key_id,
            &key_data,
            &state_hash,
            0,
        )
        .expect("create_label_edit_patch failed");

        assert_eq!(res.next_version, 1);
        assert_eq!(res.new_hash.len(), 128);
    }

    #[test]
    fn test_create_chat_label_patch() {
        let key_id = b"test_key_id_5678";
        let key_data = [0x55u8; 32];
        let state_hash = [0u8; 128];

        let res = create_chat_label_patch(
            "1",
            "628123456789@s.whatsapp.net",
            true,
            key_id,
            &key_data,
            &state_hash,
            1,
        )
        .expect("create_chat_label_patch failed");

        assert_eq!(res.next_version, 2);
    }

    #[test]
    fn test_create_message_label_patch() {
        let key_id = b"test_key_id_5678";
        let key_data = [0x55u8; 32];
        let state_hash = [0u8; 128];

        let res = create_message_label_patch(
            "1",
            "628123456789@s.whatsapp.net",
            "MSG_4242",
            false,
            key_id,
            &key_data,
            &state_hash,
            2,
        )
        .expect("create_message_label_patch failed");

        assert_eq!(res.next_version, 3);
    }
}
