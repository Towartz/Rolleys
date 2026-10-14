//! High-level, strongly-typed protobuf message builders.
//!
//! Construct valid WhatsApp Web protobuf `Message` payloads for text, media,
//! interactive buttons, polls, calendar events, call logs, reactions, and locations.

use baileys_proto::proto::message::interactive_message::{
    native_flow_message::NativeFlowButton, Body as InteractiveBody, CarouselMessage,
    Footer as InteractiveFooter, Header as InteractiveHeader, NativeFlowMessage,
};
use baileys_proto::proto::message::{
    buttons_message::{button::ButtonText, Button},
    AlbumMessage, AudioMessage, ButtonsMessage, ContactMessage, ContactsArrayMessage,
    DocumentMessage, EventMessage, ExtendedTextMessage, FutureProofMessage, ImageMessage,
    InteractiveMessage, LiveLocationMessage, LocationMessage, PollCreationMessage, ReactionMessage,
    StickerMessage, VideoMessage,
};
use baileys_proto::proto::{ContextInfo, Message, MessageKey};

/// Builds a standard conversation text message.
pub fn build_text(text: impl Into<String>) -> Message {
    Message {
        conversation: Some(text.into()),
        ..Default::default()
    }
}

/// Builds an extended text message with optional quote/reply context and link preview URL.
pub fn build_extended_text(
    text: impl Into<String>,
    quoted_key: Option<&MessageKey>,
    quoted_msg: Option<&Message>,
    matched_text: Option<&str>,
) -> Message {
    let mut ext = ExtendedTextMessage {
        text: Some(text.into()),
        matched_text: matched_text.map(|s| s.to_string()),
        ..Default::default()
    };

    if quoted_key.is_some() || quoted_msg.is_some() {
        ext.context_info = Some(Box::new(ContextInfo {
            stanza_id: quoted_key.and_then(|k| k.id.clone()),
            participant: quoted_key
                .and_then(|k| k.participant.clone().or_else(|| k.remote_jid.clone())),
            quoted_message: quoted_msg.cloned().map(Box::new),
            ..Default::default()
        }));
    }

    Message {
        extended_text_message: Some(Box::new(ext)),
        ..Default::default()
    }
}

/// Builds an image message.
pub fn build_image(
    url: Option<&str>,
    caption: Option<&str>,
    mimetype: &str,
    file_sha256: Vec<u8>,
    file_length: u64,
    media_key: Vec<u8>,
    direct_path: Option<&str>,
) -> Message {
    Message {
        image_message: Some(Box::new(ImageMessage {
            url: url.map(|s| s.to_string()),
            caption: caption.map(|s| s.to_string()),
            mimetype: Some(mimetype.to_string()),
            file_sha256: Some(file_sha256),
            file_length: Some(file_length),
            media_key: Some(media_key),
            direct_path: direct_path.map(|s| s.to_string()),
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// Builds a video message (or GIF playback).
pub fn build_video(
    url: Option<&str>,
    caption: Option<&str>,
    mimetype: &str,
    file_sha256: Vec<u8>,
    file_length: u64,
    media_key: Vec<u8>,
    direct_path: Option<&str>,
    gif_playback: bool,
    seconds: u32,
) -> Message {
    Message {
        video_message: Some(Box::new(VideoMessage {
            url: url.map(|s| s.to_string()),
            caption: caption.map(|s| s.to_string()),
            mimetype: Some(mimetype.to_string()),
            file_sha256: Some(file_sha256),
            file_length: Some(file_length),
            media_key: Some(media_key),
            direct_path: direct_path.map(|s| s.to_string()),
            gif_playback: Some(gif_playback),
            seconds: Some(seconds),
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// Builds an audio message or push-to-talk (PTT) voice note.
pub fn build_audio(
    url: Option<&str>,
    mimetype: &str,
    file_sha256: Vec<u8>,
    file_length: u64,
    media_key: Vec<u8>,
    direct_path: Option<&str>,
    ptt: bool,
    seconds: u32,
) -> Message {
    Message {
        audio_message: Some(Box::new(AudioMessage {
            url: url.map(|s| s.to_string()),
            mimetype: Some(mimetype.to_string()),
            file_sha256: Some(file_sha256),
            file_length: Some(file_length),
            media_key: Some(media_key),
            direct_path: direct_path.map(|s| s.to_string()),
            ptt: Some(ptt),
            seconds: Some(seconds),
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// Builds a sticker message.
pub fn build_sticker(
    url: Option<&str>,
    file_sha256: Vec<u8>,
    file_length: u64,
    media_key: Vec<u8>,
    direct_path: Option<&str>,
    is_animated: bool,
) -> Message {
    Message {
        sticker_message: Some(Box::new(StickerMessage {
            url: url.map(|s| s.to_string()),
            mimetype: Some("image/webp".to_string()),
            file_sha256: Some(file_sha256),
            file_length: Some(file_length),
            media_key: Some(media_key),
            direct_path: direct_path.map(|s| s.to_string()),
            is_animated: Some(is_animated),
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// Builds a document message.
pub fn build_document(
    url: Option<&str>,
    filename: &str,
    caption: Option<&str>,
    mimetype: &str,
    file_sha256: Vec<u8>,
    file_length: u64,
    media_key: Vec<u8>,
    direct_path: Option<&str>,
) -> Message {
    Message {
        document_message: Some(Box::new(DocumentMessage {
            url: url.map(|s| s.to_string()),
            file_name: Some(filename.to_string()),
            caption: caption.map(|s| s.to_string()),
            mimetype: Some(mimetype.to_string()),
            file_sha256: Some(file_sha256),
            file_length: Some(file_length),
            media_key: Some(media_key),
            direct_path: direct_path.map(|s| s.to_string()),
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// Builds an emoji reaction targeting a previous message.
pub fn build_reaction(target_key: &MessageKey, emoji: &str) -> Message {
    Message {
        reaction_message: Some(ReactionMessage {
            key: Some(target_key.clone()),
            text: Some(emoji.to_string()),
            sender_timestamp_ms: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64,
            ),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Builds a location message.
pub fn build_location(lat: f64, lon: f64, name: Option<&str>, address: Option<&str>) -> Message {
    Message {
        location_message: Some(Box::new(LocationMessage {
            degrees_latitude: Some(lat),
            degrees_longitude: Some(lon),
            name: name.map(|s| s.to_string()),
            address: address.map(|s| s.to_string()),
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// Builds a real-time live location message.
pub fn build_live_location(
    lat: f64,
    lon: f64,
    caption: Option<&str>,
    accuracy_in_meters: Option<u32>,
) -> Message {
    Message {
        live_location_message: Some(Box::new(LiveLocationMessage {
            degrees_latitude: Some(lat),
            degrees_longitude: Some(lon),
            caption: caption.map(|s| s.to_string()),
            accuracy_in_meters,
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// Builds a single contact (vCard) message.
pub fn build_contact(display_name: &str, vcard: &str) -> Message {
    Message {
        contact_message: Some(Box::new(ContactMessage {
            display_name: Some(display_name.to_string()),
            vcard: Some(vcard.to_string()),
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// Builds a multiple contacts array message.
pub fn build_contacts_array(display_name: &str, contacts: Vec<ContactMessage>) -> Message {
    Message {
        contacts_array_message: Some(Box::new(ContactsArrayMessage {
            display_name: Some(display_name.to_string()),
            contacts,
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// Builds a poll creation message.
pub fn build_poll(name: &str, option_titles: &[&str], selectable_count: u32) -> Message {
    use baileys_proto::proto::message::poll_creation_message::Option as PollOption;

    let options = option_titles
        .iter()
        .map(|&opt| PollOption {
            option_name: Some(opt.to_string()),
            option_hash: None,
        })
        .collect();

    Message {
        poll_creation_message: Some(Box::new(PollCreationMessage {
            name: Some(name.to_string()),
            options,
            selectable_options_count: Some(selectable_count),
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// Builds quick-reply buttons message.
pub fn build_quick_reply_buttons(
    text: &str,
    footer: Option<&str>,
    button_labels: &[&str],
) -> Message {
    let buttons = button_labels
        .iter()
        .enumerate()
        .map(|(i, &label)| Button {
            button_id: Some(format!("btn_{}", i + 1)),
            button_text: Some(ButtonText {
                display_text: Some(label.to_string()),
            }),
            r#type: Some(1), // Response
            native_flow_info: None,
        })
        .collect();

    Message {
        buttons_message: Some(Box::new(ButtonsMessage {
            content_text: Some(text.to_string()),
            footer_text: footer.map(|s| s.to_string()),
            buttons,
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// Builds a calendar event message (`EventMessage`) wrapped in a `Message`.
pub fn build_event_message(
    name: &str,
    description: Option<&str>,
    start_time: i64,
    end_time: Option<i64>,
    location_name: Option<&str>,
    join_link: Option<&str>,
    is_canceled: bool,
) -> Message {
    let loc = location_name.map(|loc_str| {
        Box::new(LocationMessage {
            name: Some(loc_str.to_string()),
            ..Default::default()
        })
    });

    let event = EventMessage {
        name: Some(name.to_string()),
        description: description.map(|s| s.to_string()),
        start_time: Some(start_time),
        end_time,
        location: loc,
        join_link: join_link.map(|s| s.to_string()),
        is_canceled: Some(is_canceled),
        ..Default::default()
    };

    Message {
        event_message: Some(Box::new(event)),
        ..Default::default()
    }
}

/// Constructs a `NativeFlowButton` with button name and JSON parameters.
pub fn build_native_flow_button(name: &str, button_params_json: &str) -> NativeFlowButton {
    NativeFlowButton {
        name: Some(name.to_string()),
        button_params_json: Some(button_params_json.to_string()),
    }
}

/// Convenience builder for a Call-To-Action URL button.
pub fn build_cta_url_button(
    display_text: &str,
    url: &str,
    merchant_url: Option<&str>,
) -> NativeFlowButton {
    let params = serde_json::json!({
        "display_text": display_text,
        "url": url,
        "merchant_url": merchant_url.unwrap_or(url)
    });
    build_native_flow_button("cta_url", &params.to_string())
}

/// Convenience builder for a Call-To-Action Phone Call button.
pub fn build_cta_call_button(display_text: &str, phone_number: &str) -> NativeFlowButton {
    let params = serde_json::json!({
        "display_text": display_text,
        "phone_number": phone_number
    });
    build_native_flow_button("cta_call", &params.to_string())
}

/// Convenience builder for a Call-To-Action Copy Code button.
pub fn build_cta_copy_button(display_text: &str, copy_code: &str) -> NativeFlowButton {
    let params = serde_json::json!({
        "display_text": display_text,
        "copy_code": copy_code
    });
    build_native_flow_button("cta_copy", &params.to_string())
}

/// Convenience builder for a Quick Reply button.
pub fn build_quick_reply_button(display_text: &str, id: &str) -> NativeFlowButton {
    let params = serde_json::json!({
        "display_text": display_text,
        "id": id
    });
    build_native_flow_button("quick_reply", &params.to_string())
}

/// Builds an `InteractiveMessage` with native flow buttons, with optional auto-futureproof container wrapping.
pub fn build_interactive_message(
    header_title: Option<&str>,
    body_text: &str,
    footer_text: Option<&str>,
    buttons: Vec<NativeFlowButton>,
    auto_futureproof: bool,
) -> Message {
    let header = header_title.map(|t| {
        Box::new(InteractiveHeader {
            title: Some(t.to_string()),
            has_media_attachment: Some(false),
            ..Default::default()
        })
    });

    let body = Some(InteractiveBody {
        text: Some(body_text.to_string()),
    });

    let footer = footer_text.map(|f| {
        Box::new(InteractiveFooter {
            text: Some(f.to_string()),
            has_media_attachment: Some(false),
            ..Default::default()
        })
    });

    let native_flow = NativeFlowMessage {
        buttons,
        message_params_json: None,
        message_version: Some(3),
    };

    let interactive = InteractiveMessage {
        header,
        body,
        footer,
        interactive_message: Some(
            baileys_proto::proto::message::interactive_message::InteractiveMessage::NativeFlowMessage(
                native_flow,
            ),
        ),
        ..Default::default()
    };

    let msg = Message {
        interactive_message: Some(Box::new(interactive)),
        ..Default::default()
    };

    if auto_futureproof {
        Message {
            view_once_message: Some(Box::new(FutureProofMessage {
                message: Some(Box::new(msg)),
            })),
            ..Default::default()
        }
    } else {
        msg
    }
}

/// Builds a Carousel multi-card interactive message.
pub fn build_carousel_message(
    body_text: &str,
    cards: Vec<InteractiveMessage>,
    auto_futureproof: bool,
) -> Message {
    let carousel = CarouselMessage {
        cards,
        message_version: Some(1),
        carousel_card_type: Some(1), // HSCROLL_CARDS
    };

    let interactive = InteractiveMessage {
        body: Some(InteractiveBody {
            text: Some(body_text.to_string()),
        }),
        interactive_message: Some(
            baileys_proto::proto::message::interactive_message::InteractiveMessage::CarouselMessage(
                carousel,
            ),
        ),
        ..Default::default()
    };

    let msg = Message {
        interactive_message: Some(Box::new(interactive)),
        ..Default::default()
    };

    if auto_futureproof {
        Message {
            view_once_message: Some(Box::new(FutureProofMessage {
                message: Some(Box::new(msg)),
            })),
            ..Default::default()
        }
    } else {
        msg
    }
}

/// Builds an `AlbumMessage` for grouping multiple media items.
pub fn build_album_message(expected_image_count: u32, expected_video_count: u32) -> Message {
    let album = AlbumMessage {
        expected_image_count: Some(expected_image_count),
        expected_video_count: Some(expected_video_count),
        context_info: None,
    };

    Message {
        album_message: Some(Box::new(album)),
        ..Default::default()
    }
}

/// Builds a message that replies to a forged quote from `fake_sender_jid` with `fake_quoted_text`.
pub fn build_fake_quoted_message(
    reply_text: &str,
    fake_sender_jid: &str,
    fake_quoted_text: &str,
    chat_jid: Option<&str>,
) -> Message {
    let fake_msg_id = format!("3EB0{:X}", rand::random::<u64>());
    let quoted_inner = Message {
        conversation: Some(fake_quoted_text.to_string()),
        ..Default::default()
    };
    let context_info = ContextInfo {
        stanza_id: Some(fake_msg_id),
        participant: Some(fake_sender_jid.to_string()),
        quoted_message: Some(Box::new(quoted_inner)),
        remote_jid: chat_jid.map(|s| s.to_string()),
        ..Default::default()
    };
    let ext = ExtendedTextMessage {
        text: Some(reply_text.to_string()),
        context_info: Some(Box::new(context_info)),
        ..Default::default()
    };
    Message {
        extended_text_message: Some(Box::new(ext)),
        ..Default::default()
    }
}

/// Builds a Voice Note (PTT) `Message` protobuf with realistic audio waveform.
pub fn build_voice_note_message(
    url: Option<String>,
    direct_path: Option<String>,
    media_key: Vec<u8>,
    file_sha256: Vec<u8>,
    file_enc_sha256: Vec<u8>,
    file_length: u64,
    seconds: u32,
    waveform: Option<&[u8]>,
    mimetype: Option<&str>,
) -> Message {
    let default_waveform = vec![
        0, 15, 35, 70, 95, 60, 40, 80, 100, 85, 55, 30, 65, 40, 10, 0,
    ];
    let wf = waveform.map(|w| w.to_vec()).unwrap_or(default_waveform);
    Message {
        audio_message: Some(Box::new(baileys_proto::proto::message::AudioMessage {
            url,
            mimetype: Some(mimetype.unwrap_or("audio/ogg; codecs=opus").to_string()),
            file_sha256: Some(file_sha256),
            file_length: Some(file_length),
            seconds: Some(seconds),
            ptt: Some(true),
            media_key: Some(media_key),
            file_enc_sha256: Some(file_enc_sha256),
            direct_path,
            waveform: Some(wf),
            ..Default::default()
        })),
        ..Default::default()
    }
}

pub use crate::rich_messages::wrap_view_once;

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message as ProstMessage;

    #[test]
    fn test_build_text_roundtrip() {
        let msg = build_text("Testing message builder");
        let mut buf = Vec::new();
        msg.encode(&mut buf).unwrap();

        let decoded = Message::decode(&buf[..]).unwrap();
        assert_eq!(
            decoded.conversation.as_deref(),
            Some("Testing message builder")
        );
    }

    #[test]
    fn test_build_poll_roundtrip() {
        let msg = build_poll("What's for dinner?", &["Pizza", "Sushi", "Tacos"], 1);
        let poll = msg
            .poll_creation_message
            .expect("poll_creation_message present");
        assert_eq!(poll.name.as_deref(), Some("What's for dinner?"));
        assert_eq!(poll.options.len(), 3);
        assert_eq!(poll.selectable_options_count, Some(1));
    }

    #[test]
    fn test_build_reaction_roundtrip() {
        let key = MessageKey {
            remote_jid: Some("123@s.whatsapp.net".to_string()),
            from_me: Some(false),
            id: Some("MSG123".to_string()),
            participant: None,
        };
        let msg = build_reaction(&key, "🔥");
        let react = msg.reaction_message.expect("reaction_message present");
        assert_eq!(react.text.as_deref(), Some("🔥"));
        assert_eq!(react.key.unwrap().id.as_deref(), Some("MSG123"));
    }

    #[test]
    fn test_build_interactive_native_flow() {
        let btn1 = build_cta_url_button("Open Docs", "https://example.com", None);
        let btn2 = build_quick_reply_button("Confirm", "btn_yes");
        let msg = build_interactive_message(
            Some("Interactive Title"),
            "Interactive body text",
            Some("Footer Note"),
            vec![btn1, btn2],
            true, // auto-futureproof
        );

        assert!(msg.view_once_message.is_some());
        let inner = msg.view_once_message.unwrap().message.unwrap();
        let inter = inner
            .interactive_message
            .expect("interactive_message present");
        assert_eq!(
            inter.header.unwrap().title.as_deref(),
            Some("Interactive Title")
        );
        assert_eq!(
            inter.body.unwrap().text.as_deref(),
            Some("Interactive body text")
        );
        assert_eq!(inter.footer.unwrap().text.as_deref(), Some("Footer Note"));
    }

    #[test]
    fn test_build_event_and_album() {
        let ev = build_event_message(
            "Project Demo",
            Some("Live walkthrough of Rose-Baileys"),
            1700000000,
            Some(1700003600),
            Some("Google Meet"),
            Some("https://meet.google.com/xyz"),
            false,
        );
        let ev_inner = ev.event_message.expect("event_message present");
        assert_eq!(ev_inner.name.as_deref(), Some("Project Demo"));
        assert_eq!(ev_inner.is_canceled, Some(false));

        let alb = build_album_message(3, 1);
        let alb_inner = alb.album_message.expect("album_message present");
        assert_eq!(alb_inner.expected_image_count, Some(3));
        assert_eq!(alb_inner.expected_video_count, Some(1));
    }

    #[test]
    fn test_build_contact_and_live_location() {
        let c = build_contact(
            "Alice Smith",
            "BEGIN:VCARD\nVERSION:3.0\nFN:Alice Smith\nEND:VCARD",
        );
        let c_inner = c.contact_message.as_ref().expect("contact_message present");
        assert_eq!(c_inner.display_name.as_deref(), Some("Alice Smith"));

        let loc = build_live_location(37.7749, -122.4194, Some("San Francisco"), Some(10));
        let loc_inner = loc.live_location_message.expect("live_location present");
        assert_eq!(loc_inner.degrees_latitude, Some(37.7749));
        assert_eq!(loc_inner.caption.as_deref(), Some("San Francisco"));

        let wrapped = wrap_view_once(c, true);
        assert!(wrapped.view_once_message_v2.is_some());
    }

    #[test]
    fn test_build_fake_quote_and_voice_note() {
        let fq = build_fake_quoted_message(
            "Haha that's crazy",
            "628111111111@s.whatsapp.net",
            "I will give you $1,000,000 today!",
            Some("120363000000000000@g.us"),
        );
        let ext = fq.extended_text_message.expect("extended_text_message");
        assert_eq!(ext.text.as_deref(), Some("Haha that's crazy"));
        let ctx = ext.context_info.expect("context_info");
        assert_eq!(
            ctx.participant.as_deref(),
            Some("628111111111@s.whatsapp.net")
        );
        let q_msg = ctx.quoted_message.expect("quoted_message");
        assert_eq!(
            q_msg.conversation.as_deref(),
            Some("I will give you $1,000,000 today!")
        );

        let vn = build_voice_note_message(
            Some("https://mms.whatsapp.net/v/aud.enc".into()),
            Some("/v/aud.enc".into()),
            vec![1; 32],
            vec![2; 32],
            vec![3; 32],
            65536,
            12,
            None,
            Some("audio/ogg; codecs=opus"),
        );
        let aud = vn.audio_message.expect("audio_message");
        assert_eq!(aud.ptt, Some(true));
        assert_eq!(aud.seconds, Some(12));
        assert!(aud.waveform.is_some());
        assert_eq!(aud.mimetype.as_deref(), Some("audio/ogg; codecs=opus"));
    }
}
