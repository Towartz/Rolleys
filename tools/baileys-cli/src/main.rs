use baileys_core::{BaileysClient, BaileysConfigBuilder, BaileysEvent};
use baileys_store::SqliteStore;
use std::sync::Arc;

mod commands;
mod in_chat_bot;
mod qr;
mod repl;
mod selftest;
mod ui;

use qr::{clear_latest_qr, render_baileys_small_qr, set_latest_qr};
use repl::run_repl;
use selftest::run_self_test;
use ui::{
    format_and_print_message_event, print_comfy_event, safe_print, ANSI_BLUE, ANSI_BRIGHT_BLUE,
    ANSI_BRIGHT_CYAN, ANSI_BRIGHT_GREEN, ANSI_BRIGHT_MAGENTA, ANSI_CYAN, ANSI_GRAY, ANSI_MAGENTA,
    ANSI_RED, ANSI_YELLOW, CLI_LOG_FILTER,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let res = std::thread::Builder::new()
        .name("cli-main".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_stack_size(16 * 1024 * 1024)
                .build()
                .expect("Failed to build Tokio runtime");
            rt.block_on(Box::pin(run_cli())).map_err(|e| e.to_string())
        })?
        .join()
        .map_err(|_| "CLI thread panicked")?;

    res.map_err(|e| e.into())
}

async fn run_cli() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let mut phone_arg: Option<String> = None;
    let mut db_path_arg: Option<String> = None;
    let mut invert_qr = false;
    let mut clean_session = false;
    let mut debug_mode = false;
    let mut auto_qr = false;
    let mut run_selftest = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--phone" | "-p" => {
                if i + 1 < args.len() {
                    phone_arg = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--db" | "-s" | "--session" => {
                if i + 1 < args.len() {
                    db_path_arg = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--auto-qr" => {
                auto_qr = true;
            }
            "--invert" => {
                invert_qr = true;
            }
            "--clean" | "--reset" => {
                clean_session = true;
            }
            "--debug" | "-d" | "-v" | "--verbose" => {
                debug_mode = true;
            }
            "--selftest" => {
                run_selftest = true;
            }
            "--help" | "-h" => {
                println!("Rose-Baileys CLI - Unified WhatsApp Engine\n");
                println!("Usage: baileys-cli [OPTIONS]\n");
                println!("Options:");
                println!(
                    "  -p, --phone <NUMBER>       Pair via WhatsApp 8-digit code instead of QR"
                );
                println!("  -s, --db, --session <PATH> Path to SQLite session database (default: aurora_session.db)");
                println!("      --auto-qr              Automatically display QR code matrix in terminal upon arrival");
                println!(
                    "      --invert               Invert QR colors for light terminal backgrounds"
                );
                println!("      --clean, --reset       Remove existing session database for a clean start");
                println!("      --selftest             Automatically run comprehensive self-test suite upon connection");
                println!("  -d, --debug, -v            Enable verbose protocol stanza & crypto debug logging");
                println!("  -h, --help                 Show this help message");
                return Ok(());
            }
            _ => {}
        }
        i += 1;
    }

    let filter = if debug_mode {
        tracing_subscriber::EnvFilter::new("debug")
    } else {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("baileys_cli=warn,baileys_core=warn,baileys_signal=warn,baileys_binary=warn,baileys_store=warn"))
    };

    let (filter_layer, reload_handle) = tracing_subscriber::reload::Layer::new(filter);
    crate::ui::set_tracing_reload_handle(reload_handle);

    use tracing_subscriber::layer::SubscriberExt;
    let subscriber = tracing_subscriber::registry()
        .with(filter_layer)
        .with(tracing_subscriber::fmt::layer().with_target(false));
    tracing::subscriber::set_global_default(subscriber)?;

    println!("============================================================");
    println!("   Rose-Baileys: Unified Multiplatform Rust WhatsApp Engine  ");
    println!("   Targets: Android (Aurora Chats), macOS, Linux, Windows   ");
    println!("============================================================");

    // Initialize embedded SQLite database for session persistence
    let default_store = "aurora_session.db".to_string();
    let store_path = db_path_arg.as_deref().unwrap_or(&default_store);
    if clean_session {
        print_comfy_event(
            "PURGE",
            ANSI_RED,
            &format!("Purging session database at '{}'...", store_path),
        );
        let _ = std::fs::remove_file(store_path);
        let _ = std::fs::remove_file(format!("{}-wal", store_path));
        let _ = std::fs::remove_file(format!("{}-shm", store_path));
    }
    print_comfy_event(
        "STORE",
        ANSI_BLUE,
        &format!("Opened SQLite session store at '{}'", store_path),
    );
    let store = Arc::new(SqliteStore::open(store_path)?);

    // Initialize in-chat command runner mode from SQLite persistence
    let bot_mode = in_chat_bot::init_mode_from_store(&store).await;
    print_comfy_event(
        "BOT-MODE",
        ANSI_MAGENTA,
        &format!("In-Chat Command Runner mode: {}", bot_mode.display_badge()),
    );

    // Configure client
    let mut config_builder = BaileysConfigBuilder::new().with_browser("Chrome", "Mac OS", "14.4.1");

    if let Some(ref phone) = phone_arg {
        config_builder = config_builder.with_phone_pairing(phone);
        print_comfy_event(
            "PAIR",
            ANSI_YELLOW,
            &format!("Using phone pairing mode for number: {}", phone),
        );
    }

    let config = config_builder.build();
    let client = Arc::new(BaileysClient::new(store.clone(), config));
    let mut event_rx = client.events();

    let client_for_events = client.clone();
    let store_for_events = store.clone();

    // Event listener task
    tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            match event {
                BaileysEvent::Connecting => {
                    print_comfy_event("CONN", ANSI_CYAN, "Connecting to WhatsApp Web Gateway...");
                }
                BaileysEvent::QrCode { code } => {
                    set_latest_qr(code.clone());
                    if auto_qr {
                        match render_baileys_small_qr(&code, invert_qr) {
                            Ok(qr_str) => {
                                let msg = format!(
                                    "\n[!] WhatsApp QR Code:\n\n{}\n  [Tip] Invert colors with --invert if your terminal background is light.\n  [Tip] Or pair via phone code: cargo run -p baileys-cli -- --phone <number>\n",
                                    qr_str
                                );
                                safe_print(&msg);
                            }
                            Err(e) => {
                                safe_print(&format!(
                                    "Failed to render QR code: {}\nRaw QR string: {}",
                                    e, code
                                ));
                            }
                        }
                    } else {
                        print_comfy_event(
                            "QR",
                            ANSI_YELLOW,
                            "New WhatsApp QR code ready. Type 'qr' to display on screen.",
                        );
                    }
                }
                BaileysEvent::PairingCode { code } => {
                    let pairing_block = format!(
                        "\n========================================\n    WHATSAPP PAIRING CODE:  {}\n========================================\nEnter this 8-digit code on WhatsApp:\nSettings > Linked Devices > Link with phone number\n",
                        code
                    );
                    safe_print(&pairing_block);
                }
                BaileysEvent::Connected { me_jid } => {
                    clear_latest_qr();
                    let me_lid_str = client_for_events.me_lid().await.ok().flatten();
                    let lid_display = me_lid_str.as_deref().unwrap_or("none");
                    let conn_banner = format!(
                        "\n╔══════════════════════════════════════════════════════════════╗\n║  [+] SUCCESS: Authenticated & Connected to WhatsApp Gateway!  ║\n╠══════════════════════════════════════════════════════════════╣\n║  Phone JID : {:<47} ║\n║  LID       : {:<47} ║\n╚══════════════════════════════════════════════════════════════╝\n\n------------------------------------------------------------\n Interactive REPL active. Type 'help' for command list.\n------------------------------------------------------------\n",
                        me_jid, lid_display
                    );
                    safe_print(&conn_banner);

                    if run_selftest {
                        let c_test = client_for_events.clone();
                        let s_test = store_for_events.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                            let _ = run_self_test(&c_test, &s_test).await;
                        });
                    }
                }
                BaileysEvent::Message { message } => {
                    let filter = CLI_LOG_FILTER.load(std::sync::atomic::Ordering::Relaxed);
                    if filter <= 1 {
                        format_and_print_message_event(
                            &message,
                            &client_for_events,
                            &store_for_events,
                        )
                        .await;
                    }

                    // Asynchronously evaluate message for in-chat command runner
                    let c_bot = client_for_events.clone();
                    let s_bot = store_for_events.clone();
                    let m_bot = message.clone();
                    tokio::spawn(async move {
                        in_chat_bot::handle_incoming_message(c_bot, s_bot, m_bot).await;
                    });
                }
                BaileysEvent::Receipt {
                    key_id,
                    remote_jid,
                    status,
                } => {
                    let filter = CLI_LOG_FILTER.load(std::sync::atomic::Ordering::Relaxed);
                    if filter <= 1 {
                        let col = if status == "read" {
                            ANSI_BRIGHT_CYAN
                        } else if status == "delivered" {
                            ANSI_BRIGHT_BLUE
                        } else if status == "server_ack" {
                            ANSI_BRIGHT_GREEN
                        } else if status.starts_with("error") {
                            ANSI_RED
                        } else {
                            ANSI_GRAY
                        };
                        print_comfy_event(
                            "ACK",
                            col,
                            &format!("Message {} to {}: {}", key_id, remote_jid, status),
                        );
                    }
                }
                BaileysEvent::Presence { jid, presence } => {
                    let filter = CLI_LOG_FILTER.load(std::sync::atomic::Ordering::Relaxed);
                    if filter == 0 {
                        print_comfy_event(
                            "PRESENCE",
                            ANSI_BRIGHT_MAGENTA,
                            &format!("{}: {}", jid, presence),
                        );
                    }
                }
                BaileysEvent::HistorySync {
                    sync_type,
                    progress,
                    conversations,
                    status_v3_messages,
                } => {
                    let filter = CLI_LOG_FILTER.load(std::sync::atomic::Ordering::Relaxed);
                    if filter <= 1 {
                        print_comfy_event("SYNC", ANSI_BRIGHT_CYAN, &format!("Received archive (type={}, progress={}%): {} chats, {} status updates",
                            sync_type, progress, conversations.len(), status_v3_messages.len()));
                    }
                }
                BaileysEvent::PollVote {
                    poll_id,
                    voter_jid,
                    selected_options,
                    tally,
                } => {
                    let filter = CLI_LOG_FILTER.load(std::sync::atomic::Ordering::Relaxed);
                    if filter <= 1 {
                        print_comfy_event(
                            "POLL",
                            ANSI_YELLOW,
                            &format!(
                                "Poll {} received vote from {}: {:?} (Total voters: {})",
                                poll_id, voter_jid, selected_options, tally.total_voters
                            ),
                        );
                    }
                }
                BaileysEvent::Call { call } => {
                    let filter = CLI_LOG_FILTER.load(std::sync::atomic::Ordering::Relaxed);
                    if filter <= 1 {
                        let media_type = if call.is_video {
                            "Video Call"
                        } else {
                            "Voice Call"
                        };
                        let group_info = if call.is_group {
                            format!(
                                " in group {}",
                                call.group_jid.as_deref().unwrap_or(&call.chat_jid)
                            )
                        } else {
                            "".to_string()
                        };
                        print_comfy_event(
                            "CALL",
                            ANSI_RED,
                            &format!(
                                "[{}] ID={} from {}{} (Status: {})",
                                media_type, call.id, call.from, group_info, call.status
                            ),
                        );
                    }
                }
                BaileysEvent::StoryReceived { story } => {
                    let filter = CLI_LOG_FILTER.load(std::sync::atomic::Ordering::Relaxed);
                    if filter <= 1 {
                        print_comfy_event(
                            "STORY",
                            ANSI_YELLOW,
                            &format!(
                                "New story received from {}: id={}",
                                story.author_jid, story.msg_id
                            ),
                        );
                    }
                }
                BaileysEvent::BlocklistUpdate { added, removed } => {
                    let filter = CLI_LOG_FILTER.load(std::sync::atomic::Ordering::Relaxed);
                    if filter <= 1 {
                        print_comfy_event(
                            "BLOCKLIST",
                            ANSI_YELLOW,
                            &format!(
                                "Server blocklist updated: added={}, removed={}",
                                added.len(),
                                removed.len()
                            ),
                        );
                    }
                }
                BaileysEvent::MessageEdited {
                    message_id,
                    chat_jid,
                    new_text,
                } => {
                    let filter = CLI_LOG_FILTER.load(std::sync::atomic::Ordering::Relaxed);
                    if filter <= 1 {
                        print_comfy_event(
                            "EDIT",
                            ANSI_BLUE,
                            &format!("Message {} in {}: {}", message_id, chat_jid, new_text),
                        );
                    }
                }
                BaileysEvent::MessageRevoked {
                    chat_jid,
                    message_id,
                    participant,
                } => {
                    let filter = CLI_LOG_FILTER.load(std::sync::atomic::Ordering::Relaxed);
                    if filter <= 1 {
                        let by = participant
                            .map(|p| format!(" by {}", p))
                            .unwrap_or_default();
                        print_comfy_event(
                            "REVOKE",
                            ANSI_RED,
                            &format!("Message {} deleted in {}{}", message_id, chat_jid, by),
                        );
                    }
                }
                BaileysEvent::ChatEphemeralUpdate {
                    chat_jid,
                    ephemeral_expiration,
                } => {
                    let filter = CLI_LOG_FILTER.load(std::sync::atomic::Ordering::Relaxed);
                    if filter <= 1 {
                        print_comfy_event(
                            "EPHEMERAL",
                            ANSI_CYAN,
                            &format!(
                                "Chat {} disappearing timer updated: {} ({}s)",
                                chat_jid,
                                baileys_core::format_ephemeral_duration(ephemeral_expiration),
                                ephemeral_expiration
                            ),
                        );
                    }
                }
                BaileysEvent::LabelEdit { label } => {
                    let filter = CLI_LOG_FILTER.load(std::sync::atomic::Ordering::Relaxed);
                    if filter <= 1 {
                        let action = if label.deleted { "Deleted" } else { "Saved" };
                        print_comfy_event(
                            "LABEL",
                            ANSI_MAGENTA,
                            &format!("{} Label ID={}: \"{}\"", action, label.id, label.name),
                        );
                    }
                }
                BaileysEvent::ChatLabelUpdate {
                    chat_jid,
                    label_id,
                    labeled,
                } => {
                    let filter = CLI_LOG_FILTER.load(std::sync::atomic::Ordering::Relaxed);
                    if filter <= 1 {
                        let action = if labeled {
                            "Tagged with"
                        } else {
                            "Untagged from"
                        };
                        print_comfy_event(
                            "LABEL",
                            ANSI_MAGENTA,
                            &format!("Chat {} {} Label ID={}", chat_jid, action, label_id),
                        );
                    }
                }
                BaileysEvent::MessageLabelUpdate {
                    chat_jid,
                    message_id,
                    label_id,
                    labeled,
                } => {
                    let filter = CLI_LOG_FILTER.load(std::sync::atomic::Ordering::Relaxed);
                    if filter <= 1 {
                        let action = if labeled {
                            "Tagged with"
                        } else {
                            "Untagged from"
                        };
                        print_comfy_event(
                            "LABEL",
                            ANSI_MAGENTA,
                            &format!(
                                "Message {} in {} {} Label ID={}",
                                message_id, chat_jid, action, label_id
                            ),
                        );
                    }
                }
                BaileysEvent::DeviceUpdate { user_jid, devices } => {
                    let filter = CLI_LOG_FILTER.load(std::sync::atomic::Ordering::Relaxed);
                    if filter <= 1 {
                        print_comfy_event(
                            "DEVICE",
                            ANSI_BLUE,
                            &format!(
                                "Companion device list updated for {}: {:?}",
                                user_jid, devices
                            ),
                        );
                    }
                }
                BaileysEvent::IdentityChanged { jid, timestamp } => {
                    let filter = CLI_LOG_FILTER.load(std::sync::atomic::Ordering::Relaxed);
                    if filter <= 1 {
                        print_comfy_event(
                            "IDENTITY",
                            ANSI_YELLOW,
                            &format!(
                                "Identity/Security code changed for {} at {}",
                                jid, timestamp
                            ),
                        );
                    }
                }
                BaileysEvent::MediaUpdated {
                    message_id,
                    direct_path,
                } => {
                    let filter = CLI_LOG_FILTER.load(std::sync::atomic::Ordering::Relaxed);
                    if filter <= 1 {
                        print_comfy_event(
                            "MEDIA",
                            ANSI_BRIGHT_CYAN,
                            &format!(
                                "Media re-download URL updated for message {}: {}",
                                message_id, direct_path
                            ),
                        );
                    }
                }
                BaileysEvent::Disconnected { reason } => {
                    print_comfy_event("CONN", ANSI_RED, &format!("Disconnected: {}", reason));
                }
                BaileysEvent::ChatUpdate {
                    jid,
                    pinned,
                    archived,
                    mute_until,
                    unread_count,
                } => {
                    let filter = CLI_LOG_FILTER.load(std::sync::atomic::Ordering::Relaxed);
                    if filter <= 1 {
                        let mut changes = Vec::new();
                        if let Some(p) = pinned {
                            changes.push(format!("pinned={}", p));
                        }
                        if let Some(a) = archived {
                            changes.push(format!("archived={}", a));
                        }
                        if let Some(m) = mute_until {
                            changes.push(format!("muted={:?}", m));
                        }
                        if let Some(u) = unread_count {
                            changes.push(format!("unread={}", u));
                        }
                        print_comfy_event(
                            "CHAT",
                            ANSI_BLUE,
                            &format!("Chat updated {}: {}", jid, changes.join(", ")),
                        );
                    }
                }
                BaileysEvent::ContactUpdate { contact } => {
                    let filter = CLI_LOG_FILTER.load(std::sync::atomic::Ordering::Relaxed);
                    if filter <= 1 {
                        print_comfy_event(
                            "CONTACT",
                            ANSI_BRIGHT_GREEN,
                            &format!(
                                "Contact updated {}: name={:?}, push_name={:?}, lid={:?}",
                                contact.jid, contact.name, contact.push_name, contact.lid
                            ),
                        );
                    }
                }
                BaileysEvent::AppStateSynced {
                    collections_synced,
                    mutations_applied,
                } => {
                    let filter = CLI_LOG_FILTER.load(std::sync::atomic::Ordering::Relaxed);
                    if filter <= 1 {
                        print_comfy_event(
                            "SYNCD",
                            ANSI_BRIGHT_GREEN,
                            &format!(
                                "AppState synced: {} collections, {} mutations applied",
                                collections_synced, mutations_applied
                            ),
                        );
                    }
                }
                BaileysEvent::MediaProgress {
                    task_id: _,
                    direction,
                    bytes_transferred,
                    total_bytes,
                    percentage,
                } => {
                    let filter = CLI_LOG_FILTER.load(std::sync::atomic::Ordering::Relaxed);
                    if filter <= 1 {
                        let total_mb = total_bytes as f64 / (1024.0 * 1024.0);
                        let transferred_mb = bytes_transferred as f64 / (1024.0 * 1024.0);
                        let bar_width = 20;
                        let filled = ((percentage / 100.0) * bar_width as f32).round() as usize;
                        let filled = filled.min(bar_width);
                        let empty = bar_width.saturating_sub(filled);
                        let bar = format!("{}{}", "=".repeat(filled), " ".repeat(empty));

                        if percentage >= 100.0 {
                            print_comfy_event(
                                "MEDIA",
                                ANSI_BRIGHT_GREEN,
                                &format!(
                                    "{}: [{}] 100% ({:.2} MB) complete",
                                    direction.to_uppercase(),
                                    bar,
                                    total_mb
                                ),
                            );
                        } else {
                            print_comfy_event(
                                "MEDIA",
                                ANSI_BRIGHT_CYAN,
                                &format!(
                                    "{}: [{}] {:.1}% ({:.2}/{:.2} MB)",
                                    direction.to_uppercase(),
                                    bar,
                                    percentage,
                                    transferred_mb,
                                    total_mb
                                ),
                            );
                        }
                    }
                }
            }
        }
    });

    // Start connection
    client.connect().await?;

    // Interactive REPL loop
    run_repl(&client, &store).await?;

    println!("[*] Shutting down Rose-Baileys engine...");
    client.disconnect().await;

    Ok(())
}
