//! In-Chat Command Runner for Rose-Baileys CLI.
//!
//! Allows authorized WhatsApp chats (self-chat notes, 1-on-1 direct messages, and group chats)
//! to trigger commands directly using prefixes ('.', '!', '/').
//!
//! Supports three operation modes:
//! - Off: completely disabled (default)
//! - Self: responds strictly to messages from the bot account itself (from_me)
//! - Public: responds to public contacts with tiered security permissions

use baileys_core::{BaileysClient, MessageExt, RecognizedMessageType, WebMessageInfo};
use baileys_proto::schema::SchemaInspector;
use baileys_store::{AppStateStore, ChatStore, SqliteStore};
use prost::Message as ProstMessage;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use crate::ui::{print_comfy_event, strip_ansi, ANSI_BRIGHT_MAGENTA};

/// Active execution mode for in-chat command processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InChatMode {
    /// Completely disabled. In-chat commands are ignored.
    Off,
    /// Only responds to messages sent by the bot owner (from_me == true).
    SelfOnly,
    /// Responds to both self and external contacts in private chats and groups.
    Public,
}

impl InChatMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::SelfOnly => "self",
            Self::Public => "public",
        }
    }

    pub fn display_badge(&self) -> &'static str {
        match self {
            Self::Off => "🔴 OFF",
            Self::SelfOnly => "🟡 SELF-ONLY",
            Self::Public => "🟢 PUBLIC",
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "off" | "false" | "disable" | "disabled" | "0" => Some(Self::Off),
            "self" | "selfonly" | "me" | "private" | "owner" => Some(Self::SelfOnly),
            "public" | "all" | "enable" | "enabled" | "1" => Some(Self::Public),
            _ => None,
        }
    }
}

// Global in-memory cache for fast zero-latency event processing.
// 0 = Off (default), 1 = SelfOnly, 2 = Public
static IN_CHAT_MODE_CACHE: AtomicU8 = AtomicU8::new(0);

/// Key used for persistence in SQLite kv_store.
pub const KV_STORE_KEY_MODE: &str = "in_chat_cmd_mode";

/// Returns the currently active in-chat command mode.
pub fn get_mode() -> InChatMode {
    match IN_CHAT_MODE_CACHE.load(Ordering::Relaxed) {
        1 => InChatMode::SelfOnly,
        2 => InChatMode::Public,
        _ => InChatMode::Off,
    }
}

/// Updates the in-memory mode cache.
pub fn set_mode_memory(mode: InChatMode) {
    let val = match mode {
        InChatMode::Off => 0,
        InChatMode::SelfOnly => 1,
        InChatMode::Public => 2,
    };
    IN_CHAT_MODE_CACHE.store(val, Ordering::Relaxed);
}

/// Initializes the mode from the SQLite kv_store, defaulting to Off.
pub async fn init_mode_from_store(store: &SqliteStore) -> InChatMode {
    let mode = match store.get_kv(KV_STORE_KEY_MODE).await {
        Ok(Some(val)) => InChatMode::from_str_loose(&val).unwrap_or(InChatMode::Off),
        _ => InChatMode::Off,
    };
    set_mode_memory(mode);
    mode
}

/// Updates and persists the mode in the SQLite kv_store.
pub async fn set_mode_and_persist(store: &SqliteStore, mode: InChatMode) -> Result<(), String> {
    set_mode_memory(mode);
    store
        .set_kv(KV_STORE_KEY_MODE, mode.as_str())
        .await
        .map_err(|e| e.to_string())
}

/// Extracts command name and arguments if the text starts with a recognized prefix ('.', '!', '/').
pub fn extract_command(text: &str) -> Option<(&str, &str)> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let first = trimmed.chars().next()?;
    if first == '.' || first == '!' || first == '/' {
        let remainder = &trimmed[first.len_utf8()..].trim_start();
        if remainder.is_empty() {
            return None;
        }
        let mut parts = remainder.splitn(2, char::is_whitespace);
        let cmd = parts.next().unwrap_or("");
        let args = parts.next().unwrap_or("").trim();
        Some((cmd, args))
    } else {
        None
    }
}

/// Returns true if the command is strictly restricted to the bot owner.
pub fn is_restricted_command(cmd: &str, args: &str) -> bool {
    let cmd_lower = cmd.to_lowercase();
    match cmd_lower.as_str() {
        "ping" | "status" | "onwa" | "urlpreview" | "poll" | "polltally" | "calllink" | "qr"
        | "location" | "contact" | "event" | "help" => false,
        "proto" => {
            let first_arg = args.split_whitespace().next().unwrap_or("").to_lowercase();
            first_arg == "pull" || first_arg == "inspect"
        }
        _ => true,
    }
}

/// Strips ANSI terminal codes and caps the message length at 3,500 characters.
pub fn sanitize_for_whatsapp(raw: &str) -> String {
    let clean = strip_ansi(raw);
    let trimmed = clean.trim();
    const MAX_LEN: usize = 3500;
    if trimmed.len() > MAX_LEN {
        let mut truncated = trimmed[..MAX_LEN].to_string();
        truncated.push_str("\n\n⚠️ _(Output truncated at 3,500 characters)_");
        truncated
    } else {
        trimmed.to_string()
    }
}

/// Handles an incoming message event for in-chat command execution.
pub async fn handle_incoming_message(
    client: Arc<BaileysClient>,
    store: Arc<SqliteStore>,
    msg: Box<WebMessageInfo>,
) {
    let mode = get_mode();
    if mode == InChatMode::Off {
        return;
    }

    // Extract text from message (unwrapping ephemeral, view-once, edited, etc.)
    let text = match msg.message.as_ref().and_then(|m| m.extract_text()) {
        Some(t) => t,
        None => return,
    };

    // Check for command prefix
    let (cmd, args) = match extract_command(&text) {
        Some(ca) => ca,
        None => return,
    };

    let key = match msg.key.as_ref() {
        Some(k) => k,
        None => return,
    };

    let remote_jid = match key.remote_jid.as_deref() {
        Some(j) if !j.is_empty() => j,
        _ => return,
    };

    let msg_id = match key.id.as_deref() {
        Some(id) if !id.is_empty() => id,
        _ => return,
    };

    let from_me = key.from_me.unwrap_or(false);
    let participant = key.participant.as_deref().or(msg.participant.as_deref());
    let is_group = remote_jid.ends_with("@g.us");

    // Enforce Self-Only Mode
    if mode == InChatMode::SelfOnly && !from_me {
        return;
    }

    // Enforce Tiered Permissions in Public Mode
    if !from_me && is_restricted_command(cmd, args) {
        let denied = format!(
            "⛔ *Access Denied*: Command `.{}` is restricted to the bot owner.",
            cmd
        );
        let quote_preview = if text.len() > 80 {
            format!("{}...", &text[..80])
        } else {
            text.clone()
        };
        let _ = client
            .send_text_message_with_quote(
                remote_jid,
                &denied,
                msg_id,
                participant,
                Some(&quote_preview),
            )
            .await;
        return;
    }

    // Execute Command
    match execute_command(&client, &store, cmd, args, remote_jid, from_me, is_group).await {
        Ok(Some(reply)) => {
            let sanitized = sanitize_for_whatsapp(&reply);
            let quote_preview = if text.len() > 80 {
                format!("{}...", &text[..80])
            } else {
                text.clone()
            };

            // Attempt quoted reply, fallback to direct text message if quoting fails
            let send_res = client
                .send_text_message_with_quote(
                    remote_jid,
                    &sanitized,
                    msg_id,
                    participant,
                    Some(&quote_preview),
                )
                .await;

            if send_res.is_err() {
                let _ = client.send_text_message(remote_jid, &sanitized).await;
            }

            let caller_tier = if from_me { "Owner" } else { "Public" };
            print_comfy_event(
                "BOT-CMD",
                ANSI_BRIGHT_MAGENTA,
                &format!(".{} in {} ({})", cmd, remote_jid, caller_tier),
            );
        }
        Ok(None) => {
            // Silently ignored (e.g. unrecognized prefix in group chat)
        }
        Err(err) => {
            let error_msg = format!("❌ *Command Error*: {}", err);
            let _ = client
                .send_text_message_with_quote(
                    remote_jid,
                    &error_msg,
                    msg_id,
                    participant,
                    Some(&text),
                )
                .await;
        }
    }
}

/// Executes a parsed in-chat command and produces the reply text.
async fn execute_command(
    client: &Arc<BaileysClient>,
    store: &Arc<SqliteStore>,
    cmd: &str,
    args: &str,
    remote_jid: &str,
    from_me: bool,
    is_group: bool,
) -> Result<Option<String>, String> {
    match cmd.to_lowercase().as_str() {
        "ping" => match client.ping().await {
            Ok(dur) => Ok(Some(format!(
                "🏓 *Pong!*\n• Round-trip latency: *{:.2?}*\n• Gateway: *WhatsApp Web (Rust/Tokio)*\n• Node: *Rose-Baileys Engine*",
                dur
            ))),
            Err(e) => Ok(Some(format!("❌ *Ping Failed*: {}", e))),
        },

        "status" => {
            let connected = client.is_connected();
            let me_phone = client
                .me_jid()
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| "Not paired".to_string());
            let me_lid = client
                .me_lid()
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| "none".to_string());
            let cur_mode = get_mode();
            let auto_dl = if client.is_auto_download() {
                "Enabled"
            } else {
                "Disabled"
            };

            Ok(Some(format!(
                "📊 *Rose-Baileys Engine Status*\n• *Connection*: {}\n• *Phone JID*: `{}`\n• *LID*: `{}`\n• *In-Chat Bot*: *{}*\n• *Auto-Download*: {}",
                if connected { "🟢 Connected" } else { "🔴 Disconnected" },
                me_phone,
                me_lid,
                cur_mode.display_badge(),
                auto_dl
            )))
        }

        "proto" => handle_in_chat_proto(args, store, from_me).await,

        "onwa" => {
            let numbers: Vec<&str> = args
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            if numbers.is_empty() {
                return Ok(Some(
                    "ℹ️ *Usage*: `.onwa <number1, number2...>`\nExample: `.onwa 1234567890`"
                        .into(),
                ));
            }
            match client.check_on_whatsapp(&numbers).await {
                Ok(results) => {
                    let mut out = format!(
                        "📱 *WhatsApp Registration Query* ({} checked):\n",
                        results.len()
                    );
                    for res in results {
                        let status = if res.exists {
                            "✅ *REGISTERED*"
                        } else {
                            "❌ *NOT REGISTERED*"
                        };
                        let phone_display = if res.phone_number.is_empty() {
                            &res.jid
                        } else {
                            &res.phone_number
                        };
                        out.push_str(&format!(
                            "• `{}` -> {} (JID: `{}`)\n",
                            phone_display, status, res.jid
                        ));
                    }
                    Ok(Some(out))
                }
                Err(e) => Ok(Some(format!("❌ WhatsApp check failed: {}", e))),
            }
        }

        "urlpreview" => {
            let target_url = args.trim();
            if target_url.is_empty() {
                return Ok(Some(
                    "ℹ️ *Usage*: `.urlpreview <url>`\nExample: `.urlpreview https://rust-lang.org`"
                        .into(),
                ));
            }
            match client.fetch_url_preview(target_url).await {
                Ok(Some(prev)) => {
                    let mut out =
                        format!("🔗 *OpenGraph URL Preview*\n• *URL*: {}\n", prev.matched_text);
                    if let Some(t) = prev.title {
                        out.push_str(&format!("• *Title*: *{}*\n", t));
                    }
                    if let Some(d) = prev.description {
                        out.push_str(&format!("• *Description*: _{}_\n", d));
                    }
                    Ok(Some(out))
                }
                Ok(None) => Ok(Some(format!(
                    "ℹ️ No OpenGraph metadata found for `{}`.",
                    target_url
                ))),
                Err(e) => Ok(Some(format!("❌ URL preview fetch failed: {}", e))),
            }
        }

        "poll" => {
            let parts: Vec<&str> = args
                .split('|')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            if parts.len() < 3 {
                return Ok(Some(
                    "ℹ️ *Usage*: `.poll <Question> | <Option 1> | <Option 2> [| Option 3 ...]`\nExample: `.poll Favorite Color? | Blue | Green | Red`"
                        .into(),
                ));
            }
            let question = parts[0];
            let options: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
            match client.send_poll(remote_jid, question, &options, 1).await {
                Ok(id) => Ok(Some(format!(
                    "📊 *Poll Created Successfully!*\n• *ID*: `{}`\n• *Options*: {}",
                    id,
                    options.len()
                ))),
                Err(e) => Ok(Some(format!("❌ Poll creation failed: {}", e))),
            }
        }

        "cmdmode" => {
            if !from_me {
                return Ok(Some(
                    "⛔ *Access Denied*: Mode configuration is restricted to bot owner.".into(),
                ));
            }
            let target = args.trim();
            if target.is_empty() {
                let cur = get_mode();
                return Ok(Some(format!(
                    "⚙️ *In-Chat Bot Mode*\n• *Active Mode*: {}\n• Options: `off`, `self`, `public`\n• Usage: `.cmdmode <off|self|public>`",
                    cur.display_badge()
                )));
            }

            match InChatMode::from_str_loose(target) {
                Some(new_mode) => match set_mode_and_persist(store, new_mode).await {
                    Ok(()) => Ok(Some(format!(
                        "✅ *In-Chat Bot Mode Updated*\n• New Mode: {}",
                        new_mode.display_badge()
                    ))),
                    Err(e) => Ok(Some(format!("❌ Failed to persist mode: {}", e))),
                },
                None => Ok(Some(format!(
                    "❌ Invalid mode `{}`. Valid choices: `off`, `self`, `public`.",
                    target
                ))),
            }
        }

        "qr" => {
            if client.is_connected() {
                Ok(Some(
                    "✅ *Already Connected*: Authenticated to WhatsApp Web Gateway! No QR code needed."
                        .into(),
                ))
            } else {
                match crate::qr::get_latest_qr() {
                    Some(code) => Ok(Some(format!(
                        "🔑 *Pending WhatsApp QR Code Payload*:\n`{}`\n_Check terminal display for ASCII QR matrix._",
                        code
                    ))),
                    None => Ok(Some(
                        "ℹ️ No active QR code waiting. Either already connected or waiting for connection offer."
                            .into(),
                    )),
                }
            }
        }

        "location" => {
            let parts: Vec<&str> = args.split('|').map(|s| s.trim()).collect();
            let coords: Vec<&str> = parts[0].split_whitespace().collect();
            if coords.len() < 2 {
                return Ok(Some(
                    "ℹ️ *Usage*: `.location <lat> <lng> [| Name | Address]`\nExample: `.location 37.7749 -122.4194 | San Francisco | California`".into()
                ));
            }
            if let (Ok(lat), Ok(lng)) = (coords[0].parse::<f64>(), coords[1].parse::<f64>()) {
                let name = parts.get(1).copied();
                let addr = parts.get(2).copied();
                match client.send_location(remote_jid, lat, lng, name, addr, None).await {
                    Ok(id) => Ok(Some(format!("📍 *Location Sent!*\n• *ID*: `{}`\n• *Coordinates*: {}, {}", id, lat, lng))),
                    Err(e) => Ok(Some(format!("❌ Failed to send location: {}", e))),
                }
            } else {
                Ok(Some("❌ Invalid latitude or longitude numbers.".into()))
            }
        }

        "contact" => {
            let parts: Vec<&str> = args.split_whitespace().collect();
            if parts.len() < 2 {
                return Ok(Some(
                    "ℹ️ *Usage*: `.contact <Name> <Phone>`\nExample: `.contact Alice +1234567890`".into()
                ));
            }
            let name = parts[0];
            let phone = parts[1];
            match client.send_contact(remote_jid, name, phone).await {
                Ok(id) => Ok(Some(format!("👤 *Contact Card Sent!*\n• *ID*: `{}`\n• *Name*: {}\n• *Phone*: {}", id, name, phone))),
                Err(e) => Ok(Some(format!("❌ Failed to send contact: {}", e))),
            }
        }

        "event" => {
            let parts: Vec<&str> = args.split('|').map(|s| s.trim()).collect();
            if parts.is_empty() || parts[0].is_empty() {
                return Ok(Some(
                    "ℹ️ *Usage*: `.event <Title> [| Description | Location]`\nExample: `.event Dev Sync | Weekly Rust Core Review | Discord`".into()
                ));
            }
            let name = parts[0];
            let desc = parts.get(1).copied();
            let loc = parts.get(2).copied();
            let start = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64 + 3600;
            match client.send_event(remote_jid, name, desc, start, Some(start + 3600), loc).await {
                Ok(id) => Ok(Some(format!("📅 *Calendar Event Created!*\n• *ID*: `{}`\n• *Title*: {}", id, name))),
                Err(e) => Ok(Some(format!("❌ Failed to send event: {}", e))),
            }
        }

        "sendjson" => {
            if !from_me {
                return Ok(Some("⛔ *Access Denied*: `.sendjson` is restricted to bot owner.".into()));
            }
            let raw_json = args.trim();
            if raw_json.is_empty() {
                return Ok(Some("ℹ️ *Usage*: `.sendjson <raw_json>`\nExample: `.sendjson {\"text\":\"hi\",\"mentions\":[\"123@s.whatsapp.net\"]}`".into()));
            }
            match serde_json::from_str::<serde_json::Value>(raw_json) {
                Ok(val) => match client.send_message_json(remote_jid, &val).await {
                    Ok(id) => Ok(Some(format!("✨ *JSON Message Dispatched!*\n• *ID*: `{}`", id))),
                    Err(e) => Ok(Some(format!("❌ Failed to send JSON message: {}", e))),
                },
                Err(e) => Ok(Some(format!("❌ Invalid JSON payload: {}", e))),
            }
        }

        "statusmedia" => {
            if !from_me {
                return Ok(Some("⛔ *Access Denied*: `.statusmedia` is restricted to bot owner.".into()));
            }
            let raw = args.trim();
            if raw.is_empty() {
                return Ok(Some("ℹ️ *Usage*: `.statusmedia <file_path> [| caption]`\nExample: `.statusmedia C:/media/photo.jpg | My HD Story`".into()));
            }
            let parts: Vec<&str> = raw.splitn(2, '|').map(|s| s.trim()).collect();
            let file_path = parts[0];
            let caption = parts.get(1).copied().filter(|s| !s.is_empty());

            let path = std::path::Path::new(file_path);
            if !path.exists() {
                return Ok(Some(format!("❌ File does not exist: `{}`", file_path)));
            }

            match std::fs::read(path) {
                Ok(bytes) => {
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                    let is_video = ["mp4", "mkv", "avi", "mov", "3gp", "webm"].contains(&ext.as_str());

                    let res = if is_video {
                        let mime = if ext == "mp4" { Some("video/mp4") } else { None };
                        client.send_status_video(&bytes, caption, mime, None).await
                    } else {
                        let mime = if ext == "png" { Some("image/png") } else { Some("image/jpeg") };
                        client.send_status_image(&bytes, caption, mime, None).await
                    };

                    match res {
                        Ok(id) => Ok(Some(format!("✨ *HD Status Published!*\n• *Type*: {}\n• *ID*: `{}`\n• *Size*: {} bytes", if is_video { "Video" } else { "Image" }, id, bytes.len()))),
                        Err(e) => Ok(Some(format!("❌ Failed to post status media: {}", e))),
                    }
                }
                Err(e) => Ok(Some(format!("❌ Failed to read file `{}`: {}", file_path, e))),
            }
        }

        "ghostmode" => {
            if !from_me {
                return Ok(Some("⛔ *Access Denied*: `.ghostmode` is restricted to bot owner.".into()));
            }
            let arg = args.trim().to_lowercase();
            if arg == "on" || arg == "1" || arg == "true" {
                client.set_ghost_mode(true);
                Ok(Some("👻 *Ghost Mode ENABLED*\nRead receipts & story receipts are now completely suppressed.".into()))
            } else if arg == "off" || arg == "0" || arg == "false" {
                client.set_ghost_mode(false);
                Ok(Some("👁️ *Ghost Mode DISABLED*\nNormal read receipts restored.".into()))
            } else {
                let cur = client.is_ghost_mode();
                Ok(Some(format!("👻 *Ghost Mode Status*: {}\nUsage: `.ghostmode <on|off>`", if cur { "Active (Invisible)" } else { "Inactive (Normal)" })))
            }
        }

        "fakevn" => {
            if !from_me {
                return Ok(Some("⛔ *Access Denied*: `.fakevn` is restricted to bot owner.".into()));
            }
            let file_path = args.trim();
            if file_path.is_empty() {
                return Ok(Some("ℹ️ *Usage*: `.fakevn <path_to_audio>`\nExample: `.fakevn C:/media/voice.mp3`".into()));
            }
            let path = std::path::Path::new(file_path);
            if !path.exists() {
                return Ok(Some(format!("❌ Audio file does not exist: `{}`", file_path)));
            }
            match std::fs::read(path) {
                Ok(bytes) => {
                    match client.send_voice_note(remote_jid, &bytes, 2, None).await {
                        Ok(id) => Ok(Some(format!("🎙️ *Native Voice Note Dispatched!*\n• *ID*: `{}`\n• *Size*: {} bytes", id, bytes.len()))),
                        Err(e) => Ok(Some(format!("❌ Failed to send voice note: {}", e))),
                    }
                }
                Err(e) => Ok(Some(format!("❌ Failed to read audio `{}`: {}", file_path, e))),
            }
        }

        "fakequote" => {
            if !from_me {
                return Ok(Some("⛔ *Access Denied*: `.fakequote` is restricted to bot owner.".into()));
            }
            let parts: Vec<&str> = args.split('|').map(|s| s.trim()).collect();
            if parts.len() < 3 {
                return Ok(Some("ℹ️ *Usage*: `.fakequote <Reply Text> | <Target Number/JID> | <Forged Text>`\nExample: `.fakequote Agreed! | 628123456789 | Let's do it tomorrow`".into()));
            }
            let reply_text = parts[0];
            let faker_jid = parts[1];
            let faker_text = parts[2];
            match client.send_fake_quoted_message(remote_jid, reply_text, faker_jid, faker_text).await {
                Ok(id) => Ok(Some(format!("🎭 *Fake Quoted Reply Dispatched!*\n• *ID*: `{}`\n• *Quoting*: `{}`\n• *Message*: \"{}\"", id, faker_jid, faker_text))),
                Err(e) => Ok(Some(format!("❌ Failed to send fake quote: {}", e))),
            }
        }

        "help" => {
            let cur_mode = get_mode();
            let mut h = format!(
                "🤖 *Rose-Baileys In-Chat Command Menu*\n• *Active Mode*: {}\n\n*Public Commands*:\n",
                cur_mode.display_badge()
            );
            h.push_str("• `.ping` - Gateway round-trip latency\n");
            h.push_str("• `.status` - Connection & engine status\n");
            h.push_str("• `.proto list` - List recognized proto types & schema stats\n");
            h.push_str("• `.proto search <q>` - Search WAProto messages & fields\n");
            h.push_str("• `.proto view <type>` - View schema definition\n");
            h.push_str("• `.onwa <num>` - Check if phone number is on WhatsApp\n");
            h.push_str("• `.urlpreview <url>` - OpenGraph link preview\n");
            h.push_str("• `.poll <Q> | <O1> | <O2>` - Create encrypted poll\n");
            h.push_str("• `.location <lat> <lng> [| Name | Addr]` - Share location pin\n");
            h.push_str("• `.contact <Name> <Phone>` - Send vCard contact\n");
            h.push_str("• `.event <Title> [| Desc | Loc]` - Create calendar event\n");
            h.push_str("• `.qr` - Pairing QR code status\n");
            h.push_str("• `.help` - Show this menu\n");

            if from_me {
                h.push_str("\n*Owner / Admin Commands*:\n");
                h.push_str("• `.cmdmode [off|self|public]` - Switch bot mode\n");
                h.push_str("• `.sendjson <json>` - Send rich/arbitrary JSON message\n");
                h.push_str("• `.statusmedia <path> [| cap]` - Post HD status story\n");
                h.push_str("• `.ghostmode [on|off]` - Invisible ghost read receipts\n");
                h.push_str("• `.fakevn <path>` - Send audio as native voice note\n");
                h.push_str("• `.fakequote <reply> | <faker> | <msg>` - Forged quoted reply\n");
                h.push_str("• `.proto inspect [id|last]` - Deep protobuf inspection\n");
                h.push_str("• `.proto pull` - Pure Rust live WhatsApp Web schema extraction\n");
            }
            h.push_str("\n_Prefixes supported: `.` `!` `/`_");
            Ok(Some(h))
        }

        _ => {
            // If sent by owner or in a 1-on-1 private chat, notify user about unknown command
            if from_me || !is_group {
                Ok(Some(format!(
                    "❓ *Unknown Command*: `.{}`\nType `.help` for available commands.",
                    cmd
                )))
            } else {
                // Silently ignore unrecognized commands in public group chats to avoid spam
                Ok(None)
            }
        }
    }
}

/// Dispatches proto subcommands for in-chat requests.
async fn handle_in_chat_proto(
    args: &str,
    store: &Arc<SqliteStore>,
    from_me: bool,
) -> Result<Option<String>, String> {
    let mut parts = args.split_whitespace();
    let sub = parts.next().unwrap_or("").to_lowercase();
    let rest = parts.collect::<Vec<&str>>().join(" ");

    match sub.as_str() {
        "list" => {
            let types = RecognizedMessageType::all_canonical();
            let mut out = String::from("📦 *Rose-Baileys Recognized Message Types*\n");
            for (i, t) in types.iter().take(15).enumerate() {
                out.push_str(&format!(
                    "  {}. `{}` - {}\n",
                    i + 1,
                    t.as_str(),
                    t.display_name()
                ));
            }
            let stats = SchemaInspector::global().stats();
            out.push_str(&format!(
                "\n📊 *WAProto.proto In-Memory Index*:\n• *Messages*: {}\n• *Enums*: {}\n• *Fields*: {}\n• *Schema Size*: {} KB",
                stats.total_messages,
                stats.total_enums,
                stats.total_fields,
                stats.schema_bytes / 1024
            ));
            Ok(Some(out))
        }

        "search" => {
            let query = rest.trim();
            if query.is_empty() {
                return Ok(Some(
                    "ℹ️ *Usage*: `.proto search <term>`\nExample: `.proto search poll`".into(),
                ));
            }
            let matches = SchemaInspector::global().search(query);
            if matches.is_empty() {
                return Ok(Some(format!(
                    "🔍 *WAProto Search*: No matches found for `{}`.",
                    query
                )));
            }
            let mut out = format!(
                "🔍 *WAProto Search Results for* `{}` ({} found):\n",
                query,
                matches.len()
            );
            for m in matches.iter().take(20) {
                match &m.item_type {
                    baileys_proto::schema::SchemaItemType::Message => {
                        out.push_str(&format!("• 📦 *[MSG]* `{}`\n", m.name));
                    }
                    baileys_proto::schema::SchemaItemType::Enum => {
                        out.push_str(&format!("• 🏷️ *[ENUM]* `{}`\n", m.name));
                    }
                    baileys_proto::schema::SchemaItemType::Field {
                        parent_message,
                        field_type,
                        field_number,
                    } => {
                        out.push_str(&format!(
                            "• 🔹 *[FLD]* `{}.{}` -> `{}` _(#{})_\n",
                            parent_message, m.name, field_type, field_number
                        ));
                    }
                    baileys_proto::schema::SchemaItemType::EnumValue {
                        parent_enum,
                        number,
                    } => {
                        out.push_str(&format!(
                            "• 🔸 *[VAL]* `{}.{}` = {}\n",
                            parent_enum, m.name, number
                        ));
                    }
                }
            }
            if matches.len() > 20 {
                out.push_str(&format!(
                    "\n_... and {} more matches truncated._",
                    matches.len() - 20
                ));
            }
            Ok(Some(out))
        }

        "view" => {
            let type_name = rest.trim();
            if type_name.is_empty() {
                return Ok(Some(
                    "ℹ️ *Usage*: `.proto view <MessageType | EnumType>`\nExample: `.proto view PollCreationMessage`"
                        .into(),
                ));
            }
            if let Some(def) = SchemaInspector::global().get_definition(type_name) {
                Ok(Some(format!(
                    "📄 *WAProto Definition: {}*\n```proto\n{}\n```",
                    type_name, def
                )))
            } else {
                Ok(Some(format!(
                    "❌ Type `{}` not found in WAProto.proto schema.\nTip: Use `.proto search {}` to find related symbols.",
                    type_name, type_name
                )))
            }
        }

        "inspect" => {
            if !from_me {
                return Ok(Some(
                    "⛔ *Access Denied*: Proto inspection is restricted to the bot owner.".into(),
                ));
            }
            let target = if rest.trim().is_empty() {
                "last"
            } else {
                rest.trim()
            };
            let msg_res = if target.eq_ignore_ascii_case("last") {
                store.get_latest_message().await
            } else {
                store.get_message(target).await
            };

            match msg_res {
                Ok(Some(stored)) => {
                    let mut out = format!("🔬 *WAProto Deep Inspection* [`{}`]\n", stored.id);
                    out.push_str(&format!("• *Chat*: `{}`\n", stored.chat_jid));
                    out.push_str(&format!("• *Sender*: `{}`\n", stored.sender_jid));
                    out.push_str(&format!("• *From Me*: {}\n", stored.from_me));
                    out.push_str(&format!("• *Summary*: {}\n", stored.summary));
                    out.push_str(&format!(
                        "• *Protobuf Bytes*: {} bytes\n",
                        stored.proto_bytes.len()
                    ));

                    if !stored.proto_bytes.is_empty() {
                        if let Ok(raw_proto) = <baileys_proto::proto::Message as ProstMessage>::decode(
                            &stored.proto_bytes[..],
                        ) {
                            let unwrap_res = raw_proto.unwrap_wrappers();
                            out.push_str(&format!(
                                "• *Canonical Type*: `{}` ({})\n",
                                unwrap_res.real_type.as_str(),
                                unwrap_res.real_type.display_name()
                            ));
                            if !unwrap_res.wrappers.is_empty() {
                                out.push_str(&format!(
                                    "• *Wrappers*: `{}`\n",
                                    unwrap_res.wrappers.join(" -> ")
                                ));
                            }
                            if let Some(text) = raw_proto.extract_text() {
                                out.push_str(&format!("• *Extracted Text*: \"{}\"\n", text));
                            }
                        }
                    }
                    Ok(Some(out))
                }
                Ok(None) => Ok(Some(format!(
                    "❌ Message `{}` not found in local session store.",
                    target
                ))),
                Err(e) => Ok(Some(format!("❌ Store query error: {}", e))),
            }
        }

        "pull" => {
            if !from_me {
                return Ok(Some(
                    "⛔ *Access Denied*: Proto schema pull is restricted to the bot owner.".into(),
                ));
            }
            let default_target = std::path::Path::new("crates/baileys-proto/proto/WAProto.proto");
            let target_path = if default_target.exists() {
                Some(default_target)
            } else {
                None
            };
            let start = std::time::Instant::now();
            match baileys_proto::extractor::pull_latest_waproto(target_path).await {
                Ok(result) => {
                    let dur = start.elapsed();
                    Ok(Some(format!(
                        "🚀 *WAProto Live Schema Extraction Succeeded!*\n• *WA Revision*: `{}`\n• *Root Entities*: {}\n• *Total Entities*: {}\n• *Schema Size*: {} KB\n• *Elapsed*: {:.2?}\n• *Engine*: Pure Rust AST (`oxc_parser`)\n• *Saved To*: `{}`",
                        result.client_revision,
                        result.root_entities,
                        result.total_entities,
                        result.schema_bytes / 1024,
                        dur,
                        result.output_file.as_deref().unwrap_or("In-Memory Schema (Verified)")
                    )))
                }
                Err(e) => Ok(Some(format!("❌ *WAProto Pull Failed*: {}", e))),
            }
        }

        _ => Ok(Some(
            "ℹ️ *WAProto Command Usage*:\n• `.proto list` - List recognized types\n• `.proto search <term>` - Search schema\n• `.proto view <type>` - View protobuf definition\n• `.proto inspect [id|last]` - Deep message inspection (Owner only)\n• `.proto pull` - Pull live WhatsApp Web schema (Owner only)"
                .into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_command() {
        assert_eq!(extract_command(".ping"), Some(("ping", "")));
        assert_eq!(extract_command("!status"), Some(("status", "")));
        assert_eq!(extract_command("/proto list"), Some(("proto", "list")));
        assert_eq!(
            extract_command(".proto search poll"),
            Some(("proto", "search poll"))
        );
        assert_eq!(
            extract_command("  !poll Question | Opt1 | Opt2  "),
            Some(("poll", "Question | Opt1 | Opt2"))
        );
        assert_eq!(extract_command("Hello world"), None);
        assert_eq!(extract_command("."), None);
        assert_eq!(extract_command(""), None);
    }

    #[test]
    fn test_in_chat_mode_parsing() {
        assert_eq!(InChatMode::from_str_loose("off"), Some(InChatMode::Off));
        assert_eq!(InChatMode::from_str_loose("disable"), Some(InChatMode::Off));
        assert_eq!(
            InChatMode::from_str_loose("self"),
            Some(InChatMode::SelfOnly)
        );
        assert_eq!(
            InChatMode::from_str_loose("private"),
            Some(InChatMode::SelfOnly)
        );
        assert_eq!(
            InChatMode::from_str_loose("public"),
            Some(InChatMode::Public)
        );
        assert_eq!(InChatMode::from_str_loose("all"), Some(InChatMode::Public));
        assert_eq!(InChatMode::from_str_loose("unknown"), None);
    }

    #[test]
    fn test_is_restricted_command() {
        assert!(is_restricted_command("cmdmode", ""));
        assert!(is_restricted_command("proto", "pull"));
        assert!(is_restricted_command("proto", "inspect last"));
        assert!(is_restricted_command("exit", ""));
        assert!(is_restricted_command("block", "123"));

        assert!(!is_restricted_command("ping", ""));
        assert!(!is_restricted_command("status", ""));
        assert!(!is_restricted_command("proto", "list"));
        assert!(!is_restricted_command("proto", "search poll"));
        assert!(!is_restricted_command("proto", "view PollCreationMessage"));
        assert!(!is_restricted_command("onwa", "123456789"));
        assert!(!is_restricted_command("location", "37 -122"));
        assert!(!is_restricted_command("contact", "Alice 123"));
        assert!(!is_restricted_command("event", "Sync"));
        assert!(is_restricted_command("sendjson", "{}"));
        assert!(is_restricted_command("statusmedia", "photo.jpg"));
        assert!(is_restricted_command("ghostmode", "on"));
        assert!(is_restricted_command("fakevn", "audio.mp3"));
        assert!(is_restricted_command("fakequote", "a | b | c"));
        assert!(!is_restricted_command("help", ""));
    }

    #[test]
    fn test_extract_rich_commands() {
        assert_eq!(
            extract_command(".location 37.77 -122.41 | SF"),
            Some(("location", "37.77 -122.41 | SF"))
        );
        assert_eq!(
            extract_command(".contact Bob +12345"),
            Some(("contact", "Bob +12345"))
        );
        assert_eq!(
            extract_command(".event Launch | Party | Zoom"),
            Some(("event", "Launch | Party | Zoom"))
        );
        assert_eq!(
            extract_command(".sendjson {\"text\":\"hi\"}"),
            Some(("sendjson", "{\"text\":\"hi\"}"))
        );
        assert_eq!(
            extract_command(".statusmedia ./img.png | Vacation"),
            Some(("statusmedia", "./img.png | Vacation"))
        );
        assert_eq!(extract_command(".ghostmode on"), Some(("ghostmode", "on")));
        assert_eq!(
            extract_command(".fakevn ./vn.mp3"),
            Some(("fakevn", "./vn.mp3"))
        );
        assert_eq!(
            extract_command(".fakequote hi | 123 | hello"),
            Some(("fakequote", "hi | 123 | hello"))
        );
    }

    #[test]
    fn test_sanitize_for_whatsapp() {
        let ansi_text = "\x1b[31mError:\x1b[0m \x1b[1mSomething went wrong!\x1b[0m";
        assert_eq!(
            sanitize_for_whatsapp(ansi_text),
            "Error: Something went wrong!"
        );

        let long_text = "a".repeat(4000);
        let sanitized = sanitize_for_whatsapp(&long_text);
        assert!(sanitized.len() < 3600);
        assert!(sanitized.contains("Output truncated at 3,500 characters"));
    }

    #[tokio::test]
    async fn test_in_chat_mode_persistence_sqlite() {
        let store = SqliteStore::open_in_memory().expect("open memory store");

        // Verify initial state
        let initial = init_mode_from_store(&store).await;
        assert_eq!(initial, InChatMode::Off);
        assert_eq!(get_mode(), InChatMode::Off);

        // Switch to SelfOnly and persist
        set_mode_and_persist(&store, InChatMode::SelfOnly)
            .await
            .unwrap();
        assert_eq!(get_mode(), InChatMode::SelfOnly);
        assert_eq!(init_mode_from_store(&store).await, InChatMode::SelfOnly);

        // Switch to Public and persist
        set_mode_and_persist(&store, InChatMode::Public)
            .await
            .unwrap();
        assert_eq!(get_mode(), InChatMode::Public);
        assert_eq!(init_mode_from_store(&store).await, InChatMode::Public);

        // Switch back to Off
        set_mode_and_persist(&store, InChatMode::Off).await.unwrap();
        assert_eq!(get_mode(), InChatMode::Off);
        assert_eq!(init_mode_from_store(&store).await, InChatMode::Off);
    }
}
