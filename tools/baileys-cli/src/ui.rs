use baileys_core::{BaileysClient, WebMessageInfo};
use baileys_store::{ChatStore, SqliteStore};
use std::sync::atomic::AtomicU8;
use std::sync::{Arc, RwLock};

// ANSI terminal color constants for comfy reads
pub const ANSI_RESET: &str = "\x1b[0m";
pub const ANSI_BOLD: &str = "\x1b[1m";
pub const ANSI_GRAY: &str = "\x1b[90m";
pub const ANSI_RED: &str = "\x1b[31m";
pub const ANSI_YELLOW: &str = "\x1b[33m";
pub const ANSI_BLUE: &str = "\x1b[34m";
pub const ANSI_MAGENTA: &str = "\x1b[35m";
pub const ANSI_CYAN: &str = "\x1b[36m";
pub const ANSI_BRIGHT_GREEN: &str = "\x1b[92m";
pub const ANSI_BRIGHT_BLUE: &str = "\x1b[94m";
pub const ANSI_BRIGHT_MAGENTA: &str = "\x1b[95m";
pub const ANSI_BRIGHT_CYAN: &str = "\x1b[96m";

// 0: All, 1: EventsOnly, 2: ErrorsOnly, 3: Quiet
pub static CLI_LOG_FILTER: AtomicU8 = AtomicU8::new(0);

use tracing_subscriber::reload::Handle;
use tracing_subscriber::{EnvFilter, Registry};

pub type TracingReloadHandle = Handle<EnvFilter, Registry>;
pub static TRACING_RELOAD_HANDLE: RwLock<Option<TracingReloadHandle>> = RwLock::new(None);

pub fn set_tracing_reload_handle(handle: TracingReloadHandle) {
    if let Ok(mut lock) = TRACING_RELOAD_HANDLE.write() {
        *lock = Some(handle);
    }
}

pub fn reload_tracing_filter(filter_str: &str) -> Result<(), String> {
    if let Ok(lock) = TRACING_RELOAD_HANDLE.read() {
        if let Some(ref handle) = *lock {
            let new_filter = EnvFilter::try_new(filter_str)
                .map_err(|e| format!("Invalid filter '{}': {}", filter_str, e))?;
            handle
                .reload(new_filter)
                .map_err(|e| format!("Failed to reload tracing filter: {}", e))?;
            return Ok(());
        }
    }
    Err("Tracing reload handle not initialized".to_string())
}

/// Trait for thread-safe external log printing that cooperates with the interactive line-editor
pub trait LogPrinter: Send + Sync {
    fn print_log(&self, text: String);
}

/// Global registry for the active REPL external printer
pub static LOG_PRINTER: RwLock<Option<Box<dyn LogPrinter>>> = RwLock::new(None);

/// Registers an external printer so asynchronous logs do not erase active user typing
pub fn set_log_printer(printer: Box<dyn LogPrinter>) {
    if let Ok(mut lock) = LOG_PRINTER.write() {
        *lock = Some(printer);
    }
}

/// Non-destructively prints a log line or multi-line block without clobbering active input
pub fn safe_print(text: &str) {
    if let Ok(lock) = LOG_PRINTER.read() {
        if let Some(ref p) = *lock {
            p.print_log(text.to_string());
            return;
        }
    }
    println!("{}", text);
}

pub fn log_ts() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

pub fn badge(tag: &str, color: &str) -> String {
    format!(
        "{}[{}{}{}]{}",
        ANSI_BOLD, color, tag, ANSI_RESET, ANSI_RESET
    )
}

pub fn print_comfy_event(tag: &str, color: &str, msg: &str) {
    let formatted = format!(
        "{}[{}]{} {} {}",
        ANSI_GRAY,
        log_ts(),
        ANSI_RESET,
        badge(tag, color),
        msg
    );
    safe_print(&formatted);
}

pub async fn format_and_print_message_event(
    message: &WebMessageInfo,
    client: &Arc<BaileysClient>,
    store: &Arc<SqliteStore>,
) {
    let key = message.key.as_ref();
    let remote_jid = key
        .and_then(|k| k.remote_jid.as_deref())
        .unwrap_or("unknown");
    let participant = key.and_then(|k| k.participant.as_deref());
    let from_me = key.and_then(|k| k.from_me).unwrap_or(false);
    let msg_id = key.and_then(|k| k.id.as_deref()).unwrap_or("<unknown>");
    let is_group = remote_jid.ends_with("@g.us");

    // Resolve Chat / Group Name
    let mut chat_name = None;
    if is_group {
        if let Ok(Some(chat)) = store.get_chat(remote_jid).await {
            chat_name = chat.name;
        }
        if chat_name.is_none() {
            if let Ok(meta) = client.group_fetch_metadata(remote_jid).await {
                chat_name = Some(meta.subject);
            }
        }
    } else if let Ok(Some(contact)) = store.get_contact(remote_jid).await {
        chat_name = contact.name.or(contact.push_name);
    }

    // Resolve Sender Name, Phone, and LID
    let (sender_name, sender_phone, sender_lid) = if from_me {
        let my_jid = client
            .me_jid()
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| "me".to_string());
        let my_lid = client
            .me_lid()
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| "none".to_string());
        ("Me (Self)".to_string(), my_jid, my_lid)
    } else {
        let raw_sender = participant.unwrap_or(remote_jid);
        let push_name = message.push_name.clone();
        let mut phone = raw_sender.to_string();
        let mut lid = "unknown".to_string();

        if raw_sender.ends_with("@lid") {
            lid = raw_sender.to_string();
            if let Ok(Some(c)) = store.get_contact(raw_sender).await {
                phone = c.jid;
            }
        } else if raw_sender.ends_with("@s.whatsapp.net") {
            phone = raw_sender.to_string();
            if let Ok(Some(c)) = store.get_contact(raw_sender).await {
                if let Some(l) = c.lid {
                    lid = l;
                }
            }
        }

        let s_name = push_name.unwrap_or_else(|| {
            if is_group {
                phone.clone()
            } else {
                chat_name.clone().unwrap_or_else(|| phone.clone())
            }
        });
        (s_name, phone, lid)
    };

    let text_summary = message
        .message
        .as_ref()
        .map(baileys_core::extract_message_summary)
        .unwrap_or_else(|| "<empty>".to_string());

    let (badge_tag, badge_col) = if from_me {
        ("MSG-OUT", ANSI_BRIGHT_BLUE)
    } else {
        ("MSG-IN", ANSI_BRIGHT_GREEN)
    };

    let mut block = String::new();
    block.push_str(&format!(
        "{}[{}]{} {} {}{}{}\n",
        ANSI_GRAY,
        log_ts(),
        ANSI_RESET,
        badge(badge_tag, badge_col),
        ANSI_BOLD,
        if is_group {
            "[GROUP CHAT]"
        } else {
            "[DIRECT 1-ON-1]"
        },
        ANSI_RESET
    ));
    if is_group {
        let title = chat_name.as_deref().unwrap_or("<unknown group name>");
        block.push_str(&format!(
            "  ├─ Group Subject : {}{}{}\n",
            ANSI_BOLD, title, ANSI_RESET
        ));
        block.push_str(&format!("  ├─ Group JID     : {}\n", remote_jid));
    } else {
        if let Some(ref name) = chat_name {
            block.push_str(&format!("  ├─ Contact Name  : {}\n", name));
        }
        block.push_str(&format!("  ├─ Remote Chat   : {}\n", remote_jid));
    }
    block.push_str(&format!("  ├─ Sender Name   : {}\n", sender_name));
    block.push_str(&format!("  ├─ Sender Phone  : {}\n", sender_phone));
    block.push_str(&format!("  ├─ Sender LID    : {}\n", sender_lid));
    block.push_str(&format!("  ├─ From Me       : {}\n", from_me));
    block.push_str(&format!("  ├─ Message ID    : {}\n", msg_id));
    block.push_str(&format!(
        "  └─ Content       : {}{}{}",
        ANSI_BOLD, text_summary, ANSI_RESET
    ));

    safe_print(&block);
}

/// Strips ANSI terminal escape sequences (e.g. \x1b[31m, \x1b[0m, etc.) from a string.
pub fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                              // Consume CSI parameter bytes until final terminating command byte
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi_colors() {
        let colored = "\x1b[31mHello\x1b[0m \x1b[1;32mWorld!\x1b[0m";
        assert_eq!(strip_ansi(colored), "Hello World!");

        let clean = "No escape sequences here";
        assert_eq!(strip_ansi(clean), "No escape sequences here");

        let mixed = "\x1b[96m[22:30:15]\x1b[0m \x1b[1m*Bold Text*\x1b[0m";
        assert_eq!(strip_ansi(mixed), "[22:30:15] *Bold Text*");
    }
}
