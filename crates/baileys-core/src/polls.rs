use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::CoreError;
use baileys_crypto::{aes_256_gcm_decrypt, aes_256_gcm_encrypt, hmac_sha256, sha256, CryptoError};
use baileys_proto::proto::{
    message::{
        interactive_message::{
            native_flow_message::NativeFlowButton, Body, Footer, Header, NativeFlowMessage,
        },
        list_message::{ListType, Row, Section},
        poll_creation_message::Option as PollOptionProto,
        InteractiveMessage, ListMessage, PollCreationMessage, PollEncValue, PollType,
        PollUpdateMessage, PollVoteMessage,
    },
    Message, MessageKey,
};

/// Computes SHA-256 hash of a poll option text.
pub fn hash_poll_option(name: &str) -> [u8; 32] {
    sha256(name.as_bytes())
}

/// Derives the 32-byte AES-256-GCM decryption/encryption key for a WhatsApp poll vote.
/// Matches Baileys and WhatsApp Web protocol:
/// 1. sign = [poll_msg_id, poll_creator_jid, voter_jid, "Poll Vote", 0x01]
/// 2. key0 = HMAC-SHA256(key = [0u8; 32], data = poll_enc_key)
/// 3. dec_key = HMAC-SHA256(key = key0, data = sign)
pub fn derive_poll_vote_key(
    poll_enc_key: &[u8],
    poll_creator_jid: &str,
    poll_msg_id: &str,
    voter_jid: &str,
) -> Result<[u8; 32], CryptoError> {
    let mut sign = Vec::new();
    sign.extend_from_slice(poll_msg_id.as_bytes());
    sign.extend_from_slice(poll_creator_jid.as_bytes());
    sign.extend_from_slice(voter_jid.as_bytes());
    sign.extend_from_slice(b"Poll Vote");
    sign.push(1u8);

    let zeros = [0u8; 32];
    let key0 = hmac_sha256(poll_enc_key, &zeros)?;
    let dec_key = hmac_sha256(&sign, &key0)?;
    Ok(dec_key)
}

/// Decrypts a WhatsApp poll vote payload using AES-256-GCM.
/// Returns the list of selected option SHA-256 hashes.
pub fn decrypt_poll_vote(
    vote: &PollEncValue,
    poll_creator_jid: &str,
    poll_msg_id: &str,
    poll_enc_key: &[u8],
    voter_jid: &str,
) -> Result<Vec<Vec<u8>>, CoreError> {
    let enc_payload = vote
        .enc_payload
        .as_deref()
        .ok_or_else(|| CoreError::Protocol("Missing enc_payload in PollEncValue".to_string()))?;
    let enc_iv = vote
        .enc_iv
        .as_deref()
        .ok_or_else(|| CoreError::Protocol("Missing enc_iv in PollEncValue".to_string()))?;

    let dec_key = derive_poll_vote_key(poll_enc_key, poll_creator_jid, poll_msg_id, voter_jid)
        .map_err(|e| CoreError::Crypto(e.to_string()))?;

    let aad = format!("{}\0{}", poll_msg_id, voter_jid);
    let decrypted = aes_256_gcm_decrypt(enc_payload, &dec_key, enc_iv, aad.as_bytes())
        .map_err(|e| CoreError::Crypto(format!("Failed to decrypt poll vote: {}", e)))?;

    let vote_proto = <PollVoteMessage as prost::Message>::decode(&decrypted[..]).map_err(|e| {
        CoreError::Protocol(format!("Failed to decode PollVoteMessage proto: {}", e))
    })?;

    Ok(vote_proto.selected_options)
}

/// Encrypts a WhatsApp poll vote into a `PollEncValue`.
pub fn encrypt_poll_vote(
    selected_hashes: &[Vec<u8>],
    poll_creator_jid: &str,
    poll_msg_id: &str,
    poll_enc_key: &[u8],
    voter_jid: &str,
) -> Result<PollEncValue, CoreError> {
    let dec_key = derive_poll_vote_key(poll_enc_key, poll_creator_jid, poll_msg_id, voter_jid)
        .map_err(|e| CoreError::Crypto(e.to_string()))?;

    let vote_proto = PollVoteMessage {
        selected_options: selected_hashes.to_vec(),
    };

    let mut plaintext = Vec::new();
    prost::Message::encode(&vote_proto, &mut plaintext)
        .map_err(|e| CoreError::Protocol(format!("Failed to encode PollVoteMessage: {}", e)))?;

    let mut iv = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut iv);

    let aad = format!("{}\0{}", poll_msg_id, voter_jid);
    let ciphertext = aes_256_gcm_encrypt(&plaintext, &dec_key, &iv, aad.as_bytes())
        .map_err(|e| CoreError::Crypto(format!("Failed to encrypt poll vote: {}", e)))?;

    Ok(PollEncValue {
        enc_payload: Some(ciphertext),
        enc_iv: Some(iv.to_vec()),
    })
}

/// Builds a `Message` protobuf containing a `PollCreationMessage`.
pub fn build_poll_creation_message(
    question: &str,
    options: &[String],
    selectable_count: u32,
    enc_key: &[u8],
) -> Message {
    let proto_options: Vec<PollOptionProto> = options
        .iter()
        .map(|opt| PollOptionProto {
            option_name: Some(opt.clone()),
            option_hash: None,
        })
        .collect();

    let count = if selectable_count == 0 {
        1
    } else {
        selectable_count
    };

    let poll = PollCreationMessage {
        enc_key: Some(enc_key.to_vec()),
        name: Some(question.to_string()),
        options: proto_options,
        selectable_options_count: Some(count),
        poll_type: Some(PollType::Poll as i32),
        ..Default::default()
    };

    Message {
        poll_creation_message: Some(Box::new(poll)),
        ..Default::default()
    }
}

/// Builds a `Message` protobuf containing a `PollUpdateMessage`.
pub fn build_poll_update_message(poll_creation_key: MessageKey, vote_enc: PollEncValue) -> Message {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let update = PollUpdateMessage {
        poll_creation_message_key: Some(poll_creation_key),
        vote: Some(vote_enc),
        metadata: None,
        sender_timestamp_ms: Some(now),
    };

    Message {
        poll_update_message: Some(update),
        ..Default::default()
    }
}

/// Interactive button configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InteractiveButton {
    pub name: String,
    pub button_params_json: String,
}

impl InteractiveButton {
    /// Creates a quick reply button.
    pub fn quick_reply(display_text: &str, id: &str) -> Self {
        let params = serde_json::json!({
            "display_text": display_text,
            "id": id,
        });
        Self {
            name: "quick_reply".to_string(),
            button_params_json: params.to_string(),
        }
    }

    /// Creates a CTA URL button (opens a webpage).
    pub fn cta_url(display_text: &str, url: &str) -> Self {
        let params = serde_json::json!({
            "display_text": display_text,
            "url": url,
            "merchant_url": url,
        });
        Self {
            name: "cta_url".to_string(),
            button_params_json: params.to_string(),
        }
    }

    /// Creates a CTA Call button (dials a phone number).
    pub fn cta_call(display_text: &str, phone_number: &str) -> Self {
        let params = serde_json::json!({
            "display_text": display_text,
            "phone_number": phone_number,
        });
        Self {
            name: "cta_call".to_string(),
            button_params_json: params.to_string(),
        }
    }

    /// Creates a CTA Copy button (copies text/coupon to clipboard).
    pub fn cta_copy(display_text: &str, copy_code: &str) -> Self {
        let params = serde_json::json!({
            "display_text": display_text,
            "copy_code": copy_code,
        });
        Self {
            name: "cta_copy".to_string(),
            button_params_json: params.to_string(),
        }
    }
}

/// Builds an `InteractiveMessage` with native flow action buttons.
pub fn build_interactive_button_message(
    body: &str,
    buttons: &[InteractiveButton],
    header: Option<&str>,
    footer: Option<&str>,
) -> Message {
    let proto_buttons: Vec<NativeFlowButton> = buttons
        .iter()
        .map(|b| NativeFlowButton {
            name: Some(b.name.clone()),
            button_params_json: Some(b.button_params_json.clone()),
        })
        .collect();

    let native_flow = NativeFlowMessage {
        buttons: proto_buttons,
        message_params_json: None,
        message_version: Some(3),
    };

    let header_node = header.map(|h| Header {
        title: Some(h.to_string()),
        ..Default::default()
    });

    let footer_node = footer.map(|f| Footer {
        text: Some(f.to_string()),
        ..Default::default()
    });

    let interactive = InteractiveMessage {
        header: header_node.map(Box::new),
        body: Some(Body {
            text: Some(body.to_string()),
        }),
        footer: footer_node.map(Box::new),
        interactive_message: Some(
            baileys_proto::proto::message::interactive_message::InteractiveMessage::NativeFlowMessage(
                native_flow,
            ),
        ),
        ..Default::default()
    };

    crate::interactive::wrap_interactive_view_once(interactive)
}

/// Row in an interactive list section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListRow {
    pub title: String,
    pub description: Option<String>,
    pub row_id: String,
}

/// Section in an interactive list message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListSection {
    pub title: String,
    pub rows: Vec<ListRow>,
}

/// Builds a `ListMessage` with single-selection menu sections.
pub fn build_list_message(
    title: &str,
    description: &str,
    button_text: &str,
    sections: &[ListSection],
    footer: Option<&str>,
) -> Message {
    let proto_sections: Vec<Section> = sections
        .iter()
        .map(|s| {
            let rows: Vec<Row> = s
                .rows
                .iter()
                .map(|r| Row {
                    title: Some(r.title.clone()),
                    description: r.description.clone(),
                    row_id: Some(r.row_id.clone()),
                })
                .collect();
            Section {
                title: Some(s.title.clone()),
                rows,
            }
        })
        .collect();

    let list = ListMessage {
        title: Some(title.to_string()),
        description: Some(description.to_string()),
        button_text: Some(button_text.to_string()),
        list_type: Some(ListType::SingleSelect as i32),
        sections: proto_sections,
        product_list_info: None,
        footer_text: footer.map(|f| f.to_string()),
        context_info: None,
    };

    Message {
        list_message: Some(Box::new(list)),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_poll_vote_encryption_decryption_roundtrip() {
        let enc_key = [0x55u8; 32];
        let poll_msg_id = "3EB0123456789ABC";
        let poll_creator = "62812345678@s.whatsapp.net";
        let voter = "62898765432@s.whatsapp.net";

        let opt1_hash = hash_poll_option("Rust").to_vec();
        let opt2_hash = hash_poll_option("TypeScript").to_vec();
        let selected = vec![opt1_hash.clone(), opt2_hash.clone()];

        let vote_enc = encrypt_poll_vote(&selected, poll_creator, poll_msg_id, &enc_key, voter)
            .expect("Encryption failed");

        assert!(vote_enc.enc_payload.is_some());
        assert_eq!(vote_enc.enc_iv.as_ref().unwrap().len(), 12);

        let decrypted = decrypt_poll_vote(&vote_enc, poll_creator, poll_msg_id, &enc_key, voter)
            .expect("Decryption failed");

        assert_eq!(decrypted.len(), 2);
        assert_eq!(decrypted[0], opt1_hash);
        assert_eq!(decrypted[1], opt2_hash);
    }

    #[test]
    fn test_poll_creation_message_builder() {
        let enc_key = [0xAAu8; 32];
        let question = "What is your main programming language?";
        let options = vec!["Rust".to_string(), "Go".to_string(), "C++".to_string()];

        let msg = build_poll_creation_message(question, &options, 1, &enc_key);
        let poll = msg.poll_creation_message.unwrap();

        assert_eq!(poll.name.as_deref(), Some(question));
        assert_eq!(poll.options.len(), 3);
        assert_eq!(poll.options[0].option_name.as_deref(), Some("Rust"));
        assert_eq!(poll.selectable_options_count, Some(1));
        assert_eq!(poll.enc_key.as_deref(), Some(&enc_key[..]));
    }

    #[test]
    fn test_interactive_button_message_builder() {
        let buttons = vec![
            InteractiveButton::quick_reply("Option 1", "opt_1"),
            InteractiveButton::cta_url("Visit Website", "https://whatsapp.com"),
        ];

        let msg = build_interactive_button_message(
            "Welcome to Rose-Baileys!",
            &buttons,
            Some("Interactive Menu"),
            Some("Powered by Rust"),
        );

        assert!(msg.view_once_message.is_some());
        let inner = msg.view_once_message.unwrap().message.unwrap();
        assert_eq!(
            inner
                .message_context_info
                .as_ref()
                .and_then(|c| c.device_list_metadata_version),
            Some(2)
        );
        let inter = inner.interactive_message.unwrap();
        assert_eq!(
            inter.body.unwrap().text.as_deref(),
            Some("Welcome to Rose-Baileys!")
        );
        assert_eq!(
            inter.header.unwrap().title.as_deref(),
            Some("Interactive Menu")
        );
        assert_eq!(
            inter.footer.unwrap().text.as_deref(),
            Some("Powered by Rust")
        );
    }

    #[test]
    fn test_list_message_builder() {
        let sections = vec![ListSection {
            title: "Programming Languages".to_string(),
            rows: vec![
                ListRow {
                    title: "Rust".to_string(),
                    description: Some("Blazingly fast & memory efficient".to_string()),
                    row_id: "lang_rust".to_string(),
                },
                ListRow {
                    title: "TypeScript".to_string(),
                    description: Some("Typed JavaScript at Any Scale".to_string()),
                    row_id: "lang_ts".to_string(),
                },
            ],
        }];

        let msg = build_list_message(
            "Select Language",
            "Choose your primary language:",
            "View List",
            &sections,
            Some("Rose-Baileys"),
        );

        let list = msg.list_message.unwrap();
        assert_eq!(list.title.as_deref(), Some("Select Language"));
        assert_eq!(list.button_text.as_deref(), Some("View List"));
        assert_eq!(list.sections.len(), 1);
        assert_eq!(list.sections[0].rows.len(), 2);
    }
}
