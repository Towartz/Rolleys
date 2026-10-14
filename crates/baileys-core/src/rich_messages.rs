//! Rich Messages Protocol for WhatsApp.
//!
//! Provides builders and utilities for:
//! - Static Location messages (`LocationMessage`)
//! - Live Geolocation messages (`LiveLocationMessage`)
//! - Single Contact & vCard messages (`ContactMessage`)
//! - Multiple Contacts Array messages (`ContactsArrayMessage`)
//! - Group Invite preview messages (`GroupInviteMessage`)
//! - View-Once ephemeral media envelope wrapping & unwrapping (`view_once_message` v1 & v2).

use baileys_proto::proto::message::{
    AudioMessage, ContactMessage, ContactsArrayMessage, DocumentMessage, FutureProofMessage,
    GroupInviteMessage, LiveLocationMessage, LocationMessage, StickerMessage, VideoMessage,
};
use baileys_proto::proto::Message;

/// Builds a static `LocationMessage` wrapped in a `Message`.
pub fn build_location_message(
    latitude: f64,
    longitude: f64,
    name: Option<&str>,
    address: Option<&str>,
    url: Option<&str>,
    jpeg_thumbnail: Option<Vec<u8>>,
) -> Message {
    let loc = LocationMessage {
        degrees_latitude: Some(latitude),
        degrees_longitude: Some(longitude),
        name: name.map(|s| s.to_string()),
        address: address.map(|s| s.to_string()),
        url: url.map(|s| s.to_string()),
        is_live: Some(false),
        accuracy_in_meters: None,
        speed_in_mps: None,
        degrees_clockwise_from_magnetic_north: None,
        comment: None,
        jpeg_thumbnail,
        context_info: None,
    };

    Message {
        location_message: Some(Box::new(loc)),
        ..Default::default()
    }
}

/// Builds a real-time `LiveLocationMessage` wrapped in a `Message`.
pub fn build_live_location_message(
    latitude: f64,
    longitude: f64,
    accuracy_in_meters: Option<u32>,
    speed_in_mps: Option<f32>,
    degrees_clockwise: Option<u32>,
    caption: Option<&str>,
    sequence_number: Option<i64>,
    time_offset: Option<u32>,
    jpeg_thumbnail: Option<Vec<u8>>,
) -> Message {
    let live = LiveLocationMessage {
        degrees_latitude: Some(latitude),
        degrees_longitude: Some(longitude),
        accuracy_in_meters,
        speed_in_mps,
        degrees_clockwise_from_magnetic_north: degrees_clockwise,
        caption: caption.map(|s| s.to_string()),
        sequence_number,
        time_offset,
        jpeg_thumbnail,
        context_info: None,
    };

    Message {
        live_location_message: Some(Box::new(live)),
        ..Default::default()
    }
}

/// Generates a standard vCard 3.0 string for contact cards.
pub fn build_vcard_string(
    display_name: &str,
    phone_number: &str,
    organization: Option<&str>,
) -> String {
    let clean_phone: String = phone_number
        .trim_start_matches('+')
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();

    let mut vcard = format!(
        "BEGIN:VCARD\r\nVERSION:3.0\r\nN:;{};;;\r\nFN:{}\r\n",
        display_name, display_name
    );

    if let Some(org) = organization {
        if !org.trim().is_empty() {
            vcard.push_str(&format!("ORG:{}\r\n", org));
        }
    }

    vcard.push_str(&format!(
        "TEL;type=CELL;type=VOICE;waid={}:{}\r\nEND:VCARD",
        clean_phone, phone_number
    ));

    vcard
}

/// Builds a single contact card (`ContactMessage`) wrapped in a `Message`.
pub fn build_contact_message(display_name: &str, vcard: &str) -> Message {
    let con = ContactMessage {
        display_name: Some(display_name.to_string()),
        vcard: Some(vcard.to_string()),
        ..Default::default()
    };

    Message {
        contact_message: Some(Box::new(con)),
        ..Default::default()
    }
}

/// Builds a multi-contact card (`ContactsArrayMessage`) wrapped in a `Message`.
pub fn build_contacts_array_message(display_name: &str, contacts: Vec<ContactMessage>) -> Message {
    let arr = ContactsArrayMessage {
        display_name: Some(display_name.to_string()),
        contacts,
        context_info: None,
    };

    Message {
        contacts_array_message: Some(Box::new(arr)),
        ..Default::default()
    }
}

/// Builds a `GroupInviteMessage` wrapped in a `Message`.
pub fn build_group_invite_message(
    group_jid: &str,
    invite_code: &str,
    invite_expiration: i64,
    group_name: &str,
    caption: Option<&str>,
    jpeg_thumbnail: Option<Vec<u8>>,
) -> Message {
    let inv = GroupInviteMessage {
        group_jid: Some(group_jid.to_string()),
        invite_code: Some(invite_code.to_string()),
        invite_expiration: Some(invite_expiration),
        group_name: Some(group_name.to_string()),
        caption: caption.map(|s| s.to_string()),
        jpeg_thumbnail,
        context_info: None,
        group_type: Some(0), // DEFAULT
    };

    Message {
        group_invite_message: Some(Box::new(inv)),
        ..Default::default()
    }
}

/// Wraps any media message inside a View-Once container.
pub fn wrap_view_once(inner: Message, v2: bool) -> Message {
    if v2 {
        Message {
            view_once_message_v2: Some(Box::new(FutureProofMessage {
                message: Some(Box::new(inner)),
            })),
            ..Default::default()
        }
    } else {
        Message {
            view_once_message: Some(Box::new(FutureProofMessage {
                message: Some(Box::new(inner)),
            })),
            ..Default::default()
        }
    }
}

/// Recursively unwraps a View-Once container if present, returning a reference to the inner message.
pub fn unwrap_view_once(msg: &Message) -> &Message {
    if let Some(ref vo) = msg.view_once_message {
        if let Some(ref inner) = vo.message {
            return unwrap_view_once(inner);
        }
    }
    if let Some(ref vo) = msg.view_once_message_v2 {
        if let Some(ref inner) = vo.message {
            return unwrap_view_once(inner);
        }
    }
    if let Some(ref vo) = msg.view_once_message_v2_extension {
        if let Some(ref inner) = vo.message {
            return unwrap_view_once(inner);
        }
    }
    msg
}

/// Builds a rich voice note / PTT audio `AudioMessage` wrapped in a `Message`.
pub fn build_rich_voice_note_message(
    url: Option<&str>,
    direct_path: Option<&str>,
    media_key: Option<&[u8]>,
    file_sha256: Option<&[u8]>,
    file_enc_sha256: Option<&[u8]>,
    file_length: u64,
    seconds: u32,
    waveform: Option<&[u8]>,
) -> Message {
    let audio = AudioMessage {
        url: url.map(|s| s.to_string()),
        mimetype: Some("audio/ogg; codecs=opus".to_string()),
        file_sha256: file_sha256.map(|b| b.to_vec()),
        file_length: Some(file_length),
        seconds: Some(seconds),
        ptt: Some(true),
        media_key: media_key.map(|b| b.to_vec()),
        file_enc_sha256: file_enc_sha256.map(|b| b.to_vec()),
        direct_path: direct_path.map(|s| s.to_string()),
        media_key_timestamp: Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
        ),
        waveform: waveform.map(|w| w.to_vec()),
        ..Default::default()
    };

    Message {
        audio_message: Some(Box::new(audio)),
        ..Default::default()
    }
}

/// Builds a `DocumentMessage` wrapped in a `Message`.
pub fn build_document_message(
    url: Option<&str>,
    direct_path: Option<&str>,
    media_key: Option<&[u8]>,
    file_sha256: Option<&[u8]>,
    file_enc_sha256: Option<&[u8]>,
    file_length: u64,
    filename: &str,
    mimetype: &str,
    title: Option<&str>,
    page_count: Option<u32>,
    caption: Option<&str>,
    jpeg_thumbnail: Option<Vec<u8>>,
) -> Message {
    let doc = DocumentMessage {
        url: url.map(|s| s.to_string()),
        mimetype: Some(mimetype.to_string()),
        title: title.map(|s| s.to_string()),
        file_sha256: file_sha256.map(|b| b.to_vec()),
        file_length: Some(file_length),
        page_count,
        media_key: media_key.map(|b| b.to_vec()),
        file_name: Some(filename.to_string()),
        file_enc_sha256: file_enc_sha256.map(|b| b.to_vec()),
        direct_path: direct_path.map(|s| s.to_string()),
        media_key_timestamp: Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
        ),
        contact_vcard: None,
        thumbnail_direct_path: None,
        thumbnail_sha256: None,
        thumbnail_enc_sha256: None,
        jpeg_thumbnail,
        caption: caption.map(|s| s.to_string()),
        ..Default::default()
    };

    Message {
        document_message: Some(Box::new(doc)),
        ..Default::default()
    }
}

/// Builds a `VideoMessage` wrapped in a `Message`.
pub fn build_video_message(
    url: Option<&str>,
    direct_path: Option<&str>,
    media_key: Option<&[u8]>,
    file_sha256: Option<&[u8]>,
    file_enc_sha256: Option<&[u8]>,
    file_length: u64,
    caption: Option<&str>,
    is_gif: bool,
    seconds: Option<u32>,
    jpeg_thumbnail: Option<Vec<u8>>,
) -> Message {
    let vid = VideoMessage {
        url: url.map(|s| s.to_string()),
        mimetype: Some("video/mp4".to_string()),
        file_sha256: file_sha256.map(|b| b.to_vec()),
        file_length: Some(file_length),
        seconds,
        media_key: media_key.map(|b| b.to_vec()),
        caption: caption.map(|s| s.to_string()),
        gif_playback: Some(is_gif),
        file_enc_sha256: file_enc_sha256.map(|b| b.to_vec()),
        direct_path: direct_path.map(|s| s.to_string()),
        media_key_timestamp: Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
        ),
        jpeg_thumbnail,
        ..Default::default()
    };

    Message {
        video_message: Some(Box::new(vid)),
        ..Default::default()
    }
}

/// Builds a `StickerMessage` wrapped in a `Message`.
pub fn build_sticker_message(
    url: Option<&str>,
    direct_path: Option<&str>,
    media_key: Option<&[u8]>,
    file_sha256: Option<&[u8]>,
    file_enc_sha256: Option<&[u8]>,
    file_length: u64,
    is_animated: bool,
    is_avatar: bool,
) -> Message {
    let stk = StickerMessage {
        url: url.map(|s| s.to_string()),
        file_sha256: file_sha256.map(|b| b.to_vec()),
        file_enc_sha256: file_enc_sha256.map(|b| b.to_vec()),
        media_key: media_key.map(|b| b.to_vec()),
        mimetype: Some("image/webp".to_string()),
        height: Some(512),
        width: Some(512),
        direct_path: direct_path.map(|s| s.to_string()),
        file_length: Some(file_length),
        media_key_timestamp: Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
        ),
        is_animated: Some(is_animated),
        is_avatar: Some(is_avatar),
        ..Default::default()
    };

    Message {
        sticker_message: Some(Box::new(stk)),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_location_message() {
        let msg = build_location_message(
            -6.2088,
            106.8456,
            Some("Monas"),
            Some("Gambir, Central Jakarta"),
            Some("https://maps.google.com"),
            None,
        );

        assert!(msg.location_message.is_some());
        let loc = msg.location_message.unwrap();
        assert_eq!(loc.degrees_latitude, Some(-6.2088));
        assert_eq!(loc.degrees_longitude, Some(106.8456));
        assert_eq!(loc.name.as_deref(), Some("Monas"));
        assert_eq!(loc.address.as_deref(), Some("Gambir, Central Jakarta"));
    }

    #[test]
    fn test_build_live_location_message() {
        let msg = build_live_location_message(
            -6.2000,
            106.8166,
            Some(15),
            Some(1.2),
            Some(90),
            Some("On my way"),
            Some(1),
            Some(0),
            None,
        );

        assert!(msg.live_location_message.is_some());
        let live = msg.live_location_message.unwrap();
        assert_eq!(live.degrees_latitude, Some(-6.2000));
        assert_eq!(live.accuracy_in_meters, Some(15));
        assert_eq!(live.caption.as_deref(), Some("On my way"));
    }

    #[test]
    fn test_build_vcard_and_contact_message() {
        let vcard = build_vcard_string("John Doe", "+1234567890", Some("Rose Project"));
        assert!(vcard.contains("BEGIN:VCARD"));
        assert!(vcard.contains("FN:John Doe"));
        assert!(vcard.contains("ORG:Rose Project"));
        assert!(vcard.contains("waid=1234567890:+1234567890"));
        assert!(vcard.contains("END:VCARD"));

        let msg = build_contact_message("John Doe", &vcard);
        assert!(msg.contact_message.is_some());
        let con = msg.contact_message.unwrap();
        assert_eq!(con.display_name.as_deref(), Some("John Doe"));
        assert_eq!(con.vcard.as_deref(), Some(vcard.as_str()));
    }

    #[test]
    fn test_build_contacts_array_message() {
        let c1 = ContactMessage {
            display_name: Some("Alice".to_string()),
            vcard: Some("VCARD ALICE".to_string()),
            ..Default::default()
        };
        let c2 = ContactMessage {
            display_name: Some("Bob".to_string()),
            vcard: Some("VCARD BOB".to_string()),
            ..Default::default()
        };

        let msg = build_contacts_array_message("Alice & Bob", vec![c1, c2]);
        assert!(msg.contacts_array_message.is_some());
        let arr = msg.contacts_array_message.unwrap();
        assert_eq!(arr.display_name.as_deref(), Some("Alice & Bob"));
        assert_eq!(arr.contacts.len(), 2);
    }

    #[test]
    fn test_build_group_invite_message() {
        let msg = build_group_invite_message(
            "12345-67890@g.us",
            "AbCdEf123456",
            1798765432,
            "Rust Developers Group",
            Some("Join our group!"),
            None,
        );

        assert!(msg.group_invite_message.is_some());
        let inv = msg.group_invite_message.unwrap();
        assert_eq!(inv.group_jid.as_deref(), Some("12345-67890@g.us"));
        assert_eq!(inv.invite_code.as_deref(), Some("AbCdEf123456"));
        assert_eq!(inv.invite_expiration, Some(1798765432));
        assert_eq!(inv.group_name.as_deref(), Some("Rust Developers Group"));
        assert_eq!(inv.caption.as_deref(), Some("Join our group!"));
    }

    #[test]
    fn test_view_once_wrap_and_unwrap() {
        let inner = Message {
            conversation: Some("Secret photo or text".to_string()),
            ..Default::default()
        };

        let wrapped_v1 = wrap_view_once(inner.clone(), false);
        assert!(wrapped_v1.view_once_message.is_some());
        let unwrapped_v1 = unwrap_view_once(&wrapped_v1);
        assert_eq!(
            unwrapped_v1.conversation.as_deref(),
            Some("Secret photo or text")
        );

        let wrapped_v2 = wrap_view_once(inner.clone(), true);
        assert!(wrapped_v2.view_once_message_v2.is_some());
        let unwrapped_v2 = unwrap_view_once(&wrapped_v2);
        assert_eq!(
            unwrapped_v2.conversation.as_deref(),
            Some("Secret photo or text")
        );
    }

    #[test]
    fn test_build_voice_note_message() {
        let msg = build_rich_voice_note_message(
            Some("https://mmg.whatsapp.net/audio/123"),
            Some("/v/t62/123"),
            Some(&[1u8; 32]),
            Some(&[2u8; 32]),
            Some(&[3u8; 32]),
            45000,
            15,
            Some(&[10, 20, 30, 40, 50]),
        );
        assert!(msg.audio_message.is_some());
        let aud = msg.audio_message.unwrap();
        assert_eq!(aud.mimetype.as_deref(), Some("audio/ogg; codecs=opus"));
        assert_eq!(aud.ptt, Some(true));
        assert_eq!(aud.seconds, Some(15));
        assert_eq!(aud.waveform.as_deref(), Some(&[10u8, 20, 30, 40, 50][..]));
    }

    #[test]
    fn test_build_document_message() {
        let msg = build_document_message(
            Some("https://mmg.whatsapp.net/doc/456"),
            Some("/v/t62/456"),
            Some(&[4u8; 32]),
            Some(&[5u8; 32]),
            Some(&[6u8; 32]),
            1048576,
            "Invoice_2026.pdf",
            "application/pdf",
            Some("Invoice 2026"),
            Some(3),
            Some("Here is your invoice"),
            None,
        );
        assert!(msg.document_message.is_some());
        let doc = msg.document_message.unwrap();
        assert_eq!(doc.file_name.as_deref(), Some("Invoice_2026.pdf"));
        assert_eq!(doc.mimetype.as_deref(), Some("application/pdf"));
        assert_eq!(doc.page_count, Some(3));
        assert_eq!(doc.caption.as_deref(), Some("Here is your invoice"));
    }

    #[test]
    fn test_build_video_and_sticker_message() {
        let v_msg = build_video_message(
            Some("https://mmg.whatsapp.net/vid/789"),
            Some("/v/t62/789"),
            Some(&[7u8; 32]),
            Some(&[8u8; 32]),
            Some(&[9u8; 32]),
            5242880,
            Some("Cool video"),
            true,
            Some(12),
            None,
        );
        assert!(v_msg.video_message.is_some());
        let vid = v_msg.video_message.unwrap();
        assert_eq!(vid.caption.as_deref(), Some("Cool video"));
        assert_eq!(vid.gif_playback, Some(true));
        assert_eq!(vid.seconds, Some(12));

        let s_msg = build_sticker_message(
            Some("https://mmg.whatsapp.net/stk/101"),
            Some("/v/t62/101"),
            Some(&[10u8; 32]),
            Some(&[11u8; 32]),
            Some(&[12u8; 32]),
            65536,
            true,
            false,
        );
        assert!(s_msg.sticker_message.is_some());
        let stk = s_msg.sticker_message.unwrap();
        assert_eq!(stk.mimetype.as_deref(), Some("image/webp"));
        assert_eq!(stk.is_animated, Some(true));
        assert_eq!(stk.is_avatar, Some(false));
    }
}
