use crate::selftest::run_self_test;
use crate::ui::{set_log_printer, LogPrinter, CLI_LOG_FILTER};
use baileys_core::proto_recognizer::{MessageExt, RecognizedMessageType};
use baileys_core::{
    BaileysClient, CarouselCard, InteractiveButton, InteractiveMessageBuilder, ListRow, ListSection,
};
use baileys_proto::schema::SchemaInspector;
use baileys_store::{ChatStore, NewsletterStore, SessionStore, SqliteStore};
use prost::Message as ProstMessage;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Config, Context, Editor, ExternalPrinter, Helper};
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

pub struct RustylineLogPrinter<P: ExternalPrinter + Send + Sync + 'static> {
    printer: Mutex<P>,
}

impl<P: ExternalPrinter + Send + Sync + 'static> LogPrinter for RustylineLogPrinter<P> {
    fn print_log(&self, text: String) {
        if let Ok(mut p) = self.printer.lock() {
            let _ = p.print(text);
        }
    }
}

const REPL_COMMANDS: &[&str] = &[
    "ping",
    "status",
    "qr",
    "selftest",
    "pair",
    "send",
    "react",
    "download",
    "downloadmedia",
    "autodownload",
    "logfilter",
    "profilepic",
    "setpfp",
    "removepfp",
    "presencesub",
    "presence",
    "onwa",
    "groupinfo",
    "groupcreate",
    "groupadd",
    "groupkick",
    "grouppromote",
    "groupdemote",
    "groupsubject",
    "groupdesc",
    "groupsetting",
    "groupinvite",
    "groupjoin",
    "groupparticipating",
    "groupjoinapproval",
    "grouprequests",
    "grouprequestupdate",
    "groupmemberaddmode",
    "grouprevokev4",
    "groupacceptv4",
    "newsletters",
    "newsletter",
    "follow",
    "unfollow",
    "channelmute",
    "channelcreate",
    "channelpost",
    "channelimage",
    "channelstory",
    "channelimagestory",
    "channelreact",
    "channelmsgs",
    "channelpromote",
    "channeldemote",
    "telemetry",
    "statusupdate",
    "statusfetch",
    "bizprofile",
    "privacy",
    "privacyset",
    "blocklist",
    "blocklistlocal",
    "block",
    "unblock",
    "poll",
    "vote",
    "polltally",
    "sendbuttons",
    "sendlist",
    "sendquickreply",
    "sendcarousel",
    "communitycreate",
    "communitycreatesub",
    "communitylink",
    "communityunlink",
    "communitysubgroups",
    "communityinfo",
    "communityinvite",
    "communityjoin",
    "communityleave",
    "callreject",
    "calls",
    "statuspost",
    "statusmedia",
    "statusview",
    "stories",
    "storiesauthor",
    "labels",
    "labeladd",
    "labeldel",
    "labelchat",
    "unlabelchat",
    "labelmsg",
    "unlabelmsg",
    "chatlabels",
    "labeledchats",
    "sendlocation",
    "sendlivelocation",
    "sendcontact",
    "sendgroupinvite",
    "sendevent",
    "sendjson",
    "ghostmode",
    "fakevn",
    "fakequote",
    "catalog",
    "product",
    "collections",
    "sendproduct",
    "sendorder",
    "orders",
    "cachedcatalog",
    "cachedcollections",
    "calllink",
    "reachoutstatus",
    "newchatcap",
    "cleandirty",
    "appstatesync",
    "syncd",
    "botlist",
    "channeladmincount",
    "channelsublive",
    "communityrevokeinvite",
    "communityinviteinfo",
    "communityephemeral",
    "communitysetting",
    "communitymemberaddmode",
    "communityjoinapproval",
    "communityparticipating",
    "productdel",
    "usynclid",
    "usyncpn",
    "usyncstatus",
    "devices",
    "deviceremove",
    "mediaretry",
    "urlpreview",
    "sendpreview",
    "sendimage",
    "sendmedia",
    "senddoc",
    "sendvideo",
    "sendvoice",
    "sendsticker",
    "proto",
    "cmdmode",
    "cmdstatus",
    "help",
    "exit",
    "quit",
];

#[derive(Default)]
pub struct RoseHelper;

impl Completer for RoseHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let (start, word) = match line[..pos].rfind(' ') {
            Some(idx) => (idx + 1, &line[idx + 1..pos]),
            None => (0, &line[..pos]),
        };

        if start == 0 {
            let matches: Vec<Pair> = REPL_COMMANDS
                .iter()
                .filter(|cmd| cmd.starts_with(word))
                .map(|cmd| Pair {
                    display: cmd.to_string(),
                    replacement: cmd.to_string(),
                })
                .collect();
            Ok((start, matches))
        } else {
            let prev_word = line[..start].trim();
            if prev_word.eq_ignore_ascii_case("qr") {
                let subcommands = ["large", "full", "raw", "small"];
                let matches: Vec<Pair> = subcommands
                    .iter()
                    .filter(|cmd| cmd.starts_with(word))
                    .map(|cmd| Pair {
                        display: cmd.to_string(),
                        replacement: cmd.to_string(),
                    })
                    .collect();
                Ok((start, matches))
            } else if prev_word.eq_ignore_ascii_case("logfilter") {
                let subcommands = ["debug", "trace", "all", "events", "errors", "quiet"];
                let matches: Vec<Pair> = subcommands
                    .iter()
                    .filter(|cmd| cmd.starts_with(word))
                    .map(|cmd| Pair {
                        display: cmd.to_string(),
                        replacement: cmd.to_string(),
                    })
                    .collect();
                Ok((start, matches))
            } else if prev_word.eq_ignore_ascii_case("help") {
                let categories = [
                    "messaging",
                    "groups",
                    "status",
                    "channels",
                    "community",
                    "commerce",
                    "privacy",
                    "waproto",
                    "system",
                ];
                let matches: Vec<Pair> = categories
                    .iter()
                    .chain(REPL_COMMANDS.iter())
                    .filter(|cmd| cmd.starts_with(word))
                    .map(|cmd| Pair {
                        display: cmd.to_string(),
                        replacement: cmd.to_string(),
                    })
                    .collect();
                Ok((start, matches))
            } else if prev_word.eq_ignore_ascii_case("autodownload") {
                let subcommands = ["on", "off"];
                let matches: Vec<Pair> = subcommands
                    .iter()
                    .filter(|cmd| cmd.starts_with(word))
                    .map(|cmd| Pair {
                        display: cmd.to_string(),
                        replacement: cmd.to_string(),
                    })
                    .collect();
                Ok((start, matches))
            } else if prev_word.eq_ignore_ascii_case("proto") {
                let subcommands = ["list", "search", "view", "inspect", "pull"];
                let matches: Vec<Pair> = subcommands
                    .iter()
                    .filter(|cmd| cmd.starts_with(word))
                    .map(|cmd| Pair {
                        display: cmd.to_string(),
                        replacement: cmd.to_string(),
                    })
                    .collect();
                Ok((start, matches))
            } else if prev_word.eq_ignore_ascii_case("cmdmode") {
                let subcommands = ["off", "self", "public"];
                let matches: Vec<Pair> = subcommands
                    .iter()
                    .filter(|cmd| cmd.starts_with(word))
                    .map(|cmd| Pair {
                        display: cmd.to_string(),
                        replacement: cmd.to_string(),
                    })
                    .collect();
                Ok((start, matches))
            } else {
                Ok((pos, Vec::new()))
            }
        }
    }
}

impl Hinter for RoseHelper {
    type Hint = String;
}

impl Highlighter for RoseHelper {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        _default: bool,
    ) -> std::borrow::Cow<'b, str> {
        if prompt.contains("online") {
            std::borrow::Cow::Owned("rose[\x1b[92monline\x1b[0m] > ".to_string())
        } else if prompt.contains("connecting") {
            std::borrow::Cow::Owned("rose[\x1b[33mconnecting\x1b[0m] > ".to_string())
        } else {
            std::borrow::Cow::Borrowed(prompt)
        }
    }
}

impl Validator for RoseHelper {}

impl Helper for RoseHelper {}

fn get_prompt(is_connected: bool) -> String {
    if is_connected {
        "rose[online] > ".to_string()
    } else {
        "rose[connecting] > ".to_string()
    }
}

pub async fn resolve_target_jid(client: &BaileysClient, target: &str) -> Option<String> {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.eq_ignore_ascii_case("me") || trimmed.eq_ignore_ascii_case("self") {
        if let Ok(Some(lid)) = client.me_lid().await {
            if let Some((user, server)) = lid.split_once('@') {
                let base_user = user.split(':').next().unwrap_or(user);
                return Some(format!("{}@{}", base_user, server));
            } else {
                return Some(lid);
            }
        }
        if let Ok(Some(jid)) = client.me_jid().await {
            if let Some((user, server)) = jid.split_once('@') {
                let base_user = user.split(':').next().unwrap_or(user);
                return Some(format!("{}@{}", base_user, server));
            } else {
                return Some(jid);
            }
        }
        None
    } else if !trimmed.contains('@') {
        if let Ok(Some(my_pn)) = client.me_jid().await {
            let my_user = my_pn
                .split('@')
                .next()
                .unwrap_or(&my_pn)
                .split(':')
                .next()
                .unwrap_or("");
            if !my_user.is_empty() && trimmed == my_user {
                if let Ok(Some(lid)) = client.me_lid().await {
                    if let Some((user, server)) = lid.split_once('@') {
                        let base_user = user.split(':').next().unwrap_or(user);
                        return Some(format!("{}@{}", base_user, server));
                    }
                }
            }
        }
        Some(format!("{}@s.whatsapp.net", trimmed))
    } else {
        Some(trimmed.to_string())
    }
}

pub fn parse_target_path_extra(raw: &str) -> Option<(String, String, Option<String>)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (target, rest) = trimmed.split_once(' ')?;
    let target = target.trim();
    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }

    if let Some(stripped) = rest.strip_prefix('"') {
        if let Some(end_idx) = stripped.find('"') {
            let path = &stripped[..end_idx];
            let extra = stripped[end_idx + 1..].trim();
            let extra_opt = if extra.is_empty() {
                None
            } else {
                Some(extra.to_string())
            };
            return Some((target.to_string(), path.to_string(), extra_opt));
        }
    } else if let Some(stripped) = rest.strip_prefix('\'') {
        if let Some(end_idx) = stripped.find('\'') {
            let path = &stripped[..end_idx];
            let extra = stripped[end_idx + 1..].trim();
            let extra_opt = if extra.is_empty() {
                None
            } else {
                Some(extra.to_string())
            };
            return Some((target.to_string(), path.to_string(), extra_opt));
        }
    }

    if let Some((p, extra)) = rest.split_once(' ') {
        let p_trimmed = p.trim().trim_matches(|c| c == '"' || c == '\'');
        let extra_trimmed = extra.trim();
        Some((
            target.to_string(),
            p_trimmed.to_string(),
            if extra_trimmed.is_empty() {
                None
            } else {
                Some(extra_trimmed.to_string())
            },
        ))
    } else {
        let p_trimmed = rest.trim().trim_matches(|c| c == '"' || c == '\'');
        Some((target.to_string(), p_trimmed.to_string(), None))
    }
}

pub async fn run_repl(
    client: &Arc<BaileysClient>,
    store: &Arc<SqliteStore>,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::builder().auto_add_history(true).build();

    let mut rl = Editor::with_config(config)?;
    rl.set_helper(Some(RoseHelper));

    // Register external printer for non-destructive log interleaving
    if let Ok(printer) = rl.create_external_printer() {
        set_log_printer(Box::new(RustylineLogPrinter {
            printer: Mutex::new(printer),
        }));
    }

    let history_file = ".rose_history";
    let _ = rl.load_history(history_file);

    let rl_arc = Arc::new(Mutex::new(rl));

    loop {
        let prompt = get_prompt(client.is_connected());
        let rl_clone = rl_arc.clone();

        let line_res =
            tokio::task::spawn_blocking(move || rl_clone.lock().unwrap().readline(&prompt)).await?;

        match line_res {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(mut lock) = rl_arc.lock() {
                    let _ = lock.save_history(history_file);
                }
                if trimmed.eq_ignore_ascii_case("exit") || trimmed.eq_ignore_ascii_case("quit") {
                    println!("[*] Exiting...");
                    break;
                } else if trimmed.eq_ignore_ascii_case("ping") {
                    println!("[*] Pinging WhatsApp Web Gateway...");
                    match client.ping().await {
                        Ok(dur) => println!("[+] Pong! Round-trip latency: {:?}", dur),
                        Err(e) => println!("[-] Ping failed: {}", e),
                    }
                } else if trimmed.eq_ignore_ascii_case("selftest") {
                    let _ = run_self_test(client, store).await;
                } else if trimmed.eq_ignore_ascii_case("status") {
                    let connected = client.is_connected();
                    let me = client
                        .me_jid()
                        .await
                        .unwrap_or(None)
                        .unwrap_or_else(|| "Not paired".into());
                    let lid = client
                        .me_lid()
                        .await
                        .unwrap_or(None)
                        .unwrap_or_else(|| "none".into());
                    let autodl = client.is_auto_download();
                    let bot_mode = crate::in_chat_bot::get_mode();
                    println!("[*] Connection Status: connected={}, me={}, lid={}, auto_download={}, in_chat_bot={}", connected, me, lid, autodl, bot_mode.display_badge());
                } else if trimmed.eq_ignore_ascii_case("telemetry") {
                    let snapshot = client.telemetry_snapshot();
                    let state = client.get_connection_state();
                    println!("\n============================================================");
                    println!("           ROSE-BAILEYS ENGINE HEALTH & TELEMETRY");
                    println!("============================================================");
                    println!("  Inbound Messages Count    : {}", snapshot.messages_in);
                    println!("  Outbound Messages Count   : {}", snapshot.messages_out);
                    println!(
                        "  Last Ping/Pong Latency    : {} ms",
                        snapshot.last_ping_latency_ms
                    );
                    println!(
                        "  Signal Cache Hit Rate     : {:.2}%",
                        snapshot.signal_cache_hit_rate_pct
                    );
                    println!("  Gateway Reconnects        : {}", snapshot.reconnect_count);
                    println!("  Connection FSM State      : {:?}", state);
                    println!("============================================================\n");
                } else if trimmed.eq_ignore_ascii_case("qr")
                    || trimmed.to_lowercase().starts_with("qr ")
                {
                    let sub = trimmed
                        .strip_prefix("qr")
                        .or_else(|| trimmed.strip_prefix("QR"))
                        .unwrap_or("")
                        .trim()
                        .to_lowercase();
                    if client.is_connected() {
                        println!("[+] Already connected and authenticated to WhatsApp! No QR code needed.");
                    } else {
                        match crate::qr::get_latest_qr() {
                            Some(code) => match sub.as_str() {
                                "raw" => {
                                    println!("[+] Raw QR Code payload: {}", code);
                                }
                                "large" | "full" | "big" => {
                                    println!("\n[!] WhatsApp QR Code (Full Size):\n");
                                    match crate::qr::render_baileys_large_qr(&code) {
                                        Ok(qr_str) => println!("{}", qr_str),
                                        Err(e) => {
                                            eprintln!("[-] Failed to render large QR code: {}", e)
                                        }
                                    }
                                }
                                _ => {
                                    println!(
                                        "\n[!] WhatsApp QR Code (Baileys Node.js Compact Size):\n"
                                    );
                                    match crate::qr::render_baileys_small_qr(&code, false) {
                                        Ok(qr_str) => {
                                            println!("{}", qr_str);
                                            println!("    [Tip] Type 'qr large' for full-size, or 'qr raw' for raw string.\n");
                                        }
                                        Err(e) => eprintln!("[-] Failed to render QR code: {}", e),
                                    }
                                }
                            },
                            None => {
                                println!("[-] No QR code available yet. Connecting to WhatsApp Web Gateway...");
                            }
                        }
                    }
                } else if trimmed.eq_ignore_ascii_case("logfilter")
                    || trimmed.to_lowercase().starts_with("logfilter ")
                {
                    let mode = trimmed
                        .strip_prefix("logfilter")
                        .or_else(|| trimmed.strip_prefix("LOGFILTER"))
                        .unwrap_or("")
                        .trim()
                        .to_lowercase();
                    match mode.as_str() {
                        "debug" => match crate::ui::reload_tracing_filter("debug") {
                            Ok(_) => {
                                CLI_LOG_FILTER.store(0, std::sync::atomic::Ordering::SeqCst);
                                println!("[+] Log filter set to DEBUG. Internal tracing logs and raw stanzas will be printed live.");
                            }
                            Err(e) => eprintln!("[-] Failed to reload tracing filter: {}", e),
                        },
                        "trace" => match crate::ui::reload_tracing_filter("trace") {
                            Ok(_) => {
                                CLI_LOG_FILTER.store(0, std::sync::atomic::Ordering::SeqCst);
                                println!("[+] Log filter set to TRACE. Highly verbose tracing logs enabled.");
                            }
                            Err(e) => eprintln!("[-] Failed to reload tracing filter: {}", e),
                        },
                        "all" => {
                            let _ = crate::ui::reload_tracing_filter("baileys_cli=info,baileys_core=info,baileys_signal=warn,baileys_binary=warn,baileys_store=warn");
                            CLI_LOG_FILTER.store(0, std::sync::atomic::Ordering::SeqCst);
                            println!("[+] Log filter set to ALL events (messages, presence, acks, sync, calls).");
                        }
                        "events" => {
                            let _ = crate::ui::reload_tracing_filter("baileys_cli=warn,baileys_core=warn,baileys_signal=warn,baileys_binary=warn,baileys_store=warn");
                            CLI_LOG_FILTER.store(1, std::sync::atomic::Ordering::SeqCst);
                            println!("[+] Log filter set to EVENTS ONLY (messages, acks, calls, status stories).");
                        }
                        "errors" => {
                            let _ = crate::ui::reload_tracing_filter("error");
                            CLI_LOG_FILTER.store(2, std::sync::atomic::Ordering::SeqCst);
                            println!("[+] Log filter set to ERRORS ONLY.");
                        }
                        "quiet" => {
                            let _ = crate::ui::reload_tracing_filter("off");
                            CLI_LOG_FILTER.store(3, std::sync::atomic::Ordering::SeqCst);
                            println!("[+] Log filter set to QUIET (silent, connection-only).");
                        }
                        _ => {
                            println!("Usage: logfilter <debug|trace|all|events|errors|quiet>");
                            let current =
                                match CLI_LOG_FILTER.load(std::sync::atomic::Ordering::SeqCst) {
                                    0 => "all (everything / debug)",
                                    1 => "events (messages, acks, calls, status stories)",
                                    2 => "errors (critical errors only)",
                                    _ => "quiet (silent)",
                                };
                            println!("Current filter: {}", current);
                        }
                    }
                } else if let Some(phone) = trimmed
                    .strip_prefix("pair ")
                    .or_else(|| trimmed.strip_prefix("pairingcode "))
                {
                    let phone_clean = phone.trim();
                    if !phone_clean.is_empty() {
                        println!(
                            "[*] Requesting WhatsApp 8-digit pairing code for '{}'...",
                            phone_clean
                        );
                        match client.request_pairing_code(phone_clean).await {
                            Ok(_code) => {
                                // The pairing code banner is emitted via BaileysEvent::PairingCode and rendered by the event loop
                            }
                            Err(e) => eprintln!("[-] Pairing code request failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: pair <PHONE_NUMBER>  (e.g. pair 62812345678)");
                    }
                } else if trimmed.to_lowercase().starts_with("send ") {
                    let rest = trimmed[5..].trim();
                    if let Some((target, msg_text)) = rest.split_once(' ') {
                        let norm_target = match resolve_target_jid(client, target).await {
                            Some(t) => t,
                            None => {
                                eprintln!("[-] Could not resolve target '{}'. If using 'me' or 'self', make sure client is connected.", target);
                                continue;
                            }
                        };
                        println!("[*] Sending encrypted message to '{}'...", norm_target);
                        match client.send_text_message(&norm_target, msg_text).await {
                            Ok(msg_id) => {
                                println!("[+] Message successfully sent!");
                                println!("    Target JID : {}", norm_target);
                                println!("    Message ID : {}", msg_id);
                                println!("    Content    : {}", msg_text);
                            }
                            Err(e) => eprintln!("[-] Send failed: {}", e),
                        }
                    } else {
                        if let Some(spec) = crate::commands::find_command("send") {
                            println!("{}", crate::commands::format_missing_args(spec, "<TEXT>"));
                        } else {
                            println!("[-] Usage: send <JID|me> <TEXT>  (e.g. send me Hello!)");
                        }
                    }
                } else if trimmed.to_lowercase().starts_with("react ") {
                    let parts: Vec<&str> = trimmed[6..].split_whitespace().collect();
                    if parts.len() >= 3 {
                        let target_jid = parts[0];
                        let target_msg_id = parts[1];
                        let emoji = parts[2];
                        println!(
                            "[*] Reacting with '{}' to message {}...",
                            emoji, target_msg_id
                        );
                        match client.send_reaction(target_jid, target_msg_id, emoji).await {
                            Ok(_) => println!("[+] Reaction sent!"),
                            Err(e) => eprintln!("[-] React failed: {}", e),
                        }
                    } else {
                        if let Some(spec) = crate::commands::find_command("react") {
                            println!(
                                "{}",
                                crate::commands::format_missing_args(spec, "<MSG_ID> <EMOJI>")
                            );
                        } else {
                            println!("[-] Usage: react <JID> <MSG_ID> <EMOJI>");
                        }
                    }
                } else if trimmed.to_lowercase().starts_with("downloadmedia")
                    || trimmed.to_lowercase().starts_with("download")
                    || trimmed.to_lowercase().starts_with("dl ")
                    || trimmed.to_lowercase() == "dl"
                {
                    let lower = trimmed.to_lowercase();
                    let rest = if lower.starts_with("downloadmedia") {
                        trimmed["downloadmedia".len()..].trim()
                    } else if lower.starts_with("download") {
                        trimmed["download".len()..].trim()
                    } else {
                        trimmed["dl".len()..].trim()
                    };

                    let mut parts = rest.split_whitespace();
                    let msg_id = parts.next();
                    let dest_path = parts.next();

                    if let Some(id) = msg_id {
                        if id.is_empty() {
                            if let Some(spec) = crate::commands::find_command("downloadmedia") {
                                println!(
                                    "{}",
                                    crate::commands::format_missing_args(spec, "<MSG_ID>")
                                );
                            } else {
                                println!("[-] Usage: downloadmedia <MSG_ID> [DEST_PATH]");
                            }
                        } else {
                            println!("[*] Streaming media download for message '{}'...", id);
                            let target = dest_path.map(std::path::Path::new);
                            match client.download_message_media_to_path(id, target).await {
                                Ok(path) => println!(
                                    "[+] Successfully downloaded & decrypted to: {}",
                                    path.display()
                                ),
                                Err(e) => eprintln!("[-] Streaming media download failed: {}", e),
                            }
                        }
                    } else {
                        if let Some(spec) = crate::commands::find_command("downloadmedia") {
                            println!("{}", crate::commands::format_missing_args(spec, "<MSG_ID>"));
                        } else {
                            println!("[-] Usage: downloadmedia <MSG_ID> [DEST_PATH]");
                        }
                    }
                } else if trimmed.to_lowercase().starts_with("autodownload ") {
                    let mode = trimmed[13..].trim().to_lowercase();
                    if mode == "on" || mode == "true" || mode == "1" {
                        client.set_auto_download(true);
                        println!("[+] Auto-download enabled: incoming media will be saved to ./downloads/");
                    } else if mode == "off" || mode == "false" || mode == "0" {
                        client.set_auto_download(false);
                        println!("[-] Auto-download disabled.");
                    } else {
                        println!("[-] Usage: autodownload <on|off>");
                    }
                } else if trimmed.to_lowercase().starts_with("presence ") {
                    let state = trimmed[9..].trim();
                    println!("[*] Updating presence to '{}'...", state);
                    match client.send_presence_update(state, None).await {
                        Ok(_) => println!("[+] Presence updated!"),
                        Err(e) => eprintln!("[-] Presence update failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("edit ") {
                    let rest = trimmed[5..].trim();
                    let parts: Vec<&str> = rest.splitn(3, ' ').collect();
                    if parts.len() == 3 {
                        let target_jid = parts[0];
                        let target_msg_id = parts[1];
                        let new_text = parts[2];
                        println!(
                            "[*] Editing message {} in '{}' to '{}'...",
                            target_msg_id, target_jid, new_text
                        );
                        match client
                            .edit_message(target_jid, target_msg_id, new_text)
                            .await
                        {
                            Ok(env_id) => println!("[+] Message edited! (Envelope ID: {})", env_id),
                            Err(e) => eprintln!("[-] Edit failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: edit <JID> <MSG_ID> <NEW_TEXT>");
                    }
                } else if trimmed.to_lowercase().starts_with("revoke ") {
                    let parts: Vec<&str> = trimmed[7..].split_whitespace().collect();
                    if parts.len() >= 2 {
                        let target_jid = parts[0];
                        let target_msg_id = parts[1];
                        println!(
                            "[*] Revoking message {} in '{}'...",
                            target_msg_id, target_jid
                        );
                        match client.revoke_message(target_jid, target_msg_id).await {
                            Ok(env_id) => {
                                println!("[+] Revoke message sent! (Envelope ID: {})", env_id)
                            }
                            Err(e) => eprintln!("[-] Revoke failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: revoke <JID> <MSG_ID>");
                    }
                } else if trimmed.to_lowercase().starts_with("disappearing ") {
                    let parts: Vec<&str> = trimmed[13..].split_whitespace().collect();
                    if parts.len() >= 2 {
                        let target_jid = parts[0];
                        let duration_str = parts[1];
                        if let Some(secs) = baileys_core::parse_ephemeral_duration(duration_str) {
                            println!(
                                "[*] Setting disappearing messages timer for '{}' to {} ({}s)...",
                                target_jid,
                                baileys_core::format_ephemeral_duration(secs),
                                secs
                            );
                            match client.set_chat_ephemeral(target_jid, secs).await {
                                Ok(_) => println!("[+] Disappearing messages timer updated!"),
                                Err(e) => {
                                    eprintln!("[-] Failed to update disappearing timer: {}", e)
                                }
                            }
                        } else {
                            println!(
                                "[-] Invalid duration: '{}'. Valid values: off, 24h, 7d, 90d",
                                duration_str
                            );
                        }
                    } else {
                        println!("[-] Usage: disappearing <JID> <off|24h|7d|90d>");
                    }
                } else if trimmed.to_lowercase().starts_with("disappearingget ") {
                    let target_jid = trimmed[15..].trim();
                    println!(
                        "[*] Querying disappearing messages timer for '{}'...",
                        target_jid
                    );
                    match client.get_chat_ephemeral(target_jid).await {
                        Ok(Some((exp, ts))) => {
                            let label = baileys_core::format_ephemeral_duration(exp);
                            let ts_str = ts
                                .map(|t| format!(" (set at unix timestamp {})", t))
                                .unwrap_or_default();
                            println!(
                                "[+] Ephemeral Disappearing Setting: {} ({}s){}",
                                label, exp, ts_str
                            );
                        }
                        Ok(None) => println!(
                            "[*] Chat has no ephemeral disappearing timer configured (Off)"
                        ),
                        Err(e) => eprintln!("[-] Failed to query disappearing timer: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("read ") {
                    let parts: Vec<&str> = trimmed[5..].split_whitespace().collect();
                    if parts.len() >= 2 {
                        let target_jid = parts[0];
                        let target_msg_id = parts[1];
                        let participant = parts.get(2).copied();
                        println!(
                            "[*] Sending read receipt for message {} in '{}'...",
                            target_msg_id, target_jid
                        );
                        match client
                            .send_read_receipt(target_jid, target_msg_id, participant)
                            .await
                        {
                            Ok(_) => println!("[+] Read receipt sent!"),
                            Err(e) => eprintln!("[-] Send read receipt failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: read <JID> <MSG_ID> [PARTICIPANT]");
                    }
                } else if trimmed.to_lowercase().starts_with("groupinfo ") {
                    let group_jid = trimmed[10..].trim();
                    println!("[*] Querying group metadata for '{}'...", group_jid);
                    match client.group_fetch_metadata(group_jid).await {
                        Ok(m) => {
                            println!("[+] Group info received:");
                            println!("    ID:                  {}", m.id);
                            println!("    Subject:             {}", m.subject);
                            println!(
                                "    Owner:               {}",
                                m.owner.as_deref().unwrap_or("none")
                            );
                            println!("    Total Participants:  {}", m.size);
                            println!("    Member Add Mode:     {}", m.member_add_mode);
                            println!(
                                "    Join Approval Mode:  {}",
                                if m.join_approval_mode { "ON" } else { "OFF" }
                            );
                            println!(
                                "    Announcements Only:  {}",
                                if m.announce { "YES" } else { "NO" }
                            );
                            println!(
                                "    Edit Info Locked:    {}",
                                if m.restrict { "YES" } else { "NO" }
                            );
                            if let Some(desc) = &m.description {
                                println!("    Description:         {}", desc);
                            }
                            println!("    Participants (first 10):");
                            for p in m.participants.iter().take(10) {
                                let role = p.admin.as_deref().unwrap_or("member");
                                println!("      - {} [{}]", p.jid, role);
                            }
                            if m.participants.len() > 10 {
                                println!("      ... and {} more", m.participants.len() - 10);
                            }
                        }
                        Err(e) => eprintln!("[-] Group metadata query failed: {}", e),
                    }
                } else if trimmed.eq_ignore_ascii_case("chats") {
                    println!("[*] Querying saved chats from SQLite store...");
                    match client.get_chats().await {
                        Ok(list) => {
                            println!("\n[+] Total Saved Chats: {}", list.len());
                            for (idx, c) in list.iter().enumerate().take(35) {
                                let name = c.name.as_deref().unwrap_or("<no name>");
                                let summary = c.last_message_summary.as_deref().unwrap_or("");
                                let mut badges = Vec::new();
                                if c.pinned {
                                    badges.push("📌PIN");
                                }
                                if c.archived {
                                    badges.push("📦ARCHIVE");
                                }
                                if let Some(mute_end) = c.mute_until {
                                    if mute_end == u64::MAX {
                                        badges.push("🔇MUTED(forever)");
                                    } else {
                                        badges.push("🔇MUTED");
                                    }
                                }
                                let badge_str = if badges.is_empty() {
                                    String::new()
                                } else {
                                    format!("[{}] ", badges.join(" "))
                                };
                                println!(
                                    "  {:2}. {:<35} | {:<20} | {:<16} | unread: {:2} | {}",
                                    idx + 1,
                                    c.jid,
                                    name,
                                    badge_str,
                                    c.unread_count,
                                    summary
                                );
                            }
                        }
                        Err(e) => eprintln!("[-] Failed to query chats: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("history ") {
                    let parts: Vec<&str> = trimmed[8..].split_whitespace().collect();
                    if !parts.is_empty() {
                        let jid = parts[0];
                        let limit = parts
                            .get(1)
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(20);
                        println!("[*] Querying history for '{}' (limit: {})...", jid, limit);
                        match client.get_messages(jid, limit, None).await {
                            Ok(msgs) => {
                                println!("[+] Retrieved {} messages:", msgs.len());
                                for m in msgs.iter().rev() {
                                    let who = if m.from_me { "me" } else { &m.sender_jid };
                                    let star_badge = if m.is_starred { " [★]" } else { "" };
                                    println!("  [{}]{} {}: {}", m.id, star_badge, who, m.summary);
                                }
                            }
                            Err(e) => eprintln!("[-] History query failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: history <JID> [LIMIT]");
                    }
                } else if trimmed.to_lowercase().starts_with("mute ") {
                    let rest = trimmed[5..].trim();
                    let parts: Vec<&str> = rest.split_whitespace().collect();
                    if !parts.is_empty() {
                        let target_jid = parts[0];
                        let dur_arg = parts
                            .get(1)
                            .map(|s| s.to_lowercase())
                            .unwrap_or_else(|| "8h".to_string());
                        let duration_opt = if dur_arg == "off"
                            || dur_arg == "unmute"
                            || dur_arg == "0"
                        {
                            None
                        } else if dur_arg == "forever" || dur_arg == "always" || dur_arg == "max" {
                            Some(u64::MAX)
                        } else if dur_arg.ends_with('h') {
                            let hours: u64 = dur_arg[..dur_arg.len() - 1].parse().unwrap_or(8);
                            Some(hours * 3600)
                        } else if dur_arg.ends_with('d') {
                            let days: u64 = dur_arg[..dur_arg.len() - 1].parse().unwrap_or(7);
                            Some(days * 86400)
                        } else if dur_arg.ends_with('m') {
                            let mins: u64 = dur_arg[..dur_arg.len() - 1].parse().unwrap_or(30);
                            Some(mins * 60)
                        } else if dur_arg.ends_with('s') {
                            let secs: u64 = dur_arg[..dur_arg.len() - 1].parse().unwrap_or(60);
                            Some(secs)
                        } else if let Ok(secs) = dur_arg.parse::<u64>() {
                            Some(secs)
                        } else {
                            Some(8 * 3600)
                        };

                        match duration_opt {
                            None => println!("[*] Unmuting chat '{}'...", target_jid),
                            Some(u64::MAX) => {
                                println!("[*] Muting chat '{}' indefinitely...", target_jid)
                            }
                            Some(secs) => println!(
                                "[*] Muting chat '{}' for {} seconds (~{}h)...",
                                target_jid,
                                secs,
                                secs / 3600
                            ),
                        }

                        match client.chat_modify_mute(target_jid, duration_opt).await {
                            Ok(_) => println!("[+] Mute status successfully updated!"),
                            Err(e) => eprintln!("[-] Mute failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: mute <JID> [8h|7d|forever|off]");
                    }
                } else if trimmed.to_lowercase().starts_with("pin ") {
                    let parts: Vec<&str> = trimmed[4..].split_whitespace().collect();
                    if parts.len() >= 2 {
                        let target_jid = parts[0];
                        let pin_val = match parts[1].to_lowercase().as_str() {
                            "on" | "true" | "1" => true,
                            "off" | "false" | "0" => false,
                            _ => true,
                        };
                        println!("[*] Setting pin={} for chat '{}'...", pin_val, target_jid);
                        match client.chat_modify_pin(target_jid, pin_val).await {
                            Ok(_) => println!("[+] Chat pin status updated!"),
                            Err(e) => eprintln!("[-] Pin failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: pin <JID> <on|off>");
                    }
                } else if trimmed.to_lowercase().starts_with("archive ") {
                    let parts: Vec<&str> = trimmed[8..].split_whitespace().collect();
                    if parts.len() >= 2 {
                        let target_jid = parts[0];
                        let archive_val = match parts[1].to_lowercase().as_str() {
                            "on" | "true" | "1" => true,
                            "off" | "false" | "0" => false,
                            _ => true,
                        };
                        println!(
                            "[*] Setting archive={} for chat '{}'...",
                            archive_val, target_jid
                        );
                        match client.chat_modify_archive(target_jid, archive_val).await {
                            Ok(_) => println!("[+] Chat archive status updated!"),
                            Err(e) => eprintln!("[-] Archive failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: archive <JID> <on|off>");
                    }
                } else if trimmed.to_lowercase().starts_with("star ") {
                    let parts: Vec<&str> = trimmed[5..].split_whitespace().collect();
                    if parts.len() >= 3 {
                        let target_jid = parts[0];
                        let target_msg_id = parts[1];
                        let star_val = match parts[2].to_lowercase().as_str() {
                            "on" | "true" | "1" => true,
                            "off" | "false" | "0" => false,
                            _ => true,
                        };
                        println!(
                            "[*] Setting star={} for message {} in '{}'...",
                            star_val, target_msg_id, target_jid
                        );
                        match client
                            .chat_modify_star(target_jid, target_msg_id, false, star_val)
                            .await
                        {
                            Ok(_) => println!("[+] Message star status updated!"),
                            Err(e) => eprintln!("[-] Star failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: star <JID> <MSG_ID> <on|off>");
                    }
                } else if trimmed.to_lowercase().starts_with("chatclear ") {
                    let parts: Vec<&str> = trimmed[10..].split_whitespace().collect();
                    if !parts.is_empty() {
                        let target_jid = parts[0];
                        let keep_starred = parts
                            .get(1)
                            .map(|v| matches!(v.to_lowercase().as_str(), "true" | "1" | "yes"))
                            .unwrap_or(false);
                        println!(
                            "[*] Clearing chat '{}' (keep_starred: {})...",
                            target_jid, keep_starred
                        );
                        match client.chat_modify_clear(target_jid, keep_starred).await {
                            Ok(_) => println!("[+] Chat cleared successfully!"),
                            Err(e) => eprintln!("[-] Chat clear failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: chatclear <JID> [keep_starred: true|false]");
                    }
                } else if trimmed.to_lowercase().starts_with("chatdelete ") {
                    let target_jid = trimmed[11..].trim();
                    if !target_jid.is_empty() {
                        println!("[*] Deleting chat '{}'...", target_jid);
                        match client.chat_modify_delete(target_jid).await {
                            Ok(_) => println!("[+] Chat deleted successfully!"),
                            Err(e) => eprintln!("[-] Chat delete failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: chatdelete <JID>");
                    }
                } else if trimmed.to_lowercase().starts_with("chatunread ") {
                    let parts: Vec<&str> = trimmed[11..].split_whitespace().collect();
                    if parts.len() >= 2 {
                        let target_jid = parts[0];
                        let unread =
                            matches!(parts[1].to_lowercase().as_str(), "on" | "true" | "1");
                        println!("[*] Setting chat unread={} for '{}'...", unread, target_jid);
                        match client.chat_modify_mark_unread(target_jid, unread).await {
                            Ok(_) => println!("[+] Chat unread status updated!"),
                            Err(e) => eprintln!("[-] Chat unread update failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: chatunread <JID> <on|off>");
                    }
                } else if trimmed.to_lowercase().starts_with("historyfetch ") {
                    let parts: Vec<&str> = trimmed[13..].split_whitespace().collect();
                    if parts.len() >= 2 {
                        let target_jid = parts[0];
                        let count = parts[1].parse::<u32>().unwrap_or(50);
                        let oldest_msg_id = parts.get(2).copied().unwrap_or("");
                        let oldest_from_me = parts
                            .get(3)
                            .map(|v| matches!(v.to_lowercase().as_str(), "true" | "1" | "me"))
                            .unwrap_or(false);
                        let oldest_timestamp = parts
                            .get(4)
                            .and_then(|s| s.parse::<u64>().ok())
                            .unwrap_or(0);

                        println!(
                            "[*] Requesting history for '{}' (count: {}, oldest_id: '{}')...",
                            target_jid, count, oldest_msg_id
                        );
                        match client
                            .fetch_message_history(
                                target_jid,
                                count,
                                oldest_msg_id,
                                oldest_from_me,
                                oldest_timestamp,
                            )
                            .await
                        {
                            Ok(iq_id) => {
                                println!("[+] History fetch request dispatched (IQ ID: {})", iq_id)
                            }
                            Err(e) => eprintln!("[-] History fetch failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: historyfetch <JID> <COUNT> [OLDEST_ID] [FROM_ME: true|false] [TIMESTAMP]");
                    }
                } else if trimmed.to_lowercase().starts_with("contact ") {
                    let jid = trimmed[8..].trim();
                    println!("[*] Querying contact info for '{}'...", jid);
                    match client.get_contact(jid).await {
                        Ok(Some(c)) => {
                            println!("[+] Contact Record for {}:", jid);
                            println!("    Name: {}", c.name.as_deref().unwrap_or("<none>"));
                            println!(
                                "    PushName: {}",
                                c.push_name.as_deref().unwrap_or("<none>")
                            );
                            println!("    LID: {}", c.lid.as_deref().unwrap_or("<none>"));
                        }
                        Ok(None) => println!("[-] No contact record found in SQLite for '{}'", jid),
                        Err(e) => eprintln!("[-] Contact query failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("groupcreate ") {
                    let rest = trimmed[12..].trim();
                    let (subject, participants) = if let Some((s, p_str)) = rest.split_once(' ') {
                        let p_list: Vec<String> = p_str
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty() && s.to_lowercase() != "self")
                            .collect();
                        (s.trim(), p_list)
                    } else {
                        (rest.trim(), Vec::new())
                    };
                    let p_desc = if participants.is_empty() {
                        "self only".to_string()
                    } else {
                        format!("{} participants", participants.len())
                    };
                    println!("[*] Creating group '{}' with {}...", subject, p_desc);
                    match client.group_create(subject, &participants).await {
                        Ok(gid) => {
                            println!("[+] Group created successfully!");
                            println!("    Subject   : {}", subject);
                            println!("    Group JID : {}", gid);
                        }
                        Err(e) => eprintln!("[-] Group creation failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("groupadd ") {
                    let rest = trimmed[9..].trim();
                    if let Some((grp, p_str)) = rest.split_once(' ') {
                        let participants: Vec<String> =
                            p_str.split(',').map(|s| s.trim().to_string()).collect();
                        println!(
                            "[*] Adding {} participants to group '{}'...",
                            participants.len(),
                            grp
                        );
                        match client
                            .group_participants_update(grp, &participants, "add")
                            .await
                        {
                            Ok(res) => println!("[+] Add participant results: {:?}", res),
                            Err(e) => eprintln!("[-] Group add failed: {}", e),
                        }
                    } else {
                        if let Some(spec) = crate::commands::find_command("groupadd") {
                            println!(
                                "{}",
                                crate::commands::format_missing_args(spec, "<P1,P2...>")
                            );
                        } else {
                            println!("[-] Usage: groupadd <GROUP_JID> <P1,P2...>");
                        }
                    }
                } else if trimmed.to_lowercase().starts_with("groupkick ") {
                    let rest = trimmed[10..].trim();
                    if let Some((grp, p_str)) = rest.split_once(' ') {
                        let participants: Vec<String> =
                            p_str.split(',').map(|s| s.trim().to_string()).collect();
                        println!(
                            "[*] Removing {} participants from group '{}'...",
                            participants.len(),
                            grp
                        );
                        match client
                            .group_participants_update(grp, &participants, "remove")
                            .await
                        {
                            Ok(res) => println!("[+] Remove participant results: {:?}", res),
                            Err(e) => eprintln!("[-] Group kick failed: {}", e),
                        }
                    } else {
                        if let Some(spec) = crate::commands::find_command("groupkick") {
                            println!(
                                "{}",
                                crate::commands::format_missing_args(spec, "<P1,P2...>")
                            );
                        } else {
                            println!("[-] Usage: groupkick <GROUP_JID> <P1,P2...>");
                        }
                    }
                } else if trimmed.to_lowercase().starts_with("grouppromote ") {
                    let rest = trimmed[13..].trim();
                    if let Some((grp, p_str)) = rest.split_once(' ') {
                        let participants: Vec<String> =
                            p_str.split(',').map(|s| s.trim().to_string()).collect();
                        println!(
                            "[*] Promoting {} participants in group '{}'...",
                            participants.len(),
                            grp
                        );
                        match client
                            .group_participants_update(grp, &participants, "promote")
                            .await
                        {
                            Ok(res) => println!("[+] Promote results: {:?}", res),
                            Err(e) => eprintln!("[-] Group promote failed: {}", e),
                        }
                    } else {
                        if let Some(spec) = crate::commands::find_command("grouppromote") {
                            println!(
                                "{}",
                                crate::commands::format_missing_args(spec, "<P1,P2...>")
                            );
                        } else {
                            println!("[-] Usage: grouppromote <GROUP_JID> <P1,P2...>");
                        }
                    }
                } else if trimmed.to_lowercase().starts_with("groupdemote ") {
                    let rest = trimmed[12..].trim();
                    if let Some((grp, p_str)) = rest.split_once(' ') {
                        let participants: Vec<String> =
                            p_str.split(',').map(|s| s.trim().to_string()).collect();
                        println!(
                            "[*] Demoting {} participants in group '{}'...",
                            participants.len(),
                            grp
                        );
                        match client
                            .group_participants_update(grp, &participants, "demote")
                            .await
                        {
                            Ok(res) => println!("[+] Demote results: {:?}", res),
                            Err(e) => eprintln!("[-] Group demote failed: {}", e),
                        }
                    } else {
                        if let Some(spec) = crate::commands::find_command("groupdemote") {
                            println!(
                                "{}",
                                crate::commands::format_missing_args(spec, "<P1,P2...>")
                            );
                        } else {
                            println!("[-] Usage: groupdemote <GROUP_JID> <P1,P2...>");
                        }
                    }
                } else if trimmed.to_lowercase().starts_with("groupsubject ") {
                    let rest = trimmed[13..].trim();
                    if let Some((grp, subject)) = rest.split_once(' ') {
                        println!(
                            "[*] Updating subject for group '{}' to '{}'...",
                            grp, subject
                        );
                        match client.group_update_subject(grp, subject).await {
                            Ok(_) => println!("[+] Group subject updated!"),
                            Err(e) => eprintln!("[-] Update subject failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: groupsubject <GROUP_JID> <NEW_SUBJECT>");
                    }
                } else if trimmed.to_lowercase().starts_with("groupdesc ") {
                    let rest = trimmed[10..].trim();
                    if let Some((grp, desc)) = rest.split_once(' ') {
                        println!("[*] Updating description for group '{}'...", grp);
                        match client.group_update_description(grp, desc).await {
                            Ok(_) => println!("[+] Group description updated!"),
                            Err(e) => eprintln!("[-] Update description failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: groupdesc <GROUP_JID> <DESCRIPTION>");
                    }
                } else if trimmed.to_lowercase().starts_with("groupinvite ") {
                    let grp = trimmed[12..].trim();
                    println!("[*] Requesting invite code for '{}'...", grp);
                    match client.group_get_invite_code(grp).await {
                        Ok(code) => println!(
                            "[+] Group invite code: {}\n    Link: https://chat.whatsapp.com/{}",
                            code, code
                        ),
                        Err(e) => eprintln!("[-] Get invite code failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("groupjoin ") {
                    let code = trimmed[10..].trim();
                    println!("[*] Joining group via invite code '{}'...", code);
                    match client.group_accept_invite(code).await {
                        Ok(jid) => println!("[+] Successfully joined group! JID: {}", jid),
                        Err(e) => eprintln!("[-] Join group failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("groupleave ") {
                    let grp = trimmed[11..].trim();
                    println!("[*] Leaving group '{}'...", grp);
                    match client.group_leave(grp).await {
                        Ok(_) => println!("[+] Successfully left group!"),
                        Err(e) => eprintln!("[-] Leave group failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("groupsetting ") {
                    let rest = trimmed[13..].trim();
                    if let Some((grp, setting)) = rest.split_once(' ') {
                        println!(
                            "[*] Updating group setting for '{}' to '{}'...",
                            grp, setting
                        );
                        match client.group_setting_update(grp, setting).await {
                            Ok(_) => println!("[+] Group setting updated!"),
                            Err(e) => eprintln!("[-] Group setting update failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: groupsetting <GRP> <announcement|not_announcement|locked|unlocked>");
                    }
                } else if trimmed.to_lowercase().starts_with("grouprevokeinvite ") {
                    let grp = trimmed[17..].trim();
                    println!("[*] Revoking invite code for '{}'...", grp);
                    match client.group_revoke_invite(grp).await {
                        Ok(code) => println!(
                            "[+] New group invite code: {}\n    Link: https://chat.whatsapp.com/{}",
                            code, code
                        ),
                        Err(e) => eprintln!("[-] Revoke invite failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("groupinviteinfo ") {
                    let code = trimmed[16..].trim();
                    println!("[*] Querying invite info for code '{}'...", code);
                    match client.group_get_invite_info(code).await {
                        Ok(node) => println!("[+] Invite info: {:?}", node),
                        Err(e) => eprintln!("[-] Query invite info failed: {}", e),
                    }
                } else if trimmed.eq_ignore_ascii_case("groupparticipating") {
                    println!("[*] Querying all participating groups from WhatsApp...");
                    match client.group_fetch_all_participating().await {
                        Ok(groups) => {
                            println!("\n[+] Total Participating Groups: {}", groups.len());
                            for (idx, g) in groups.iter().enumerate() {
                                let approval = if g.join_approval_mode {
                                    " [Approval: ON]"
                                } else {
                                    ""
                                };
                                println!(
                                    "  {:2}. {:<35} | {:<25} | {} members | mode: {}{}",
                                    idx + 1,
                                    g.id,
                                    g.subject,
                                    g.size,
                                    g.member_add_mode,
                                    approval
                                );
                            }
                        }
                        Err(e) => eprintln!("[-] Failed to fetch participating groups: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("groupjoinapproval ") {
                    let rest = trimmed[18..].trim();
                    if let Some((grp, mode)) = rest.split_once(' ') {
                        println!(
                            "[*] Setting join approval mode for '{}' to '{}'...",
                            grp, mode
                        );
                        match client.group_join_approval_mode(grp, mode).await {
                            Ok(_) => println!("[+] Group join approval mode set to '{}'!", mode),
                            Err(e) => eprintln!("[-] Group join approval update failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: groupjoinapproval <GROUP_JID> <on|off>");
                    }
                } else if trimmed.to_lowercase().starts_with("grouprequests ") {
                    let grp = trimmed[14..].trim();
                    println!("[*] Querying pending join requests for '{}'...", grp);
                    match client.group_request_participants_list(grp).await {
                        Ok(reqs) => {
                            println!("[+] Pending Join Requests ({}):", reqs.len());
                            for (idx, r) in reqs.iter().enumerate() {
                                println!(
                                    "  {:2}. JID: {} | Method: {} | Request Time: {}",
                                    idx + 1,
                                    r.jid,
                                    r.request_method.as_deref().unwrap_or("unknown"),
                                    r.requested_time
                                );
                            }
                        }
                        Err(e) => eprintln!("[-] Failed to fetch join requests: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("grouprequestupdate ") {
                    let rest = trimmed[19..].trim();
                    let parts: Vec<&str> = rest.split_whitespace().collect();
                    if parts.len() >= 3 {
                        let grp = parts[0];
                        let action = parts[1];
                        let participants: Vec<String> =
                            parts[2].split(',').map(|s| s.trim().to_string()).collect();
                        println!(
                            "[*] Updating {} join requests ({}) for '{}'...",
                            participants.len(),
                            action,
                            grp
                        );
                        match client
                            .group_request_participants_update(grp, &participants, action)
                            .await
                        {
                            Ok(res) => println!("[+] Join request update completed: {:?}", res),
                            Err(e) => eprintln!("[-] Join request update failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: grouprequestupdate <GROUP_JID> <approve|reject> <JID1,JID2...>");
                    }
                } else if trimmed.to_lowercase().starts_with("groupmemberaddmode ") {
                    let rest = trimmed[19..].trim();
                    if let Some((grp, mode)) = rest.split_once(' ') {
                        println!("[*] Setting member add mode for '{}' to '{}'...", grp, mode);
                        match client.group_member_add_mode(grp, mode).await {
                            Ok(_) => println!("[+] Member add mode set to '{}'!", mode),
                            Err(e) => eprintln!("[-] Failed to set member add mode: {}", e),
                        }
                    } else {
                        println!(
                            "[-] Usage: groupmemberaddmode <GROUP_JID> <admin_add|all_member_add>"
                        );
                    }
                } else if trimmed.to_lowercase().starts_with("grouprevokev4 ") {
                    let rest = trimmed[14..].trim();
                    if let Some((grp, jid)) = rest.split_once(' ') {
                        println!("[*] Revoking v4 group invite for '{}' in '{}'...", jid, grp);
                        match client.group_revoke_invite_v4(grp, jid).await {
                            Ok(_) => println!("[+] Group v4 invite revoked!"),
                            Err(e) => eprintln!("[-] Failed to revoke v4 invite: {}", e),
                        }
                    } else {
                        println!("[-] Usage: grouprevokev4 <GROUP_JID> <USER_JID>");
                    }
                } else if trimmed.to_lowercase().starts_with("groupacceptv4 ") {
                    let rest = trimmed[14..].trim();
                    let parts: Vec<&str> = rest.split_whitespace().collect();
                    if parts.len() >= 4 {
                        let group_jid = parts[0];
                        let code = parts[1];
                        let exp = parts[2].parse::<i64>().unwrap_or(0);
                        let admin = parts[3];
                        println!(
                            "[*] Accepting v4 group invite for '{}' from admin '{}'...",
                            group_jid, admin
                        );
                        match client
                            .group_accept_invite_v4(group_jid, code, exp, admin)
                            .await
                        {
                            Ok(gid) => println!("[+] Joined group via v4 invite! JID: {}", gid),
                            Err(e) => eprintln!("[-] Failed to accept v4 invite: {}", e),
                        }
                    } else {
                        println!("[-] Usage: groupacceptv4 <GROUP_JID> <INVITE_CODE> <EXPIRATION> <ADMIN_JID>");
                    }
                } else if trimmed.to_lowercase().starts_with("reply ") {
                    let parts: Vec<&str> = trimmed[6..].split_whitespace().collect();
                    if parts.len() >= 3 {
                        let to_jid = parts[0];
                        let quoted_id = parts[1];
                        let text = parts[2..].join(" ");
                        println!(
                            "[*] Sending reply to message {} in '{}'...",
                            quoted_id, to_jid
                        );
                        match client
                            .send_text_message_with_quote(to_jid, &text, quoted_id, None, None)
                            .await
                        {
                            Ok(msg_id) => println!("[+] Reply sent! (ID: {})", msg_id),
                            Err(e) => eprintln!("[-] Reply failed: {}", e),
                        }
                    } else {
                        if let Some(spec) = crate::commands::find_command("reply") {
                            println!(
                                "{}",
                                crate::commands::format_missing_args(
                                    spec,
                                    "<QUOTED_MSG_ID> <TEXT>"
                                )
                            );
                        } else {
                            println!("[-] Usage: reply <JID> <QUOTED_MSG_ID> <TEXT>");
                        }
                    }
                } else if trimmed.to_lowercase().starts_with("sendimage ") {
                    if let Some((target, file_path, caption)) =
                        parse_target_path_extra(&trimmed[10..])
                    {
                        let norm_target = match resolve_target_jid(client, &target).await {
                            Some(t) => t,
                            None => {
                                eprintln!("[-] Could not resolve target '{}'. If using 'me' or 'self', make sure client is connected.", target);
                                continue;
                            }
                        };
                        match std::fs::read(&file_path) {
                            Ok(bytes) => {
                                println!(
                                    "[*] Sending image ({} bytes) to '{}'...",
                                    bytes.len(),
                                    norm_target
                                );
                                if let Some(ref cap) = caption {
                                    println!("    Caption: {}", cap);
                                }
                                match client
                                    .send_image_message(&norm_target, &bytes, caption.as_deref())
                                    .await
                                {
                                    Ok(msg_id) => {
                                        println!("[+] Image message successfully sent!");
                                        println!("    Target JID : {}", norm_target);
                                        println!("    Message ID : {}", msg_id);
                                    }
                                    Err(e) => eprintln!("[-] Send image failed: {}", e),
                                }
                            }
                            Err(e) => {
                                eprintln!("[-] Failed to read image file '{}': {}", file_path, e)
                            }
                        }
                    } else {
                        println!("[-] Usage: sendimage <JID|me> <FILE_PATH> [CAPTION]");
                    }
                } else if trimmed.to_lowercase().starts_with("sendmedia ")
                    || trimmed.to_lowercase() == "sendmedia"
                {
                    let raw_args = trimmed["sendmedia".len()..].trim();
                    let mut parts = raw_args.splitn(3, ' ');
                    if let (Some(target), Some(first_arg), Some(rest_path)) =
                        (parts.next(), parts.next(), parts.next())
                    {
                        let norm_target = match resolve_target_jid(client, target).await {
                            Some(t) => t,
                            None => {
                                eprintln!("[-] Could not resolve target '{}'.", target);
                                continue;
                            }
                        };
                        let (file_path_raw, caption_raw) =
                            if let Some(stripped) = rest_path.strip_prefix('"') {
                                if let Some(end_idx) = stripped.find('"') {
                                    let p = &stripped[..end_idx];
                                    let c = stripped[end_idx + 1..].trim();
                                    (
                                        p.to_string(),
                                        if c.is_empty() {
                                            None
                                        } else {
                                            Some(c.to_string())
                                        },
                                    )
                                } else {
                                    (rest_path.trim_matches('"').to_string(), None)
                                }
                            } else if let Some((p, c)) = rest_path.split_once(' ') {
                                (p.trim().to_string(), Some(c.trim().to_string()))
                            } else {
                                (rest_path.trim().to_string(), None)
                            };

                        let (parsed_media_type, file_path, caption) = if let Ok(mt) =
                            first_arg.parse::<baileys_media::MediaType>()
                        {
                            (mt, file_path_raw, caption_raw)
                        } else if let Ok(mt) = file_path_raw.parse::<baileys_media::MediaType>() {
                            (mt, first_arg.to_string(), caption_raw)
                        } else {
                            println!("[-] Unknown media type. Valid: image, video, audio, ptt, document, sticker");
                            continue;
                        };

                        let path = std::path::Path::new(&file_path);
                        if !path.exists() {
                            eprintln!("[-] File '{}' not found.", file_path);
                            continue;
                        }

                        println!(
                            "[*] Sending media {:?} ({}) to '{}' via streaming pipeline...",
                            parsed_media_type, file_path, norm_target
                        );
                        if let Some(ref cap) = caption {
                            println!("    Caption: {}", cap);
                        }

                        match client
                            .send_media_file_streaming(
                                &norm_target,
                                path,
                                parsed_media_type,
                                caption.as_deref(),
                            )
                            .await
                        {
                            Ok(msg_id) => {
                                println!("[+] Media message successfully sent!");
                                println!("    Target JID : {}", norm_target);
                                println!("    Message ID : {}", msg_id);
                            }
                            Err(e) => eprintln!("[-] Streaming media send failed: {}", e),
                        }
                    } else {
                        if let Some(spec) = crate::commands::find_command("sendmedia") {
                            println!(
                                "{}",
                                crate::commands::format_missing_args(
                                    spec,
                                    "<JID> <TYPE|PATH> <PATH|TYPE> [CAPTION]"
                                )
                            );
                        } else {
                            println!("[-] Usage: sendmedia <JID|me> <image|video|audio|document|sticker> <FILE_PATH> [CAPTION]");
                        }
                    }
                } else if trimmed.to_lowercase().starts_with("sendvoice ") {
                    if let Some((target, file_path, seconds_opt)) =
                        parse_target_path_extra(&trimmed[10..])
                    {
                        let norm_target = match resolve_target_jid(client, &target).await {
                            Some(t) => t,
                            None => {
                                eprintln!("[-] Could not resolve target '{}'.", target);
                                continue;
                            }
                        };
                        let seconds = seconds_opt.and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
                        match std::fs::read(&file_path) {
                            Ok(bytes) => {
                                println!(
                                    "[*] Sending voice note ({} bytes, {}s) to '{}'...",
                                    bytes.len(),
                                    seconds,
                                    norm_target
                                );
                                match client
                                    .send_voice_note(&norm_target, &bytes, seconds, None)
                                    .await
                                {
                                    Ok(msg_id) => println!("[+] Voice note sent! (ID: {})", msg_id),
                                    Err(e) => eprintln!("[-] Send voice note failed: {}", e),
                                }
                            }
                            Err(e) => {
                                eprintln!("[-] Failed to read audio file '{}': {}", file_path, e)
                            }
                        }
                    } else {
                        println!("[-] Usage: sendvoice <JID|me> <FILE_PATH> [SECONDS]");
                    }
                } else if trimmed.to_lowercase().starts_with("senddoc ") {
                    let raw = trimmed[8..].trim();
                    let pipe_parts: Vec<&str> = raw.split('|').map(|s| s.trim()).collect();
                    if let Some((target, file_path, filename_opt)) =
                        parse_target_path_extra(pipe_parts[0])
                    {
                        let norm_target = match resolve_target_jid(client, &target).await {
                            Some(t) => t,
                            None => {
                                eprintln!("[-] Could not resolve target '{}'.", target);
                                continue;
                            }
                        };
                        let default_name = std::path::Path::new(&file_path)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("document.bin");
                        let filename = filename_opt.as_deref().unwrap_or(default_name);
                        let caption = pipe_parts.get(1).copied();
                        match std::fs::read(&file_path) {
                            Ok(bytes) => {
                                println!(
                                    "[*] Sending document '{}' ({} bytes) to '{}'...",
                                    filename,
                                    bytes.len(),
                                    norm_target
                                );
                                match client
                                    .send_document(
                                        &norm_target,
                                        &bytes,
                                        filename,
                                        "application/octet-stream",
                                        None,
                                        None,
                                        caption,
                                    )
                                    .await
                                {
                                    Ok(msg_id) => println!("[+] Document sent! (ID: {})", msg_id),
                                    Err(e) => eprintln!("[-] Send document failed: {}", e),
                                }
                            }
                            Err(e) => eprintln!("[-] Failed to read file '{}': {}", file_path, e),
                        }
                    } else {
                        println!("[-] Usage: senddoc <JID|me> <FILE_PATH> [FILENAME] [| CAPTION]");
                    }
                } else if trimmed.to_lowercase().starts_with("sendvideo ") {
                    let raw = trimmed[10..].trim();
                    let pipe_parts: Vec<&str> = raw.split('|').map(|s| s.trim()).collect();
                    if let Some((target, file_path, _)) = parse_target_path_extra(pipe_parts[0]) {
                        let norm_target = match resolve_target_jid(client, &target).await {
                            Some(t) => t,
                            None => {
                                eprintln!("[-] Could not resolve target '{}'.", target);
                                continue;
                            }
                        };
                        let caption = pipe_parts.get(1).copied();
                        let is_gif = pipe_parts
                            .get(2)
                            .map(|s| matches!(s.to_lowercase().as_str(), "true" | "1" | "gif"))
                            .unwrap_or(false);
                        match std::fs::read(&file_path) {
                            Ok(bytes) => {
                                println!(
                                    "[*] Sending video ({} bytes, is_gif: {}) to '{}'...",
                                    bytes.len(),
                                    is_gif,
                                    norm_target
                                );
                                match client
                                    .send_video(&norm_target, &bytes, caption, is_gif, None)
                                    .await
                                {
                                    Ok(msg_id) => println!("[+] Video sent! (ID: {})", msg_id),
                                    Err(e) => eprintln!("[-] Send video failed: {}", e),
                                }
                            }
                            Err(e) => {
                                eprintln!("[-] Failed to read video file '{}': {}", file_path, e)
                            }
                        }
                    } else {
                        println!("[-] Usage: sendvideo <JID|me> <FILE_PATH> [| CAPTION] [| is_gif: true|false]");
                    }
                } else if trimmed.to_lowercase().starts_with("sendsticker ") {
                    if let Some((target, file_path, anim_opt)) =
                        parse_target_path_extra(&trimmed[12..])
                    {
                        let norm_target = match resolve_target_jid(client, &target).await {
                            Some(t) => t,
                            None => {
                                eprintln!("[-] Could not resolve target '{}'.", target);
                                continue;
                            }
                        };
                        let is_animated = anim_opt
                            .map(|s| matches!(s.to_lowercase().as_str(), "true" | "1" | "anim"))
                            .unwrap_or(false);
                        match std::fs::read(&file_path) {
                            Ok(bytes) => {
                                println!(
                                    "[*] Sending sticker ({} bytes, animated: {}) to '{}'...",
                                    bytes.len(),
                                    is_animated,
                                    norm_target
                                );
                                match client
                                    .send_sticker_message(&norm_target, &bytes, is_animated)
                                    .await
                                {
                                    Ok(msg_id) => println!("[+] Sticker sent! (ID: {})", msg_id),
                                    Err(e) => eprintln!("[-] Send sticker failed: {}", e),
                                }
                            }
                            Err(e) => {
                                eprintln!("[-] Failed to read sticker file '{}': {}", file_path, e)
                            }
                        }
                    } else {
                        println!(
                            "[-] Usage: sendsticker <JID|me> <FILE_PATH> [is_animated: true|false]"
                        );
                    }
                } else if trimmed.to_lowercase().starts_with("statusupdate ") {
                    let status = trimmed[13..].trim();
                    println!("[*] Updating bio/about status to: '{}'...", status);
                    match client.update_profile_status(status).await {
                        Ok(_) => println!("[+] Status updated!"),
                        Err(e) => eprintln!("[-] Update status failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("statusfetch ") {
                    let target_jid = trimmed[12..].trim();
                    println!("[*] Fetching About / Bio status for '{}'...", target_jid);
                    match client.fetch_status(target_jid).await {
                        Ok(Some(st)) => {
                            let time_str = st
                                .set_at
                                .map(|t| format!(" (set at: {})", t))
                                .unwrap_or_default();
                            println!("[+] Status: \"{}\"{}", st.status, time_str);
                        }
                        Ok(None) => println!("[-] No status found for '{}'", target_jid),
                        Err(e) => eprintln!("[-] Failed to fetch status: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("bizprofile ") {
                    let target_jid = trimmed[11..].trim();
                    println!("[*] Querying Business Profile for '{}'...", target_jid);
                    match client.get_business_profile(target_jid).await {
                        Ok(Some(bp)) => {
                            println!("[+] WhatsApp Business Profile for {}:", bp.jid);
                            if let Some(ref d) = bp.description {
                                println!("    Description: {}", d);
                            }
                            if let Some(ref a) = bp.address {
                                println!("    Address:     {}", a);
                            }
                            if let Some(ref e) = bp.email {
                                println!("    Email:       {}", e);
                            }
                            if let Some(ref c) = bp.category {
                                println!("    Category:    {}", c);
                            }
                            for w in &bp.websites {
                                println!("    Website:     {}", w);
                            }
                        }
                        Ok(None) => println!("[-] No business profile found for '{}'", target_jid),
                        Err(e) => eprintln!("[-] Failed to fetch business profile: {}", e),
                    }
                } else if trimmed.eq_ignore_ascii_case("privacy") {
                    println!("[*] Fetching privacy settings...");
                    match client.fetch_privacy_settings().await {
                        Ok(settings) => {
                            println!("[+] Privacy Settings:");
                            if let Some(ref v) = settings.readreceipts {
                                println!("    Read Receipts:    {}", v);
                            }
                            if let Some(ref v) = settings.profile {
                                println!("    Profile Photo:    {}", v);
                            }
                            if let Some(ref v) = settings.status {
                                println!("    Status / Stories: {}", v);
                            }
                            if let Some(ref v) = settings.online {
                                println!("    Online Presence:  {}", v);
                            }
                            if let Some(ref v) = settings.last {
                                println!("    Last Seen:        {}", v);
                            }
                            if let Some(ref v) = settings.groupadd {
                                println!("    Group Add:        {}", v);
                            }
                            if let Some(ref v) = settings.calladd {
                                println!("    Call Add:         {}", v);
                            }
                            for (k, v) in &settings.raw {
                                if ![
                                    "readreceipts",
                                    "profile",
                                    "status",
                                    "online",
                                    "last",
                                    "groupadd",
                                    "calladd",
                                ]
                                .contains(&k.as_str())
                                {
                                    println!("    {}: {}", k, v);
                                }
                            }
                        }
                        Err(e) => eprintln!("[-] Fetch privacy settings failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("privacyset ") {
                    let rest = trimmed[11..].trim();
                    if let Some((cat, val)) = rest.split_once(' ') {
                        println!("[*] Updating privacy '{}' -> '{}'...", cat, val);
                        match client.update_privacy_setting(cat, val).await {
                            Ok(_) => println!("[+] Privacy setting updated!"),
                            Err(e) => eprintln!("[-] Privacy setting update failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: privacyset <CATEGORY> <VALUE>");
                    }
                } else if trimmed.eq_ignore_ascii_case("blocklist") {
                    println!("[*] Fetching blocklist from WhatsApp server...");
                    match client.fetch_blocklist().await {
                        Ok(list) => {
                            println!("[+] Server Blocked Contacts: {}", list.len());
                            for (i, j) in list.iter().enumerate() {
                                println!("    {}. {}", i + 1, j);
                            }
                        }
                        Err(e) => eprintln!("[-] Fetch blocklist failed: {}", e),
                    }
                } else if trimmed.eq_ignore_ascii_case("blocklistlocal") {
                    println!("[*] Reading blocked contacts from local SQLite store...");
                    match client.get_blocked_contacts().await {
                        Ok(list) => {
                            println!("[+] Locally Cached Blocked Contacts: {}", list.len());
                            for (i, j) in list.iter().enumerate() {
                                println!("    {}. {}", i + 1, j);
                            }
                        }
                        Err(e) => eprintln!("[-] Failed to read local blocklist: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("block ") {
                    let jid = trimmed[6..].trim();
                    println!("[*] Blocking '{}'...", jid);
                    match client.update_block_status(jid, "block").await {
                        Ok(_) => println!("[+] Contact blocked!"),
                        Err(e) => eprintln!("[-] Block failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("unblock ") {
                    let jid = trimmed[8..].trim();
                    println!("[*] Unblocking '{}'...", jid);
                    match client.update_block_status(jid, "unblock").await {
                        Ok(_) => println!("[+] Contact unblocked!"),
                        Err(e) => eprintln!("[-] Unblock failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("profilepic ") {
                    let parts: Vec<&str> = trimmed[11..].split_whitespace().collect();
                    if !parts.is_empty() {
                        let jid = parts[0];
                        let ptype = parts.get(1).copied().unwrap_or("preview");
                        println!(
                            "[*] Querying profile picture for '{}' (type: {})...",
                            jid, ptype
                        );
                        match client.profile_picture_url(jid, ptype).await {
                            Ok(Some(url)) => println!("[+] Profile picture URL:\n    {}", url),
                            Ok(None) => println!(
                                "[-] No profile picture found or access denied for '{}'",
                                jid
                            ),
                            Err(e) => eprintln!("[-] Query profile picture failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: profilepic <JID> [preview|image]");
                    }
                } else if trimmed.to_lowercase().starts_with("setpfp ") {
                    let parts: Vec<&str> = trimmed[7..].split_whitespace().collect();
                    if !parts.is_empty() {
                        let file_path = parts[0];
                        let target_jid = parts.get(1).copied();
                        match std::fs::read(file_path) {
                            Ok(bytes) => {
                                println!(
                                    "[*] Setting profile picture from '{}' ({} bytes)...",
                                    file_path,
                                    bytes.len()
                                );
                                match client.update_profile_picture(target_jid, &bytes).await {
                                    Ok(_) => println!("[+] Profile picture updated successfully!"),
                                    Err(e) => eprintln!("[-] Update profile picture failed: {}", e),
                                }
                            }
                            Err(e) => {
                                eprintln!("[-] Failed to read image file '{}': {}", file_path, e)
                            }
                        }
                    } else {
                        println!("[-] Usage: setpfp <IMAGE_FILE_PATH> [TARGET_JID]");
                    }
                } else if trimmed.to_lowercase().starts_with("removepfp") {
                    let target_jid = trimmed
                        .strip_prefix("removepfp")
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty());
                    println!("[*] Removing profile picture...");
                    match client.remove_profile_picture(target_jid).await {
                        Ok(_) => println!("[+] Profile picture removed successfully!"),
                        Err(e) => eprintln!("[-] Remove profile picture failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("presencesub ") {
                    let jid = trimmed[12..].trim();
                    println!("[*] Subscribing to presence for '{}'...", jid);
                    match client.presence_subscribe(jid).await {
                        Ok(_) => println!("[+] Subscribed to presence updates for '{}'!", jid),
                        Err(e) => eprintln!("[-] Presence subscription failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("onwa ") {
                    let numbers_str = trimmed[5..].trim();
                    let numbers: Vec<&str> = numbers_str
                        .split(',')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .collect();
                    println!(
                        "[*] Checking {} number(s) on WhatsApp via USync...",
                        numbers.len()
                    );
                    match client.check_on_whatsapp(&numbers).await {
                        Ok(results) => {
                            println!("[+] USync Query Results ({} found):", results.len());
                            for res in results {
                                let status_label = if res.exists {
                                    "EXISTS"
                                } else {
                                    "NOT REGISTERED"
                                };
                                let phone = if res.phone_number.is_empty() {
                                    "-"
                                } else {
                                    &res.phone_number
                                };
                                println!(
                                    "    {:<35} (Phone: {:<15}) -> {}",
                                    res.jid, phone, status_label
                                );
                            }
                        }
                        Err(e) => eprintln!("[-] USync query failed: {}", e),
                    }
                } else if trimmed.eq_ignore_ascii_case("devices")
                    || trimmed.to_lowercase().starts_with("devices ")
                {
                    let arg = if trimmed.len() > 7 {
                        trimmed[7..].trim()
                    } else {
                        ""
                    };
                    if arg.is_empty() {
                        println!("[*] Querying paired companion devices for our account...");
                        match client.get_paired_devices().await {
                            Ok(devs) => {
                                println!("[+] Paired Companion Devices ({} total):", devs.len());
                                for (i, d) in devs.iter().enumerate() {
                                    let primary_tag =
                                        if d.id == 0 { " (Primary Device)" } else { "" };
                                    let key_idx = d
                                        .key_index
                                        .map(|k| k.to_string())
                                        .unwrap_or_else(|| "-".to_string());
                                    println!(
                                        "    {:2}. Device ID: {:<3} | Key Index: {:<4}{}",
                                        i + 1,
                                        d.id,
                                        key_idx,
                                        primary_tag
                                    );
                                }
                            }
                            Err(e) => eprintln!("[-] Failed to query paired devices: {}", e),
                        }
                    } else {
                        let jid = if arg.contains('@') {
                            arg.to_string()
                        } else {
                            format!("{}@s.whatsapp.net", arg.trim_start_matches('+'))
                        };
                        println!("[*] Querying devices for '{}' via USync...", jid);
                        match client.get_user_devices(&jid).await {
                            Ok(dev_ids) => {
                                println!("[+] Retrieved {} device(s) for {}:", dev_ids.len(), jid);
                                for (i, id) in dev_ids.iter().enumerate() {
                                    let primary_tag =
                                        if *id == 0 { " (Primary Device)" } else { "" };
                                    println!(
                                        "    {:2}. Device ID: {:<3}{}",
                                        i + 1,
                                        id,
                                        primary_tag
                                    );
                                }
                            }
                            Err(e) => eprintln!("[-] USync get_user_devices failed: {}", e),
                        }
                    }
                } else if trimmed.to_lowercase().starts_with("deviceremove ") {
                    let id_str = trimmed[13..].trim();
                    match id_str.parse::<u32>() {
                        Ok(dev_id) => {
                            println!("[*] Revoking/unlinking companion device ID {}...", dev_id);
                            match client.remove_companion_device(dev_id).await {
                                Ok(_) => println!(
                                    "[+] Companion device ID {} successfully unlinked/revoked!",
                                    dev_id
                                ),
                                Err(e) => eprintln!("[-] Failed to remove companion device: {}", e),
                            }
                        }
                        Err(_) => eprintln!(
                            "[-] Invalid device ID '{}'. Must be an integer (e.g. deviceremove 1)",
                            id_str
                        ),
                    }
                } else if trimmed.eq_ignore_ascii_case("newsletters") {
                    println!("[*] Querying subscribed WhatsApp Channels (Newsletters)...");
                    match client.newsletter_get_subscribed().await {
                        Ok(channels) => {
                            println!("\n[+] Total Subscribed Channels: {}", channels.len());
                            for (i, ch) in channels.iter().enumerate() {
                                let role_str = ch.role.as_deref().unwrap_or("SUBSCRIBER");
                                let mute_badge = if ch.muted { "🔇MUTED" } else { "🔔ACTIVE" };
                                let verified_badge =
                                    if ch.verified { " [✓VERIFIED]" } else { "" };
                                println!(
                                    "  {:2}. {:<32} | {:<24}{} | {:<10} | {:<8} | {} subs",
                                    i + 1,
                                    ch.jid,
                                    ch.name,
                                    verified_badge,
                                    role_str,
                                    mute_badge,
                                    ch.subscribers_count
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!(
                                "[-] Live query failed: {}. Falling back to SQLite cache...",
                                e
                            );
                            if let Ok(cached) = store.get_newsletters().await {
                                println!("[+] Cached Channels ({}):", cached.len());
                                for (i, ch) in cached.iter().enumerate() {
                                    println!(
                                        "  {:2}. {:<32} | {:<24} | {} subs",
                                        i + 1,
                                        ch.jid,
                                        ch.name,
                                        ch.subscribers_count
                                    );
                                }
                            }
                        }
                    }
                } else if trimmed.to_lowercase().starts_with("newsletter ") {
                    let input = trimmed[11..].trim();
                    println!("[*] Querying channel metadata for '{}'...", input);
                    match client.newsletter_query_metadata(input).await {
                        Ok(m) => {
                            println!("[+] Channel Metadata:");
                            println!("    JID:         {}", m.jid);
                            println!("    Name:        {}", m.name);
                            println!("    Verified:    {}", m.verified);
                            println!("    Subscribers: {}", m.subscribers_count);
                            println!("    Role:        {}", m.role.as_deref().unwrap_or("NONE"));
                            println!("    Muted:       {}", m.muted);
                            if let Some(ref d) = m.description {
                                println!("    Description: {}", d);
                            }
                            if let Some(ref code) = m.invite_code {
                                println!("    Invite Link: https://whatsapp.com/channel/{}", code);
                            }
                            if let Some(ref pic) = m.picture_url {
                                println!("    Picture URL: {}", pic);
                            }
                        }
                        Err(e) => eprintln!("[-] Channel metadata query failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("follow ") {
                    let jid = trimmed[7..].trim();
                    println!("[*] Following WhatsApp Channel '{}'...", jid);
                    match client.newsletter_follow(jid).await {
                        Ok(_) => println!("[+] Successfully followed channel '{}'!", jid),
                        Err(e) => eprintln!("[-] Follow failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("unfollow ") {
                    let jid = trimmed[9..].trim();
                    println!("[*] Unfollowing WhatsApp Channel '{}'...", jid);
                    match client.newsletter_unfollow(jid).await {
                        Ok(_) => println!("[+] Successfully unfollowed channel '{}'!", jid),
                        Err(e) => eprintln!("[-] Unfollow failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("channelmute ") {
                    let parts: Vec<&str> = trimmed[12..].split_whitespace().collect();
                    if parts.len() >= 2 {
                        let jid = parts[0];
                        let mute_val = matches!(
                            parts[1].to_lowercase().as_str(),
                            "on" | "true" | "1" | "mute"
                        );
                        println!("[*] Setting mute={} for channel '{}'...", mute_val, jid);
                        match client.newsletter_mute(jid, mute_val).await {
                            Ok(_) => println!("[+] Channel mute updated to {}!", mute_val),
                            Err(e) => eprintln!("[-] Channel mute failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: channelmute <JID> <on|off>");
                    }
                } else if trimmed.to_lowercase().starts_with("channelcreate ") {
                    let rest = trimmed[14..].trim();
                    let (name, desc) = if let Some((n, d)) = rest.split_once('|') {
                        (n.trim(), Some(d.trim()))
                    } else {
                        (rest, None)
                    };
                    println!("[*] Creating WhatsApp Channel '{}'...", name);
                    match client.newsletter_create(name, desc).await {
                        Ok(m) => {
                            println!("[+] Channel successfully created!");
                            println!("    JID:  {}", m.jid);
                            println!("    Name: {}", m.name);
                            if let Some(ref code) = m.invite_code {
                                println!("    Link: https://whatsapp.com/channel/{}", code);
                            }
                        }
                        Err(e) => eprintln!("[-] Channel creation failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("channelpost ") {
                    let rest = trimmed[12..].trim();
                    if let Some((jid, post_text)) = rest.split_once(' ') {
                        println!("[*] Broadcasting post to channel '{}'...", jid);
                        match client.newsletter_send_post(jid, post_text).await {
                            Ok(id) => println!("[+] Broadcast post sent! (ID: {})", id),
                            Err(e) => eprintln!("[-] Broadcast post failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: channelpost <JID> <TEXT>");
                    }
                } else if trimmed.to_lowercase().starts_with("channelimage ") {
                    let rest = trimmed[13..].trim();
                    if let Some((jid, path_and_caption)) = rest.split_once(' ') {
                        let (path_str, caption) = match path_and_caption.split_once(' ') {
                            Some((p, c)) => (p.trim(), Some(c.trim())),
                            None => (path_and_caption.trim(), None),
                        };
                        match tokio::fs::read(path_str).await {
                            Ok(bytes) => {
                                println!(
                                    "[*] Uploading unencrypted image ({} bytes) to channel '{}'...",
                                    bytes.len(),
                                    jid
                                );
                                match client.newsletter_send_image(jid, &bytes, caption).await {
                                    Ok(id) => {
                                        println!("[+] Channel image post published! (ID: {})", id)
                                    }
                                    Err(e) => eprintln!("[-] Channel image post failed: {}", e),
                                }
                            }
                            Err(e) => {
                                eprintln!("[-] Failed to read image file '{}': {}", path_str, e)
                            }
                        }
                    } else {
                        println!("[-] Usage: channelimage <JID> <FILE_PATH> [CAPTION]");
                    }
                } else if trimmed.to_lowercase().starts_with("channelstory ") {
                    let parts: Vec<&str> = trimmed[13..].splitn(4, ' ').collect();
                    if parts.len() >= 4 {
                        let jid = parts[0];
                        if let Ok(server_id) = parts[1].parse::<i32>() {
                            let name = parts[2];
                            let text = parts[3];
                            // By default isolate to self unless '--all' is specified
                            let (text, audience) = if text.ends_with("--all") {
                                (text.trim_end_matches("--all").trim(), None)
                            } else {
                                let creds = store.get_creds().await.ok().flatten();
                                let me = creds.and_then(|c| c.me_jid);
                                let aud = me.map(|m| vec![m]);
                                (text, aud)
                            };
                            println!("[*] Publishing interactive Channel Story to status@broadcast (Channel: {}, Post ID: {}, Audience: {:?})...", name, server_id, audience);
                            match client
                                .newsletter_send_status_story(jid, server_id, name, text, audience)
                                .await
                            {
                                Ok(id) => {
                                    println!("[+] Channel Status Story published! (ID: {})", id)
                                }
                                Err(e) => eprintln!("[-] Channel Status Story failed: {}", e),
                            }
                        } else {
                            println!("[-] Invalid SERVER_ID: must be a number");
                        }
                    } else {
                        println!("[-] Usage: channelstory <JID> <SERVER_ID> <NAME> <TEXT> [--all]");
                    }
                } else if trimmed.to_lowercase().starts_with("channelimagestory ") {
                    let parts: Vec<&str> = trimmed[18..].split_whitespace().collect();
                    if parts.len() >= 4 {
                        let jid = parts[0];
                        if let Ok(server_id) = parts[1].parse::<i32>() {
                            let name = parts[2];
                            let path_str = parts[3];
                            let caption = if parts.len() > 4 && parts[4] != "--all" {
                                Some(parts[4..].join(" "))
                            } else {
                                None
                            };
                            let is_all = parts.contains(&"--all");
                            let audience = if is_all {
                                None
                            } else {
                                let creds = store.get_creds().await.ok().flatten();
                                creds.and_then(|c| c.me_jid).map(|m| vec![m])
                            };
                            match tokio::fs::read(path_str).await {
                                Ok(bytes) => {
                                    println!("[*] Publishing interactive HD Image Story to status@broadcast (Channel: {}, Post ID: {}, Size: {} bytes, Audience: {:?})...", name, server_id, bytes.len(), audience);
                                    match client.newsletter_send_image_status_story(&bytes, caption.as_deref(), jid, server_id, name, audience).await {
                                                Ok(id) => println!("[+] Interactive HD Channel Image Story published! (ID: {})", id),
                                                Err(e) => eprintln!("[-] Channel Image Story failed: {}", e),
                                            }
                                }
                                Err(e) => {
                                    eprintln!("[-] Failed to read image file '{}': {}", path_str, e)
                                }
                            }
                        } else {
                            println!("[-] Invalid SERVER_ID: must be a number");
                        }
                    } else {
                        println!("[-] Usage: channelimagestory <JID> <SERVER_ID> <NAME> <IMAGE_PATH> [CAPTION] [--all]");
                    }
                } else if trimmed.to_lowercase().starts_with("channelreact ") {
                    let parts: Vec<&str> = trimmed[13..].split_whitespace().collect();
                    if parts.len() >= 3 {
                        let jid = parts[0];
                        if let Ok(server_id) = parts[1].parse::<u64>() {
                            let emoji = parts[2];
                            println!(
                                "[*] Reacting with '{}' to channel post {}...",
                                emoji, server_id
                            );
                            match client.newsletter_react(jid, server_id, emoji).await {
                                Ok(_) => println!("[+] Channel reaction sent!"),
                                Err(e) => eprintln!("[-] Channel reaction failed: {}", e),
                            }
                        } else {
                            println!("[-] Invalid server_id: must be a number");
                        }
                    } else {
                        println!("[-] Usage: channelreact <JID> <SERVER_ID> <EMOJI>");
                    }
                } else if trimmed.to_lowercase().starts_with("channelmsgs ") {
                    let parts: Vec<&str> = trimmed[12..].split_whitespace().collect();
                    if !parts.is_empty() {
                        let jid = parts[0];
                        let count = parts
                            .get(1)
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(20);
                        println!("[*] Fetching {} channel messages for '{}'...", count, jid);
                        match client.newsletter_fetch_messages(jid, count, None).await {
                            Ok(msgs) => {
                                println!("[+] Retrieved {} channel messages:", msgs.len());
                                for m in &msgs {
                                    let reacts = if m.reactions.is_empty() {
                                        String::new()
                                    } else {
                                        let r_strs: Vec<String> = m
                                            .reactions
                                            .iter()
                                            .map(|(k, v)| format!("{}:{}", k, v))
                                            .collect();
                                        format!(" [{}]", r_strs.join(" "))
                                    };
                                    let views = m
                                        .views_count
                                        .map(|v| format!(" ({} views)", v))
                                        .unwrap_or_default();
                                    println!(
                                        "  [ID: {}]{}{}: {}",
                                        m.server_id, views, reacts, m.summary
                                    );
                                }
                            }
                            Err(e) => eprintln!("[-] Fetch channel messages failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: channelmsgs <JID> [COUNT]");
                    }
                } else if trimmed.to_lowercase().starts_with("channelpromote ") {
                    let parts: Vec<&str> = trimmed[15..].split_whitespace().collect();
                    if parts.len() >= 2 {
                        let (channel, user) = (parts[0], parts[1]);
                        println!(
                            "[*] Promoting user '{}' to admin in channel '{}'...",
                            user, channel
                        );
                        match client.newsletter_promote(channel, user).await {
                            Ok(_) => println!("[+] User promoted to channel admin successfully!"),
                            Err(e) => eprintln!("[-] Channel admin promotion failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: channelpromote <CHANNEL_JID> <USER_JID>");
                    }
                } else if trimmed.to_lowercase().starts_with("channeldemote ") {
                    let parts: Vec<&str> = trimmed[14..].split_whitespace().collect();
                    if parts.len() >= 2 {
                        let (channel, user) = (parts[0], parts[1]);
                        println!(
                            "[*] Demoting user '{}' from admin in channel '{}'...",
                            user, channel
                        );
                        match client.newsletter_demote(channel, user).await {
                            Ok(_) => println!("[+] User demoted from channel admin successfully!"),
                            Err(e) => eprintln!("[-] Channel admin demotion failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: channeldemote <CHANNEL_JID> <USER_JID>");
                    }
                } else if trimmed.to_lowercase().starts_with("poll ") {
                    let rest = trimmed[5..].trim();
                    let parts: Vec<&str> = rest.split('|').map(|s| s.trim()).collect();
                    if parts.len() >= 3 {
                        if let Some((target, question)) = parts[0].split_once(' ') {
                            let options: Vec<String> =
                                parts[1..].iter().map(|s| s.to_string()).collect();
                            println!(
                                "[*] Creating poll '{}' for '{}' with {} options...",
                                question,
                                target,
                                options.len()
                            );
                            match client.send_poll(target, question, &options, 1).await {
                                Ok(msg_id) => {
                                    println!("[+] Poll successfully created! (ID: {})", msg_id)
                                }
                                Err(e) => eprintln!("[-] Create poll failed: {}", e),
                            }
                        } else {
                            if let Some(spec) = crate::commands::find_command("poll") {
                                println!(
                                    "{}",
                                    crate::commands::format_missing_args(spec, "<QUESTION>")
                                );
                            } else {
                                println!("[-] Usage: poll <JID> <QUESTION> | <OPT1> | <OPT2> [| OPT3...]");
                            }
                        }
                    } else {
                        if let Some(spec) = crate::commands::find_command("poll") {
                            println!(
                                "{}",
                                crate::commands::format_missing_args(
                                    spec,
                                    "<OPT1> | <OPT2> (at least 2 options separated by '|')"
                                )
                            );
                        } else {
                            println!(
                                "[-] Usage: poll <JID> <QUESTION> | <OPT1> | <OPT2> [| OPT3...]"
                            );
                        }
                    }
                } else if trimmed.to_lowercase().starts_with("vote ") {
                    let parts: Vec<&str> = trimmed[5..].split_whitespace().collect();
                    if parts.len() >= 3 {
                        let target_jid = parts[0];
                        let poll_id = parts[1];
                        let selected: Vec<String> = parts[2..]
                            .join(" ")
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .collect();
                        println!(
                            "[*] Submitting vote on poll '{}' to '{}': {:?}...",
                            poll_id, target_jid, selected
                        );
                        match client.send_poll_vote(target_jid, poll_id, &selected).await {
                            Ok(msg_id) => {
                                println!("[+] Encrypted vote submitted! (Update ID: {})", msg_id)
                            }
                            Err(e) => eprintln!("[-] Vote submission failed: {}", e),
                        }
                    } else {
                        if let Some(spec) = crate::commands::find_command("vote") {
                            println!(
                                "{}",
                                crate::commands::format_missing_args(
                                    spec,
                                    "<POLL_ID> <OPT1,OPT2...>"
                                )
                            );
                        } else {
                            println!("[-] Usage: vote <JID> <POLL_ID> <OPT1,OPT2...>");
                        }
                    }
                } else if trimmed.to_lowercase().starts_with("polltally ") {
                    let poll_id = trimmed[10..].trim();
                    println!("[*] Querying tally for poll '{}'...", poll_id);
                    match client.get_poll_tally(poll_id).await {
                        Ok(Some(tally)) => {
                            println!("[+] Poll: {}", tally.poll_id);
                            println!("    Question: {}", tally.question);
                            println!("    Total Voters: {}", tally.total_voters);
                            for (opt, count) in &tally.option_votes {
                                println!("    - {}: {} vote(s)", opt, count);
                            }
                        }
                        Ok(None) => println!("[-] Poll '{}' not found in store.", poll_id),
                        Err(e) => eprintln!("[-] Error querying tally: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("sendbuttons ") {
                    let rest = trimmed[12..].trim();
                    let parts: Vec<&str> = rest.split('|').map(|s| s.trim()).collect();
                    if parts.len() >= 2 {
                        if let Some((target, body)) = parts[0].split_once(' ') {
                            let mut buttons = Vec::new();
                            for b_str in &parts[1..] {
                                if let Some((b_title, b_val)) = b_str.split_once(':') {
                                    if b_val.starts_with("http://") || b_val.starts_with("https://")
                                    {
                                        buttons.push(InteractiveButton::cta_url(b_title, b_val));
                                    } else {
                                        buttons
                                            .push(InteractiveButton::quick_reply(b_title, b_val));
                                    }
                                } else {
                                    buttons.push(InteractiveButton::quick_reply(b_str, b_str));
                                }
                            }
                            println!("[*] Sending interactive buttons to '{}'...", target);
                            match client
                                .send_interactive_buttons(target, body, &buttons, None, None)
                                .await
                            {
                                Ok(msg_id) => {
                                    println!("[+] Interactive buttons sent! (ID: {})", msg_id)
                                }
                                Err(e) => eprintln!("[-] Send buttons failed: {}", e),
                            }
                        } else {
                            if let Some(spec) = crate::commands::find_command("sendbuttons") {
                                println!(
                                    "{}",
                                    crate::commands::format_missing_args(
                                        spec,
                                        "<BODY> | <TITLE:URL|REPLY:PARAM>..."
                                    )
                                );
                            } else {
                                println!("[-] Usage: sendbuttons <JID> <BODY> | <TITLE:URL|REPLY:PARAM>...");
                            }
                        }
                    } else {
                        if let Some(spec) = crate::commands::find_command("sendbuttons") {
                            println!(
                                "{}",
                                crate::commands::format_missing_args(
                                    spec,
                                    "<BUTTONS> (at least one button separated by '|')"
                                )
                            );
                        } else {
                            println!(
                                "[-] Usage: sendbuttons <JID> <BODY> | <TITLE:URL|REPLY:PARAM>..."
                            );
                        }
                    }
                } else if trimmed.to_lowercase().starts_with("sendlist ") {
                    let rest = trimmed[9..].trim();
                    let parts: Vec<&str> = rest.split('|').map(|s| s.trim()).collect();
                    if parts.len() >= 4 {
                        if let Some((target, title)) = parts[0].split_once(' ') {
                            let btn_text = parts[1];
                            let sec_title = parts[2];
                            let mut rows = Vec::new();
                            for r_str in &parts[3..] {
                                let r_parts: Vec<&str> =
                                    r_str.split(':').map(|s| s.trim()).collect();
                                if r_parts.len() >= 3 {
                                    rows.push(ListRow {
                                        title: r_parts[0].to_string(),
                                        description: Some(r_parts[1].to_string()),
                                        row_id: r_parts[2].to_string(),
                                    });
                                } else if r_parts.len() == 2 {
                                    rows.push(ListRow {
                                        title: r_parts[0].to_string(),
                                        description: None,
                                        row_id: r_parts[1].to_string(),
                                    });
                                } else {
                                    rows.push(ListRow {
                                        title: r_str.to_string(),
                                        description: None,
                                        row_id: r_str.to_string(),
                                    });
                                }
                            }
                            let section = ListSection {
                                title: sec_title.to_string(),
                                rows,
                            };
                            println!("[*] Sending interactive list menu to '{}'...", target);
                            match client
                                .send_list_message(target, title, title, btn_text, &[section], None)
                                .await
                            {
                                Ok(msg_id) => println!("[+] List message sent! (ID: {})", msg_id),
                                Err(e) => eprintln!("[-] Send list failed: {}", e),
                            }
                        } else {
                            if let Some(spec) = crate::commands::find_command("sendlist") {
                                println!(
                                    "{}",
                                    crate::commands::format_missing_args(spec, "<TITLE>")
                                );
                            } else {
                                println!("[-] Usage: sendlist <JID> <TITLE> | <BTN_TEXT> | <SEC_NAME> | <ROW_TITLE:ROW_DESC:ROW_ID>");
                            }
                        }
                    } else {
                        if let Some(spec) = crate::commands::find_command("sendlist") {
                            println!("{}", crate::commands::format_missing_args(spec, "<BTN_TEXT> | <SEC_NAME> | <ROW_TITLE:ROW_DESC:ROW_ID> (requires 4 pipe segments)"));
                        } else {
                            println!("[-] Usage: sendlist <JID> <TITLE> | <BTN_TEXT> | <SEC_NAME> | <ROW_TITLE:ROW_DESC:ROW_ID>");
                        }
                    }
                } else if trimmed.to_lowercase().starts_with("sendquickreply ") {
                    let rest = trimmed[15..].trim();
                    let parts: Vec<&str> = rest.split('|').map(|s| s.trim()).collect();
                    if parts.len() >= 2 {
                        if let Some((target, text)) = parts[0].split_once(' ') {
                            let mut builder = InteractiveMessageBuilder::new(text);
                            for btn_str in &parts[1..] {
                                if let Some((btn_id, btn_display)) = btn_str.split_once(':') {
                                    builder =
                                        builder.add_quick_reply(btn_id.trim(), btn_display.trim());
                                } else {
                                    builder = builder.add_quick_reply(btn_str, btn_str);
                                }
                            }
                            println!(
                                "[*] Sending native flow quick reply buttons to '{}'...",
                                target
                            );
                            match client.send_interactive_message(target, builder).await {
                                Ok(msg_id) => println!(
                                    "[+] Interactive Quick Reply message sent! (ID: {})",
                                    msg_id
                                ),
                                Err(e) => eprintln!("[-] Send quick reply failed: {}", e),
                            }
                        } else {
                            println!("[-] Usage: sendquickreply <JID> <TEXT> | <BTN1_ID:BTN1_TEXT> [| <BTN2_ID:BTN2_TEXT>...]");
                        }
                    } else {
                        println!("[-] Usage: sendquickreply <JID> <TEXT> | <BTN1_ID:BTN1_TEXT> [| <BTN2_ID:BTN2_TEXT>...]");
                    }
                } else if trimmed.to_lowercase().starts_with("sendcarousel ") {
                    let rest = trimmed[13..].trim();
                    let parts: Vec<&str> = rest.split('|').map(|s| s.trim()).collect();
                    if parts.len() >= 2 {
                        if let Some((target, title)) = parts[0].split_once(' ') {
                            let mut builder = InteractiveMessageBuilder::new(title);
                            for card_str in &parts[1..] {
                                let c_parts: Vec<&str> =
                                    card_str.split(':').map(|s| s.trim()).collect();
                                if c_parts.len() >= 3 {
                                    let card = CarouselCard::new(c_parts[0])
                                        .add_url_button(c_parts[1], c_parts[2]);
                                    builder = builder.add_carousel_card(card);
                                } else if c_parts.len() == 2 {
                                    let card = CarouselCard::new(c_parts[0])
                                        .add_quick_reply(c_parts[1], c_parts[1]);
                                    builder = builder.add_carousel_card(card);
                                } else {
                                    let card = CarouselCard::new(*card_str);
                                    builder = builder.add_carousel_card(card);
                                }
                            }
                            println!("[*] Sending interactive carousel cards to '{}'...", target);
                            match client.send_interactive_message(target, builder).await {
                                Ok(msg_id) => println!(
                                    "[+] Interactive Carousel message sent! (ID: {})",
                                    msg_id
                                ),
                                Err(e) => eprintln!("[-] Send carousel failed: {}", e),
                            }
                        } else {
                            println!("[-] Usage: sendcarousel <JID> <TITLE> | <CARD1_TEXT:CARD1_BTN_TEXT:CARD1_URL>...");
                        }
                    } else {
                        println!("[-] Usage: sendcarousel <JID> <TITLE> | <CARD1_TEXT:CARD1_BTN_TEXT:CARD1_URL>...");
                    }
                } else if trimmed.to_lowercase().starts_with("communitycreate ") {
                    let rest = trimmed[16..].trim();
                    let (name, desc) = if let Some((n, d)) = rest.split_once(' ') {
                        (n, Some(d))
                    } else {
                        (rest, None)
                    };
                    println!("[*] Creating WhatsApp Community '{}'...", name);
                    match client.community_create(name, desc).await {
                        Ok(meta) => {
                            println!("[+] Community successfully created!");
                            println!("    JID: {}", meta.id);
                            println!("    Subject: {}", meta.subject);
                        }
                        Err(e) => eprintln!("[-] Create community failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("communitycreatesub ") {
                    let parts: Vec<&str> = trimmed[18..].split_whitespace().collect();
                    if parts.len() >= 3 {
                        let comm_jid = parts[0];
                        let name = parts[1];
                        let participants: Vec<String> = parts[2]
                            .split(',')
                            .map(|s| {
                                let clean = s.trim();
                                if clean.contains('@') {
                                    clean.to_string()
                                } else {
                                    format!("{}@s.whatsapp.net", clean.trim_start_matches('+'))
                                }
                            })
                            .collect();
                        println!(
                            "[*] Creating subgroup '{}' under community '{}'...",
                            name, comm_jid
                        );
                        match client
                            .community_create_subgroup(comm_jid, name, &participants)
                            .await
                        {
                            Ok(gid) => println!("[+] Subgroup created and linked! (JID: {})", gid),
                            Err(e) => eprintln!("[-] Create subgroup failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: communitycreatesub <COMMUNITY_JID> <NAME> <P1,P2...>");
                    }
                } else if trimmed.to_lowercase().starts_with("communitylink ") {
                    let parts: Vec<&str> = trimmed[14..].split_whitespace().collect();
                    if parts.len() >= 2 {
                        let comm_jid = parts[0];
                        let group_jid = parts[1];
                        println!(
                            "[*] Linking group '{}' into community '{}'...",
                            group_jid, comm_jid
                        );
                        match client.community_link_subgroup(comm_jid, group_jid).await {
                            Ok(_) => println!("[+] Successfully linked group into community!"),
                            Err(e) => eprintln!("[-] Link failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: communitylink <COMMUNITY_JID> <GROUP_JID>");
                    }
                } else if trimmed.to_lowercase().starts_with("communityunlink ") {
                    let parts: Vec<&str> = trimmed[16..].split_whitespace().collect();
                    if parts.len() >= 2 {
                        let comm_jid = parts[0];
                        let group_jid = parts[1];
                        println!(
                            "[*] Unlinking group '{}' from community '{}'...",
                            group_jid, comm_jid
                        );
                        match client.community_unlink_subgroup(comm_jid, group_jid).await {
                            Ok(_) => println!("[+] Successfully unlinked group from community!"),
                            Err(e) => eprintln!("[-] Unlink failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: communityunlink <COMMUNITY_JID> <GROUP_JID>");
                    }
                } else if trimmed.to_lowercase().starts_with("communitysubgroups ") {
                    let comm_jid = trimmed[19..].trim();
                    println!(
                        "[*] Fetching linked subgroups for community '{}'...",
                        comm_jid
                    );
                    match client.community_fetch_subgroups(comm_jid).await {
                        Ok(subgroups) => {
                            println!("[+] Found {} linked subgroup(s):", subgroups.len());
                            for sub in subgroups {
                                let size_str = sub
                                    .size
                                    .map(|s| format!(" ({} members)", s))
                                    .unwrap_or_default();
                                println!("  - [{}] {}{}", sub.id, sub.subject, size_str);
                            }
                        }
                        Err(e) => eprintln!("[-] Fetch subgroups failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("communityinfo ") {
                    let comm_jid = trimmed[14..].trim();
                    println!("[*] Fetching metadata for community '{}'...", comm_jid);
                    match client.community_get_metadata(comm_jid).await {
                        Ok(meta) => {
                            println!("[+] Community Information:");
                            println!("    JID: {}", meta.id);
                            println!("    Subject: {}", meta.subject);
                            if let Some(desc) = meta.description {
                                println!("    Description: {}", desc);
                            }
                            if let Some(owner) = meta.owner {
                                println!("    Owner: {}", owner);
                            }
                            println!("    Total Participants: {}", meta.size);
                            println!("    Is Announce Community: {}", meta.is_community_announce);
                        }
                        Err(e) => eprintln!("[-] Fetch community info failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("communityinvite ") {
                    let comm_jid = trimmed[16..].trim();
                    println!("[*] Fetching invite code for community '{}'...", comm_jid);
                    match client.community_get_invite_code(comm_jid).await {
                        Ok(code) => {
                            println!("[+] Invite Code: {}", code);
                            println!("    Invite Link: https://chat.whatsapp.com/{}", code);
                        }
                        Err(e) => eprintln!("[-] Get invite failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("communityjoin ") {
                    let code = trimmed[14..]
                        .trim()
                        .trim_start_matches("https://chat.whatsapp.com/");
                    println!("[*] Joining community via invite code '{}'...", code);
                    match client.community_accept_invite(code).await {
                        Ok(jid) => println!("[+] Successfully joined community! (JID: {})", jid),
                        Err(e) => eprintln!("[-] Join community failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("communityleave ") {
                    let comm_jid = trimmed[15..].trim();
                    println!("[*] Leaving community '{}'...", comm_jid);
                    match client.community_leave(comm_jid).await {
                        Ok(_) => println!("[+] Successfully left community!"),
                        Err(e) => eprintln!("[-] Leave community failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("callreject ") {
                    let rest = trimmed[11..].trim();
                    let parts: Vec<&str> = rest.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let call_id = parts[0];
                        let call_from = parts[1];
                        println!("[*] Rejecting call '{}' from '{}'...", call_id, call_from);
                        match client.reject_call(call_id, call_from).await {
                            Ok(_) => println!("[+] Successfully rejected call!"),
                            Err(e) => eprintln!("[-] Reject call failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: callreject <CALL_ID> <CALLER_JID>");
                    }
                } else if trimmed.eq_ignore_ascii_case("calls")
                    || trimmed.to_lowercase().starts_with("calls ")
                {
                    let limit = if trimmed.len() > 5 {
                        trimmed[5..].trim().parse::<u32>().unwrap_or(20)
                    } else {
                        20
                    };
                    match client.get_call_history(limit).await {
                        Ok(calls) => {
                            println!("\n=== WhatsApp Call History ({} entries) ===", calls.len());
                            if calls.is_empty() {
                                println!("No calls recorded in database.");
                            } else {
                                for c in calls {
                                    let kind = if c.is_video { "Video" } else { "Voice" };
                                    let grp = if c.is_group { " [Group]" } else { "" };
                                    println!(
                                        "  [t={}] [{}{}] ID: {} | Caller: {} | Status: {}",
                                        c.timestamp, kind, grp, c.call_id, c.caller_jid, c.status
                                    );
                                }
                            }
                            println!("========================================\n");
                        }
                        Err(e) => eprintln!("[-] Failed to fetch call history: {}", e),
                    }
                } else if let Some(raw_suffix) = trimmed.strip_prefix("statuspost ") {
                    let raw = raw_suffix.trim();
                    let parts: Vec<&str> = raw.split('|').map(|s| s.trim()).collect();
                    let text = parts[0];
                    let bg_color = parts.get(1).copied().filter(|s| !s.is_empty());
                    let font = parts.get(2).and_then(|s| s.parse::<i32>().ok());

                    println!("[*] Publishing text story to status@broadcast...");
                    match client.send_text_status(text, bg_color, font, None).await {
                        Ok(msg_id) => {
                            println!("[+] Status posted successfully! Message ID: {}", msg_id)
                        }
                        Err(e) => eprintln!("[-] Failed to post status: {}", e),
                    }
                } else if let Some(raw_suffix) = trimmed.strip_prefix("statuslink ") {
                    let raw = raw_suffix.trim();
                    let parts: Vec<&str> = raw.split('|').map(|s| s.trim()).collect();
                    let url = parts[0];
                    let title = parts.get(1).copied().filter(|s| !s.is_empty());
                    let desc = parts.get(2).copied().filter(|s| !s.is_empty());
                    let custom_text = parts.get(3).copied().filter(|s| !s.is_empty());
                    let bg_color = parts.get(4).copied().filter(|s| !s.is_empty());
                    let font = parts.get(5).and_then(|s| s.parse::<i32>().ok());

                    println!(
                        "[*] Publishing link status story to status@broadcast (URL: {})...",
                        url
                    );
                    match client
                        .send_link_status(url, title, desc, custom_text, bg_color, font, None, None)
                        .await
                    {
                        Ok(msg_id) => println!(
                            "[+] Link status posted successfully! Message ID: {}",
                            msg_id
                        ),
                        Err(e) => eprintln!("[-] Failed to post link status: {}", e),
                    }
                } else if let Some(raw_suffix) = trimmed.strip_prefix("statusaudio ") {
                    let raw = raw_suffix.trim();
                    let parts: Vec<&str> = raw.splitn(2, '|').map(|s| s.trim()).collect();
                    let file_path = parts[0];
                    let seconds = parts
                        .get(1)
                        .and_then(|s| s.parse::<u32>().ok())
                        .unwrap_or(0);

                    let path = std::path::Path::new(file_path);
                    if !path.exists() {
                        eprintln!("[-] File does not exist: {}", file_path);
                    } else {
                        match std::fs::read(path) {
                            Ok(bytes) => {
                                println!("[*] Uploading HD Audio / Voice Note status story ({} bytes, {}s)...", bytes.len(), seconds);
                                match client
                                    .send_status_audio(
                                        &bytes,
                                        seconds,
                                        Some("audio/ogg; codecs=opus"),
                                        None,
                                    )
                                    .await
                                {
                                    Ok(msg_id) => println!(
                                        "[+] Audio status posted successfully! Message ID: {}",
                                        msg_id
                                    ),
                                    Err(e) => eprintln!("[-] Failed to post audio status: {}", e),
                                }
                            }
                            Err(e) => {
                                eprintln!("[-] Failed to read audio file {}: {}", file_path, e)
                            }
                        }
                    }
                } else if let Some(raw_suffix) = trimmed.strip_prefix("statusmedia ") {
                    let raw = raw_suffix.trim();
                    let parts: Vec<&str> = raw.splitn(2, '|').map(|s| s.trim()).collect();
                    let file_path = parts[0];
                    let caption = parts.get(1).copied().filter(|s| !s.is_empty());

                    let path = std::path::Path::new(file_path);
                    if !path.exists() {
                        eprintln!("[-] File does not exist: {}", file_path);
                    } else {
                        match std::fs::read(path) {
                            Ok(bytes) => {
                                let ext = path
                                    .extension()
                                    .and_then(|e| e.to_str())
                                    .unwrap_or("")
                                    .to_lowercase();
                                let is_video = ["mp4", "mkv", "avi", "mov", "3gp", "webm"]
                                    .contains(&ext.as_str());
                                println!(
                                    "[*] Uploading HD {} status story ({} bytes)...",
                                    if is_video { "video" } else { "image" },
                                    bytes.len()
                                );

                                let result = if is_video {
                                    let mime = if ext == "mp4" {
                                        Some("video/mp4")
                                    } else {
                                        None
                                    };
                                    client.send_status_video(&bytes, caption, mime, None).await
                                } else {
                                    let mime = if ext == "png" {
                                        Some("image/png")
                                    } else {
                                        Some("image/jpeg")
                                    };
                                    client.send_status_image(&bytes, caption, mime, None).await
                                };

                                match result {
                                            Ok(msg_id) => println!("[+] HD Status story published successfully! Message ID: {}", msg_id),
                                            Err(e) => eprintln!("[-] Failed to post HD status media: {}", e),
                                        }
                            }
                            Err(e) => eprintln!("[-] Failed to read file {}: {}", file_path, e),
                        }
                    }
                } else if let Some(raw_suffix) = trimmed.strip_prefix("statusview ") {
                    let parts: Vec<&str> = raw_suffix.split_whitespace().collect();
                    if parts.len() < 2 {
                        println!("Usage: statusview <MSG_ID> <AUTHOR_JID>");
                    } else {
                        let msg_id = parts[0];
                        let author_jid = parts[1];
                        match client.send_status_read_receipt(msg_id, author_jid).await {
                            Ok(_) => println!(
                                "[+] Status {} marked as viewed & read receipt sent to {}",
                                msg_id, author_jid
                            ),
                            Err(e) => eprintln!("[-] Failed to send status view receipt: {}", e),
                        }
                    }
                } else if let Some(raw_suffix) = trimmed.strip_prefix("storiesauthor ") {
                    let author_jid = raw_suffix.trim();
                    match client.get_stories_by_author(author_jid).await {
                        Ok(stories) => {
                            println!(
                                "\n=== Stories by {} ({} total) ===",
                                author_jid,
                                stories.len()
                            );
                            if stories.is_empty() {
                                println!("  (No stories found for this author)");
                            } else {
                                for s in stories {
                                    let viewed_mark = if s.viewed {
                                        "[✓ VIEWED]"
                                    } else {
                                        "[• NEW]"
                                    };
                                    let content = s
                                        .content
                                        .as_deref()
                                        .or(s.caption.as_deref())
                                        .unwrap_or("<media>");
                                    println!(
                                        "  {} [{}] ID: {} (t={}) | {}",
                                        viewed_mark, s.message_type, s.msg_id, s.timestamp, content
                                    );
                                }
                            }
                            println!("================================================\n");
                        }
                        Err(e) => eprintln!("[-] Failed to fetch author stories: {}", e),
                    }
                } else if trimmed == "stories" || trimmed.starts_with("stories ") {
                    let arg = trimmed.strip_prefix("stories").unwrap_or("").trim();
                    let limit = arg
                        .split_whitespace()
                        .next()
                        .and_then(|s| s.parse::<u32>().ok())
                        .unwrap_or(20);
                    let unviewed_only = arg.contains("unviewed");

                    match client.get_recent_stories(limit, unviewed_only).await {
                        Ok(stories) => {
                            let filter_label = if unviewed_only { "Unviewed" } else { "Recent" };
                            println!(
                                "\n=== {} Status Stories ({} shown) ===",
                                filter_label,
                                stories.len()
                            );
                            if stories.is_empty() {
                                println!("  (No status stories found)");
                            } else {
                                for s in stories {
                                    let viewed_mark = if s.viewed {
                                        "[✓ VIEWED]"
                                    } else {
                                        "[• NEW]"
                                    };
                                    let content = s
                                        .content
                                        .as_deref()
                                        .or(s.caption.as_deref())
                                        .unwrap_or("<media>");
                                    println!(
                                        "  {} [{}] From: {} (ID: {}) | {}",
                                        viewed_mark,
                                        s.message_type,
                                        s.author_jid,
                                        s.msg_id,
                                        content
                                    );
                                }
                            }
                            println!("==========================================\n");
                        }
                        Err(e) => eprintln!("[-] Failed to fetch recent stories: {}", e),
                    }
                } else if trimmed.eq_ignore_ascii_case("labels") {
                    println!("[*] Querying WhatsApp Business labels from SQLite...");
                    match client.get_labels().await {
                        Ok(labels) => {
                            println!(
                                "\n=== WhatsApp Business Labels ({} found) ===",
                                labels.len()
                            );
                            if labels.is_empty() {
                                println!("  No labels defined yet. Create one with: labeladd <NAME> [| COLOR]");
                            } else {
                                for l in &labels {
                                    let hex = baileys_core::get_label_color_hex(l.color);
                                    println!("  ID: {:<4} | Name: {:<20} | Color: {:2} ({}) | Predefined: {:?}",
                                                l.id, l.name, l.color, hex, l.predefined_id);
                                }
                            }
                            println!("==========================================\n");
                        }
                        Err(e) => eprintln!("[-] Failed to get labels: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("labeladd ") {
                    let parts: Vec<&str> = trimmed[9..].split('|').map(|s| s.trim()).collect();
                    if !parts.is_empty() && !parts[0].is_empty() {
                        let name = parts[0];
                        let color = parts
                            .get(1)
                            .and_then(|c| c.parse::<i32>().ok())
                            .unwrap_or(0);
                        println!("[*] Creating label '{}' (color {})...", name, color);
                        match client.upsert_label(None, name, color, None).await {
                            Ok(id) => {
                                println!("[+] Label created successfully! Assigned ID: {}", id)
                            }
                            Err(e) => eprintln!("[-] Failed to create label: {}", e),
                        }
                    } else {
                        println!("[-] Usage: labeladd <NAME> [| COLOR (0..19)]");
                    }
                } else if trimmed.to_lowercase().starts_with("labeldel ") {
                    let label_id = trimmed[9..].trim();
                    if !label_id.is_empty() {
                        println!("[*] Deleting label ID '{}'...", label_id);
                        match client.delete_label(label_id).await {
                            Ok(_) => println!("[+] Label {} deleted successfully!", label_id),
                            Err(e) => eprintln!("[-] Failed to delete label: {}", e),
                        }
                    } else {
                        println!("[-] Usage: labeldel <LABEL_ID>");
                    }
                } else if trimmed.to_lowercase().starts_with("labelchat ") {
                    let parts: Vec<&str> = trimmed[10..].split_whitespace().collect();
                    if parts.len() >= 2 {
                        let jid = parts[0];
                        let label_id = parts[1];
                        println!("[*] Tagging chat '{}' with label '{}'...", jid, label_id);
                        match client.add_chat_label(jid, label_id).await {
                            Ok(_) => {
                                println!("[+] Chat successfully tagged with label {}!", label_id)
                            }
                            Err(e) => eprintln!("[-] Failed to tag chat: {}", e),
                        }
                    } else {
                        println!("[-] Usage: labelchat <JID> <LABEL_ID>");
                    }
                } else if trimmed.to_lowercase().starts_with("unlabelchat ") {
                    let parts: Vec<&str> = trimmed[12..].split_whitespace().collect();
                    if parts.len() >= 2 {
                        let jid = parts[0];
                        let label_id = parts[1];
                        println!("[*] Removing label '{}' from chat '{}'...", label_id, jid);
                        match client.remove_chat_label(jid, label_id).await {
                            Ok(_) => println!("[+] Label {} removed from chat!", label_id),
                            Err(e) => eprintln!("[-] Failed to remove label from chat: {}", e),
                        }
                    } else {
                        println!("[-] Usage: unlabelchat <JID> <LABEL_ID>");
                    }
                } else if trimmed.to_lowercase().starts_with("labelmsg ") {
                    let parts: Vec<&str> = trimmed[9..].split_whitespace().collect();
                    if parts.len() >= 3 {
                        let jid = parts[0];
                        let msg_id = parts[1];
                        let label_id = parts[2];
                        println!(
                            "[*] Tagging message '{}' in '{}' with label '{}'...",
                            msg_id, jid, label_id
                        );
                        match client.add_message_label(jid, msg_id, label_id).await {
                            Ok(_) => {
                                println!("[+] Message successfully tagged with label {}!", label_id)
                            }
                            Err(e) => eprintln!("[-] Failed to tag message: {}", e),
                        }
                    } else {
                        println!("[-] Usage: labelmsg <JID> <MSG_ID> <LABEL_ID>");
                    }
                } else if trimmed.to_lowercase().starts_with("unlabelmsg ") {
                    let parts: Vec<&str> = trimmed[11..].split_whitespace().collect();
                    if parts.len() >= 3 {
                        let jid = parts[0];
                        let msg_id = parts[1];
                        let label_id = parts[2];
                        println!(
                            "[*] Removing label '{}' from message '{}' in '{}'...",
                            label_id, msg_id, jid
                        );
                        match client.remove_message_label(jid, msg_id, label_id).await {
                            Ok(_) => println!("[+] Label {} removed from message!", label_id),
                            Err(e) => eprintln!("[-] Failed to remove label from message: {}", e),
                        }
                    } else {
                        println!("[-] Usage: unlabelmsg <JID> <MSG_ID> <LABEL_ID>");
                    }
                } else if trimmed.to_lowercase().starts_with("chatlabels ") {
                    let jid = trimmed[11..].trim();
                    if !jid.is_empty() {
                        println!("[*] Querying labels for chat '{}'...", jid);
                        match client.get_chat_labels(jid).await {
                            Ok(labels) => {
                                println!("[+] Labels associated with {}: {:?}", jid, labels)
                            }
                            Err(e) => eprintln!("[-] Failed to get chat labels: {}", e),
                        }
                    } else {
                        println!("[-] Usage: chatlabels <JID>");
                    }
                } else if trimmed.to_lowercase().starts_with("labeledchats ") {
                    let label_id = trimmed[13..].trim();
                    if !label_id.is_empty() {
                        println!("[*] Querying chats tagged with label '{}'...", label_id);
                        match client.get_chats_by_label(label_id).await {
                            Ok(chats) => println!("[+] Chats with label {}: {:?}", label_id, chats),
                            Err(e) => eprintln!("[-] Failed to get labeled chats: {}", e),
                        }
                    } else {
                        println!("[-] Usage: labeledchats <LABEL_ID>");
                    }
                } else if trimmed.to_lowercase().starts_with("sendlocation ") {
                    let raw_args = trimmed[13..].trim();
                    let pipe_parts: Vec<&str> = raw_args.split('|').map(|s| s.trim()).collect();
                    let first_part: Vec<&str> = pipe_parts[0].split_whitespace().collect();
                    if first_part.len() >= 3 {
                        let to_jid = first_part[0];
                        if let (Ok(lat), Ok(lng)) =
                            (first_part[1].parse::<f64>(), first_part[2].parse::<f64>())
                        {
                            let name = pipe_parts.get(1).copied();
                            let addr = pipe_parts.get(2).copied();
                            println!("[*] Sending location ({}, {}) to {}...", lat, lng, to_jid);
                            match client
                                .send_location(to_jid, lat, lng, name, addr, None)
                                .await
                            {
                                Ok(msg_id) => {
                                    println!("[+] Location sent successfully! (ID: {})", msg_id)
                                }
                                Err(e) => eprintln!("[-] Send location failed: {}", e),
                            }
                        } else {
                            println!(
                                "[-] Invalid coordinates. Latitude and longitude must be numbers."
                            );
                        }
                    } else {
                        println!("[-] Usage: sendlocation <JID> <LAT> <LNG> [| NAME | ADDR]");
                    }
                } else if trimmed.to_lowercase().starts_with("sendlivelocation ") {
                    let raw_args = trimmed[17..].trim();
                    let pipe_parts: Vec<&str> = raw_args.split('|').map(|s| s.trim()).collect();
                    let first_part: Vec<&str> = pipe_parts[0].split_whitespace().collect();
                    if first_part.len() >= 3 {
                        let to_jid = first_part[0];
                        if let (Ok(lat), Ok(lng)) =
                            (first_part[1].parse::<f64>(), first_part[2].parse::<f64>())
                        {
                            let caption = pipe_parts.get(1).copied();
                            println!(
                                "[*] Sending live location ({}, {}) to {}...",
                                lat, lng, to_jid
                            );
                            match client
                                .send_live_location(to_jid, lat, lng, Some(20), caption)
                                .await
                            {
                                Ok(msg_id) => println!(
                                    "[+] Live location sent successfully! (ID: {})",
                                    msg_id
                                ),
                                Err(e) => eprintln!("[-] Send live location failed: {}", e),
                            }
                        } else {
                            println!(
                                "[-] Invalid coordinates. Latitude and longitude must be numbers."
                            );
                        }
                    } else {
                        println!("[-] Usage: sendlivelocation <JID> <LAT> <LNG> [| CAPTION]");
                    }
                } else if trimmed.to_lowercase().starts_with("sendcontact ") {
                    let parts: Vec<&str> = trimmed[12..].split_whitespace().collect();
                    if parts.len() >= 3 {
                        let to_jid = parts[0];
                        let name = parts[1];
                        let phone = parts[2];
                        println!(
                            "[*] Sending contact '{}' ({}) to {}...",
                            name, phone, to_jid
                        );
                        match client.send_contact(to_jid, name, phone).await {
                            Ok(msg_id) => println!("[+] Contact card sent! (ID: {})", msg_id),
                            Err(e) => eprintln!("[-] Send contact failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: sendcontact <JID> <NAME> <PHONE>");
                    }
                } else if trimmed.to_lowercase().starts_with("sendgroupinvite ") {
                    let raw_args = trimmed[16..].trim();
                    let pipe_parts: Vec<&str> = raw_args.split('|').map(|s| s.trim()).collect();
                    let first_part: Vec<&str> = pipe_parts[0].split_whitespace().collect();
                    if first_part.len() >= 5 {
                        let to_jid = first_part[0];
                        let group_jid = first_part[1];
                        let code = first_part[2];
                        let exp = first_part[3].parse::<i64>().unwrap_or(0);
                        let name = first_part[4];
                        let caption = pipe_parts.get(1).copied();
                        println!("[*] Sending group invite for '{}' to {}...", name, to_jid);
                        match client
                            .send_group_invite(to_jid, group_jid, code, exp, name, caption)
                            .await
                        {
                            Ok(msg_id) => println!("[+] Group invite sent! (ID: {})", msg_id),
                            Err(e) => eprintln!("[-] Send group invite failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: sendgroupinvite <JID> <GROUP_JID> <CODE> <EXP_SEC> <NAME> [| CAPTION]");
                    }
                } else if trimmed.to_lowercase().starts_with("sendevent ") {
                    let raw_args = trimmed[10..].trim();
                    let pipe_parts: Vec<&str> = raw_args.split('|').map(|s| s.trim()).collect();
                    if let Some((to_jid, name)) = pipe_parts[0].split_once(' ') {
                        let norm_target = resolve_target_jid(client, to_jid)
                            .await
                            .unwrap_or_else(|| to_jid.to_string());
                        let desc = pipe_parts.get(1).copied();
                        let loc = pipe_parts.get(2).copied();
                        let start = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64
                            + 3600;
                        println!(
                            "[*] Sending calendar event '{}' to {}...",
                            name, norm_target
                        );
                        match client
                            .send_event(&norm_target, name, desc, start, Some(start + 3600), loc)
                            .await
                        {
                            Ok(msg_id) => println!("[+] Calendar event sent! (ID: {})", msg_id),
                            Err(e) => eprintln!("[-] Send event failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: sendevent <JID> <TITLE> [| DESC | LOCATION]");
                    }
                } else if trimmed.to_lowercase().starts_with("sendjson ") {
                    let rest = trimmed[9..].trim();
                    if let Some((to_jid, raw_json)) = rest.split_once(' ') {
                        let norm_target = resolve_target_jid(client, to_jid)
                            .await
                            .unwrap_or_else(|| to_jid.to_string());
                        match serde_json::from_str::<serde_json::Value>(raw_json.trim()) {
                            Ok(val) => {
                                println!("[*] Dispatching JSON payload to {}...", norm_target);
                                match client.send_message_json(&norm_target, &val).await {
                                    Ok(msg_id) => {
                                        println!("[+] JSON message sent! (ID: {})", msg_id)
                                    }
                                    Err(e) => eprintln!("[-] Send JSON failed: {}", e),
                                }
                            }
                            Err(e) => eprintln!("[-] Invalid JSON: {}", e),
                        }
                    } else {
                        println!("[-] Usage: sendjson <JID> <JSON>");
                    }
                } else if trimmed.eq_ignore_ascii_case("ghostmode")
                    || trimmed.to_lowercase().starts_with("ghostmode ")
                {
                    let arg = trimmed
                        .strip_prefix("ghostmode")
                        .map(|s| s.trim())
                        .unwrap_or("");
                    if arg.eq_ignore_ascii_case("on")
                        || arg == "1"
                        || arg.eq_ignore_ascii_case("true")
                    {
                        client.set_ghost_mode(true);
                        println!(
                            "[+] Ghost Mode ENABLED (Read receipts & status receipts suppressed)"
                        );
                    } else if arg.eq_ignore_ascii_case("off")
                        || arg == "0"
                        || arg.eq_ignore_ascii_case("false")
                    {
                        client.set_ghost_mode(false);
                        println!("[-] Ghost Mode DISABLED (Normal read receipts)");
                    } else {
                        let cur = client.is_ghost_mode();
                        println!(
                            "[*] Ghost Mode is currently: {}",
                            if cur {
                                "ENABLED (Invisible)"
                            } else {
                                "DISABLED (Normal)"
                            }
                        );
                        println!("    Usage: ghostmode <on|off>");
                    }
                } else if trimmed.to_lowercase().starts_with("fakevn ") {
                    let rest = trimmed[7..].trim();
                    let parts: Vec<&str> = rest.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let to_jid = parts[0];
                        let audio_path = parts[1];
                        let seconds = parts.get(2).and_then(|s| s.parse::<u32>().ok());
                        let path = std::path::Path::new(audio_path);
                        if !path.exists() {
                            eprintln!("[-] Audio file not found: {}", audio_path);
                        } else {
                            match std::fs::read(path) {
                                Ok(bytes) => {
                                    let norm_target = resolve_target_jid(client, to_jid)
                                        .await
                                        .unwrap_or_else(|| to_jid.to_string());
                                    println!(
                                        "[*] Sending native fake voice note ({} bytes) to {}...",
                                        bytes.len(),
                                        norm_target
                                    );
                                    match client
                                        .send_voice_note(
                                            &norm_target,
                                            &bytes,
                                            seconds.unwrap_or(2),
                                            None,
                                        )
                                        .await
                                    {
                                        Ok(id) => {
                                            println!("[+] Voice note sent successfully! ID: {}", id)
                                        }
                                        Err(e) => eprintln!("[-] Failed to send voice note: {}", e),
                                    }
                                }
                                Err(e) => eprintln!("[-] Failed to read audio: {}", e),
                            }
                        }
                    } else {
                        println!("[-] Usage: fakevn <JID> <AUDIO_FILE> [DURATION_SECS]");
                    }
                } else if trimmed.to_lowercase().starts_with("fakequote ") {
                    let rest = trimmed[10..].trim();
                    let pipe_parts: Vec<&str> = rest.split('|').map(|s| s.trim()).collect();
                    if pipe_parts.len() >= 3 {
                        if let Some((to_jid, reply_text)) = pipe_parts[0].split_once(' ') {
                            let faker_jid = pipe_parts[1];
                            let faker_text = pipe_parts[2];
                            let norm_target = resolve_target_jid(client, to_jid)
                                .await
                                .unwrap_or_else(|| to_jid.to_string());
                            let norm_faker = resolve_target_jid(client, faker_jid)
                                .await
                                .unwrap_or_else(|| faker_jid.to_string());
                            println!(
                                "[*] Sending fake quote to {} quoting {}: \"{}\"...",
                                norm_target, norm_faker, faker_text
                            );
                            match client
                                .send_fake_quoted_message(
                                    &norm_target,
                                    reply_text,
                                    &norm_faker,
                                    faker_text,
                                )
                                .await
                            {
                                Ok(id) => println!("[+] Fake quoted message sent! ID: {}", id),
                                Err(e) => eprintln!("[-] Failed to send fake quote: {}", e),
                            }
                        } else {
                            println!("[-] Usage: fakequote <JID> <REPLY_TEXT> | <FAKER_JID> | <QUOTED_TEXT>");
                        }
                    } else {
                        println!(
                            "[-] Usage: fakequote <JID> <REPLY_TEXT> | <FAKER_JID> | <QUOTED_TEXT>"
                        );
                    }
                } else if trimmed.to_lowercase().starts_with("catalog ") {
                    let parts: Vec<&str> = trimmed[8..].split_whitespace().collect();
                    if !parts.is_empty() {
                        let biz_jid = parts[0];
                        let limit = parts
                            .get(1)
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(10);
                        println!("[*] Fetching catalog for {} (limit: {})...", biz_jid, limit);
                        match client.get_catalog(biz_jid, limit, None).await {
                            Ok(res) => {
                                println!(
                                    "[+] Catalog fetched: {} products returned (cursor: {:?}):",
                                    res.products.len(),
                                    res.cursor
                                );
                                for p in &res.products {
                                    let price_display = if p.price_amount_1000 > 0 {
                                        format!(
                                            "{:.2} {}",
                                            p.price_amount_1000 as f64 / 1000.0,
                                            p.currency
                                        )
                                    } else {
                                        "Free / Contact".to_string()
                                    };
                                    println!(
                                        "  • [{}] {} - {} (Images: {})",
                                        p.id,
                                        p.title,
                                        price_display,
                                        p.image_urls.len()
                                    );
                                    if let Some(ref desc) = p.description {
                                        println!("    Desc: {}", desc);
                                    }
                                    if let Some(ref url) = p.url {
                                        println!("    URL: {}", url);
                                    }
                                }
                            }
                            Err(e) => eprintln!("[-] Fetch catalog failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: catalog <BIZ_JID> [LIMIT]");
                    }
                } else if trimmed.to_lowercase().starts_with("product ") {
                    let parts: Vec<&str> = trimmed[8..].split_whitespace().collect();
                    if parts.len() >= 2 {
                        let biz_jid = parts[0];
                        let product_id = parts[1];
                        println!("[*] Fetching product '{}' from {}...", product_id, biz_jid);
                        match client.get_product(biz_jid, product_id).await {
                            Ok(p) => {
                                println!("[+] Product Details:");
                                println!("  ID:          {}", p.id);
                                println!("  Title:       {}", p.title);
                                println!(
                                    "  Price:       {:.2} {}",
                                    p.price_amount_1000 as f64 / 1000.0,
                                    p.currency
                                );
                                println!(
                                    "  Description: {}",
                                    p.description.as_deref().unwrap_or("None")
                                );
                                if let Some(ref retailer_id) = p.retailer_id {
                                    println!("  Retailer ID: {}", retailer_id);
                                }
                                if let Some(ref url) = p.url {
                                    println!("  URL:         {}", url);
                                }
                                println!("  Images ({}):", p.image_urls.len());
                                for u in &p.image_urls {
                                    println!("    - {}", u);
                                }
                            }
                            Err(e) => eprintln!("[-] Fetch product failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: product <BIZ_JID> <PRODUCT_ID>");
                    }
                } else if trimmed.to_lowercase().starts_with("collections ") {
                    let parts: Vec<&str> = trimmed[12..].split_whitespace().collect();
                    if !parts.is_empty() {
                        let biz_jid = parts[0];
                        let limit = parts
                            .get(1)
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(10);
                        println!(
                            "[*] Fetching collections for {} (limit: {})...",
                            biz_jid, limit
                        );
                        match client.get_collections(biz_jid, limit, None).await {
                            Ok(res) => {
                                println!("[+] Collections fetched: {} collections returned (cursor: {:?}):", res.collections.len(), res.cursor);
                                for c in &res.collections {
                                    println!(
                                        "  • [{}] {} (Status: {}, Products: {})",
                                        c.id, c.name, c.status, c.products_count
                                    );
                                }
                            }
                            Err(e) => eprintln!("[-] Fetch collections failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: collections <BIZ_JID> [LIMIT]");
                    }
                } else if trimmed.to_lowercase().starts_with("sendproduct ") {
                    let raw_args = trimmed[12..].trim();
                    let pipe_parts: Vec<&str> = raw_args.split('|').map(|s| s.trim()).collect();
                    let first_part: Vec<&str> = pipe_parts[0].split_whitespace().collect();
                    if first_part.len() >= 6 {
                        let to_jid = first_part[0];
                        let biz_jid = first_part[1];
                        let pid = first_part[2];
                        let title = first_part[3];
                        let price_1000 = first_part[4].parse::<i64>().unwrap_or(0);
                        let currency = first_part[5];
                        let caption = pipe_parts.get(1).copied();

                        let product = baileys_core::CatalogProduct {
                            id: pid.to_string(),
                            biz_jid: biz_jid.to_string(),
                            title: title.to_string(),
                            description: caption.map(|s| s.to_string()),
                            currency: currency.to_string(),
                            price_amount_1000: price_1000,
                            sale_price_amount_1000: None,
                            retailer_id: None,
                            url: None,
                            image_urls: Vec::new(),
                            is_hidden: false,
                            availability: "in_stock".to_string(),
                        };

                        println!("[*] Sending product card '{}' to {}...", title, to_jid);
                        match client
                            .send_product_message(to_jid, biz_jid, &product, caption, None)
                            .await
                        {
                            Ok(msg_id) => println!("[+] Product card sent! (ID: {})", msg_id),
                            Err(e) => eprintln!("[-] Send product card failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: sendproduct <TO_JID> <BIZ_JID> <PID> <TITLE> <PRICE_1000> <CURRENCY> [| CAPTION]");
                    }
                } else if trimmed.to_lowercase().starts_with("sendorder ") {
                    let raw_args = trimmed[10..].trim();
                    let pipe_parts: Vec<&str> = raw_args.split('|').map(|s| s.trim()).collect();
                    let first_part: Vec<&str> = pipe_parts[0].split_whitespace().collect();
                    if first_part.len() >= 7 {
                        let to_jid = first_part[0];
                        let seller_jid = first_part[1];
                        let order_id = first_part[2];
                        let title = first_part[3];
                        let count = first_part[4].parse::<i32>().unwrap_or(1);
                        let total = first_part[5].parse::<i64>().unwrap_or(0);
                        let currency = first_part[6];
                        let message = pipe_parts.get(1).copied();

                        println!("[*] Sending order card '{}' to {}...", title, to_jid);
                        match client
                            .send_order_message(
                                to_jid, seller_jid, order_id, title, count, total, currency,
                                message, None,
                            )
                            .await
                        {
                            Ok(msg_id) => println!("[+] Order card sent! (ID: {})", msg_id),
                            Err(e) => eprintln!("[-] Send order card failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: sendorder <TO_JID> <SELLER_JID> <ORDER_ID> <TITLE> <ITEMS> <TOTAL_1000> <CURRENCY> [| MSG]");
                    }
                } else if trimmed.to_lowercase().starts_with("orders") {
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    let chat_jid = parts.get(1).copied();
                    let limit = parts
                        .get(2)
                        .and_then(|s| s.parse::<u32>().ok())
                        .unwrap_or(20);

                    let chat_target = chat_jid.unwrap_or("");
                    println!(
                        "[*] Querying cached commerce orders (filter: '{}', limit: {})...",
                        chat_target, limit
                    );
                    match client.get_cached_orders(chat_target, limit).await {
                        Ok(orders) => {
                            println!("[+] Found {} stored order(s):", orders.len());
                            for ord in &orders {
                                let title = ord.order_title.as_deref().unwrap_or("Order");
                                let total = format!(
                                    "{:.2} {}",
                                    ord.total_amount_1000 as f64 / 1000.0,
                                    ord.total_currency_code
                                );
                                println!(
                                    "  • [{}] {} - {} items, {} (Seller: {}, Status: {}, Chat: {})",
                                    ord.order_id,
                                    title,
                                    ord.item_count,
                                    total,
                                    ord.seller_jid,
                                    ord.status,
                                    ord.chat_jid
                                );
                                if let Some(ref m) = ord.message {
                                    println!("    Message: {}", m);
                                }
                            }
                        }
                        Err(e) => eprintln!("[-] Query cached orders failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("cachedcatalog ") {
                    let biz_jid = trimmed[14..].trim();
                    if !biz_jid.is_empty() {
                        match client.get_cached_catalog(biz_jid).await {
                            Ok(products) => {
                                println!(
                                    "[+] Cached Catalog for {} ({} items):",
                                    biz_jid,
                                    products.len()
                                );
                                for p in &products {
                                    let total = format!(
                                        "{:.2} {}",
                                        p.price_amount_1000 as f64 / 1000.0,
                                        p.currency
                                    );
                                    println!("  • [{}] {} - {}", p.product_id, p.title, total);
                                    if let Some(ref d) = p.description {
                                        println!("    Desc: {}", d);
                                    }
                                }
                            }
                            Err(e) => eprintln!("[-] Query cached catalog failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: cachedcatalog <BIZ_JID>");
                    }
                } else if trimmed.to_lowercase().starts_with("cachedcollections ") {
                    let biz_jid = trimmed[18..].trim();
                    if !biz_jid.is_empty() {
                        match client.get_cached_collections(biz_jid).await {
                            Ok(cols) => {
                                println!(
                                    "[+] Cached Collections for {} ({} items):",
                                    biz_jid,
                                    cols.len()
                                );
                                for c in &cols {
                                    println!(
                                        "  • [{}] {} (Status: {})",
                                        c.collection_id, c.name, c.status
                                    );
                                }
                            }
                            Err(e) => eprintln!("[-] Query cached collections failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: cachedcollections <BIZ_JID>");
                    }
                } else if trimmed.to_lowercase().starts_with("calllink") {
                    let arg = trimmed
                        .strip_prefix("calllink")
                        .unwrap_or("")
                        .trim()
                        .to_lowercase();
                    let is_video = arg == "video";
                    println!(
                        "[*] Generating WhatsApp call link (mode: {})...",
                        if is_video { "video" } else { "audio" }
                    );
                    match client.create_call_link(is_video).await {
                        Ok(Some(token)) => {
                            println!("[+] Call link generated successfully!");
                            println!("    Token: {}", token);
                            println!(
                                "    URL:   https://call.whatsapp.com/{}/{}",
                                if is_video { "video" } else { "voice" },
                                token
                            );
                        }
                        Ok(None) => println!("[-] Call link creation did not return a token."),
                        Err(e) => eprintln!("[-] Create call link failed: {}", e),
                    }
                } else if trimmed.eq_ignore_ascii_case("reachoutstatus") {
                    println!(
                        "[*] Querying account reachout timelock / spam enforcement standing..."
                    );
                    match client.fetch_account_reachout_timelock().await {
                        Ok(Some((active, reason, ends))) => {
                            println!(
                                "[+] Standing: active={}, reason={:?}, ends={:?}",
                                active, reason, ends
                            );
                        }
                        Ok(None) => println!(
                            "[+] Account has no active reachout timelocks or restrictions."
                        ),
                        Err(e) => eprintln!("[-] Reachout timelock query failed: {}", e),
                    }
                } else if trimmed.eq_ignore_ascii_case("newchatcap") {
                    println!("[*] Querying new chat message cap info quota...");
                    match client.fetch_new_chat_message_cap().await {
                        Ok(Some(val)) => println!("[+] Message capping info: {}", val),
                        Ok(None) => println!("[+] No message capping info returned."),
                        Err(e) => eprintln!("[-] Message capping query failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("cleandirty") {
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let dirty_type = parts[1];
                        println!("[*] Cleaning dirty bits for '{}'...", dirty_type);
                        match client.clean_dirty_bits(dirty_type, None).await {
                            Ok(()) => println!("[+] Clean dirty bits successfully submitted."),
                            Err(e) => eprintln!("[-] Clean dirty bits failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: cleandirty <account_sync|groups>");
                    }
                } else if trimmed.to_lowercase().starts_with("appstatesync")
                    || trimmed.to_lowercase().starts_with("syncd")
                {
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    let mode = if parts.len() >= 2 {
                        parts[1].to_lowercase()
                    } else {
                        "all".to_string()
                    };
                    let collections: &[&str] = match mode.as_str() {
                        "critical" => &[
                            baileys_core::CRITICAL_BLOCK,
                            baileys_core::CRITICAL_UNBLOCK_LOW,
                        ],
                        "regular" => &[
                            baileys_core::REGULAR_HIGH,
                            baileys_core::REGULAR_LOW,
                            baileys_core::REGULAR,
                        ],
                        _ => baileys_core::ALL_COLLECTIONS,
                    };
                    println!(
                        "[*] Initiating AppState resync for collections: {:?}...",
                        collections
                    );
                    match client.resync_app_state(collections, false).await {
                        Ok(summary) => {
                            println!(
                                        "[+] AppState sync complete: {} collections synced, {} mutations applied to SQLite store.",
                                        summary.collections_synced, summary.mutations_applied
                                    );
                        }
                        Err(e) => eprintln!("[-] AppState sync failed: {}", e),
                    }
                } else if trimmed.eq_ignore_ascii_case("botlist") {
                    println!("[*] Fetching WhatsApp AI Bot list v2...");
                    match client.get_bot_list_v2().await {
                        Ok(bots) => {
                            println!("[+] Received {} AI Bots:", bots.len());
                            for (jid, persona) in bots {
                                println!("  • JID: {} (Persona ID: {})", jid, persona);
                            }
                        }
                        Err(e) => eprintln!("[-] Query bot list failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("channeladmincount ") {
                    let nl_jid = trimmed[18..].trim();
                    if !nl_jid.is_empty() {
                        println!("[*] Querying admin count for channel '{}'...", nl_jid);
                        match client.newsletter_admin_count(nl_jid).await {
                            Ok(count) => println!("[+] Channel admin count: {}", count),
                            Err(e) => eprintln!("[-] Query admin count failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: channeladmincount <CHANNEL_JID>");
                    }
                } else if trimmed.to_lowercase().starts_with("channelsublive ") {
                    let nl_jid = trimmed[15..].trim();
                    if !nl_jid.is_empty() {
                        println!(
                            "[*] Subscribing to live updates for channel '{}'...",
                            nl_jid
                        );
                        match client.subscribe_newsletter_updates(nl_jid).await {
                            Ok(dur) => println!("[+] Subscribed! Server duration: {:?}", dur),
                            Err(e) => {
                                eprintln!("[-] Channel live updates subscription failed: {}", e)
                            }
                        }
                    } else {
                        println!("[-] Usage: channelsublive <CHANNEL_JID>");
                    }
                } else if trimmed.to_lowercase().starts_with("communityrevokeinvite ") {
                    let comm_jid = trimmed[22..].trim();
                    if !comm_jid.is_empty() {
                        println!("[*] Revoking invite link for Community '{}'...", comm_jid);
                        match client.community_revoke_invite(comm_jid).await {
                            Ok(code) => println!("[+] Revoked! New invite code: {}", code),
                            Err(e) => eprintln!("[-] Revoke invite failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: communityrevokeinvite <COMMUNITY_JID>");
                    }
                } else if trimmed.to_lowercase().starts_with("communityinviteinfo ") {
                    let code = trimmed[20..].trim();
                    if !code.is_empty() {
                        println!("[*] Querying community invite info for code '{}'...", code);
                        match client.community_get_invite_info(code).await {
                            Ok(info) => {
                                println!("[+] Community Invite Info:");
                                println!("    JID:     {}", info.jid);
                                println!("    Subject: {}", info.subject);
                                println!("    Creator: {:?}", info.creator);
                                println!("    Size:    {:?}", info.size);
                            }
                            Err(e) => eprintln!("[-] Query community invite info failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: communityinviteinfo <CODE>");
                    }
                } else if trimmed.to_lowercase().starts_with("communityephemeral ") {
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    if parts.len() >= 3 {
                        let comm_jid = parts[1];
                        let sec: u32 = parts[2].parse().unwrap_or(0);
                        println!(
                            "[*] Setting ephemeral duration to {}s on community '{}'...",
                            sec, comm_jid
                        );
                        match client.community_toggle_ephemeral(comm_jid, sec).await {
                            Ok(()) => println!("[+] Community ephemeral timer updated."),
                            Err(e) => eprintln!("[-] Community ephemeral update failed: {}", e),
                        }
                    } else {
                        println!(
                            "[-] Usage: communityephemeral <COMMUNITY_JID> <DURATION_SECONDS>"
                        );
                    }
                } else if trimmed.to_lowercase().starts_with("communitysetting ") {
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    if parts.len() >= 3 {
                        let comm_jid = parts[1];
                        let setting = parts[2];
                        println!(
                            "[*] Updating community setting '{}' on '{}'...",
                            setting, comm_jid
                        );
                        match client.community_setting_update(comm_jid, setting).await {
                            Ok(()) => println!("[+] Community setting successfully updated."),
                            Err(e) => eprintln!("[-] Community setting update failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: communitysetting <COMMUNITY_JID> <announcement|locked|not_announcement|unlocked>");
                    }
                } else if trimmed
                    .to_lowercase()
                    .starts_with("communitymemberaddmode ")
                {
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    if parts.len() >= 3 {
                        let comm_jid = parts[1];
                        let mode = parts[2];
                        println!(
                            "[*] Setting member add mode '{}' on community '{}'...",
                            mode, comm_jid
                        );
                        match client.community_member_add_mode(comm_jid, mode).await {
                            Ok(()) => println!("[+] Community member add mode updated."),
                            Err(e) => eprintln!("[-] Update member add mode failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: communitymemberaddmode <COMMUNITY_JID> <admin_add|all_member_add>");
                    }
                } else if trimmed.to_lowercase().starts_with("communityjoinapproval ") {
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    if parts.len() >= 3 {
                        let comm_jid = parts[1];
                        let mode = parts[2];
                        println!(
                            "[*] Setting join approval mode '{}' on community '{}'...",
                            mode, comm_jid
                        );
                        match client.community_join_approval_mode(comm_jid, mode).await {
                            Ok(()) => println!("[+] Community join approval mode updated."),
                            Err(e) => eprintln!("[-] Update join approval mode failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: communityjoinapproval <COMMUNITY_JID> <on|off>");
                    }
                } else if trimmed.eq_ignore_ascii_case("communityparticipating") {
                    println!("[*] Fetching all participating communities...");
                    match client.community_fetch_all_participating().await {
                        Ok(jids) => {
                            println!("[+] Participating in {} communities:", jids.len());
                            for j in jids {
                                println!("  • {}", j);
                            }
                        }
                        Err(e) => eprintln!("[-] Fetch participating communities failed: {}", e),
                    }
                } else if trimmed.to_lowercase().starts_with("productdel ") {
                    let rest = trimmed[11..].trim();
                    let ids: Vec<&str> = rest
                        .split(',')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .collect();
                    if !ids.is_empty() {
                        println!("[*] Deleting products {:?} from catalog...", ids);
                        match client.product_delete(&ids).await {
                            Ok(()) => println!("[+] Products successfully deleted."),
                            Err(e) => eprintln!("[-] Product deletion failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: productdel <ID1,ID2...>");
                    }
                } else if trimmed.to_lowercase().starts_with("usynclid ") {
                    let rest = trimmed[9..].trim();
                    let jids: Vec<&str> = rest
                        .split(',')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .collect();
                    if !jids.is_empty() {
                        println!("[*] Resolving LIDs for PN JIDs {:?} via USync...", jids);
                        match client.get_lid_for_pn(&jids).await {
                            Ok(pairs) => {
                                println!("[+] USync LID mappings:");
                                for (pn, lid) in pairs {
                                    println!("  • {} -> {}", pn, lid);
                                }
                            }
                            Err(e) => eprintln!("[-] USync LID query failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: usynclid <PN_JID1,PN_JID2...>");
                    }
                } else if trimmed.to_lowercase().starts_with("usyncpn ") {
                    let rest = trimmed[8..].trim();
                    let jids: Vec<&str> = rest
                        .split(',')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .collect();
                    if !jids.is_empty() {
                        println!("[*] Resolving PNs for LID JIDs {:?} via USync...", jids);
                        match client.get_pn_for_lid(&jids).await {
                            Ok(pairs) => {
                                println!("[+] USync PN mappings:");
                                for (lid, pn) in pairs {
                                    println!("  • {} -> {}", lid, pn);
                                }
                            }
                            Err(e) => eprintln!("[-] USync PN query failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: usyncpn <LID_JID1,LID_JID2...>");
                    }
                } else if trimmed.to_lowercase().starts_with("usyncstatus ") {
                    let rest = trimmed[12..].trim();
                    let jids: Vec<&str> = rest
                        .split(',')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .collect();
                    if !jids.is_empty() {
                        println!("[*] Fetching contact statuses for {:?} via USync...", jids);
                        match client.get_user_status(&jids).await {
                            Ok(statuses) => {
                                println!("[+] Contact Text Statuses:");
                                for (jid, text, ts) in statuses {
                                    println!("  • {}: \"{}\" (Updated at: {})", jid, text, ts);
                                }
                            }
                            Err(e) => eprintln!("[-] USync status query failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: usyncstatus <JID1,JID2...>");
                    }
                } else if trimmed.to_lowercase().starts_with("mediaretry ") {
                    let msg_id = trimmed[11..].trim();
                    if !msg_id.is_empty() {
                        println!(
                            "[*] Requesting media re-download URL for message {}...",
                            msg_id
                        );
                        match client.update_media_message(msg_id).await {
                                    Ok(()) => println!("[+] Media retry request sent successfully! Awaiting server response..."),
                                    Err(e) => eprintln!("[-] Media retry request failed: {}", e),
                                }
                    } else {
                        println!("[-] Usage: mediaretry <MSG_ID>");
                    }
                } else if trimmed.to_lowercase().starts_with("urlpreview ") {
                    let text = trimmed[11..].trim();
                    if !text.is_empty() {
                        println!("[*] Extracting OpenGraph preview for text: \"{}\"...", text);
                        match client.fetch_url_preview(text).await {
                            Ok(Some(preview)) => {
                                println!("[+] Link Preview Found:");
                                println!("  • Matched URL : {}", preview.canonical_url);
                                if let Some(ref title) = preview.title {
                                    println!("  • Title       : {}", title);
                                }
                                if let Some(ref desc) = preview.description {
                                    println!("  • Description : {}", desc);
                                }
                                if let Some(ref thumb) = preview.jpeg_thumbnail {
                                    println!("  • Thumbnail   : {} bytes (JPEG)", thumb.len());
                                }
                            }
                            Ok(None) => println!("[-] No link or OpenGraph preview found in text."),
                            Err(e) => eprintln!("[-] Link preview failed: {}", e),
                        }
                    } else {
                        println!("[-] Usage: urlpreview <TEXT>");
                    }
                } else if trimmed.to_lowercase().starts_with("sendpreview ") {
                    let parts: Vec<&str> = trimmed[12..].trim().splitn(2, ' ').collect();
                    if parts.len() == 2 {
                        let to_jid = parts[0].trim();
                        let text = parts[1].trim();
                        let norm_target = match resolve_target_jid(client, to_jid).await {
                            Some(t) => t,
                            None => {
                                eprintln!("[-] Could not resolve target '{}'.", to_jid);
                                continue;
                            }
                        };
                        println!(
                            "[*] Sending message with rich link preview to {}...",
                            norm_target
                        );
                        match client.send_text_with_preview(&norm_target, text).await {
                            Ok(id) => println!("[+] Sent message with link preview (ID: {})", id),
                            Err(e) => eprintln!("[-] Failed to send message with preview: {}", e),
                        }
                    } else {
                        println!("[-] Usage: sendpreview <JID|me> <TEXT>");
                    }
                } else if trimmed.eq_ignore_ascii_case("proto")
                    || trimmed.to_lowercase().starts_with("proto ")
                {
                    let sub = if trimmed.len() >= 5 {
                        trimmed[5..].trim()
                    } else {
                        ""
                    };
                    handle_proto_command(sub, store).await;
                } else if trimmed.eq_ignore_ascii_case("cmdmode")
                    || trimmed.to_lowercase().starts_with("cmdmode ")
                    || trimmed.eq_ignore_ascii_case("cmdstatus")
                {
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    if parts.len() < 2 || parts[0].eq_ignore_ascii_case("cmdstatus") {
                        let cur = crate::in_chat_bot::get_mode();
                        println!("\n=== In-Chat Command Runner Status ===");
                        println!("  Active Mode : {}", cur.display_badge());
                        println!("  Description : {}", match cur {
                                    crate::in_chat_bot::InChatMode::Off => "Completely disabled. In-chat commands are ignored.",
                                    crate::in_chat_bot::InChatMode::SelfOnly => "Responds strictly to self-chat notes & messages from owner (from_me).",
                                    crate::in_chat_bot::InChatMode::Public => "Responds to public contacts with tiered security permissions.",
                                });
                        println!("  Prefixes    : .  !  /");
                        println!("  Usage       : cmdmode [off | self | public]\n");
                    } else {
                        match crate::in_chat_bot::InChatMode::from_str_loose(parts[1]) {
                            Some(new_mode) => {
                                match crate::in_chat_bot::set_mode_and_persist(store, new_mode)
                                    .await
                                {
                                    Ok(()) => println!(
                                        "[+] In-Chat Bot Mode updated to: {}",
                                        new_mode.display_badge()
                                    ),
                                    Err(e) => eprintln!("[-] Failed to update bot mode: {}", e),
                                }
                            }
                            None => {
                                eprintln!(
                                    "[-] Invalid mode '{}'. Valid choices: off, self, public",
                                    parts[1]
                                );
                            }
                        }
                    }
                } else if trimmed.eq_ignore_ascii_case("help")
                    || trimmed.to_lowercase().starts_with("help ")
                {
                    let sub = trimmed
                        .strip_prefix("help")
                        .or_else(|| trimmed.strip_prefix("HELP"))
                        .unwrap_or("")
                        .trim();
                    let query = if sub.is_empty() { None } else { Some(sub) };
                    crate::commands::print_help(query);
                } else if trimmed.eq_ignore_ascii_case("proto")
                    || trimmed.to_lowercase().starts_with("proto ")
                {
                    let sub = trimmed
                        .strip_prefix("proto")
                        .or_else(|| trimmed.strip_prefix("PROTO"))
                        .unwrap_or("")
                        .trim();
                    handle_proto_command(sub, store).await;
                } else {
                    let (cmd_name, _cmd_args) = match trimmed.split_once(' ') {
                        Some((cmd, rest)) => (cmd.trim().to_lowercase(), rest.trim()),
                        None => (trimmed.trim().to_lowercase(), ""),
                    };

                    if let Some(spec) = crate::commands::find_command(&cmd_name) {
                        let missing_part = spec.missing_hint;
                        println!(
                            "{}",
                            crate::commands::format_missing_args(spec, missing_part)
                        );
                    } else if let Some(suggestion) = crate::commands::suggest_command(&cmd_name) {
                        println!("[-] Unknown command: '{}'. Did you mean '{}'? Type 'help' for options.", trimmed, suggestion);
                    } else {
                        println!(
                            "[-] Unknown command: '{}'. Type 'help' for options.",
                            trimmed
                        );
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("\n[*] Ctrl+C received, shutting down Rose-Baileys engine...");
                break;
            }
            Err(ReadlineError::Eof) => {
                info!("Stdin closed / EOF reached. Exiting REPL loop.");
                break;
            }
            Err(e) => {
                warn!("Error reading from line editor: {}", e);
                break;
            }
        }
    }

    if let Ok(mut lock) = rl_arc.lock() {
        let _ = lock.save_history(history_file);
    }

    Ok(())
}

async fn handle_proto_command(sub: &str, store: &Arc<SqliteStore>) {
    let parts: Vec<&str> = sub.split_whitespace().collect();
    if parts.is_empty() {
        print_proto_help();
        return;
    }

    match parts[0].to_lowercase().as_str() {
        "list" => {
            println!("\n=== Recognized High-Level Message Types (Rose-Baileys Intelligence) ===");
            let canonical = RecognizedMessageType::all_canonical();
            for (i, t) in canonical.iter().enumerate() {
                println!("  [{:>2}] {:<24} - {}", i + 1, t.as_str(), t.display_name());
            }

            let stats = SchemaInspector::global().stats();
            println!("\n=== WAProto.proto Schema In-Memory Index ===");
            println!("  Total Messages : {}", stats.total_messages);
            println!("  Total Enums    : {}", stats.total_enums);
            println!("  Total Fields   : {}", stats.total_fields);
            println!("  Schema Size    : {} KB\n", stats.schema_bytes / 1024);
        }

        "search" => {
            if parts.len() < 2 {
                println!("[-] Usage: proto search <query>");
                println!("    Example: proto search poll");
                println!("    Example: proto search interactive");
                return;
            }
            let query = parts[1..].join(" ");
            println!("[*] Searching WAProto schema for '{}'...", query);
            let matches = SchemaInspector::global().search(&query);
            if matches.is_empty() {
                println!(
                    "[-] No messages, enums, or fields found matching '{}'.",
                    query
                );
                return;
            }

            println!("\nFound {} match(es) in schema:", matches.len());
            for m in matches.iter().take(40) {
                match &m.item_type {
                    baileys_proto::schema::SchemaItemType::Message => {
                        println!("  [MSG]  {:<32} (Message Definition)", m.name);
                    }
                    baileys_proto::schema::SchemaItemType::Enum => {
                        println!("  [ENUM] {:<32} (Enum Definition)", m.name);
                    }
                    baileys_proto::schema::SchemaItemType::Field {
                        parent_message,
                        field_type,
                        field_number,
                    } => {
                        println!(
                            "  [FLD]  {}.{:<20} -> {:<15} #{}",
                            parent_message, m.name, field_type, field_number
                        );
                    }
                    baileys_proto::schema::SchemaItemType::EnumValue {
                        parent_enum,
                        number,
                    } => {
                        println!("  [VAL]  {}.{:<20} = {}", parent_enum, m.name, number);
                    }
                }
            }
            if matches.len() > 40 {
                println!(
                    "  ... and {} more matches truncated (be more specific).",
                    matches.len() - 40
                );
            }
            println!();
        }

        "view" => {
            if parts.len() < 2 {
                println!("[-] Usage: proto view <MessageType | EnumType>");
                println!("    Example: proto view PollCreationMessage");
                println!("    Example: proto view ReactionMessage");
                return;
            }
            let type_name = parts[1];
            if let Some(def) = SchemaInspector::global().get_definition(type_name) {
                println!("\n=== WAProto Definition: {} ===\n{}\n", type_name, def);
            } else {
                println!(
                    "[-] Type '{}' not found in WAProto.proto schema.",
                    type_name
                );
                println!(
                    "    Tip: Use 'proto search {}' to find related symbols.",
                    type_name
                );
            }
        }

        "inspect" => {
            if parts.len() < 2 {
                println!("[-] Usage: proto inspect <id | last>");
                println!("    Example: proto inspect last");
                println!("    Example: proto inspect 3EB0C4...");
                return;
            }
            let target = parts[1];
            let msg_res = if target.eq_ignore_ascii_case("last") {
                store.get_latest_message().await
            } else {
                store.get_message(target).await
            };

            match msg_res {
                Ok(Some(stored)) => {
                    println!("\n=== Inspecting Message [{}] ===", stored.id);
                    println!("  Chat JID  : {}", stored.chat_jid);
                    println!("  Sender JID: {}", stored.sender_jid);
                    println!("  From Me   : {}", stored.from_me);
                    println!("  Timestamp : {}", stored.timestamp);
                    println!("  Summary   : {}", stored.summary);
                    println!("  Bytes Size: {} bytes", stored.proto_bytes.len());

                    if stored.proto_bytes.is_empty() {
                        println!("[-] Raw Protobuf bytes are empty for this message record.");
                        return;
                    }

                    match <baileys_proto::proto::Message as ProstMessage>::decode(
                        &stored.proto_bytes[..],
                    ) {
                        Ok(raw_proto) => {
                            let unwrap_res = raw_proto.unwrap_wrappers();
                            println!("\n--- Wrapper Unwrapping & Auto-Recognition ---");
                            println!(
                                "  Canonical Type : {} ({})",
                                unwrap_res.real_type.as_str(),
                                unwrap_res.real_type.display_name()
                            );
                            if unwrap_res.wrappers.is_empty() {
                                println!("  Wrapper Chain  : None (Direct message payload)");
                            } else {
                                println!("  Wrapper Chain  : {}", unwrap_res.wrappers.join(" -> "));
                            }

                            if let Some(text) = raw_proto.extract_text() {
                                println!("  Extracted Text : \"{}\"", text);
                            }
                            if let Some(media) = raw_proto.extract_media_meta() {
                                println!(
                                    "  Media Metadata : Mime='{}', Size={} bytes, DirectPath='{}'",
                                    media.mimetype.as_deref().unwrap_or("none"),
                                    media.file_length,
                                    media.direct_path.as_deref().unwrap_or("none")
                                );
                            }
                            if let Some(quoted) = raw_proto.extract_quoted_message() {
                                println!(
                                    "  Quoted Message : Recognized as {:?}",
                                    quoted.recognize_type()
                                );
                            }

                            println!("\n--- Decoded JSON Structure ---");
                            let json_val = raw_proto.to_json_value();
                            match serde_json::to_string_pretty(&json_val) {
                                Ok(pretty) => println!("{}", pretty),
                                Err(e) => println!("[-] JSON format error: {}", e),
                            }
                            println!();
                        }
                        Err(e) => {
                            println!("[-] Failed to decode protobuf bytes into Message: {}", e);
                        }
                    }
                }
                Ok(None) => {
                    if target.eq_ignore_ascii_case("last") {
                        println!("[-] No messages found in the local SQLite database.");
                    } else {
                        println!(
                            "[-] Message with ID '{}' not found in SQLite store.",
                            target
                        );
                    }
                }
                Err(e) => {
                    println!("[-] Database error querying message: {}", e);
                }
            }
        }

        "pull" => {
            println!("[*] Pulling latest WAProto directly from live WhatsApp Web (Pure Rust AST Engine)...");
            let default_target = std::path::Path::new("crates/baileys-proto/proto/WAProto.proto");
            let target_path = if default_target.exists() {
                Some(default_target)
            } else {
                None
            };
            let start = std::time::Instant::now();
            match baileys_proto::extractor::pull_latest_waproto(target_path).await {
                Ok(res) => {
                    let dur = start.elapsed();
                    println!(
                        "[+] Successfully extracted WAProto schema in pure Rust! ({:.2?})",
                        dur
                    );
                    println!("    WA Revision   : {}", res.client_revision);
                    println!("    Root entities : {}", res.root_entities);
                    println!("    Total fields  : {}", res.total_entities);
                    println!("    Schema size   : {} KB", res.schema_bytes / 1024);
                    println!(
                        "    Output target : {}",
                        res.output_file
                            .as_deref()
                            .unwrap_or("In-Memory Schema (Verified)")
                    );
                    if res.output_file.is_some() {
                        println!(
                            "    To rebuild Rust proto bindings: cargo build -p baileys-proto"
                        );
                    }
                }
                Err(e) => {
                    eprintln!("[-] Schema extraction failed: {}", e);
                }
            }
        }

        _ => {
            println!("[-] Unknown proto subcommand: '{}'", parts[0]);
            print_proto_help();
        }
    }
}

fn print_proto_help() {
    println!("\n=== WAProto Intelligence Suite Commands ===");
    println!("   proto list                   List all 28 recognized types & schema statistics");
    println!("   proto search <query>         Search messages, enums, and fields in WAProto");
    println!("   proto view <TypeName>        Display schema definition for a message or enum");
    println!("   proto inspect <id | last>    Decode message, unwrap wrappers & show pretty JSON");
    println!(
        "   proto pull                   Pull fresh WAProto from live WhatsApp Web JS bundle\n"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dynamic_prompt() {
        let connecting_prompt = get_prompt(false);
        assert_eq!(connecting_prompt, "rose[connecting] > ");
        let highlighted_connecting = RoseHelper.highlight_prompt(&connecting_prompt, true);
        assert!(highlighted_connecting.contains("\x1b[33m")); // Yellow

        let online_prompt = get_prompt(true);
        assert_eq!(online_prompt, "rose[online] > ");
        let highlighted_online = RoseHelper.highlight_prompt(&online_prompt, true);
        assert!(highlighted_online.contains("\x1b[92m")); // Bright green
    }

    #[test]
    fn test_rose_helper_completer() {
        let helper = RoseHelper;
        let dummy_ctx = rustyline::history::DefaultHistory::new();
        let (start, matches) = helper.complete("se", 2, &Context::new(&dummy_ctx)).unwrap();
        assert_eq!(start, 0);
        let matches_strs: Vec<String> = matches.into_iter().map(|m| m.replacement).collect();
        assert!(matches_strs.contains(&"send".to_string()));
        assert!(matches_strs.contains(&"selftest".to_string()));
        assert!(matches_strs.contains(&"sendevent".to_string()));
        assert!(matches_strs.contains(&"sendjson".to_string()));

        let (_, g_matches) = helper.complete("gh", 2, &Context::new(&dummy_ctx)).unwrap();
        let g_strs: Vec<String> = g_matches.into_iter().map(|m| m.replacement).collect();
        assert!(g_strs.contains(&"ghostmode".to_string()));

        let (_, f_matches) = helper.complete("fa", 2, &Context::new(&dummy_ctx)).unwrap();
        let f_strs: Vec<String> = f_matches.into_iter().map(|m| m.replacement).collect();
        assert!(f_strs.contains(&"fakevn".to_string()));
        assert!(f_strs.contains(&"fakequote".to_string()));

        let (start2, matches2) = helper
            .complete("qr l", 4, &Context::new(&dummy_ctx))
            .unwrap();
        assert_eq!(start2, 3);
        let matches2_strs: Vec<String> = matches2.into_iter().map(|m| m.replacement).collect();
        assert!(matches2_strs.contains(&"large".to_string()));

        let (start3, matches3) = helper
            .complete("proto s", 7, &Context::new(&dummy_ctx))
            .unwrap();
        assert_eq!(start3, 6);
        let matches3_strs: Vec<String> = matches3.into_iter().map(|m| m.replacement).collect();
        assert!(matches3_strs.contains(&"search".to_string()));

        let (start4, matches4) = helper
            .complete("cmdmode p", 9, &Context::new(&dummy_ctx))
            .unwrap();
        assert_eq!(start4, 8);
        let matches4_strs: Vec<String> = matches4.into_iter().map(|m| m.replacement).collect();
        assert!(matches4_strs.contains(&"public".to_string()));

        let (start5, matches5) = helper
            .complete("cmdm", 4, &Context::new(&dummy_ctx))
            .unwrap();
        assert_eq!(start5, 0);
        let matches5_strs: Vec<String> = matches5.into_iter().map(|m| m.replacement).collect();
        assert!(matches5_strs.contains(&"cmdmode".to_string()));

        let (start6, matches6) = helper
            .complete("logfilter d", 11, &Context::new(&dummy_ctx))
            .unwrap();
        assert_eq!(start6, 10);
        let matches6_strs: Vec<String> = matches6.into_iter().map(|m| m.replacement).collect();
        assert!(matches6_strs.contains(&"debug".to_string()));

        let (start7, matches7) = helper
            .complete("logfilter t", 11, &Context::new(&dummy_ctx))
            .unwrap();
        assert_eq!(start7, 10);
        let matches7_strs: Vec<String> = matches7.into_iter().map(|m| m.replacement).collect();
        assert!(matches7_strs.contains(&"trace".to_string()));

        let (start8, matches8) = helper
            .complete("help mess", 9, &Context::new(&dummy_ctx))
            .unwrap();
        assert_eq!(start8, 5);
        let matches8_strs: Vec<String> = matches8.into_iter().map(|m| m.replacement).collect();
        assert!(matches8_strs.contains(&"messaging".to_string()));

        let (start9, matches9) = helper
            .complete("help poll", 9, &Context::new(&dummy_ctx))
            .unwrap();
        assert_eq!(start9, 5);
        let matches9_strs: Vec<String> = matches9.into_iter().map(|m| m.replacement).collect();
        assert!(matches9_strs.contains(&"poll".to_string()));
        assert!(matches9_strs.contains(&"polltally".to_string()));
    }

    #[test]
    fn test_parse_target_path_extra() {
        // Plain arguments
        let res1 = parse_target_path_extra("me C:\\test\\file.jpg hello caption").unwrap();
        assert_eq!(res1.0, "me");
        assert_eq!(res1.1, "C:\\test\\file.jpg");
        assert_eq!(res1.2, Some("hello caption".to_string()));

        // Quoted path with spaces
        let res2 = parse_target_path_extra(
            "62812345678 \"C:\\Users\\Admin\\Downloads\\my photo.png\" Cute photo!",
        )
        .unwrap();
        assert_eq!(res2.0, "62812345678");
        assert_eq!(res2.1, "C:\\Users\\Admin\\Downloads\\my photo.png");
        assert_eq!(res2.2, Some("Cute photo!".to_string()));

        // Single quotes without caption
        let res3 = parse_target_path_extra("self 'C:\\sticker.webp'").unwrap();
        assert_eq!(res3.0, "self");
        assert_eq!(res3.1, "C:\\sticker.webp");
        assert_eq!(res3.2, None);
    }

    #[test]
    fn test_proto_schema_and_recognizer_integration() {
        let stats = SchemaInspector::global().stats();
        assert!(stats.total_messages > 100);
        assert!(stats.total_enums > 10);
        assert!(stats.total_fields > 500);

        // Search test
        let matches = SchemaInspector::global().search("poll");
        assert!(!matches.is_empty());

        // Definition view test
        let def = SchemaInspector::global().get_definition("ReactionMessage");
        assert!(def.is_some());
        assert!(def.unwrap().contains("text"));

        // Recognizer test
        let msg = baileys_proto::proto::Message {
            conversation: Some("Test message".to_string()),
            ..Default::default()
        };
        let unwrapped = msg.unwrap_wrappers();
        assert_eq!(unwrapped.real_type, RecognizedMessageType::Conversation);
        assert_eq!(msg.extract_text(), Some("Test message".to_string()));
    }
}
