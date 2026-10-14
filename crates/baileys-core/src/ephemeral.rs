//! Message Edit, Revoke and Ephemeral Disappearing Messages protocol.
//!
//! Provides builders and parsers for:
//! - Message edits (`ProtocolMessage.Type.MESSAGE_EDIT = 14`)
//! - Message revokes / deletes (`ProtocolMessage.Type.REVOKE = 0`)
//! - Chat disappearing timers (`ProtocolMessage.Type.EPHEMERAL_SETTING = 3` and Group `<ephemeral expiration="..."/>` IQ)
//! - Ephemeral message envelope wrapping and unwrapping (`FutureProofMessage`).

use baileys_binary::BinaryNode;
use baileys_proto::proto::message::protocol_message::Type as ProtocolType;
use baileys_proto::proto::message::{FutureProofMessage, ProtocolMessage};
use baileys_proto::proto::{Message, MessageKey};

/// Standard WhatsApp disappearing message durations in seconds.
pub const EPHEMERAL_OFF: u32 = 0;
pub const EPHEMERAL_24_HOURS: u32 = 86400;
pub const EPHEMERAL_7_DAYS: u32 = 604800;
pub const EPHEMERAL_90_DAYS: u32 = 7776000;

/// Parses a human duration string (e.g. "off", "0", "24h", "1d", "7d", "1w", "90d", "3m") into seconds.
pub fn parse_ephemeral_duration(s: &str) -> Option<u32> {
    let clean = s.trim().to_lowercase();
    match clean.as_str() {
        "off" | "0" | "disable" | "disabled" | "none" => Some(EPHEMERAL_OFF),
        "24h" | "1d" | "24hours" | "86400" => Some(EPHEMERAL_24_HOURS),
        "7d" | "1w" | "7days" | "604800" => Some(EPHEMERAL_7_DAYS),
        "90d" | "3m" | "90days" | "7776000" => Some(EPHEMERAL_90_DAYS),
        _ => clean.parse::<u32>().ok(),
    }
}

/// Formats ephemeral expiration seconds into human-readable label.
pub fn format_ephemeral_duration(seconds: u32) -> &'static str {
    match seconds {
        0 => "Off",
        86400 => "24 hours",
        604800 => "7 days",
        7776000 => "90 days",
        _ => "Custom",
    }
}

/// Builds a `ProtocolMessage` of type `MESSAGE_EDIT` wrapped in a `Message`.
pub fn build_edit_message(
    target_jid: &str,
    target_msg_id: &str,
    from_me: bool,
    new_text: &str,
) -> Message {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let key = MessageKey {
        remote_jid: Some(target_jid.to_string()),
        from_me: Some(from_me),
        id: Some(target_msg_id.to_string()),
        participant: None,
    };

    let edited = Message {
        conversation: Some(new_text.to_string()),
        ..Default::default()
    };

    let proto = ProtocolMessage {
        key: Some(key),
        r#type: Some(ProtocolType::MessageEdit as i32),
        edited_message: Some(Box::new(edited)),
        timestamp_ms: Some(now_ms),
        ..Default::default()
    };

    Message {
        protocol_message: Some(Box::new(proto)),
        ..Default::default()
    }
}

/// Builds a `ProtocolMessage` of type `REVOKE` (delete for everyone) wrapped in a `Message`.
pub fn build_revoke_message(
    target_jid: &str,
    target_msg_id: &str,
    from_me: bool,
    participant: Option<&str>,
) -> Message {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let key = MessageKey {
        remote_jid: Some(target_jid.to_string()),
        from_me: Some(from_me),
        id: Some(target_msg_id.to_string()),
        participant: participant.map(|p| p.to_string()),
    };

    let proto = ProtocolMessage {
        key: Some(key),
        r#type: Some(ProtocolType::Revoke as i32),
        timestamp_ms: Some(now_ms),
        ..Default::default()
    };

    Message {
        protocol_message: Some(Box::new(proto)),
        ..Default::default()
    }
}

/// Builds an ephemeral setting message for 1:1 chat disappearing timers.
/// Matches Baileys `prepareDisappearingMessageSettingContent`.
pub fn build_ephemeral_setting_message(target_jid: &str, expiration_seconds: u32) -> Message {
    let now_sec = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let key = MessageKey {
        remote_jid: Some(target_jid.to_string()),
        from_me: Some(true),
        id: None,
        participant: None,
    };

    let proto = ProtocolMessage {
        key: Some(key),
        r#type: Some(ProtocolType::EphemeralSetting as i32),
        ephemeral_expiration: Some(expiration_seconds),
        ephemeral_setting_timestamp: Some(now_sec),
        ..Default::default()
    };

    Message {
        ephemeral_message: Some(Box::new(FutureProofMessage {
            message: Some(Box::new(Message {
                protocol_message: Some(Box::new(proto)),
                ..Default::default()
            })),
        })),
        ..Default::default()
    }
}

/// Builds a group IQ node for changing group disappearing messages timer.
/// Matches Baileys `groupToggleEphemeral`.
pub fn build_group_ephemeral_iq(group_jid: &str, expiration_seconds: u32) -> BinaryNode {
    let iq_id = format!("3EB0{:X}", rand::random::<u64>());
    let content = if expiration_seconds > 0 {
        BinaryNode::new("ephemeral").with_attr("expiration", expiration_seconds.to_string())
    } else {
        BinaryNode::new("not_ephemeral")
    };

    BinaryNode::new("iq")
        .with_attr("to", group_jid)
        .with_attr("type", "set")
        .with_attr("xmlns", "w:g2")
        .with_attr("id", &iq_id)
        .with_child(content)
}

/// Wraps any WhatsApp message inside an ephemeral container.
pub fn wrap_ephemeral_message(inner: Message) -> Message {
    Message {
        ephemeral_message: Some(Box::new(FutureProofMessage {
            message: Some(Box::new(inner)),
        })),
        ..Default::default()
    }
}

/// Unwraps an ephemeral container if present, returning a reference to the inner message.
pub fn unwrap_ephemeral_message(msg: &Message) -> &Message {
    if let Some(ref eph) = msg.ephemeral_message {
        if let Some(ref inner) = eph.message {
            return unwrap_ephemeral_message(inner);
        }
    }
    msg
}

/// High-level representation of a decoded `ProtocolMessage`.
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedProtocolMessage {
    MessageEdit {
        target_chat_jid: String,
        target_msg_id: String,
        new_text: String,
        edited_message: Box<Message>,
    },
    Revoke {
        target_chat_jid: String,
        target_msg_id: String,
        participant: Option<String>,
    },
    EphemeralSetting {
        target_chat_jid: String,
        expiration: u32,
        timestamp: Option<i64>,
    },
    Other,
}

/// Inspects and parses a `ProtocolMessage` from an incoming WhatsApp `Message`.
/// Recursively inspects inside `ephemeral_message` wrappers if present.
pub fn parse_protocol_message(
    msg: &Message,
    fallback_chat_jid: &str,
) -> Option<ParsedProtocolMessage> {
    // Unnest ephemeral container if present
    let effective_msg = unwrap_ephemeral_message(msg);

    let proto = effective_msg.protocol_message.as_ref()?;
    let ptype = proto.r#type?;

    let chat_jid = proto
        .key
        .as_ref()
        .and_then(|k| k.remote_jid.as_deref())
        .unwrap_or(fallback_chat_jid)
        .to_string();

    let msg_id = proto
        .key
        .as_ref()
        .and_then(|k| k.id.as_deref())
        .unwrap_or("")
        .to_string();

    let participant = proto.key.as_ref().and_then(|k| k.participant.clone());

    if ptype == ProtocolType::MessageEdit as i32 {
        let edited = proto.edited_message.clone().unwrap_or_default();
        let new_text = crate::messages::extract_message_summary(&edited);
        return Some(ParsedProtocolMessage::MessageEdit {
            target_chat_jid: chat_jid,
            target_msg_id: msg_id,
            new_text,
            edited_message: edited,
        });
    }

    if ptype == ProtocolType::Revoke as i32 {
        return Some(ParsedProtocolMessage::Revoke {
            target_chat_jid: chat_jid,
            target_msg_id: msg_id,
            participant,
        });
    }

    if ptype == ProtocolType::EphemeralSetting as i32 {
        let exp = proto.ephemeral_expiration.unwrap_or(0);
        return Some(ParsedProtocolMessage::EphemeralSetting {
            target_chat_jid: chat_jid,
            expiration: exp,
            timestamp: proto.ephemeral_setting_timestamp,
        });
    }

    Some(ParsedProtocolMessage::Other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_and_format_ephemeral_duration() {
        assert_eq!(parse_ephemeral_duration("off"), Some(0));
        assert_eq!(parse_ephemeral_duration("0"), Some(0));
        assert_eq!(parse_ephemeral_duration("24h"), Some(86400));
        assert_eq!(parse_ephemeral_duration("1d"), Some(86400));
        assert_eq!(parse_ephemeral_duration("7d"), Some(604800));
        assert_eq!(parse_ephemeral_duration("90d"), Some(7776000));
        assert_eq!(parse_ephemeral_duration("3600"), Some(3600));

        assert_eq!(format_ephemeral_duration(0), "Off");
        assert_eq!(format_ephemeral_duration(86400), "24 hours");
        assert_eq!(format_ephemeral_duration(604800), "7 days");
        assert_eq!(format_ephemeral_duration(7776000), "90 days");
        assert_eq!(format_ephemeral_duration(3600), "Custom");
    }

    #[test]
    fn test_build_and_parse_edit_message() {
        let msg = build_edit_message(
            "120363391057464982@g.us",
            "ORIG_MSG_ID",
            true,
            "Hello edited world!",
        );
        let parsed =
            parse_protocol_message(&msg, "120363391057464982@g.us").expect("Should parse edit");

        match parsed {
            ParsedProtocolMessage::MessageEdit {
                target_chat_jid,
                target_msg_id,
                new_text,
                ..
            } => {
                assert_eq!(target_chat_jid, "120363391057464982@g.us");
                assert_eq!(target_msg_id, "ORIG_MSG_ID");
                assert_eq!(new_text, "Hello edited world!");
            }
            other => panic!("Expected MessageEdit, got {:?}", other),
        }
    }

    #[test]
    fn test_build_and_parse_revoke_message() {
        let msg = build_revoke_message(
            "120363391057464982@g.us",
            "MSG_TO_DELETE",
            true,
            Some("admin@s.whatsapp.net"),
        );
        let parsed =
            parse_protocol_message(&msg, "120363391057464982@g.us").expect("Should parse revoke");

        match parsed {
            ParsedProtocolMessage::Revoke {
                target_chat_jid,
                target_msg_id,
                participant,
            } => {
                assert_eq!(target_chat_jid, "120363391057464982@g.us");
                assert_eq!(target_msg_id, "MSG_TO_DELETE");
                assert_eq!(participant.as_deref(), Some("admin@s.whatsapp.net"));
            }
            other => panic!("Expected Revoke, got {:?}", other),
        }
    }

    #[test]
    fn test_build_and_parse_ephemeral_setting_message() {
        let msg = build_ephemeral_setting_message("6283162498175@s.whatsapp.net", 86400);
        let parsed = parse_protocol_message(&msg, "6283162498175@s.whatsapp.net")
            .expect("Should parse ephemeral setting");

        match parsed {
            ParsedProtocolMessage::EphemeralSetting {
                target_chat_jid,
                expiration,
                timestamp,
            } => {
                assert_eq!(target_chat_jid, "6283162498175@s.whatsapp.net");
                assert_eq!(expiration, 86400);
                assert!(timestamp.is_some());
            }
            other => panic!("Expected EphemeralSetting, got {:?}", other),
        }
    }

    #[test]
    fn test_build_group_ephemeral_iq() {
        let node_on = build_group_ephemeral_iq("120363391057464982@g.us", 604800);
        assert_eq!(node_on.tag, "iq");
        assert_eq!(node_on.get_attr("to"), Some("120363391057464982@g.us"));
        assert_eq!(node_on.get_attr("xmlns"), Some("w:g2"));
        let eph_child = node_on
            .get_child("ephemeral")
            .expect("ephemeral tag expected");
        assert_eq!(eph_child.get_attr("expiration"), Some("604800"));

        let node_off = build_group_ephemeral_iq("120363391057464982@g.us", 0);
        assert!(node_off.get_child("not_ephemeral").is_some());
    }

    #[test]
    fn test_wrap_and_unwrap_ephemeral_message() {
        let inner = Message {
            conversation: Some("Disappearing message text".to_string()),
            ..Default::default()
        };
        let wrapped = wrap_ephemeral_message(inner);
        assert!(wrapped.ephemeral_message.is_some());

        let unwrapped = unwrap_ephemeral_message(&wrapped);
        assert_eq!(
            unwrapped.conversation.as_deref(),
            Some("Disappearing message text")
        );
    }
}
