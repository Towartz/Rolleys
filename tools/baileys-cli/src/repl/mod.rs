pub mod dispatch;
pub mod proto;

#[cfg(test)]
mod tests;

use crate::ui::{set_log_printer, LogPrinter};
use baileys_core::BaileysClient;
use baileys_store::SqliteStore;
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
                }

                dispatch::dispatch_command(trimmed, client, store).await;
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
