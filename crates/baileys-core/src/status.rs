//! WhatsApp Status / Broadcast Stories protocol handling.
//!
//! Provides functions for:
//! - Publishing text and media stories to `status@broadcast`.
//! - Parsing hex color codes and fonts for status text cards.
//! - Extracting incoming story updates from protobuf messages.
//! - Dispatching story read receipts to the author via `status@broadcast`.

use baileys_binary::BinaryNode;
use baileys_proto::proto::message::extended_text_message::FontType;
use baileys_proto::proto::message::ExtendedTextMessage;
use baileys_proto::proto::Message;
use baileys_store::StoredStory;

/// WhatsApp default story green background (ARGB: 0xFF25D366)
pub const DEFAULT_STATUS_BG_COLOR: u32 = 0xFF25D366;
/// Default text color: Pure white (ARGB: 0xFFFFFFFF)
pub const DEFAULT_STATUS_TEXT_COLOR: u32 = 0xFFFFFFFF;

/// Parses a hex color string (e.g. `"#25D366"`, `"25D366"`, `"#FF25D366"`) into a 32-bit ARGB integer.
///
/// Matches WhiskeySockets/Baileys `assertColor` behavior.
pub fn parse_hex_color(hex_str: &str) -> u32 {
    let clean = hex_str.trim().trim_start_matches('#');
    if clean.is_empty() {
        return DEFAULT_STATUS_BG_COLOR;
    }

    let normalized = if clean.len() <= 6 {
        format!("FF{:0>6}", clean)
    } else {
        clean.to_string()
    };

    u32::from_str_radix(&normalized, 16).unwrap_or(DEFAULT_STATUS_BG_COLOR)
}

/// Builds a text status `Message` protobuf addressed to `status@broadcast`.
pub fn build_text_status_message(
    text: &str,
    background_color: Option<&str>,
    font: Option<i32>,
) -> Message {
    let bg_argb = background_color
        .map(parse_hex_color)
        .unwrap_or(DEFAULT_STATUS_BG_COLOR);

    let font_val = font.unwrap_or(FontType::System as i32);

    let ext = ExtendedTextMessage {
        text: Some(text.to_string()),
        background_argb: Some(bg_argb),
        text_argb: Some(DEFAULT_STATUS_TEXT_COLOR),
        font: Some(font_val),
        ..Default::default()
    };

    Message {
        extended_text_message: Some(Box::new(ext)),
        ..Default::default()
    }
}

/// Builds a rich URL link preview status `Message` protobuf addressed to `status@broadcast`.
pub fn build_link_status_message(
    url: &str,
    title: Option<&str>,
    description: Option<&str>,
    custom_text: Option<&str>,
    background_color: Option<&str>,
    font: Option<i32>,
    thumbnail_bytes: Option<Vec<u8>>,
) -> Message {
    let bg_argb = background_color
        .map(parse_hex_color)
        .unwrap_or(DEFAULT_STATUS_BG_COLOR);

    let font_val = font.unwrap_or(FontType::System as i32);
    let display_text = custom_text.unwrap_or(url);

    let ext = ExtendedTextMessage {
        text: Some(display_text.to_string()),
        matched_text: Some(url.to_string()),
        title: title.map(|s| s.to_string()),
        description: description.map(|s| s.to_string()),
        jpeg_thumbnail: thumbnail_bytes,
        background_argb: Some(bg_argb),
        text_argb: Some(DEFAULT_STATUS_TEXT_COLOR),
        font: Some(font_val),
        preview_type: Some(0),
        ..Default::default()
    };

    Message {
        extended_text_message: Some(Box::new(ext)),
        ..Default::default()
    }
}

/// Builds an HD audio / voice note status `Message` protobuf addressed to `status@broadcast`.
pub fn build_audio_status_message(
    url: Option<String>,
    direct_path: Option<String>,
    media_key: Vec<u8>,
    file_sha256: Vec<u8>,
    file_enc_sha256: Vec<u8>,
    file_length: u64,
    seconds: u32,
    mimetype: Option<&str>,
    waveform: Option<Vec<u8>>,
) -> Message {
    let default_waveform = vec![
        0, 20, 45, 80, 95, 70, 50, 85, 100, 75, 45, 20, 60, 30, 10, 0,
    ];
    let wf = waveform.unwrap_or(default_waveform);

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

/// Detailed media attributes for HD status stories.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusMediaAttributes {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub jpeg_thumbnail: Option<Vec<u8>>,
    pub is_hd: bool,
    pub media_key_timestamp: Option<i64>,
}

/// Builds an HD image status `Message` protobuf addressed to `status@broadcast`.
pub fn build_image_status_message(
    url: Option<String>,
    direct_path: Option<String>,
    media_key: Vec<u8>,
    file_sha256: Vec<u8>,
    file_enc_sha256: Vec<u8>,
    file_length: u64,
    mimetype: Option<&str>,
    caption: Option<&str>,
    attributes: Option<StatusMediaAttributes>,
) -> Message {
    let (width, height, jpeg_thumbnail, media_key_timestamp) = match attributes {
        Some(attr) => (
            attr.width,
            attr.height,
            attr.jpeg_thumbnail,
            attr.media_key_timestamp,
        ),
        None => (None, None, None, None),
    };

    Message {
        image_message: Some(Box::new(baileys_proto::proto::message::ImageMessage {
            url,
            mimetype: Some(mimetype.unwrap_or("image/jpeg").to_string()),
            caption: caption.map(|s| s.to_string()),
            file_sha256: Some(file_sha256),
            file_length: Some(file_length),
            media_key: Some(media_key),
            file_enc_sha256: Some(file_enc_sha256),
            direct_path,
            width,
            height,
            jpeg_thumbnail,
            media_key_timestamp,
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// Builds an HD video status `Message` protobuf addressed to `status@broadcast`.
pub fn build_video_status_message(
    url: Option<String>,
    direct_path: Option<String>,
    media_key: Vec<u8>,
    file_sha256: Vec<u8>,
    file_enc_sha256: Vec<u8>,
    file_length: u64,
    seconds: u32,
    mimetype: Option<&str>,
    caption: Option<&str>,
    attributes: Option<StatusMediaAttributes>,
) -> Message {
    let (width, height, jpeg_thumbnail, media_key_timestamp) = match attributes {
        Some(attr) => (
            attr.width,
            attr.height,
            attr.jpeg_thumbnail,
            attr.media_key_timestamp,
        ),
        None => (None, None, None, None),
    };

    Message {
        video_message: Some(Box::new(baileys_proto::proto::message::VideoMessage {
            url,
            mimetype: Some(mimetype.unwrap_or("video/mp4").to_string()),
            caption: caption.map(|s| s.to_string()),
            file_sha256: Some(file_sha256),
            file_length: Some(file_length),
            seconds: Some(seconds),
            media_key: Some(media_key),
            file_enc_sha256: Some(file_enc_sha256),
            direct_path,
            width,
            height,
            jpeg_thumbnail,
            media_key_timestamp,
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// Parses an incoming status message protobuf into a `StoredStory` record.
pub fn parse_story_from_proto(
    msg_id: &str,
    author_jid: &str,
    author_name: Option<String>,
    timestamp: u64,
    proto: &Message,
) -> StoredStory {
    if let Some(ref ext) = proto.extended_text_message {
        return StoredStory {
            msg_id: msg_id.to_string(),
            author_jid: author_jid.to_string(),
            author_name,
            timestamp,
            message_type: "text".to_string(),
            content: ext.text.clone(),
            caption: None,
            has_media: false,
            viewed: false,
        };
    }

    if let Some(ref conv) = proto.conversation {
        return StoredStory {
            msg_id: msg_id.to_string(),
            author_jid: author_jid.to_string(),
            author_name,
            timestamp,
            message_type: "text".to_string(),
            content: Some(conv.clone()),
            caption: None,
            has_media: false,
            viewed: false,
        };
    }

    if let Some(ref img) = proto.image_message {
        return StoredStory {
            msg_id: msg_id.to_string(),
            author_jid: author_jid.to_string(),
            author_name,
            timestamp,
            message_type: "image".to_string(),
            content: None,
            caption: img.caption.clone(),
            has_media: true,
            viewed: false,
        };
    }

    if let Some(ref vid) = proto.video_message {
        return StoredStory {
            msg_id: msg_id.to_string(),
            author_jid: author_jid.to_string(),
            author_name,
            timestamp,
            message_type: "video".to_string(),
            content: None,
            caption: vid.caption.clone(),
            has_media: true,
            viewed: false,
        };
    }

    if proto.audio_message.is_some() {
        return StoredStory {
            msg_id: msg_id.to_string(),
            author_jid: author_jid.to_string(),
            author_name,
            timestamp,
            message_type: "audio".to_string(),
            content: None,
            caption: None,
            has_media: true,
            viewed: false,
        };
    }

    StoredStory {
        msg_id: msg_id.to_string(),
        author_jid: author_jid.to_string(),
        author_name,
        timestamp,
        message_type: "unknown".to_string(),
        content: None,
        caption: None,
        has_media: false,
        viewed: false,
    }
}

/// Builds a `<receipt>` node to mark a story from `author_jid` as viewed on WhatsApp.
pub fn build_status_read_receipt(msg_id: &str, author_jid: &str) -> BinaryNode {
    BinaryNode::new("receipt")
        .with_attr("id", msg_id)
        .with_attr("to", "status@broadcast")
        .with_attr("type", "read")
        .with_attr("participant", author_jid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_color() {
        assert_eq!(parse_hex_color("#25D366"), 0xFF25D366);
        assert_eq!(parse_hex_color("25D366"), 0xFF25D366);
        assert_eq!(parse_hex_color("#FF1E88E5"), 0xFF1E88E5);
        assert_eq!(parse_hex_color(""), DEFAULT_STATUS_BG_COLOR);
    }

    #[test]
    fn test_build_text_status_message() {
        let msg = build_text_status_message("Sunset vibes!", Some("#E91E63"), Some(2));
        assert!(msg.extended_text_message.is_some());
        let ext = msg.extended_text_message.unwrap();
        assert_eq!(ext.text.as_deref(), Some("Sunset vibes!"));
        assert_eq!(ext.background_argb, Some(0xFFE91E63));
        assert_eq!(ext.font, Some(2));
        assert_eq!(ext.text_argb, Some(DEFAULT_STATUS_TEXT_COLOR));
    }

    #[test]
    fn test_parse_story_from_proto() {
        let text_msg = build_text_status_message("Coding Rust!", None, None);
        let story = parse_story_from_proto(
            "STORY_123",
            "628123456789@s.whatsapp.net",
            Some("Developer".into()),
            1700000000,
            &text_msg,
        );
        assert_eq!(story.msg_id, "STORY_123");
        assert_eq!(story.author_jid, "628123456789@s.whatsapp.net");
        assert_eq!(story.message_type, "text");
        assert_eq!(story.content.as_deref(), Some("Coding Rust!"));
        assert!(!story.has_media);
        assert!(!story.viewed);
    }

    #[test]
    fn test_build_status_read_receipt() {
        let node = build_status_read_receipt("3EB0A1B2C3", "628123456789@s.whatsapp.net");
        assert_eq!(node.tag, "receipt");
        assert_eq!(node.get_attr("id"), Some("3EB0A1B2C3"));
        assert_eq!(node.get_attr("to"), Some("status@broadcast"));
        assert_eq!(node.get_attr("type"), Some("read"));
        assert_eq!(
            node.get_attr("participant"),
            Some("628123456789@s.whatsapp.net")
        );
    }

    #[test]
    fn test_build_image_status_message() {
        let attrs = StatusMediaAttributes {
            width: Some(1920),
            height: Some(1080),
            jpeg_thumbnail: Some(vec![0xFF, 0xD8, 0xFF, 0xD9]),
            is_hd: true,
            media_key_timestamp: Some(1700000000),
        };

        let msg = build_image_status_message(
            Some("https://mms.whatsapp.net/v/t62.7118-24/test.enc".into()),
            Some("/v/t62.7118-24/test.enc".into()),
            vec![1; 32],
            vec![2; 32],
            vec![3; 32],
            1048576,
            Some("image/jpeg"),
            Some("HD Photo Story!"),
            Some(attrs),
        );
        assert!(msg.image_message.is_some());
        let img = msg.image_message.unwrap();
        assert_eq!(img.caption.as_deref(), Some("HD Photo Story!"));
        assert_eq!(img.file_length, Some(1048576));
        assert_eq!(img.width, Some(1920));
        assert_eq!(img.height, Some(1080));
        assert_eq!(img.jpeg_thumbnail, Some(vec![0xFF, 0xD8, 0xFF, 0xD9]));
        assert_eq!(img.media_key_timestamp, Some(1700000000));
    }

    #[test]
    fn test_build_video_status_message() {
        let attrs = StatusMediaAttributes {
            width: Some(1080),
            height: Some(1920),
            jpeg_thumbnail: Some(vec![0xFF, 0xD8, 0xFF, 0xD9]),
            is_hd: true,
            media_key_timestamp: Some(1700000000),
        };

        let msg = build_video_status_message(
            Some("https://mms.whatsapp.net/v/t62.7118-24/vid.enc".into()),
            Some("/v/t62.7118-24/vid.enc".into()),
            vec![4; 32],
            vec![5; 32],
            vec![6; 32],
            5242880,
            15,
            Some("video/mp4"),
            Some("HD 60FPS Video Story!"),
            Some(attrs),
        );
        assert!(msg.video_message.is_some());
        let vid = msg.video_message.unwrap();
        assert_eq!(vid.caption.as_deref(), Some("HD 60FPS Video Story!"));
        assert_eq!(vid.file_length, Some(5242880));
        assert_eq!(vid.seconds, Some(15));
        assert_eq!(vid.width, Some(1080));
        assert_eq!(vid.height, Some(1920));
        assert_eq!(vid.jpeg_thumbnail, Some(vec![0xFF, 0xD8, 0xFF, 0xD9]));
    }

    #[test]
    fn test_build_link_status_message() {
        let msg = build_link_status_message(
            "https://rust-lang.org",
            Some("Rust Programming Language"),
            Some("A language empowering everyone to build reliable and efficient software."),
            Some("Check out Rust! https://rust-lang.org"),
            Some("#1E88E5"),
            Some(3),
            Some(vec![0xFF, 0xD8, 0xFF]),
        );
        assert!(msg.extended_text_message.is_some());
        let ext = msg.extended_text_message.unwrap();
        assert_eq!(ext.matched_text.as_deref(), Some("https://rust-lang.org"));
        assert_eq!(ext.title.as_deref(), Some("Rust Programming Language"));
        assert_eq!(ext.background_argb, Some(0xFF1E88E5));
        assert_eq!(ext.font, Some(3));
        assert_eq!(ext.jpeg_thumbnail, Some(vec![0xFF, 0xD8, 0xFF]));
    }

    #[test]
    fn test_build_audio_status_message() {
        let msg = build_audio_status_message(
            Some("https://mms.whatsapp.net/v/t62/audio.enc".into()),
            Some("/v/t62/audio.enc".into()),
            vec![7; 32],
            vec![8; 32],
            vec![9; 32],
            32768,
            12,
            Some("audio/ogg; codecs=opus"),
            Some(vec![10, 20, 30]),
        );
        assert!(msg.audio_message.is_some());
        let aud = msg.audio_message.unwrap();
        assert_eq!(aud.seconds, Some(12));
        assert_eq!(aud.ptt, Some(true));
        assert_eq!(aud.waveform.as_deref(), Some(&[10u8, 20, 30][..]));
    }
}
