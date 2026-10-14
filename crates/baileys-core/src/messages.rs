use baileys_proto::proto::{ContextInfo, Message, MessageKey};

/// Extracts a human-readable summary of any WhatsApp message protobuf.
/// Formats text, images, videos, audio/voice notes, documents, contacts, locations, reactions, events, interactive payloads, and polls.
pub fn extract_message_summary(proto: &Message) -> String {
    // 1. Unwrap container envelopes
    if let Some(ref eph) = proto.ephemeral_message {
        if let Some(ref inner) = eph.message {
            return extract_message_summary(inner);
        }
    }
    if let Some(ref vo) = proto.view_once_message {
        if let Some(ref inner) = vo.message {
            return format!("[View Once] {}", extract_message_summary(inner));
        }
    }
    if let Some(ref vo) = proto.view_once_message_v2 {
        if let Some(ref inner) = vo.message {
            return format!("[View Once] {}", extract_message_summary(inner));
        }
    }
    if let Some(ref vo) = proto.view_once_message_v2_extension {
        if let Some(ref inner) = vo.message {
            return format!("[View Once] {}", extract_message_summary(inner));
        }
    }
    if let Some(ref dwc) = proto.document_with_caption_message {
        if let Some(ref inner) = dwc.message {
            return extract_message_summary(inner);
        }
    }
    if let Some(ref edit) = proto.edited_message {
        if let Some(ref inner) = edit.message {
            return format!("[Edited] {}", extract_message_summary(inner));
        }
    }
    if let Some(ref dev) = proto.device_sent_message {
        if let Some(ref inner) = dev.message {
            return extract_message_summary(inner);
        }
    }
    if let Some(ref bot) = proto.bot_invoke_message {
        if let Some(ref inner) = bot.message {
            return extract_message_summary(inner);
        }
    }
    if let Some(ref gm) = proto.group_mentioned_message {
        if let Some(ref inner) = gm.message {
            return extract_message_summary(inner);
        }
    }
    if let Some(ref lsm) = proto.lottie_sticker_message {
        if let Some(ref inner) = lsm.message {
            return extract_message_summary(inner);
        }
    }
    if let Some(ref sm) = proto.status_mention_message {
        if let Some(ref inner) = sm.message {
            return extract_message_summary(inner);
        }
    }
    if let Some(ref gsm) = proto.group_status_mention_message {
        if let Some(ref inner) = gsm.message {
            return extract_message_summary(inner);
        }
    }
    if let Some(ref pcm) = proto.poll_creation_message_v4 {
        if let Some(ref inner) = pcm.message {
            return extract_message_summary(inner);
        }
    }
    if let Some(ref acm) = proto.associated_child_message {
        if let Some(ref inner) = acm.message {
            return extract_message_summary(inner);
        }
    }
    if let Some(ref bfm) = proto.bot_forwarded_message {
        if let Some(ref inner) = bfm.message {
            return extract_message_summary(inner);
        }
    }
    if let Some(ref gsm) = proto.group_status_message {
        if let Some(ref inner) = gsm.message {
            return extract_message_summary(inner);
        }
    }
    if let Some(ref gsm2) = proto.group_status_message_v2 {
        if let Some(ref inner) = gsm2.message {
            return extract_message_summary(inner);
        }
    }
    if let Some(ref eci) = proto.event_cover_image {
        if let Some(ref inner) = eci.message {
            return extract_message_summary(inner);
        }
    }

    // 2. Direct message contents
    if let Some(ref conv) = proto.conversation {
        if !conv.is_empty() {
            return conv.clone();
        }
    }

    if let Some(ref ext) = proto.extended_text_message {
        if let Some(ref text) = ext.text {
            if !text.is_empty() {
                return text.clone();
            }
        }
    }

    if let Some(ref img) = proto.image_message {
        if let Some(ref cap) = img.caption {
            return format!("[Image] {}", cap);
        } else {
            return "[Image]".to_string();
        }
    }

    if let Some(ref vid) = proto.video_message {
        let sec = vid.seconds.unwrap_or(0);
        if let Some(ref cap) = vid.caption {
            return format!("[Video: {}s] {}", sec, cap);
        } else {
            return format!("[Video: {}s]", sec);
        }
    }

    if let Some(ref ptv) = proto.ptv_message {
        let sec = ptv.seconds.unwrap_or(0);
        return format!("[Video Note: {}s]", sec);
    }

    if let Some(ref aud) = proto.audio_message {
        let sec = aud.seconds.unwrap_or(0);
        if aud.ptt.unwrap_or(false) {
            return format!("[Voice Note: {}s]", sec);
        } else {
            return format!("[Audio: {}s]", sec);
        }
    }

    if proto.sticker_message.is_some() {
        return "[Sticker]".to_string();
    }

    if let Some(ref doc) = proto.document_message {
        let name = doc.file_name.as_deref().unwrap_or("file");
        if let Some(ref cap) = doc.caption {
            return format!("[Document: {}] {}", name, cap);
        } else {
            return format!("[Document: {}]", name);
        }
    }

    if let Some(ref con) = proto.contact_message {
        let name = con.display_name.as_deref().unwrap_or("Contact");
        return format!("[Contact: {}]", name);
    }

    if let Some(ref contacts) = proto.contacts_array_message {
        let names: Vec<String> = contacts
            .contacts
            .iter()
            .filter_map(|c| c.display_name.clone())
            .collect();
        return format!("[Contacts: {}]", names.join(", "));
    }

    if let Some(ref loc) = proto.location_message {
        let lat = loc.degrees_latitude.unwrap_or(0.0);
        let lon = loc.degrees_longitude.unwrap_or(0.0);
        let name = loc.name.as_deref().unwrap_or("");
        if !name.is_empty() {
            return format!("[Location: {} ({}, {})]", name, lat, lon);
        } else {
            return format!("[Location: {}, {}]", lat, lon);
        }
    }

    if let Some(ref loc) = proto.live_location_message {
        let name = loc.caption.as_deref().unwrap_or("Live Location");
        return format!("[Live Location: {}]", name);
    }

    if let Some(ref react) = proto.reaction_message {
        let text = react.text.as_deref().unwrap_or("❤️");
        let target = react
            .key
            .as_ref()
            .and_then(|k| k.id.as_deref())
            .unwrap_or("");
        return format!("[Reaction: {} on msg {}]", text, target);
    }

    if let Some(ref poll) = proto.poll_creation_message {
        let name = poll.name.as_deref().unwrap_or("Poll");
        return format!("[Poll: {}]", name);
    }

    if let Some(ref poll) = proto.poll_creation_message_v2 {
        let name = poll.name.as_deref().unwrap_or("Poll");
        return format!("[Poll: {}]", name);
    }

    if let Some(ref poll) = proto.poll_creation_message_v3 {
        let name = poll.name.as_deref().unwrap_or("Poll");
        return format!("[Poll: {}]", name);
    }

    if proto.poll_update_message.is_some() {
        return "[Poll Vote]".to_string();
    }

    if let Some(ref proto_msg) = proto.protocol_message {
        if proto_msg.r#type
            == Some(baileys_proto::proto::message::protocol_message::Type::Revoke as i32)
        {
            return "[Deleted Message]".to_string();
        }
    }

    if let Some(ref inter) = proto.interactive_message {
        if let Some(ref body) = inter.body {
            if let Some(ref t) = body.text {
                if !t.is_empty() {
                    return t.clone();
                }
            }
        }
        if let Some(ref header) = inter.header {
            if let Some(ref t) = header.title {
                if !t.is_empty() {
                    return t.clone();
                }
            }
        }
    }

    if let Some(ref btn) = proto.buttons_message {
        if let Some(ref text) = btn.content_text {
            if !text.is_empty() {
                return text.clone();
            }
        }
    }

    if let Some(ref tmpl) = proto.template_message {
        if let Some(ref hyd) = tmpl.hydrated_template {
            if let Some(ref text) = hyd.hydrated_content_text {
                if !text.is_empty() {
                    return text.clone();
                }
            }
        }
    }

    if let Some(ref inv) = proto.group_invite_message {
        let name = inv.group_name.as_deref().unwrap_or("Group");
        return format!("[Group Invite: {}]", name);
    }

    if let Some(ref vo) = proto.view_once_message {
        if let Some(ref m) = vo.message {
            return format!("[View Once] {}", extract_message_summary(m));
        }
    }

    if let Some(ref vo) = proto.view_once_message_v2 {
        if let Some(ref m) = vo.message {
            return format!("[View Once] {}", extract_message_summary(m));
        }
    }

    if let Some(ref vo) = proto.view_once_message_v2_extension {
        if let Some(ref m) = vo.message {
            return format!("[View Once] {}", extract_message_summary(m));
        }
    }

    if let Some(ref prod) = proto.product_message {
        let title = prod
            .product
            .as_ref()
            .and_then(|p| p.title.as_deref())
            .unwrap_or("Product");
        return format!("[Product: {}]", title);
    }

    if let Some(ref ord) = proto.order_message {
        let title = ord.order_title.as_deref().unwrap_or("Order");
        let id = ord.order_id.as_deref().unwrap_or("");
        if !id.is_empty() {
            return format!("[Order: {} ({})]", title, id);
        } else {
            return format!("[Order: {}]", title);
        }
    }

    if let Some(ref ev) = proto.event_message {
        let name = ev.name.as_deref().unwrap_or("Event");
        return format!("[Event: {}]", name);
    }

    if let Some(ref alb) = proto.album_message {
        let count = alb.expected_image_count.unwrap_or(0) + alb.expected_video_count.unwrap_or(0);
        return format!("[Album: {} items]", count);
    }

    if let Some(ref resp) = proto.interactive_response_message {
        if let Some(baileys_proto::proto::message::interactive_response_message::InteractiveResponseMessage::NativeFlowResponseMessage(ref nf)) = resp.interactive_response_message {
            let btn_type = nf.name.as_deref().unwrap_or("button");
            if let Some(ref pj) = nf.params_json {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(pj) {
                    let id = parsed.get("id").and_then(|v| v.as_str())
                        .or_else(|| parsed.get("body").and_then(|v| v.as_str()))
                        .unwrap_or("");
                    let text = parsed.get("display_text").and_then(|v| v.as_str())
                        .or_else(|| parsed.get("title").and_then(|v| v.as_str()))
                        .unwrap_or("");
                    if !id.is_empty() && !text.is_empty() {
                        return format!("[Button Click: {} (id: {}, text: {})]", btn_type, id, text);
                    } else if !id.is_empty() {
                        return format!("[Button Click: {} (id: {})]", btn_type, id);
                    } else if !text.is_empty() {
                        return format!("[Button Click: {} (text: {})]", btn_type, text);
                    }
                }
            }
            return format!("[Button Click: {}]", btn_type);
        }
        if let Some(ref body) = resp.body {
            if let Some(ref t) = body.text {
                if !t.is_empty() {
                    return format!("[Response] {}", t);
                }
            }
        }
        return "[Interactive Response]".to_string();
    }

    if let Some(ref resp) = proto.buttons_response_message {
        let id = resp.selected_button_id.as_deref().unwrap_or("");
        let text = match resp.response {
            Some(baileys_proto::proto::message::buttons_response_message::Response::SelectedDisplayText(ref t)) => t.as_str(),
            _ => "",
        };
        if !id.is_empty() && !text.is_empty() {
            return format!("[Button Response: {} ({})]", id, text);
        } else if !id.is_empty() {
            return format!("[Button Response: {}]", id);
        } else if !text.is_empty() {
            return format!("[Button Response: {}]", text);
        } else {
            return "[Button Response]".to_string();
        }
    }

    if let Some(ref resp) = proto.list_response_message {
        let id = resp
            .single_select_reply
            .as_ref()
            .and_then(|r| r.selected_row_id.as_deref())
            .unwrap_or("");
        let title = resp.title.as_deref().unwrap_or("");
        if !id.is_empty() && !title.is_empty() {
            return format!("[List Selection: {} ({})]", id, title);
        } else if !id.is_empty() {
            return format!("[List Selection: {}]", id);
        } else if !title.is_empty() {
            return format!("[List Selection: {}]", title);
        } else {
            return "[List Selection]".to_string();
        }
    }

    if let Some(ref resp) = proto.template_button_reply_message {
        let id = resp.selected_id.as_deref().unwrap_or("");
        let text = resp.selected_display_text.as_deref().unwrap_or("");
        if !id.is_empty() && !text.is_empty() {
            return format!("[Template Reply: {} ({})]", id, text);
        } else if !id.is_empty() {
            return format!("[Template Reply: {}]", id);
        } else if !text.is_empty() {
            return format!("[Template Reply: {}]", text);
        } else {
            return "[Template Reply]".to_string();
        }
    }

    "<rich content>".to_string()
}

/// Returns true if the protobuf contains an actual user message or media,
/// and not merely protocol signaling like SenderKeyDistributionMessage.
pub fn has_real_message_content(proto: &Message) -> bool {
    proto.conversation.is_some()
        || proto.extended_text_message.is_some()
        || proto.image_message.is_some()
        || proto.video_message.is_some()
        || proto.ptv_message.is_some()
        || proto.audio_message.is_some()
        || proto.sticker_message.is_some()
        || proto.document_message.is_some()
        || proto.contact_message.is_some()
        || proto.contacts_array_message.is_some()
        || proto.location_message.is_some()
        || proto.live_location_message.is_some()
        || proto.group_invite_message.is_some()
        || proto.product_message.is_some()
        || proto.order_message.is_some()
        || proto.reaction_message.is_some()
        || proto.poll_creation_message.is_some()
        || proto.poll_creation_message_v2.is_some()
        || proto.poll_creation_message_v3.is_some()
        || proto.poll_update_message.is_some()
        || proto.interactive_message.is_some()
        || proto.interactive_response_message.is_some()
        || proto.event_message.is_some()
        || proto.album_message.is_some()
        || proto.buttons_message.is_some()
        || proto.buttons_response_message.is_some()
        || proto.template_message.is_some()
        || proto.template_button_reply_message.is_some()
        || proto.list_message.is_some()
        || proto.list_response_message.is_some()
        || proto.protocol_message.is_some()
        || proto
            .ephemeral_message
            .as_ref()
            .and_then(|e| e.message.as_ref())
            .map(|m| has_real_message_content(m))
            .unwrap_or(false)
        || proto
            .view_once_message
            .as_ref()
            .and_then(|v| v.message.as_ref())
            .map(|m| has_real_message_content(m))
            .unwrap_or(false)
        || proto
            .view_once_message_v2
            .as_ref()
            .and_then(|v| v.message.as_ref())
            .map(|m| has_real_message_content(m))
            .unwrap_or(false)
        || proto
            .view_once_message_v2_extension
            .as_ref()
            .and_then(|v| v.message.as_ref())
            .map(|m| has_real_message_content(m))
            .unwrap_or(false)
        || proto
            .document_with_caption_message
            .as_ref()
            .and_then(|d| d.message.as_ref())
            .map(|m| has_real_message_content(m))
            .unwrap_or(false)
        || proto
            .edited_message
            .as_ref()
            .and_then(|d| d.message.as_ref())
            .map(|m| has_real_message_content(m))
            .unwrap_or(false)
        || proto
            .device_sent_message
            .as_ref()
            .and_then(|d| d.message.as_ref())
            .map(|m| has_real_message_content(m))
            .unwrap_or(false)
}

/// Returns true if the message contains media attachments (even inside envelopes).
pub fn message_has_media(proto: &Message) -> bool {
    if let Some(ref eph) = proto.ephemeral_message {
        if let Some(ref inner) = eph.message {
            return message_has_media(inner);
        }
    }
    if let Some(ref vo) = proto.view_once_message {
        if let Some(ref inner) = vo.message {
            return message_has_media(inner);
        }
    }
    if let Some(ref vo) = proto.view_once_message_v2 {
        if let Some(ref inner) = vo.message {
            return message_has_media(inner);
        }
    }
    if let Some(ref vo) = proto.view_once_message_v2_extension {
        if let Some(ref inner) = vo.message {
            return message_has_media(inner);
        }
    }
    if let Some(ref dwc) = proto.document_with_caption_message {
        if let Some(ref inner) = dwc.message {
            return message_has_media(inner);
        }
    }
    proto.image_message.is_some()
        || proto.video_message.is_some()
        || proto.ptv_message.is_some()
        || proto.audio_message.is_some()
        || proto.document_message.is_some()
        || proto.sticker_message.is_some()
        || proto.lottie_sticker_message.is_some()
}

/// Recursively converts JSON object keys from camelCase to snake_case so Baileys JSON
/// can be directly deserialized into prost-generated Message structs.
pub fn convert_camel_to_snake_value(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => {
            let mut new_map = serde_json::Map::new();
            for (k, val) in map {
                let snake_k = to_snake_case(k);
                new_map.insert(snake_k, convert_camel_to_snake_value(val));
            }
            serde_json::Value::Object(new_map)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(convert_camel_to_snake_value).collect())
        }
        _ => v.clone(),
    }
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch);
        }
    }
    result
}

fn extract_context_info_from_json(json_val: &serde_json::Value) -> Option<ContextInfo> {
    let mut ctx = ContextInfo::default();
    let mut has_ctx = false;

    // 1. Mentions: mentions: ["123@s.whatsapp.net"]
    if let Some(mentions) = json_val.get("mentions").and_then(|m| m.as_array()) {
        let jids: Vec<String> = mentions
            .iter()
            .filter_map(|j| j.as_str().map(|s| s.to_string()))
            .collect();
        if !jids.is_empty() {
            ctx.mentioned_jid = jids;
            has_ctx = true;
        }
    }

    // 2. Quoted context: quoted: { key: { id, remoteJid, participant }, message: { ... } }
    if let Some(quoted) = json_val.get("quoted") {
        if let Some(key) = quoted.get("key") {
            ctx.stanza_id = key
                .get("id")
                .and_then(|i| i.as_str())
                .map(|s| s.to_string());
            ctx.participant = key
                .get("participant")
                .or_else(|| key.get("remoteJid"))
                .and_then(|p| p.as_str())
                .map(|s| s.to_string());
            has_ctx = true;
        }
        if let Some(msg_val) = quoted.get("message") {
            if let Ok(quoted_msg) = parse_message_from_json(msg_val) {
                ctx.quoted_message = Some(Box::new(quoted_msg));
                has_ctx = true;
            }
        }
    }

    if has_ctx {
        Some(ctx)
    } else {
        None
    }
}

fn attach_context_info(mut msg: Message, ctx: ContextInfo) -> Message {
    let boxed_ctx = Some(Box::new(ctx));
    if let Some(ref mut ext) = msg.extended_text_message {
        ext.context_info = boxed_ctx;
    } else if let Some(ref mut img) = msg.image_message {
        img.context_info = boxed_ctx;
    } else if let Some(ref mut vid) = msg.video_message {
        vid.context_info = boxed_ctx;
    } else if let Some(ref mut aud) = msg.audio_message {
        aud.context_info = boxed_ctx;
    } else if let Some(ref mut doc) = msg.document_message {
        doc.context_info = boxed_ctx;
    } else if let Some(ref mut stk) = msg.sticker_message {
        stk.context_info = boxed_ctx;
    } else if let Some(ref mut con) = msg.contact_message {
        con.context_info = boxed_ctx;
    } else if let Some(ref mut arr) = msg.contacts_array_message {
        arr.context_info = boxed_ctx;
    } else if let Some(ref mut loc) = msg.location_message {
        loc.context_info = boxed_ctx;
    } else if let Some(ref mut lloc) = msg.live_location_message {
        lloc.context_info = boxed_ctx;
    } else if let Some(ref mut btn) = msg.buttons_message {
        btn.context_info = boxed_ctx;
    } else if let Some(ref mut inter) = msg.interactive_message {
        inter.context_info = boxed_ctx;
    } else if let Some(ref mut ev) = msg.event_message {
        ev.context_info = boxed_ctx;
    } else if let Some(ref mut alb) = msg.album_message {
        alb.context_info = boxed_ctx;
    } else if let Some(text) = msg.conversation.take() {
        // Upgrade simple conversation to ExtendedTextMessage with context_info!
        msg.extended_text_message = Some(Box::new(
            baileys_proto::proto::message::ExtendedTextMessage {
                text: Some(text),
                context_info: boxed_ctx,
                ..Default::default()
            },
        ));
    }
    msg
}

/// Parses a serde_json::Value (either camelCase Baileys format, snake_case proto format,
/// or Baileys Node.js shorthand keys: text, location, liveLocation, contact, contacts,
/// poll, event, album, image, video, audio, document, sticker, interactive, buttons, react)
/// into a strongly-typed protobuf Message struct with full future-proof and context support.
pub fn parse_message_from_json(json_val: &serde_json::Value) -> Result<Message, String> {
    let ctx_opt = extract_context_info_from_json(json_val);
    let view_once = json_val
        .get("viewOnce")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut msg = parse_message_payload_from_json(json_val)?;

    if let Some(ctx) = ctx_opt {
        msg = attach_context_info(msg, ctx);
    }

    if view_once {
        msg = crate::proto_builders::wrap_view_once(msg, true);
    }

    Ok(msg)
}

fn parse_message_payload_from_json(json_val: &serde_json::Value) -> Result<Message, String> {
    // 1. Shorthand { text: "..." }
    if let Some(text) = json_val.get("text").and_then(|t| t.as_str()) {
        // If buttons or interactive fields are also present, allow those handlers to take precedence
        if json_val.get("buttons").is_none() && json_val.get("interactive").is_none() {
            return Ok(Message {
                conversation: Some(text.to_string()),
                ..Default::default()
            });
        }
    }

    // 2. Direct conversation { conversation: "..." }
    if let Some(conv) = json_val.get("conversation").and_then(|c| c.as_str()) {
        return Ok(Message {
            conversation: Some(conv.to_string()),
            ..Default::default()
        });
    }

    // 3. Shorthand location { location: { degreesLatitude, degreesLongitude, name, address } }
    if let Some(loc) = json_val.get("location") {
        let lat = loc
            .get("degreesLatitude")
            .or_else(|| loc.get("latitude"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let lon = loc
            .get("degreesLongitude")
            .or_else(|| loc.get("longitude"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let name = loc.get("name").and_then(|v| v.as_str());
        let address = loc.get("address").and_then(|v| v.as_str());
        return Ok(crate::proto_builders::build_location(
            lat, lon, name, address,
        ));
    }

    // 4. Shorthand live location { liveLocation: { degreesLatitude, degreesLongitude, caption, accuracyInMeters } }
    if let Some(lloc) = json_val
        .get("liveLocation")
        .or_else(|| json_val.get("live_location"))
    {
        let lat = lloc
            .get("degreesLatitude")
            .or_else(|| lloc.get("latitude"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let lon = lloc
            .get("degreesLongitude")
            .or_else(|| lloc.get("longitude"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let caption = lloc.get("caption").and_then(|v| v.as_str());
        let acc = lloc
            .get("accuracyInMeters")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
        return Ok(crate::proto_builders::build_live_location(
            lat, lon, caption, acc,
        ));
    }

    // 5. Shorthand contact { contact: { displayName, vcard } }
    if let Some(con) = json_val.get("contact") {
        let name = con
            .get("displayName")
            .or_else(|| con.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("Contact");
        let vcard = con.get("vcard").and_then(|v| v.as_str()).unwrap_or("");
        return Ok(crate::proto_builders::build_contact(name, vcard));
    }

    // 6. Shorthand contacts array { contacts: { displayName, contacts: [...] } }
    if let Some(contacts) = json_val.get("contacts") {
        let name = contacts
            .get("displayName")
            .and_then(|v| v.as_str())
            .unwrap_or("Contacts");
        let list = contacts.get("contacts").and_then(|v| v.as_array());
        let mut contact_items = Vec::new();
        if let Some(items) = list {
            for item in items {
                let fn_name = item
                    .get("displayName")
                    .or_else(|| item.get("name"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let vcard = item
                    .get("vcard")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                contact_items.push(baileys_proto::proto::message::ContactMessage {
                    display_name: fn_name,
                    vcard,
                    ..Default::default()
                });
            }
        }
        return Ok(crate::proto_builders::build_contacts_array(
            name,
            contact_items,
        ));
    }

    // 7. Shorthand poll { poll: { name, values, selectableCount } }
    if let Some(poll) = json_val.get("poll") {
        let name = poll.get("name").and_then(|v| v.as_str()).unwrap_or("Poll");
        let options_vals = poll
            .get("values")
            .or_else(|| poll.get("options"))
            .and_then(|v| v.as_array());
        let options: Vec<&str> = options_vals
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        let selectable = poll
            .get("selectableCount")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(1);
        return Ok(crate::proto_builders::build_poll(
            name, &options, selectable,
        ));
    }

    // 8. Shorthand event { event: { name, description, startTime, endTime, location, joinLink, isCanceled } }
    if let Some(ev) = json_val.get("event") {
        let name = ev.get("name").and_then(|v| v.as_str()).unwrap_or("Event");
        let desc = ev.get("description").and_then(|v| v.as_str());
        let start_time = ev
            .get("startTime")
            .and_then(|v| v.as_i64())
            .unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64
            });
        let end_time = ev.get("endTime").and_then(|v| v.as_i64());
        let loc_name = ev.get("location").and_then(|v| v.as_str());
        let join_link = ev.get("joinLink").and_then(|v| v.as_str());
        let is_canceled = ev
            .get("isCanceled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        return Ok(crate::proto_builders::build_event_message(
            name,
            desc,
            start_time,
            end_time,
            loc_name,
            join_link,
            is_canceled,
        ));
    }

    // 9. Shorthand album { album: { imageCount, videoCount } }
    if let Some(alb) = json_val.get("album") {
        let img_count = alb
            .get("imageCount")
            .or_else(|| alb.get("expectedImageCount"))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(0);
        let vid_count = alb
            .get("videoCount")
            .or_else(|| alb.get("expectedVideoCount"))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(0);
        return Ok(crate::proto_builders::build_album_message(
            img_count, vid_count,
        ));
    }

    // 10. Shorthand react { react: { text, key: { remoteJid, id } } }
    if let Some(react) = json_val.get("react") {
        let emoji = react.get("text").and_then(|v| v.as_str()).unwrap_or("❤️");
        let target_key = if let Some(k) = react.get("key") {
            MessageKey {
                remote_jid: k
                    .get("remoteJid")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                from_me: k.get("fromMe").and_then(|v| v.as_bool()),
                id: k.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()),
                participant: k
                    .get("participant")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            }
        } else {
            MessageKey::default()
        };
        return Ok(crate::proto_builders::build_reaction(&target_key, emoji));
    }

    // 11. Shorthand interactive { interactive: { header, body, footer, buttons: [...] } }
    if let Some(inter) = json_val.get("interactive") {
        let header = inter.get("header").and_then(|v| v.as_str());
        let body = inter.get("body").and_then(|v| v.as_str()).unwrap_or("");
        let footer = inter.get("footer").and_then(|v| v.as_str());
        let mut btn_list = Vec::new();
        if let Some(btns) = inter.get("buttons").and_then(|v| v.as_array()) {
            for b in btns {
                let b_type = b.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match b_type {
                    "url" | "cta_url" => {
                        let text = b
                            .get("text")
                            .or_else(|| b.get("displayText"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let url = b.get("url").and_then(|v| v.as_str()).unwrap_or("");
                        btn_list.push(crate::proto_builders::build_cta_url_button(text, url, None));
                    }
                    "call" | "cta_call" => {
                        let text = b
                            .get("text")
                            .or_else(|| b.get("displayText"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let phone = b
                            .get("phone")
                            .or_else(|| b.get("phoneNumber"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        btn_list.push(crate::proto_builders::build_cta_call_button(text, phone));
                    }
                    "copy" | "cta_copy" => {
                        let text = b
                            .get("text")
                            .or_else(|| b.get("displayText"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let code = b
                            .get("code")
                            .or_else(|| b.get("copyCode"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        btn_list.push(crate::proto_builders::build_cta_copy_button(text, code));
                    }
                    "quick_reply" | "reply" => {
                        let text = b
                            .get("text")
                            .or_else(|| b.get("displayText"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let id = b.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        btn_list.push(crate::proto_builders::build_quick_reply_button(text, id));
                    }
                    _ => {
                        if let Some(name) = b.get("name").and_then(|v| v.as_str()) {
                            let params = b
                                .get("buttonParamsJson")
                                .or_else(|| b.get("params"))
                                .map(|p| {
                                    if p.is_string() {
                                        p.as_str().unwrap().to_string()
                                    } else {
                                        p.to_string()
                                    }
                                })
                                .unwrap_or_else(|| "{}".to_string());
                            btn_list.push(crate::proto_builders::build_native_flow_button(
                                name, &params,
                            ));
                        }
                    }
                }
            }
        }
        return Ok(crate::proto_builders::build_interactive_message(
            header, body, footer, btn_list, false,
        ));
    }

    // 12. Shorthand legacy buttons { buttons: [...], text, footer }
    if let Some(btns) = json_val.get("buttons").and_then(|v| v.as_array()) {
        let text = json_val.get("text").and_then(|v| v.as_str()).unwrap_or("");
        let footer = json_val.get("footer").and_then(|v| v.as_str());
        let mut labels = Vec::new();
        for b in btns {
            let label = b
                .get("buttonText")
                .and_then(|bt| bt.get("displayText"))
                .or_else(|| b.get("text"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            labels.push(label);
        }
        return Ok(crate::proto_builders::build_quick_reply_buttons(
            text, footer, &labels,
        ));
    }

    // 13. Shorthand image { image: { url }, caption, mimetype }
    if let Some(img) = json_val.get("image") {
        let url = if img.is_string() {
            img.as_str()
        } else {
            img.get("url").and_then(|v| v.as_str())
        };
        let caption = json_val
            .get("caption")
            .or_else(|| img.get("caption"))
            .and_then(|v| v.as_str());
        let mimetype = json_val
            .get("mimetype")
            .or_else(|| img.get("mimetype"))
            .and_then(|v| v.as_str())
            .unwrap_or("image/jpeg");
        return Ok(crate::proto_builders::build_image(
            url,
            caption,
            mimetype,
            Vec::new(),
            0,
            Vec::new(),
            None,
        ));
    }

    // 14. Shorthand video { video: { url }, caption, mimetype, gifPlayback }
    if let Some(vid) = json_val.get("video") {
        let url = if vid.is_string() {
            vid.as_str()
        } else {
            vid.get("url").and_then(|v| v.as_str())
        };
        let caption = json_val
            .get("caption")
            .or_else(|| vid.get("caption"))
            .and_then(|v| v.as_str());
        let mimetype = json_val
            .get("mimetype")
            .or_else(|| vid.get("mimetype"))
            .and_then(|v| v.as_str())
            .unwrap_or("video/mp4");
        let gif = json_val
            .get("gifPlayback")
            .or_else(|| vid.get("gifPlayback"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        return Ok(crate::proto_builders::build_video(
            url,
            caption,
            mimetype,
            Vec::new(),
            0,
            Vec::new(),
            None,
            gif,
            0,
        ));
    }

    // 15. Shorthand audio { audio: { url }, ptt, mimetype }
    if let Some(aud) = json_val.get("audio") {
        let url = if aud.is_string() {
            aud.as_str()
        } else {
            aud.get("url").and_then(|v| v.as_str())
        };
        let mimetype = json_val
            .get("mimetype")
            .or_else(|| aud.get("mimetype"))
            .and_then(|v| v.as_str())
            .unwrap_or("audio/ogg; codecs=opus");
        let ptt = json_val
            .get("ptt")
            .or_else(|| aud.get("ptt"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        return Ok(crate::proto_builders::build_audio(
            url,
            mimetype,
            Vec::new(),
            0,
            Vec::new(),
            None,
            ptt,
            0,
        ));
    }

    // 16. Shorthand document { document: { url }, fileName, mimetype, caption }
    if let Some(doc) = json_val.get("document") {
        let url = if doc.is_string() {
            doc.as_str()
        } else {
            doc.get("url").and_then(|v| v.as_str())
        };
        let file_name = json_val
            .get("fileName")
            .or_else(|| doc.get("fileName"))
            .and_then(|v| v.as_str())
            .unwrap_or("document");
        let mimetype = json_val
            .get("mimetype")
            .or_else(|| doc.get("mimetype"))
            .and_then(|v| v.as_str())
            .unwrap_or("application/octet-stream");
        let caption = json_val
            .get("caption")
            .or_else(|| doc.get("caption"))
            .and_then(|v| v.as_str());
        return Ok(crate::proto_builders::build_document(
            url,
            file_name,
            caption,
            mimetype,
            Vec::new(),
            0,
            Vec::new(),
            None,
        ));
    }

    // 17. Shorthand sticker { sticker: { url } }
    if let Some(stk) = json_val.get("sticker") {
        let url = if stk.is_string() {
            stk.as_str()
        } else {
            stk.get("url").and_then(|v| v.as_str())
        };
        let is_animated = json_val
            .get("isAnimated")
            .or_else(|| stk.get("isAnimated"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        return Ok(crate::proto_builders::build_sticker(
            url,
            Vec::new(),
            0,
            Vec::new(),
            None,
            is_animated,
        ));
    }

    // 18. Extended text message { extendedTextMessage: { text, ... } }
    if let Some(ext) = json_val
        .get("extendedTextMessage")
        .or_else(|| json_val.get("extended_text_message"))
    {
        let text = ext
            .get("text")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string());
        let matched_text = ext
            .get("matchedText")
            .or_else(|| ext.get("matched_text"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string());
        let title = ext
            .get("title")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string());
        let description = ext
            .get("description")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string());

        return Ok(Message {
            extended_text_message: Some(Box::new(
                baileys_proto::proto::message::ExtendedTextMessage {
                    text,
                    matched_text,
                    title,
                    description,
                    ..Default::default()
                },
            )),
            ..Default::default()
        });
    }

    // 19. Generic serde fallback (converting camelCase to snake_case for full protobuf parity)
    let converted = convert_camel_to_snake_value(json_val);
    serde_json::from_value::<Message>(converted)
        .map_err(|e| format!("Failed to parse Message protobuf from JSON: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use baileys_proto::proto::message::*;

    #[test]
    fn test_extract_message_summary_various_types() {
        // Conversation
        let text_msg = Message {
            conversation: Some("Hello WhatsApp".into()),
            ..Default::default()
        };
        assert_eq!(extract_message_summary(&text_msg), "Hello WhatsApp");

        // Image with caption
        let img_msg = Message {
            image_message: Some(Box::new(ImageMessage {
                caption: Some("Vacation photo".into()),
                ..Default::default()
            })),
            ..Default::default()
        };
        assert_eq!(extract_message_summary(&img_msg), "[Image] Vacation photo");

        // Voice note (PTT)
        let vn_msg = Message {
            audio_message: Some(Box::new(AudioMessage {
                seconds: Some(15),
                ptt: Some(true),
                ..Default::default()
            })),
            ..Default::default()
        };
        assert_eq!(extract_message_summary(&vn_msg), "[Voice Note: 15s]");

        // Document
        let doc_msg = Message {
            document_message: Some(Box::new(DocumentMessage {
                file_name: Some("report.pdf".into()),
                ..Default::default()
            })),
            ..Default::default()
        };
        assert_eq!(extract_message_summary(&doc_msg), "[Document: report.pdf]");

        // Ephemeral Message containing text
        let eph_msg = Message {
            ephemeral_message: Some(Box::new(FutureProofMessage {
                message: Some(Box::new(Message {
                    conversation: Some("Disappearing message text".into()),
                    ..Default::default()
                })),
            })),
            ..Default::default()
        };
        assert_eq!(
            extract_message_summary(&eph_msg),
            "Disappearing message text"
        );
        assert!(has_real_message_content(&eph_msg));

        // Pure SenderKeyDistributionMessage (signaling only)
        let skdm_msg = Message {
            sender_key_distribution_message: Some(SenderKeyDistributionMessage {
                group_id: Some("12036302@g.us".into()),
                axolotl_sender_key_distribution_message: Some(vec![1, 2, 3]),
            }),
            ..Default::default()
        };
        assert!(!has_real_message_content(&skdm_msg));
    }

    #[test]
    fn test_parse_message_from_json_shorthands() {
        // 1. Shorthand { text: "...", mentions: [...] }
        let v1 = serde_json::json!({
            "text": "hello @alice",
            "mentions": ["alice@s.whatsapp.net"]
        });
        let m1 = parse_message_from_json(&v1).unwrap();
        let ext = m1
            .extended_text_message
            .expect("extended_text_message present");
        assert_eq!(ext.text.as_deref(), Some("hello @alice"));
        let ctx = ext.context_info.expect("context_info present");
        assert_eq!(ctx.mentioned_jid, vec!["alice@s.whatsapp.net".to_string()]);

        // 2. Direct proto field
        let v2 = serde_json::json!({ "conversation": "hello direct" });
        let m2 = parse_message_from_json(&v2).unwrap();
        assert_eq!(m2.conversation.as_deref(), Some("hello direct"));

        // 3. Shorthand location
        let v3 = serde_json::json!({
            "location": {
                "degreesLatitude": 37.7749,
                "degreesLongitude": -122.4194,
                "name": "SF",
                "address": "California"
            }
        });
        let m3 = parse_message_from_json(&v3).unwrap();
        let loc = m3.location_message.expect("location_message present");
        assert_eq!(loc.degrees_latitude, Some(37.7749));
        assert_eq!(loc.name.as_deref(), Some("SF"));

        // 4. Shorthand poll
        let v4 = serde_json::json!({
            "poll": {
                "name": "Team Lunch?",
                "values": ["Pizza", "Burger"],
                "selectableCount": 1
            }
        });
        let m4 = parse_message_from_json(&v4).unwrap();
        let poll = m4
            .poll_creation_message
            .expect("poll_creation_message present");
        assert_eq!(poll.name.as_deref(), Some("Team Lunch?"));
        assert_eq!(poll.options.len(), 2);

        // 5. Shorthand event
        let v5 = serde_json::json!({
            "event": {
                "name": "Sprint Retrospective",
                "startTime": 1700000000,
                "location": "Meet"
            }
        });
        let m5 = parse_message_from_json(&v5).unwrap();
        let ev = m5.event_message.expect("event_message present");
        assert_eq!(ev.name.as_deref(), Some("Sprint Retrospective"));

        // 6. Shorthand interactive with viewOnce
        let v6 = serde_json::json!({
            "interactive": {
                "body": "Click below",
                "buttons": [
                    { "type": "url", "text": "Website", "url": "https://example.com" }
                ]
            },
            "viewOnce": true
        });
        let m6 = parse_message_from_json(&v6).unwrap();
        assert!(m6.view_once_message_v2.is_some());
    }

    #[test]
    fn test_extract_message_summary_responses() {
        use baileys_proto::proto::message::interactive_response_message::NativeFlowResponseMessage;
        use baileys_proto::proto::message::list_response_message::SingleSelectReply;
        use baileys_proto::proto::message::{
            ButtonsResponseMessage, InteractiveResponseMessage, ListResponseMessage,
            TemplateButtonReplyMessage,
        };

        // 1. Native flow button click
        let m_nf = Message {
            interactive_response_message: Some(Box::new(InteractiveResponseMessage {
                interactive_response_message: Some(
                    baileys_proto::proto::message::interactive_response_message::InteractiveResponseMessage::NativeFlowResponseMessage(
                        NativeFlowResponseMessage {
                            name: Some("quick_reply".to_string()),
                            params_json: Some(r#"{"id":"btn_confirm","display_text":"Confirm Booking"}"#.to_string()),
                            ..Default::default()
                        },
                    ),
                ),
                ..Default::default()
            })),
            ..Default::default()
        };
        assert_eq!(
            extract_message_summary(&m_nf),
            "[Button Click: quick_reply (id: btn_confirm, text: Confirm Booking)]"
        );

        // 2. Buttons response
        let m_btn = Message {
            buttons_response_message: Some(Box::new(ButtonsResponseMessage {
                selected_button_id: Some("opt_1".to_string()),
                response: Some(baileys_proto::proto::message::buttons_response_message::Response::SelectedDisplayText("Option 1".to_string())),
                ..Default::default()
            })),
            ..Default::default()
        };
        assert_eq!(
            extract_message_summary(&m_btn),
            "[Button Response: opt_1 (Option 1)]"
        );

        // 3. List response
        let m_list = Message {
            list_response_message: Some(Box::new(ListResponseMessage {
                title: Some("Coffee".to_string()),
                single_select_reply: Some(SingleSelectReply {
                    selected_row_id: Some("item_espresso".to_string()),
                }),
                ..Default::default()
            })),
            ..Default::default()
        };
        assert_eq!(
            extract_message_summary(&m_list),
            "[List Selection: item_espresso (Coffee)]"
        );

        // 4. Template reply
        let m_tpl = Message {
            template_button_reply_message: Some(Box::new(TemplateButtonReplyMessage {
                selected_id: Some("tpl_1".to_string()),
                selected_display_text: Some("Yes".to_string()),
                ..Default::default()
            })),
            ..Default::default()
        };
        assert_eq!(
            extract_message_summary(&m_tpl),
            "[Template Reply: tpl_1 (Yes)]"
        );
    }
}
