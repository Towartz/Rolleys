//! Automatic WhatsApp Protobuf Message Type Recognition & Container Unwrapping Engine.
//!
//! Provides deep inspection, recursive unwrapping of container envelopes
//! (ephemeral, view-once, document-with-caption, device-sent, edited, bot-invoke),
//! canonical classification into `RecognizedMessageType`, structured media extraction,
//! and high-level query methods.

use baileys_proto::proto::{ContextInfo, Message};
use serde::{Deserialize, Serialize};

/// Canonical recognized WhatsApp message types.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RecognizedMessageType {
    Conversation,
    ExtendedText,
    Image,
    Video,
    Audio,
    Ptv,
    Sticker,
    LottieSticker,
    Document,
    Contact,
    ContactsArray,
    Location,
    LiveLocation,
    Reaction,
    PollCreation,
    PollVote,
    Protocol,
    Interactive,
    Buttons,
    Template,
    Event,
    CallLog,
    Album,
    Order,
    Invoice,
    Product,
    List,
    InteractiveResponse,
    ButtonsResponse,
    TemplateButtonReply,
    ListResponse,
    Unknown(String),
}

impl RecognizedMessageType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Conversation => "conversation",
            Self::ExtendedText => "extendedTextMessage",
            Self::Image => "imageMessage",
            Self::Video => "videoMessage",
            Self::Audio => "audioMessage",
            Self::Ptv => "ptvMessage",
            Self::Sticker => "stickerMessage",
            Self::LottieSticker => "lottieStickerMessage",
            Self::Document => "documentMessage",
            Self::Contact => "contactMessage",
            Self::ContactsArray => "contactsArrayMessage",
            Self::Location => "locationMessage",
            Self::LiveLocation => "liveLocationMessage",
            Self::Reaction => "reactionMessage",
            Self::PollCreation => "pollCreationMessage",
            Self::PollVote => "pollUpdateMessage",
            Self::Protocol => "protocolMessage",
            Self::Interactive => "interactiveMessage",
            Self::Buttons => "buttonsMessage",
            Self::Template => "templateMessage",
            Self::Event => "eventMessage",
            Self::CallLog => "callLogMessage",
            Self::Album => "albumMessage",
            Self::Order => "orderMessage",
            Self::Invoice => "invoiceMessage",
            Self::Product => "productMessage",
            Self::List => "listMessage",
            Self::InteractiveResponse => "interactiveResponseMessage",
            Self::ButtonsResponse => "buttonsResponseMessage",
            Self::TemplateButtonReply => "templateButtonReplyMessage",
            Self::ListResponse => "listResponseMessage",
            Self::Unknown(s) => s.as_str(),
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::Conversation => "Text (Simple)",
            Self::ExtendedText => "Text (Extended)",
            Self::Image => "Image",
            Self::Video => "Video",
            Self::Audio => "Audio / Voice Note",
            Self::Ptv => "Video Note (PTV)",
            Self::Sticker => "Sticker",
            Self::LottieSticker => "Lottie Sticker",
            Self::Document => "Document",
            Self::Contact => "Contact (vCard)",
            Self::ContactsArray => "Contacts Array",
            Self::Location => "Location (Static)",
            Self::LiveLocation => "Live Location",
            Self::Reaction => "Emoji Reaction",
            Self::PollCreation => "Poll Creation",
            Self::PollVote => "Poll Vote",
            Self::Protocol => "Protocol (Revoke/Edit)",
            Self::Interactive => "Interactive UI",
            Self::Buttons => "Buttons",
            Self::Template => "Template",
            Self::Event => "Calendar Event",
            Self::CallLog => "Call Log Record",
            Self::Album => "Media Album",
            Self::Order => "Commerce Order",
            Self::Invoice => "Commerce Invoice",
            Self::Product => "Commerce Product",
            Self::List => "List Message",
            Self::InteractiveResponse => "Interactive Response",
            Self::ButtonsResponse => "Button Response",
            Self::TemplateButtonReply => "Template Button Reply",
            Self::ListResponse => "List Response",
            Self::Unknown(_) => "Unknown / Custom",
        }
    }

    pub fn is_media(&self) -> bool {
        matches!(
            self,
            Self::Image
                | Self::Video
                | Self::Audio
                | Self::Ptv
                | Self::Sticker
                | Self::LottieSticker
                | Self::Document
                | Self::Album
        )
    }

    pub fn is_interactive(&self) -> bool {
        matches!(
            self,
            Self::Interactive
                | Self::Buttons
                | Self::Template
                | Self::List
                | Self::InteractiveResponse
                | Self::ButtonsResponse
                | Self::TemplateButtonReply
                | Self::ListResponse
        )
    }

    pub fn all_canonical() -> &'static [RecognizedMessageType] {
        &[
            Self::Conversation,
            Self::ExtendedText,
            Self::Image,
            Self::Video,
            Self::Audio,
            Self::Ptv,
            Self::Sticker,
            Self::LottieSticker,
            Self::Document,
            Self::Contact,
            Self::ContactsArray,
            Self::Location,
            Self::LiveLocation,
            Self::Reaction,
            Self::PollCreation,
            Self::PollVote,
            Self::Protocol,
            Self::Interactive,
            Self::Buttons,
            Self::Template,
            Self::Event,
            Self::CallLog,
            Self::Album,
            Self::Order,
            Self::Invoice,
            Self::Product,
            Self::List,
            Self::InteractiveResponse,
            Self::ButtonsResponse,
            Self::TemplateButtonReply,
            Self::ListResponse,
        ]
    }
}

/// Metadata extracted from a media message payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtoMediaMeta {
    pub media_type: RecognizedMessageType,
    pub mimetype: Option<String>,
    pub file_length: u64,
    pub file_sha256: Option<Vec<u8>>,
    pub media_key: Option<Vec<u8>>,
    pub direct_path: Option<String>,
    pub url: Option<String>,
    pub caption: Option<String>,
    pub file_name: Option<String>,
    pub is_animated: bool,
    pub duration_seconds: Option<u32>,
}

/// Result of unwrapping all container envelopes from a message.
#[derive(Debug)]
pub struct MessageUnwrapResult<'a> {
    pub payload: &'a Message,
    pub wrappers: Vec<&'static str>,
    pub real_type: RecognizedMessageType,
}

/// Extension trait providing automatic recognition and unwrap functions for any `Message`.
pub trait MessageExt {
    /// Recursively unwraps container envelopes (ephemeral, view-once, caption, device-sent, etc.)
    /// to obtain the underlying payload and the wrapper chain.
    fn unwrap_wrappers(&self) -> MessageUnwrapResult<'_>;

    /// Identifies the real, unwrapped canonical message type.
    fn recognize_type(&self) -> RecognizedMessageType;

    /// Extracts human-readable text (conversation, caption, extended text, interactive body).
    fn extract_text(&self) -> Option<String>;

    /// Extracts media metadata if the message contains image, video, audio, sticker, or document.
    fn extract_media_meta(&self) -> Option<ProtoMediaMeta>;

    /// Returns the associated `ContextInfo` (mentions, quoted message, forwarding score) if present.
    fn extract_context_info(&self) -> Option<&ContextInfo>;

    /// Returns the quoted message within `ContextInfo` if this is a reply.
    fn extract_quoted_message(&self) -> Option<&Message>;

    /// Converts message to a dynamic `serde_json::Value` for reflection and inspection.
    fn to_json_value(&self) -> serde_json::Value;
}

impl MessageExt for Message {
    fn unwrap_wrappers(&self) -> MessageUnwrapResult<'_> {
        let mut cur = self;
        let mut wrappers = Vec::new();

        loop {
            if let Some(ref eph) = cur.ephemeral_message {
                if let Some(ref inner) = eph.message {
                    wrappers.push("ephemeral");
                    cur = inner;
                    continue;
                }
            }
            if let Some(ref vo) = cur.view_once_message {
                if let Some(ref inner) = vo.message {
                    wrappers.push("view_once");
                    cur = inner;
                    continue;
                }
            }
            if let Some(ref vo) = cur.view_once_message_v2 {
                if let Some(ref inner) = vo.message {
                    wrappers.push("view_once_v2");
                    cur = inner;
                    continue;
                }
            }
            if let Some(ref vo) = cur.view_once_message_v2_extension {
                if let Some(ref inner) = vo.message {
                    wrappers.push("view_once_v2_ext");
                    cur = inner;
                    continue;
                }
            }
            if let Some(ref dwc) = cur.document_with_caption_message {
                if let Some(ref inner) = dwc.message {
                    wrappers.push("document_with_caption");
                    cur = inner;
                    continue;
                }
            }
            if let Some(ref dev) = cur.device_sent_message {
                if let Some(ref inner) = dev.message {
                    wrappers.push("device_sent");
                    cur = inner;
                    continue;
                }
            }
            if let Some(ref edit) = cur.edited_message {
                if let Some(ref inner) = edit.message {
                    wrappers.push("edited");
                    cur = inner;
                    continue;
                }
            }
            if let Some(ref bot) = cur.bot_invoke_message {
                if let Some(ref inner) = bot.message {
                    wrappers.push("bot_invoke");
                    cur = inner;
                    continue;
                }
            }
            if let Some(ref gm) = cur.group_mentioned_message {
                if let Some(ref inner) = gm.message {
                    wrappers.push("group_mentioned");
                    cur = inner;
                    continue;
                }
            }
            if let Some(ref lsm) = cur.lottie_sticker_message {
                if let Some(ref inner) = lsm.message {
                    wrappers.push("lottie_container");
                    cur = inner;
                    continue;
                }
            }
            if let Some(ref sm) = cur.status_mention_message {
                if let Some(ref inner) = sm.message {
                    wrappers.push("status_mention");
                    cur = inner;
                    continue;
                }
            }
            if let Some(ref gsm) = cur.group_status_mention_message {
                if let Some(ref inner) = gsm.message {
                    wrappers.push("group_status_mention");
                    cur = inner;
                    continue;
                }
            }
            if let Some(ref pcm) = cur.poll_creation_message_v4 {
                if let Some(ref inner) = pcm.message {
                    wrappers.push("poll_creation_v4");
                    cur = inner;
                    continue;
                }
            }
            if let Some(ref acm) = cur.associated_child_message {
                if let Some(ref inner) = acm.message {
                    wrappers.push("associated_child");
                    cur = inner;
                    continue;
                }
            }
            if let Some(ref bfm) = cur.bot_forwarded_message {
                if let Some(ref inner) = bfm.message {
                    wrappers.push("bot_forwarded");
                    cur = inner;
                    continue;
                }
            }
            if let Some(ref gsm) = cur.group_status_message {
                if let Some(ref inner) = gsm.message {
                    wrappers.push("group_status");
                    cur = inner;
                    continue;
                }
            }
            if let Some(ref gsm2) = cur.group_status_message_v2 {
                if let Some(ref inner) = gsm2.message {
                    wrappers.push("group_status_v2");
                    cur = inner;
                    continue;
                }
            }
            if let Some(ref eci) = cur.event_cover_image {
                if let Some(ref inner) = eci.message {
                    wrappers.push("event_cover_image");
                    cur = inner;
                    continue;
                }
            }

            break;
        }

        let real_type = detect_message_type(cur);
        MessageUnwrapResult {
            payload: cur,
            wrappers,
            real_type,
        }
    }

    fn recognize_type(&self) -> RecognizedMessageType {
        self.unwrap_wrappers().real_type
    }

    fn extract_text(&self) -> Option<String> {
        let unwrapped = self.unwrap_wrappers().payload;

        if let Some(ref c) = unwrapped.conversation {
            if !c.is_empty() {
                return Some(c.clone());
            }
        }
        if let Some(ref ext) = unwrapped.extended_text_message {
            if let Some(ref t) = ext.text {
                if !t.is_empty() {
                    return Some(t.clone());
                }
            }
        }
        if let Some(ref img) = unwrapped.image_message {
            if let Some(ref cap) = img.caption {
                if !cap.is_empty() {
                    return Some(cap.clone());
                }
            }
        }
        if let Some(ref vid) = unwrapped.video_message {
            if let Some(ref cap) = vid.caption {
                if !cap.is_empty() {
                    return Some(cap.clone());
                }
            }
        }
        if let Some(ref doc) = unwrapped.document_message {
            if let Some(ref cap) = doc.caption {
                if !cap.is_empty() {
                    return Some(cap.clone());
                }
            }
        }
        if let Some(ref inter) = unwrapped.interactive_message {
            if let Some(ref b) = inter.body {
                if let Some(ref t) = b.text {
                    if !t.is_empty() {
                        return Some(t.clone());
                    }
                }
            }
        }
        if let Some(ref btn) = unwrapped.buttons_message {
            if let Some(ref t) = btn.content_text {
                if !t.is_empty() {
                    return Some(t.clone());
                }
            }
        }
        if let Some(ref poll) = unwrapped.poll_creation_message {
            if let Some(ref n) = poll.name {
                return Some(n.clone());
            }
        }
        if let Some(ref poll) = unwrapped.poll_creation_message_v2 {
            if let Some(ref n) = poll.name {
                return Some(n.clone());
            }
        }
        if let Some(ref poll) = unwrapped.poll_creation_message_v3 {
            if let Some(ref n) = poll.name {
                return Some(n.clone());
            }
        }
        if let Some(ref ev) = unwrapped.event_message {
            if let Some(ref n) = ev.name {
                if !n.is_empty() {
                    return Some(n.clone());
                }
            }
        }
        if let Some(ref resp) = unwrapped.interactive_response_message {
            if let Some(baileys_proto::proto::message::interactive_response_message::InteractiveResponseMessage::NativeFlowResponseMessage(ref nf)) = resp.interactive_response_message {
                if let Some(ref pj) = nf.params_json {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(pj) {
                        if let Some(id) = parsed.get("id").and_then(|v| v.as_str()) {
                            if !id.is_empty() {
                                return Some(id.to_string());
                            }
                        }
                        if let Some(text) = parsed.get("display_text").and_then(|v| v.as_str()) {
                            if !text.is_empty() {
                                return Some(text.to_string());
                            }
                        }
                    }
                }
                if let Some(ref name) = nf.name {
                    return Some(name.clone());
                }
            }
            if let Some(ref b) = resp.body {
                if let Some(ref t) = b.text {
                    if !t.is_empty() {
                        return Some(t.clone());
                    }
                }
            }
        }
        if let Some(ref resp) = unwrapped.buttons_response_message {
            if let Some(baileys_proto::proto::message::buttons_response_message::Response::SelectedDisplayText(ref t)) = resp.response {
                if !t.is_empty() {
                    return Some(t.clone());
                }
            }
            if let Some(ref id) = resp.selected_button_id {
                if !id.is_empty() {
                    return Some(id.clone());
                }
            }
        }
        if let Some(ref resp) = unwrapped.list_response_message {
            if let Some(ref reply) = resp.single_select_reply {
                if let Some(ref id) = reply.selected_row_id {
                    if !id.is_empty() {
                        return Some(id.clone());
                    }
                }
            }
            if let Some(ref t) = resp.title {
                if !t.is_empty() {
                    return Some(t.clone());
                }
            }
        }
        if let Some(ref resp) = unwrapped.template_button_reply_message {
            if let Some(ref t) = resp.selected_display_text {
                if !t.is_empty() {
                    return Some(t.clone());
                }
            }
            if let Some(ref id) = resp.selected_id {
                if !id.is_empty() {
                    return Some(id.clone());
                }
            }
        }
        if let Some(ref con) = unwrapped.contact_message {
            if let Some(ref n) = con.display_name {
                return Some(n.clone());
            }
        }
        if let Some(ref loc) = unwrapped.location_message {
            if let Some(ref n) = loc.name {
                if !n.is_empty() {
                    return Some(n.clone());
                }
            }
            if let Some(ref a) = loc.address {
                if !a.is_empty() {
                    return Some(a.clone());
                }
            }
        }

        None
    }

    fn extract_media_meta(&self) -> Option<ProtoMediaMeta> {
        let unwrapped = self.unwrap_wrappers().payload;

        if let Some(ref img) = unwrapped.image_message {
            return Some(ProtoMediaMeta {
                media_type: RecognizedMessageType::Image,
                mimetype: img.mimetype.clone(),
                file_length: img.file_length.unwrap_or(0),
                file_sha256: img.file_sha256.clone(),
                media_key: img.media_key.clone(),
                direct_path: img.direct_path.clone(),
                url: img.url.clone(),
                caption: img.caption.clone(),
                file_name: None,
                is_animated: false,
                duration_seconds: None,
            });
        }

        if let Some(ref vid) = unwrapped.video_message {
            return Some(ProtoMediaMeta {
                media_type: RecognizedMessageType::Video,
                mimetype: vid.mimetype.clone(),
                file_length: vid.file_length.unwrap_or(0),
                file_sha256: vid.file_sha256.clone(),
                media_key: vid.media_key.clone(),
                direct_path: vid.direct_path.clone(),
                url: vid.url.clone(),
                caption: vid.caption.clone(),
                file_name: None,
                is_animated: vid.gif_playback.unwrap_or(false),
                duration_seconds: vid.seconds,
            });
        }

        if let Some(ref aud) = unwrapped.audio_message {
            return Some(ProtoMediaMeta {
                media_type: RecognizedMessageType::Audio,
                mimetype: aud.mimetype.clone(),
                file_length: aud.file_length.unwrap_or(0),
                file_sha256: aud.file_sha256.clone(),
                media_key: aud.media_key.clone(),
                direct_path: aud.direct_path.clone(),
                url: aud.url.clone(),
                caption: None,
                file_name: None,
                is_animated: false,
                duration_seconds: aud.seconds,
            });
        }

        if let Some(ref stk) = unwrapped.sticker_message {
            return Some(ProtoMediaMeta {
                media_type: RecognizedMessageType::Sticker,
                mimetype: stk.mimetype.clone(),
                file_length: stk.file_length.unwrap_or(0),
                file_sha256: stk.file_sha256.clone(),
                media_key: stk.media_key.clone(),
                direct_path: stk.direct_path.clone(),
                url: stk.url.clone(),
                caption: None,
                file_name: None,
                is_animated: stk.is_animated.unwrap_or(false),
                duration_seconds: None,
            });
        }

        if let Some(ref doc) = unwrapped.document_message {
            return Some(ProtoMediaMeta {
                media_type: RecognizedMessageType::Document,
                mimetype: doc.mimetype.clone(),
                file_length: doc.file_length.unwrap_or(0),
                file_sha256: doc.file_sha256.clone(),
                media_key: doc.media_key.clone(),
                direct_path: doc.direct_path.clone(),
                url: doc.url.clone(),
                caption: doc.caption.clone(),
                file_name: doc.file_name.clone(),
                is_animated: false,
                duration_seconds: None,
            });
        }

        None
    }

    fn extract_context_info(&self) -> Option<&ContextInfo> {
        let u = self.unwrap_wrappers().payload;
        if let Some(ref ext) = u.extended_text_message {
            return ext.context_info.as_deref();
        }
        if let Some(ref img) = u.image_message {
            return img.context_info.as_deref();
        }
        if let Some(ref vid) = u.video_message {
            return vid.context_info.as_deref();
        }
        if let Some(ref aud) = u.audio_message {
            return aud.context_info.as_deref();
        }
        if let Some(ref doc) = u.document_message {
            return doc.context_info.as_deref();
        }
        if let Some(ref stk) = u.sticker_message {
            return stk.context_info.as_deref();
        }
        if let Some(ref con) = u.contact_message {
            return con.context_info.as_deref();
        }
        if let Some(ref arr) = u.contacts_array_message {
            return arr.context_info.as_deref();
        }
        if let Some(ref loc) = u.location_message {
            return loc.context_info.as_deref();
        }
        if let Some(ref lloc) = u.live_location_message {
            return lloc.context_info.as_deref();
        }
        if let Some(ref btn) = u.buttons_message {
            return btn.context_info.as_deref();
        }
        if let Some(ref inter) = u.interactive_message {
            return inter.context_info.as_deref();
        }
        if let Some(ref ev) = u.event_message {
            return ev.context_info.as_deref();
        }
        if let Some(ref alb) = u.album_message {
            return alb.context_info.as_deref();
        }
        None
    }

    fn extract_quoted_message(&self) -> Option<&Message> {
        self.extract_context_info()
            .and_then(|c| c.quoted_message.as_deref())
    }

    fn to_json_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

fn detect_message_type(msg: &Message) -> RecognizedMessageType {
    if msg.conversation.is_some() {
        return RecognizedMessageType::Conversation;
    }
    if msg.extended_text_message.is_some() {
        return RecognizedMessageType::ExtendedText;
    }
    if msg.image_message.is_some() {
        return RecognizedMessageType::Image;
    }
    if msg.video_message.is_some() {
        return RecognizedMessageType::Video;
    }
    if msg.ptv_message.is_some() {
        return RecognizedMessageType::Ptv;
    }
    if msg.audio_message.is_some() {
        return RecognizedMessageType::Audio;
    }
    if msg.sticker_message.is_some() {
        return RecognizedMessageType::Sticker;
    }
    if msg.document_message.is_some() {
        return RecognizedMessageType::Document;
    }
    if msg.contact_message.is_some() {
        return RecognizedMessageType::Contact;
    }
    if msg.contacts_array_message.is_some() {
        return RecognizedMessageType::ContactsArray;
    }
    if msg.location_message.is_some() {
        return RecognizedMessageType::Location;
    }
    if msg.live_location_message.is_some() {
        return RecognizedMessageType::LiveLocation;
    }
    if msg.reaction_message.is_some() {
        return RecognizedMessageType::Reaction;
    }
    if msg.poll_creation_message.is_some()
        || msg.poll_creation_message_v2.is_some()
        || msg.poll_creation_message_v3.is_some()
    {
        return RecognizedMessageType::PollCreation;
    }
    if msg.poll_update_message.is_some() {
        return RecognizedMessageType::PollVote;
    }
    if msg.protocol_message.is_some() {
        return RecognizedMessageType::Protocol;
    }
    if msg.interactive_message.is_some() {
        return RecognizedMessageType::Interactive;
    }
    if msg.buttons_message.is_some() {
        return RecognizedMessageType::Buttons;
    }
    if msg.template_message.is_some() || msg.highly_structured_message.is_some() {
        return RecognizedMessageType::Template;
    }
    if msg.list_message.is_some() {
        return RecognizedMessageType::List;
    }
    if msg.order_message.is_some() {
        return RecognizedMessageType::Order;
    }
    if msg.invoice_message.is_some() {
        return RecognizedMessageType::Invoice;
    }
    if msg.product_message.is_some() {
        return RecognizedMessageType::Product;
    }
    if msg.event_message.is_some() {
        return RecognizedMessageType::Event;
    }
    if msg.album_message.is_some() {
        return RecognizedMessageType::Album;
    }
    if msg.interactive_response_message.is_some() {
        return RecognizedMessageType::InteractiveResponse;
    }
    if msg.buttons_response_message.is_some() {
        return RecognizedMessageType::ButtonsResponse;
    }
    if msg.template_button_reply_message.is_some() {
        return RecognizedMessageType::TemplateButtonReply;
    }
    if msg.list_response_message.is_some() {
        return RecognizedMessageType::ListResponse;
    }

    RecognizedMessageType::Unknown("unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use baileys_proto::proto::message::{EventMessage, FutureProofMessage, ImageMessage};

    #[test]
    fn test_recognize_simple_conversation() {
        let msg = Message {
            conversation: Some("Hello world".to_string()),
            ..Default::default()
        };
        assert_eq!(msg.recognize_type(), RecognizedMessageType::Conversation);
        assert_eq!(msg.extract_text(), Some("Hello world".to_string()));
    }

    #[test]
    fn test_recursive_unwrap_ephemeral_view_once() {
        let inner_image = Message {
            image_message: Some(Box::new(ImageMessage {
                caption: Some("Secret photo".to_string()),
                mimetype: Some("image/jpeg".to_string()),
                file_length: Some(12345),
                ..Default::default()
            })),
            ..Default::default()
        };

        let view_once = Message {
            view_once_message: Some(Box::new(FutureProofMessage {
                message: Some(Box::new(inner_image)),
            })),
            ..Default::default()
        };

        let ephemeral = Message {
            ephemeral_message: Some(Box::new(FutureProofMessage {
                message: Some(Box::new(view_once)),
            })),
            ..Default::default()
        };

        let res = ephemeral.unwrap_wrappers();
        assert_eq!(res.wrappers, vec!["ephemeral", "view_once"]);
        assert_eq!(res.real_type, RecognizedMessageType::Image);
        assert_eq!(ephemeral.extract_text(), Some("Secret photo".to_string()));

        let meta = ephemeral
            .extract_media_meta()
            .expect("Media meta should be extracted");
        assert_eq!(meta.media_type, RecognizedMessageType::Image);
        assert_eq!(meta.caption.as_deref(), Some("Secret photo"));
        assert_eq!(meta.file_length, 12345);
    }

    #[test]
    fn test_recognize_event_message() {
        let msg = Message {
            event_message: Some(Box::new(EventMessage {
                name: Some("Launch Party".to_string()),
                ..Default::default()
            })),
            ..Default::default()
        };
        assert_eq!(msg.recognize_type(), RecognizedMessageType::Event);
        assert_eq!(msg.extract_text(), Some("Launch Party".to_string()));
    }

    #[test]
    fn test_unwrap_modern_futureproof_wrappers() {
        let inner_text = Message {
            conversation: Some("Future text".to_string()),
            ..Default::default()
        };
        let status_mention = Message {
            status_mention_message: Some(Box::new(FutureProofMessage {
                message: Some(Box::new(inner_text)),
            })),
            ..Default::default()
        };
        let res = status_mention.unwrap_wrappers();
        assert_eq!(res.wrappers, vec!["status_mention"]);
        assert_eq!(res.real_type, RecognizedMessageType::Conversation);
        assert_eq!(
            status_mention.extract_text(),
            Some("Future text".to_string())
        );
    }
}
