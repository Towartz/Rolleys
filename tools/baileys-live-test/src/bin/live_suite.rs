//! Live parity suite: pairs with a real WhatsApp account and runs scenarios with real
//! assertions. Exits non-zero when any scenario fails.
//!
//! Safety: only the paired account itself is touched (self-chat, groups it creates,
//! read-only queries). Scenarios that need a second person are reported as SKIP.

use baileys_core::{BaileysClient, BaileysConfigBuilder, BaileysEvent};
use baileys_store::SqliteStore;
use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tracing_subscriber::FmtSubscriber;

const STEP_TIMEOUT: Duration = Duration::from_secs(60);
const EVENT_TIMEOUT: Duration = Duration::from_secs(20);
const PAIRING_TIMEOUT: Duration = Duration::from_secs(300);
const PACE: Duration = Duration::from_millis(1500);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Pass,
    Fail,
    Skip,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Status::Pass => "PASS",
            Status::Fail => "FAIL",
            Status::Skip => "SKIP",
        }
    }
}

struct Outcome {
    area: &'static str,
    name: String,
    status: Status,
    detail: String,
    millis: u128,
}

struct Suite {
    client: Arc<BaileysClient>,
    store: Arc<SqliteStore>,
    only: Option<String>,
    with_community: bool,
    chat_group: Option<String>,
    results: Vec<Outcome>,
}

impl Suite {
    fn selected(&self, area: &str) -> bool {
        self.only.as_deref().is_none_or(|o| o == area)
    }

    /// Runs one scenario. The body returns `Ok(detail)` to pass or `Err(reason)` to fail.
    async fn step<F, Fut>(&mut self, area: &'static str, name: &str, f: F)
    where
        F: FnOnce(Arc<BaileysClient>) -> Fut,
        Fut: Future<Output = Result<String, String>>,
    {
        if !self.selected(area) {
            return;
        }
        let started = Instant::now();
        let res = match tokio::time::timeout(STEP_TIMEOUT, f(self.client.clone())).await {
            Ok(r) => r,
            Err(_) => Err(format!("timed out after {}s", STEP_TIMEOUT.as_secs())),
        };
        let (status, detail) = match res {
            Ok(d) => (Status::Pass, d),
            Err(e) => (Status::Fail, e),
        };
        println!("  [{}] {}.{} - {}", status.label(), area, name, detail);
        self.results.push(Outcome {
            area,
            name: name.to_string(),
            status,
            detail,
            millis: started.elapsed().as_millis(),
        });
        tokio::time::sleep(PACE).await;
    }

    fn skip(&mut self, area: &'static str, name: &str, reason: &str) {
        if !self.selected(area) {
            return;
        }
        println!("  [SKIP] {}.{} - {}", area, name, reason);
        self.results.push(Outcome {
            area,
            name: name.to_string(),
            status: Status::Skip,
            detail: reason.to_string(),
            millis: 0,
        });
    }
}

/// Waits for the first event the predicate maps to `Some`. Subscribe *before* acting.
async fn expect_event<T>(
    rx: &mut broadcast::Receiver<BaileysEvent>,
    what: &str,
    pick: impl Fn(&BaileysEvent) -> Option<T>,
) -> Result<T, String> {
    let deadline = tokio::time::Instant::now() + EVENT_TIMEOUT;
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Err(_) => return Err(format!("API call ok but event not seen: {what}")),
            Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(broadcast::error::RecvError::Closed)) => {
                return Err("event channel closed".into())
            }
            Ok(Ok(ev)) => {
                if let Some(v) = pick(&ev) {
                    return Ok(v);
                }
            }
        }
    }
}

fn e<T: std::fmt::Display>(err: T) -> String {
    err.to_string()
}

fn print_pairing_code(code: &str) {
    println!("\n==============================================================");
    println!("  WHATSAPP PAIRING CODE:  {code}");
    println!("  Phone: Settings > Linked Devices > Link with phone number");
    println!(
        "  Enter the code above. Waiting up to {}s...",
        PAIRING_TIMEOUT.as_secs()
    );
    println!("==============================================================\n");
}

fn usage() {
    println!("live-suite: live parity checks against a real WhatsApp account\n");
    println!("Usage: live-suite --phone <NUMBER> [OPTIONS]\n");
    println!("  -p, --phone <NUMBER>  Number to pair by code (or env LIVE_PHONE)");
    println!("  -s, --db <PATH>       Session database (default .sessions/live_suite.db)");
    println!("      --clean           Delete the session first (forces re-pairing)");
    println!("      --only <AREA>     Run one area: core, messaging, rich, chat, appstate, reaction, history, account, media, groups, community, newsletter, privacy");
    println!("      --community       Also run community scenarios (leaves a stub community)");
    println!(
        "      --chat-group <NAME>  Run chat scenarios on an existing group (no messages sent)"
    );
    println!("      --json            Print the report as JSON after the table");
    println!("  -h, --help");
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut phone = std::env::var("LIVE_PHONE").ok();
    let mut db_path = std::env::var("ROLLEYS_LIVE_SESSION_DB")
        .unwrap_or_else(|_| ".sessions/live_suite.db".to_string());
    let mut chat_group = None::<String>;
    let (mut clean, mut json, mut with_community, mut only) = (false, false, false, None::<String>);

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--phone" | "-p" if i + 1 < args.len() => {
                phone = Some(args[i + 1].clone());
                i += 1;
            }
            "--db" | "-s" if i + 1 < args.len() => {
                db_path = args[i + 1].clone();
                i += 1;
            }
            "--only" if i + 1 < args.len() => {
                only = Some(args[i + 1].clone());
                i += 1;
            }
            "--clean" => clean = true,
            "--json" => json = true,
            "--community" => with_community = true,
            "--chat-group" if i + 1 < args.len() => {
                chat_group = Some(args[i + 1].clone());
                i += 1;
            }
            "--help" | "-h" => return usage(),
            other => {
                eprintln!("unknown argument: {other}\n");
                usage();
                std::process::exit(2);
            }
        }
        i += 1;
    }
    let Some(phone) = phone.map(|p| p.replace('+', "").trim().to_string()) else {
        eprintln!("--phone is required (or set LIVE_PHONE)\n");
        usage();
        std::process::exit(2);
    };

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    let _ = tracing::subscriber::set_global_default(
        FmtSubscriber::builder()
            .with_env_filter(filter)
            .with_target(false)
            .finish(),
    );

    if let Some(parent) = Path::new(&db_path).parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    if clean {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{db_path}{suffix}"));
        }
    }

    let store = match SqliteStore::open(&db_path) {
        Ok(s) => Arc::new(s),
        Err(err) => {
            eprintln!("cannot open session db '{db_path}': {err}");
            std::process::exit(2);
        }
    };
    let config = BaileysConfigBuilder::new()
        .with_browser("Chrome", "Mac OS", "14.4.1")
        .with_phone_pairing(&phone)
        .with_dynamic_version(true)
        .build();
    let client = Arc::new(BaileysClient::new(store.clone(), config));

    // Subscribe before connecting so neither the pairing code nor `Connected` is missed.
    let mut rx = client.events();
    println!("[*] Connecting (session: {db_path}, phone: +{phone})...");
    if let Err(err) = client.connect().await {
        eprintln!("connect failed: {err}");
        std::process::exit(2);
    }

    let me_jid = {
        let deadline = tokio::time::Instant::now() + PAIRING_TIMEOUT;
        loop {
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Err(_) => {
                    eprintln!("timed out waiting for pairing/connection");
                    std::process::exit(2);
                }
                Ok(Ok(BaileysEvent::PairingCode { code })) => print_pairing_code(&code),
                Ok(Ok(BaileysEvent::Connected { me_jid })) => break me_jid,
                Ok(Ok(BaileysEvent::Disconnected { reason })) => {
                    println!("[!] disconnected while waiting: {reason}");
                }
                Ok(Ok(_)) | Ok(Err(broadcast::error::RecvError::Lagged(_))) => {}
                Ok(Err(broadcast::error::RecvError::Closed)) => {
                    eprintln!("event channel closed before connecting");
                    std::process::exit(2);
                }
            }
        }
    };
    drop(rx);
    println!("[+] Connected as {me_jid}\n");
    tokio::time::sleep(Duration::from_secs(3)).await;

    let self_jid = format!("{phone}@s.whatsapp.net");
    let mut suite = Suite {
        client: client.clone(),
        store: store.clone(),
        only,
        with_community,
        chat_group,
        results: Vec::new(),
    };

    run_core(&mut suite, &phone).await;
    run_messaging(&mut suite, &self_jid).await;
    run_rich(&mut suite, &self_jid).await;
    run_chat(&mut suite, &self_jid).await;
    run_appstate(&mut suite).await;
    run_reaction(&mut suite, &self_jid).await;
    run_history(&mut suite).await;
    run_media(&mut suite, &self_jid).await;
    run_groups(&mut suite).await;
    run_community(&mut suite).await;
    run_newsletter(&mut suite).await;
    run_privacy(&mut suite, &self_jid).await;
    run_account(&mut suite, &self_jid).await;

    report(&suite.results, json);
    let failed = suite.results.iter().any(|r| r.status == Status::Fail);
    std::process::exit(if failed { 1 } else { 0 });
}

async fn run_core(s: &mut Suite, phone: &str) {
    println!("== core ==");
    let phone = phone.to_string();
    s.step(
        "core",
        "me_jid matches paired number",
        move |c| async move {
            let got = c.me_jid().await.map_err(e)?.ok_or("no me_jid")?;
            let user = got.split(['@', ':']).next().unwrap_or("");
            if user == phone {
                Ok(got)
            } else {
                Err(format!("expected user {phone}, got {got}"))
            }
        },
    )
    .await;
    s.step("core", "ping", |c| async move {
        let d = c.ping().await.map_err(e)?;
        Ok(format!("{} ms", d.as_millis()))
    })
    .await;
}

/// Subscribes first, runs `send`, then requires the server to answer the returned stanza id.
/// Fails when the server rejects it (`error_<code>`), so "call returned Ok" is not enough.
async fn send_acked<Fut, Er>(
    c: &Arc<BaileysClient>,
    send: impl FnOnce() -> Fut,
) -> Result<(String, String), String>
where
    Fut: Future<Output = Result<String, Er>>,
    Er: std::fmt::Display,
{
    let mut rx = c.events();
    let id = send().await.map_err(e)?;
    if id.is_empty() {
        return Err("empty message id".into());
    }
    let want = id.clone();
    let status = expect_event(&mut rx, &format!("receipt for {id}"), move |ev| match ev {
        BaileysEvent::Receipt { key_id, status, .. } if *key_id == want => Some(status.clone()),
        _ => None,
    })
    .await?;
    if status.starts_with("error") {
        return Err(format!("server rejected {id}: {status}"));
    }
    Ok((id, status))
}

async fn run_messaging(s: &mut Suite, self_jid: &str) {
    println!("== messaging (self-chat) ==");
    let to = self_jid.to_string();
    let text_id = Arc::new(tokio::sync::Mutex::new(String::new()));
    let poll_id = Arc::new(tokio::sync::Mutex::new(String::new()));

    let (t, slot) = (to.clone(), text_id.clone());
    s.step("messaging", "send text", move |c| async move {
        let (id, st) = send_acked(&c, || c.send_text_message(&t, "live-suite: hello")).await?;
        *slot.lock().await = id.clone();
        Ok(format!("id={id} {st}"))
    })
    .await;

    if text_id.lock().await.is_empty() {
        for name in ["react", "edit", "revoke"] {
            s.skip("messaging", name, "depends on 'send text'");
        }
    } else {
        let (t, slot) = (to.clone(), text_id.clone());
        s.step("messaging", "react", move |c| async move {
            let target = slot.lock().await.clone();
            let (id, st) = send_acked(&c, || c.send_reaction(&t, &target, "👍")).await?;
            Ok(format!("id={id} {st}"))
        })
        .await;
        let (t, slot) = (to.clone(), text_id.clone());
        s.step("messaging", "edit", move |c| async move {
            let target = slot.lock().await.clone();
            let (id, st) = send_acked(&c, || {
                c.edit_message(&t, &target, "live-suite: hello (edited)")
            })
            .await?;
            Ok(format!("id={id} {st}"))
        })
        .await;
        let (t, slot) = (to.clone(), text_id.clone());
        s.step("messaging", "revoke", move |c| async move {
            let target = slot.lock().await.clone();
            let (id, st) = send_acked(&c, || c.revoke_message(&t, &target)).await?;
            Ok(format!("id={id} {st}"))
        })
        .await;
    }

    let (t, slot) = (to.clone(), poll_id.clone());
    s.step("messaging", "poll", move |c| async move {
        let opts = vec!["A".to_string(), "B".to_string()];
        let (id, st) = send_acked(&c, || c.send_poll(&t, "live-suite poll", &opts, 1)).await?;
        *slot.lock().await = id.clone();
        Ok(format!("id={id} {st}"))
    })
    .await;

    if poll_id.lock().await.is_empty() {
        s.skip("messaging", "poll vote", "depends on 'poll'");
    } else {
        let (t, slot) = (to.clone(), poll_id.clone());
        s.step("messaging", "poll vote", move |c| async move {
            let target = slot.lock().await.clone();
            let picked = vec!["A".to_string()];
            let (id, st) = send_acked(&c, || c.send_poll_vote(&t, &target, &picked)).await?;
            Ok(format!("id={id} {st}"))
        })
        .await;
    }
}

/// 1x1 PNG, used as a valid image payload.
const TINY_PNG: [u8; 70] = [
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0xFC, 0xCF, 0xC0, 0x50,
    0x0F, 0x00, 0x04, 0x85, 0x01, 0x80, 0x84, 0xA9, 0x8C, 0x21, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// Deterministic, non-compressible-looking bytes. Length is deliberately not a multiple
/// of 16 so AES-CBC padding is exercised.
fn pattern_bytes(len: usize, seed: u32) -> Vec<u8> {
    let mut x = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            (x >> 8) as u8
        })
        .collect()
}

/// Downloads the media of a message we just sent and requires the bytes to match exactly.
async fn download_and_compare(
    c: &Arc<BaileysClient>,
    id: &str,
    expected: &[u8],
) -> Result<String, String> {
    let path = std::env::temp_dir().join(format!("live-suite-{id}.bin"));
    let saved = c
        .download_message_media_to_path(id, Some(&path))
        .await
        .map_err(e)?;
    let got = tokio::fs::read(&saved).await.map_err(e);
    let _ = tokio::fs::remove_file(&saved).await;
    let got = got?;
    if got == expected {
        Ok(format!("{} bytes identical", got.len()))
    } else {
        let first_diff = got.iter().zip(expected).position(|(a, b)| a != b);
        Err(format!(
            "downloaded {} bytes, expected {} (first diff at {:?})",
            got.len(),
            expected.len(),
            first_diff
        ))
    }
}

async fn run_media(s: &mut Suite, self_jid: &str) {
    println!("== media (upload → download → compare) ==");
    if !s.selected("media") {
        return;
    }

    // (label, payload) pairs; the send closure per kind differs, the verification does not.
    let doc = Arc::new(pattern_bytes(100_003, 7));
    let audio = Arc::new(pattern_bytes(5_001, 11));
    let image: Arc<Vec<u8>> = Arc::new(TINY_PNG.to_vec());

    let doc_id = Arc::new(tokio::sync::Mutex::new(String::new()));
    let (t, p, slot) = (self_jid.to_string(), doc.clone(), doc_id.clone());
    s.step("media", "document upload", move |c| async move {
        let (id, st) = send_acked(&c, || {
            c.send_document_message(&t, &p, "live-suite.bin", "application/octet-stream", None)
        })
        .await?;
        *slot.lock().await = id.clone();
        Ok(format!("id={id} {st}, {} bytes", p.len()))
    })
    .await;
    if doc_id.lock().await.is_empty() {
        s.skip("media", "document download", "depends on 'document upload'");
    } else {
        let (p, slot) = (doc.clone(), doc_id.clone());
        s.step("media", "document download", move |c| async move {
            let id = slot.lock().await.clone();
            download_and_compare(&c, &id, &p).await
        })
        .await;
    }

    let img_id = Arc::new(tokio::sync::Mutex::new(String::new()));
    let (t, p, slot) = (self_jid.to_string(), image.clone(), img_id.clone());
    s.step("media", "image upload", move |c| async move {
        let (id, st) = send_acked(&c, || c.send_image_message(&t, &p, Some("live-suite"))).await?;
        *slot.lock().await = id.clone();
        Ok(format!("id={id} {st}"))
    })
    .await;
    if img_id.lock().await.is_empty() {
        s.skip("media", "image download", "depends on 'image upload'");
    } else {
        let (p, slot) = (image.clone(), img_id.clone());
        s.step("media", "image download", move |c| async move {
            let id = slot.lock().await.clone();
            download_and_compare(&c, &id, &p).await
        })
        .await;
    }

    let audio_id = Arc::new(tokio::sync::Mutex::new(String::new()));
    let (t, p, slot) = (self_jid.to_string(), audio.clone(), audio_id.clone());
    s.step("media", "audio upload", move |c| async move {
        let (id, st) = send_acked(&c, || c.send_audio_message(&t, &p, false, Some(1))).await?;
        *slot.lock().await = id.clone();
        Ok(format!("id={id} {st}"))
    })
    .await;
    if audio_id.lock().await.is_empty() {
        s.skip("media", "audio download", "depends on 'audio upload'");
    } else {
        let (p, slot) = (audio.clone(), audio_id.clone());
        s.step("media", "audio download", move |c| async move {
            let id = slot.lock().await.clone();
            download_and_compare(&c, &id, &p).await
        })
        .await;
    }
}

/// Retries a read-back until it yields a value; server-side state is often eventually consistent.
async fn eventually<T, Fut>(what: &str, mut probe: impl FnMut() -> Fut) -> Result<T, String>
where
    Fut: Future<Output = Result<Option<T>, String>>,
{
    let deadline = tokio::time::Instant::now() + EVENT_TIMEOUT;
    loop {
        if let Some(v) = probe().await? {
            return Ok(v);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "not observed within {}s: {what}",
                EVENT_TIMEOUT.as_secs()
            ));
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

type Slot<T> = Arc<tokio::sync::Mutex<Option<T>>>;

fn slot<T>() -> Slot<T> {
    Arc::new(tokio::sync::Mutex::new(None))
}

async fn run_community(s: &mut Suite) {
    println!("== community (self-created) ==");
    if !s.selected("community") {
        return;
    }
    // WhatsApp has no way to delete a community through this API, only to leave it, so each run
    // leaves a stub behind on the account. Require an explicit request.
    if s.only.as_deref() != Some("community") && !s.with_community {
        s.skip(
            "community",
            "all",
            "opt-in (leaves a stub community behind): use --only community or --community",
        );
        return;
    }

    let community: Slot<String> = slot();
    let sub: Slot<String> = slot();
    let subject = format!(
        "live-suite community {}",
        chrono::Local::now().format("%H:%M:%S")
    );

    let (cslot, subj) = (community.clone(), subject.clone());
    s.step("community", "create", move |c| async move {
        let meta = c
            .community_create(&subj, Some("live-suite community"))
            .await
            .map_err(e)?;
        if !meta.id.ends_with("@g.us") {
            return Err(format!("unexpected community id {}", meta.id));
        }
        if meta.subject != subj {
            return Err(format!("subject {:?} != {:?}", meta.subject, subj));
        }
        *cslot.lock().await = Some(meta.id.clone());
        Ok(meta.id)
    })
    .await;

    let Some(cid) = community.lock().await.clone() else {
        for name in [
            "metadata",
            "create subgroup",
            "subgroup listed",
            "invite code",
            "unlink subgroup",
        ] {
            s.skip("community", name, "community was not created");
        }
        return;
    };

    let (cid2, subj) = (cid.clone(), subject.clone());
    s.step("community", "metadata", move |c| async move {
        let meta = c.community_get_metadata(&cid2).await.map_err(e)?;
        if !meta.is_community {
            return Err("metadata says is_community=false".into());
        }
        if meta.subject != subj {
            return Err(format!("subject {:?} != {:?}", meta.subject, subj));
        }
        Ok(format!("{} participant(s)", meta.participants.len()))
    })
    .await;

    let (cid2, sslot) = (cid.clone(), sub.clone());
    s.step("community", "create subgroup", move |c| async move {
        let gid = c
            .community_create_subgroup(&cid2, "live-suite subgroup", &[])
            .await
            .map_err(e)?;
        *sslot.lock().await = Some(gid.clone());
        Ok(gid)
    })
    .await;

    if let Some(gid) = sub.lock().await.clone() {
        let cid2 = cid.clone();
        let g = gid.clone();
        s.step("community", "subgroup listed", move |c| {
            let (cid2, g) = (cid2.clone(), g.clone());
            async move {
                eventually("subgroup in fetch_subgroups", || {
                    let (c, cid2, g) = (c.clone(), cid2.clone(), g.clone());
                    async move {
                        let subs = c.community_fetch_subgroups(&cid2).await.map_err(e)?;
                        Ok(subs.iter().any(|x| x.id == g).then_some(subs.len()))
                    }
                })
                .await
                .map(|n| format!("{n} linked group(s), ours included"))
            }
        })
        .await;
    } else {
        s.skip("community", "subgroup listed", "subgroup was not created");
    }

    let cid2 = cid.clone();
    s.step("community", "invite code", move |c| async move {
        let code = c.community_get_invite_code(&cid2).await.map_err(e)?;
        if code.is_empty() {
            return Err("empty invite code".into());
        }
        Ok(format!("code len {}", code.len()))
    })
    .await;

    if let Some(gid) = sub.lock().await.clone() {
        let (cid2, g) = (cid.clone(), gid.clone());
        s.step("community", "unlink subgroup", move |c| {
            let (cid2, g) = (cid2.clone(), g.clone());
            async move {
                c.community_unlink_subgroup(&cid2, &g).await.map_err(e)?;
                eventually("subgroup gone from fetch_subgroups", || {
                    let (c, cid2, g) = (c.clone(), cid2.clone(), g.clone());
                    async move {
                        let subs = c.community_fetch_subgroups(&cid2).await.map_err(e)?;
                        Ok((!subs.iter().any(|x| x.id == g)).then_some(()))
                    }
                })
                .await
                .map(|_| "unlinked".to_string())
            }
        })
        .await;
        // Cleanup always runs.
        let g = gid.clone();
        s.step(
            "community",
            "leave subgroup (cleanup)",
            move |c| async move {
                c.group_leave(&g).await.map_err(e)?;
                Ok("left".into())
            },
        )
        .await;
    } else {
        s.skip("community", "unlink subgroup", "subgroup was not created");
    }

    let cid2 = cid.clone();
    s.step("community", "leave (cleanup)", move |c| async move {
        c.community_leave(&cid2).await.map_err(e)?;
        Ok("left (stub community remains on the account)".into())
    })
    .await;
}

async fn run_newsletter(s: &mut Suite) {
    println!("== newsletter (self-created channel) ==");
    if !s.selected("newsletter") {
        return;
    }
    let chan: Slot<String> = slot();
    let post: Slot<u64> = slot();
    let name = format!(
        "live-suite channel {}",
        chrono::Local::now().format("%H:%M:%S")
    );
    let renamed = format!("{name} renamed");

    let (cs, n) = (chan.clone(), name.clone());
    s.step("newsletter", "create", move |c| async move {
        let meta = c
            .newsletter_create(&n, Some("live-suite channel"))
            .await
            .map_err(e)?;
        if !meta.jid.ends_with("@newsletter") {
            return Err(format!("unexpected channel jid {}", meta.jid));
        }
        *cs.lock().await = Some(meta.jid.clone());
        Ok(format!("{} role={:?}", meta.jid, meta.role))
    })
    .await;

    let Some(jid) = chan.lock().await.clone() else {
        for name in [
            "query metadata",
            "rename",
            "description",
            "picture set → removed",
            "subscribers",
            "post → visible",
            "react → visible",
            "view receipt",
            "mute toggle",
            "admin count",
            "listed in subscribed",
        ] {
            s.skip("newsletter", name, "channel was not created");
        }
        return;
    };

    let (j, n) = (jid.clone(), name.clone());
    s.step("newsletter", "query metadata", move |c| async move {
        let meta = c.newsletter_query_metadata(&j).await.map_err(e)?;
        if meta.name != n {
            return Err(format!("name {:?} != {:?}", meta.name, n));
        }
        Ok(format!("name ok, {} subscriber(s)", meta.subscribers_count))
    })
    .await;

    let (j, r) = (jid.clone(), renamed.clone());
    s.step("newsletter", "rename", move |c| async move {
        c.newsletter_update_name(&j, &r).await.map_err(e)?;
        eventually("new name in query_metadata", || {
            let (c, j, r) = (c.clone(), j.clone(), r.clone());
            async move {
                let meta = c.newsletter_query_metadata(&j).await.map_err(e)?;
                Ok((meta.name == r).then_some(()))
            }
        })
        .await
        .map(|_| "renamed".to_string())
    })
    .await;

    let j = jid.clone();
    s.step("newsletter", "description", move |c| async move {
        let text = "live-suite description";
        c.newsletter_update_description(&j, text).await.map_err(e)?;
        eventually("description in query_metadata", || {
            let (c, j) = (c.clone(), j.clone());
            async move {
                let meta = c.newsletter_query_metadata(&j).await.map_err(e)?;
                Ok((meta.description.as_deref() == Some(text)).then_some(()))
            }
        })
        .await
        .map(|_| "description updated".to_string())
    })
    .await;

    let j = jid.clone();
    s.step("newsletter", "picture set → removed", move |c| async move {
        c.newsletter_update_picture(&j, &TINY_PNG)
            .await
            .map_err(e)?;
        eventually("picture url in query_metadata", || {
            let (c, j) = (c.clone(), j.clone());
            async move {
                let meta = c.newsletter_query_metadata(&j).await.map_err(e)?;
                Ok(meta.picture_url.filter(|u| !u.is_empty()))
            }
        })
        .await?;
        c.newsletter_remove_picture(&j).await.map_err(e)?;
        eventually("picture gone from query_metadata", || {
            let (c, j) = (c.clone(), j.clone());
            async move {
                let meta = c.newsletter_query_metadata(&j).await.map_err(e)?;
                Ok(meta.picture_url.is_none_or(|u| u.is_empty()).then_some(()))
            }
        })
        .await
        .map(|_| "picture set, seen, removed".to_string())
    })
    .await;

    let j = jid.clone();
    s.step("newsletter", "subscribers", move |c| async move {
        let own = c.newsletter_subscribers(&j).await.map_err(e)?;
        // The list of followed channels carries no counts, so ask each channel directly; at least
        // one followed channel must report subscribers, which proves the count is really read.
        let followed = c.newsletter_get_subscribed().await.map_err(e)?;
        let mut best: Option<(String, u64)> = None;
        for m in followed.iter().filter(|m| m.jid != j).take(8) {
            let n = c.newsletter_subscribers(&m.jid).await.map_err(e)?;
            if best.as_ref().is_none_or(|(_, b)| n > *b) {
                best = Some((m.jid.clone(), n));
            }
        }
        match best {
            Some((jid, n)) if n > 0 => Ok(format!("own {own}, {jid} has {n}")),
            Some((jid, _)) => Err(format!(
                "none of the followed channels reported subscribers (best: {jid})"
            )),
            None => Err("not following any other channel to compare against".into()),
        }
    })
    .await;

    let (j, pslot) = (jid.clone(), post.clone());
    s.step("newsletter", "post → visible", move |c| async move {
        let text = format!(
            "live-suite post {}",
            chrono::Local::now().format("%H:%M:%S")
        );
        c.newsletter_send_post(&j, &text).await.map_err(e)?;
        let sid = eventually("post in fetch_messages", || {
            let (c, j, text) = (c.clone(), j.clone(), text.clone());
            async move {
                let msgs = c.newsletter_fetch_messages(&j, 20, None).await.map_err(e)?;
                Ok(msgs
                    .iter()
                    .find(|m| m.summary.contains(&text))
                    .map(|m| m.server_id))
            }
        })
        .await?;
        *pslot.lock().await = Some(sid);
        Ok(format!("server_id={sid}"))
    })
    .await;

    if let Some(sid) = post.lock().await.as_ref().copied() {
        let j = jid.clone();
        s.step("newsletter", "react → visible", move |c| async move {
            c.newsletter_react(&j, sid, "👍").await.map_err(e)?;
            eventually("reaction in fetch_messages", || {
                let (c, j) = (c.clone(), j.clone());
                async move {
                    let msgs = c.newsletter_fetch_messages(&j, 20, None).await.map_err(e)?;
                    Ok(msgs
                        .iter()
                        .find(|m| m.server_id == sid)
                        .and_then(|m| m.reactions.get("👍").copied())
                        .filter(|n| *n > 0))
                }
            })
            .await
            .map(|n| format!("👍 x{n}"))
        })
        .await;
        let j = jid.clone();
        s.step("newsletter", "view receipt", move |c| async move {
            c.newsletter_view_receipt(&j, &[sid]).await.map_err(e)?;
            Ok("sent".into())
        })
        .await;
    } else {
        s.skip("newsletter", "react → visible", "post was not found");
        s.skip("newsletter", "view receipt", "post was not found");
    }

    let j = jid.clone();
    s.step("newsletter", "mute toggle", move |c| async move {
        c.newsletter_mute(&j, true).await.map_err(e)?;
        c.newsletter_mute(&j, false).await.map_err(e)?;
        Ok("muted then unmuted".into())
    })
    .await;

    let j = jid.clone();
    s.step("newsletter", "admin count", move |c| async move {
        let n = c.newsletter_admin_count(&j).await.map_err(e)?;
        if n < 1 {
            return Err(format!("expected at least 1 admin (the owner), got {n}"));
        }
        Ok(format!("{n} admin(s)"))
    })
    .await;

    let j = jid.clone();
    s.step("newsletter", "listed in subscribed", move |c| async move {
        let all = c.newsletter_get_subscribed().await.map_err(e)?;
        if all.iter().any(|m| m.jid == j) {
            Ok(format!("{} channel(s), ours included", all.len()))
        } else {
            Err(format!(
                "{j} missing from {} subscribed channel(s)",
                all.len()
            ))
        }
    })
    .await;

    // Cleanup always runs, then verifies the channel is really gone.
    let j = jid.clone();
    s.step("newsletter", "delete (cleanup)", move |c| async move {
        c.newsletter_delete(&j).await.map_err(e)?;
        eventually("channel absent from subscribed", || {
            let (c, j) = (c.clone(), j.clone());
            async move {
                let all = c.newsletter_get_subscribed().await.map_err(e)?;
                Ok((!all.iter().any(|m| m.jid == j)).then_some(()))
            }
        })
        .await
        .map(|_| "deleted".to_string())
    })
    .await;
}

/// One self-chat send that must be acked by the server. `$send` may use `$c` (the client) and
/// `$t` (a clone of the target JID).
macro_rules! ack_step {
    ($s:expr, $name:expr, $to:expr, |$c:ident, $t:ident| $send:expr) => {{
        let $t = $to.clone();
        $s.step("rich", $name, move |client| async move {
            let (id, st) = send_acked(&client, || async {
                let $c = &client;
                $send.await
            })
            .await?;
            Ok(format!("id={id} {st}"))
        })
        .await;
    }};
}

async fn run_rich(s: &mut Suite, self_jid: &str) {
    use baileys_core::baileys_proto::proto::{Message, MessageKey};
    use baileys_core::{InteractiveButton, ListRow, ListSection};

    println!("== rich message types (self-chat) ==");
    if !s.selected("rich") {
        return;
    }
    let to = self_jid.to_string();

    // A base message the reply/pin/comment scenarios can point at.
    let base: Slot<String> = slot();
    let (t, b) = (to.clone(), base.clone());
    s.step("rich", "base text", move |c| async move {
        let (id, st) = send_acked(&c, || c.send_text_message(&t, "live-suite: base")).await?;
        *b.lock().await = Some(id.clone());
        Ok(format!("id={id} {st}"))
    })
    .await;
    let base_id = base.lock().await.clone();

    ack_step!(s, "location", to, |c, t| c.send_location(
        &t,
        -6.2088,
        106.8456,
        Some("live-suite"),
        Some("Jakarta"),
        None
    ));
    ack_step!(s, "live location", to, |c, t| c.send_live_location(
        &t,
        -6.2088,
        106.8456,
        Some(20),
        Some("live-suite")
    ));
    ack_step!(s, "contact", to, |c, t| c.send_contact(
        &t,
        "Live Suite",
        "+6281234567890"
    ));
    ack_step!(s, "vcard", to, |c, t| c.send_vcard(
        &t,
        "Live Suite",
        "BEGIN:VCARD\nVERSION:3.0\nFN:Live Suite\nTEL;type=CELL:+6281234567890\nEND:VCARD"
    ));
    ack_step!(s, "contacts array", to, |c, t| c.send_contacts_array(
        &t,
        "2 contacts",
        vec![
            ("Live A".to_string(), "+6281111111111".to_string()),
            ("Live B".to_string(), "+6282222222222".to_string()),
        ]
    ));
    ack_step!(s, "event", to, |c, t| c.send_event(
        &t,
        "live-suite event",
        Some("created by live-suite"),
        chrono::Utc::now().timestamp() + 86_400,
        None,
        Some("Jakarta")
    ));
    ack_step!(s, "album", to, |c, t| c.send_album(&t, 2, 0));
    ack_step!(s, "view-once (text payload)", to, |c, t| c.send_view_once(
        &t,
        Message {
            conversation: Some("live-suite: view once".to_string()),
            ..Default::default()
        },
        true
    ));
    ack_step!(s, "forward", to, |c, t| c.forward_message(
        &t,
        Message {
            conversation: Some("live-suite: forwarded".to_string()),
            ..Default::default()
        },
        true,
        1
    ));
    ack_step!(s, "json message", to, |c, t| c.send_message_json(
        &t,
        &serde_json::json!({ "conversation": "live-suite: from json" })
    ));
    ack_step!(s, "quick-reply buttons", to, |c, t| c
        .send_interactive_buttons(
            &t,
            "live-suite buttons",
            &[
                InteractiveButton::quick_reply("One", "1"),
                InteractiveButton::quick_reply("Two", "2"),
            ],
            Some("header"),
            Some("footer")
        ));
    ack_step!(s, "list", to, |c, t| c.send_list_message(
        &t,
        "live-suite list",
        "pick one",
        "Open",
        &[ListSection {
            title: "Section".to_string(),
            rows: vec![
                ListRow {
                    title: "Row 1".to_string(),
                    description: Some("first".to_string()),
                    row_id: "r1".to_string(),
                },
                ListRow {
                    title: "Row 2".to_string(),
                    description: None,
                    row_id: "r2".to_string(),
                },
            ],
        }],
        Some("footer")
    ));

    let Some(base_id) = base_id else {
        for name in ["reply (quote)", "comment", "pin", "unpin"] {
            s.skip("rich", name, "depends on 'base text'");
        }
        return;
    };

    let (self_for_quote, bid) = (to.clone(), base_id.clone());
    ack_step!(s, "reply (quote)", to, |c, t| c
        .send_text_message_with_quote(
            &t,
            "live-suite: reply",
            &bid,
            Some(self_for_quote.as_str()),
            Some("live-suite: base")
        ));
    let bid = base_id.clone();
    ack_step!(s, "comment", to, |c, t| c.send_comment(
        &t,
        &MessageKey {
            remote_jid: Some(t.clone()),
            from_me: Some(true),
            id: Some(bid.clone()),
            participant: None,
        },
        "live-suite: comment"
    ));
    let bid = base_id.clone();
    ack_step!(s, "pin", to, |c, t| c.pin_message(&t, &bid, true, 86_400));
    let bid = base_id.clone();
    // Always unpin so the chat is left as we found it.
    ack_step!(s, "unpin", to, |c, t| c.pin_message(&t, &bid, false, 0));
}

/// Paired action/restore steps: the restore step always runs so the chat is left as we found it.
macro_rules! chat_step {
    ($s:expr, $name:expr, $to:expr, |$c:ident, $t:ident| $call:expr) => {{
        let $t = $to.clone();
        $s.step("chat", $name, move |client| async move {
            let $c = &client;
            $call.await.map_err(e)?;
            Ok("accepted by server".to_string())
        })
        .await;
    }};
}

/// `(version, hash, number of indexed entries)` per collection, as persisted locally.
async fn app_state_fingerprint(
    store: &SqliteStore,
    collections: &[&str],
) -> Result<Vec<(String, u64, String, usize)>, String> {
    use baileys_core::syncd::StoredLTHashState;
    use baileys_store::SignalKeysStore;
    let mut out = Vec::new();
    for col in collections {
        let raw = store
            .get_key("app-state-sync-version", col)
            .await
            .map_err(e)?
            .ok_or_else(|| format!("no stored state for {col}"))?;
        let st: StoredLTHashState = serde_json::from_slice(&raw).map_err(e)?;
        out.push((
            col.to_string(),
            st.version,
            st.hash_hex,
            st.index_value_hex.len(),
        ));
    }
    Ok(out)
}

/// Finds one joined group by name: an exact (case-insensitive) match wins, otherwise a single
/// "contains" match. Several candidates is an error: never guess which real group to touch.
/// Returns `(jid, description for the report)`.
async fn resolve_group(c: &BaileysClient, wanted: &str) -> Result<(String, String), String> {
    let wanted = wanted.to_lowercase();
    let all = c.group_fetch_all_participating().await.map_err(e)?;
    let exact: Vec<_> = all
        .iter()
        .filter(|g| g.subject.to_lowercase() == wanted)
        .collect();
    let loose: Vec<_> = all
        .iter()
        .filter(|g| g.subject.to_lowercase().contains(&wanted))
        .collect();
    let cands = if exact.len() == 1 { exact } else { loose };
    match cands.as_slice() {
        [g] => Ok((
            g.id.clone(),
            format!(
                "{:?} {} ({} members)",
                g.subject,
                g.id,
                g.participants.len()
            ),
        )),
        [] => Err(format!(
            "no joined group matching {wanted:?} among {}",
            all.len()
        )),
        many => Err(format!(
            "{} groups match {wanted:?}, pass a more specific name: {:?}",
            many.len(),
            many.iter()
                .take(8)
                .map(|g| g.subject.as_str())
                .collect::<Vec<_>>()
        )),
    }
}

async fn run_chat(s: &mut Suite, self_jid: &str) {
    use baileys_core::syncd::{
        CRITICAL_BLOCK, CRITICAL_UNBLOCK_LOW, REGULAR, REGULAR_HIGH, REGULAR_LOW,
    };

    println!("== chat modify / app-state sync (self-chat) ==");
    if !s.selected("chat") {
        return;
    }
    // Chat scenarios default to the self-chat. With --chat-group they run against an existing
    // group instead; that path only toggles mute/pin/archive/unread (each restored) and never
    // sends a message or touches membership.
    let group_name = s.chat_group.clone();
    let to = match &group_name {
        None => self_jid.to_string(),
        Some(name) => {
            let wanted = name.to_lowercase();
            let resolved: Slot<String> = slot();
            let rs = resolved.clone();
            s.step("chat", "resolve group", move |c| async move {
                let (jid, detail) = resolve_group(&c, &wanted).await?;
                *rs.lock().await = Some(jid);
                Ok(detail)
            })
            .await;
            let found = resolved.lock().await.clone();
            match found {
                Some(jid) => jid,
                None => return,
            }
        }
    };

    s.step("chat", "app-state full resync", |c| async move {
        let sum = c
            .resync_app_state(
                &[
                    CRITICAL_BLOCK,
                    CRITICAL_UNBLOCK_LOW,
                    REGULAR_HIGH,
                    REGULAR_LOW,
                    REGULAR,
                ],
                true,
            )
            .await
            .map_err(e)?;
        if sum.collections_failed > 0 || sum.collections_synced == 0 {
            return Err(format!(
                "synced={} failed={}",
                sum.collections_synced, sum.collections_failed
            ));
        }
        Ok(format!(
            "{} collections, {} mutations",
            sum.collections_synced, sum.mutations_applied
        ))
    })
    .await;

    let base: Slot<String> = slot();
    let (t, b) = (to.clone(), base.clone());
    if group_name.is_none() {
        s.step("chat", "base text", move |c| async move {
            let (id, st) =
                send_acked(&c, || c.send_text_message(&t, "live-suite: chat target")).await?;
            *b.lock().await = Some(id.clone());
            Ok(format!("id={id} {st}"))
        })
        .await;
    }

    chat_step!(s, "mute 1h", to, |c, t| c.chat_modify_mute(&t, Some(3600)));
    chat_step!(s, "unmute (restore)", to, |c, t| c
        .chat_modify_mute(&t, None));
    chat_step!(s, "pin", to, |c, t| c.chat_modify_pin(&t, true));
    chat_step!(s, "unpin (restore)", to, |c, t| c
        .chat_modify_pin(&t, false));
    chat_step!(s, "archive", to, |c, t| c.chat_modify_archive(&t, true));
    chat_step!(s, "unarchive (restore)", to, |c, t| c
        .chat_modify_archive(&t, false));
    chat_step!(s, "mark unread", to, |c, t| c
        .chat_modify_mark_unread(&t, true));
    chat_step!(s, "mark read (restore)", to, |c, t| c
        .chat_modify_mark_unread(&t, false));

    if let Some(id) = base.lock().await.clone() {
        let i = id.clone();
        chat_step!(s, "star message", to, |c, t| c
            .chat_modify_star(&t, &i, true, true));
        let i = id.clone();
        chat_step!(s, "unstar (restore)", to, |c, t| c
            .chat_modify_star(&t, &i, true, false));
    } else {
        let why = "needs a message we sent (self-chat only; no messages are sent to groups)";
        s.skip("chat", "star message", why);
        s.skip("chat", "unstar (restore)", why);
    }

    // Contacts live in critical_unblock_low and use REMOVE, which no other step exercises.
    let store_c = s.store.clone();
    s.step(
        "chat",
        "contact add → remove (chain matches snapshot)",
        move |c| async move {
            use baileys_core::syncd::{ContactEdit, CRITICAL_UNBLOCK_LOW};
            let cols = [CRITICAL_UNBLOCK_LOW];
            let jid = "6280000000001@s.whatsapp.net";
            let edit = ContactEdit {
                full_name: Some("Live Suite Test".into()),
                first_name: Some("Live".into()),
                pn_jid: Some(jid.into()),
                // WhatsApp-level contact only; nothing is written to the phone's address book.
                save_on_primary_addressbook: Some(false),
                ..Default::default()
            };
            // Start from the server's truth so the chained hash is built on a complete state.
            c.resync_app_state(&cols, true).await.map_err(e)?;
            c.add_or_edit_contact(jid, &edit).await.map_err(e)?;
            c.remove_contact(jid).await.map_err(e)?;
            let chained = app_state_fingerprint(&store_c, &cols).await?;
            c.resync_app_state(&cols, true).await.map_err(e)?;
            let rebuilt = app_state_fingerprint(&store_c, &cols).await?;
            for (a, b) in chained.iter().zip(&rebuilt) {
                if a.1 != b.1 {
                    return Err(format!(
                        "{}: server at v{} but we chained to v{} (changed elsewhere; rerun)",
                        a.0, b.1, a.1
                    ));
                }
                if a.2 != b.2 || a.3 != b.3 {
                    return Err(format!(
                        "{} v{}: chained hash/entries ({}.., {}) differ from snapshot ({}.., {})",
                        a.0,
                        a.1,
                        &a.2[..16],
                        a.3,
                        &b.2[..16],
                        b.3
                    ));
                }
            }
            Ok(format!(
                "{} v{} {} entries, hash identical",
                rebuilt[0].0, rebuilt[0].1, rebuilt[0].3
            ))
        },
    )
    .await;

    // The server cannot check our LTHash, so rebuild the collections from its snapshot and require
    // the state we chained locally (version, hash, entry count) to be identical. This is what
    // catches a wrong hash that the individual acks above cannot.
    let store = s.store.clone();
    s.step("chat", "local chain matches server snapshot", move |c| async move {
        use baileys_core::syncd::{REGULAR_HIGH, REGULAR_LOW};
        let cols = [REGULAR_HIGH, REGULAR_LOW];
        let chained = app_state_fingerprint(&store, &cols).await?;
        let sum = c.resync_app_state(&cols, true).await.map_err(e)?;
        if sum.collections_failed > 0 {
            return Err(format!("{} collection(s) failed to resync", sum.collections_failed));
        }
        let rebuilt = app_state_fingerprint(&store, &cols).await?;
        for (a, b) in chained.iter().zip(&rebuilt) {
            if a.1 != b.1 {
                return Err(format!(
                    "{}: server is at v{} but we chained to v{} (changed elsewhere during the check; rerun)",
                    a.0, b.1, a.1
                ));
            }
            if a.2 != b.2 || a.3 != b.3 {
                return Err(format!(
                    "{} v{}: chained hash/entries ({}.., {}) differ from snapshot ({}.., {})",
                    a.0,
                    a.1,
                    &a.2[..16],
                    a.3,
                    &b.2[..16],
                    b.3
                ));
            }
        }
        Ok(chained
            .iter()
            .map(|f| format!("{} v{} {} entries", f.0, f.1, f.3))
            .collect::<Vec<_>>()
            .join(", "))
    })
    .await;
}

async fn run_appstate(s: &mut Suite) {
    use baileys_core::syncd::{
        CRITICAL_BLOCK, CRITICAL_UNBLOCK_LOW, REGULAR, REGULAR_HIGH, REGULAR_LOW,
    };
    println!("== app-state snapshot rebuild (read-only) ==");
    if s.only.as_deref() != Some("appstate") {
        return;
    }
    s.step("appstate", "forced snapshot resync", |c| async move {
        let mut rx = c.events();
        let sum = c
            .resync_app_state(
                &[
                    CRITICAL_BLOCK,
                    CRITICAL_UNBLOCK_LOW,
                    REGULAR_HIGH,
                    REGULAR_LOW,
                    REGULAR,
                ],
                true,
            )
            .await
            .map_err(e)?;
        if sum.collections_failed > 0 || sum.collections_synced == 0 {
            return Err(format!(
                "synced={} failed={}",
                sum.collections_synced, sum.collections_failed
            ));
        }
        // Report which app-state events the snapshot produced (informational; depends on the account).
        let mut seen: std::collections::BTreeMap<&'static str, usize> = Default::default();
        let mut dropped = 0u64;
        loop {
            let ev = match rx.try_recv() {
                Ok(ev) => ev,
                Err(broadcast::error::TryRecvError::Lagged(n)) => {
                    dropped += n;
                    continue;
                }
                Err(_) => break,
            };
            let name = match ev {
                BaileysEvent::ChatUpdate { .. } => "chats.update",
                BaileysEvent::ContactUpdate { .. } => "contacts.update",
                BaileysEvent::ContactsUpsert { .. } => "contacts.upsert",
                BaileysEvent::SettingsUpdate { .. } => "settings.update",
                BaileysEvent::ChatsLock { .. } => "chats.lock",
                BaileysEvent::LidMappingUpdate { .. } => "lid-mapping.update",
                BaileysEvent::MessageStarred { .. } => "messages.update(starred)",
                BaileysEvent::ChatsDelete { .. } => "chats.delete",
                BaileysEvent::MessagesDelete { .. } => "messages.delete",
                BaileysEvent::LabelEdit { .. } => "labels.edit",
                BaileysEvent::ChatLabelUpdate { .. } | BaileysEvent::MessageLabelUpdate { .. } => {
                    "labels.association"
                }
                _ => continue,
            };
            *seen.entry(name).or_default() += 1;
        }
        Ok(format!(
            "{} collections, {} mutations; events: {:?}; dropped by a full event channel: {}",
            sum.collections_synced, sum.mutations_applied, seen, dropped
        ))
    })
    .await;
}

async fn run_reaction(s: &mut Suite, self_jid: &str) {
    println!("== reaction (interactive: react from your phone) ==");
    if s.only.as_deref() != Some("reaction") {
        return;
    }
    let to = self_jid.to_string();
    s.step("reaction", "phone reaction → messages.reaction", move |c| async move {
        let mut rx = c.events();
        let (id, _) = send_acked(&c, || {
            c.send_text_message(&to, "live-suite: react to this message from your phone")
        })
        .await?;
        println!("    -> open 'Message yourself' on the phone and react to the newest message ({id})");
        // Reactions from our own phone arrive wrapped in deviceSentMessage.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(55);
        loop {
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Err(_) => return Err("no reaction seen within 55s".into()),
                Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
                Ok(Err(_)) => return Err("event channel closed".into()),
                Ok(Ok(BaileysEvent::MessagesReaction { reaction })) if reaction.message_id == id => {
                    return Ok(format!(
                        "{:?} by {} (from_me={}, removed={})",
                        reaction.text, reaction.sender, reaction.sender_from_me, reaction.removed
                    ));
                }
                Ok(Ok(_)) => {}
            }
        }
    })
    .await;
}

async fn run_history(s: &mut Suite) {
    println!("== history (on-demand fetch from the phone) ==");
    if !s.selected("history") {
        return;
    }
    // On-demand history for the self-chat comes back empty (chat returned with 0 messages, with
    // either PN or LID addressing; cause unknown), so only a group anchor is a meaningful check.
    let Some(group) = s.chat_group.clone() else {
        s.skip(
            "history",
            "on-demand fetch → messaging-history.set",
            "needs --chat-group (self-chat on-demand history returns no messages; open question)",
        );
        return;
    };
    let store = s.store.clone();
    s.step(
        "history",
        "on-demand fetch → messaging-history.set",
        move |c| async move {
            // The phone returns messages older than the anchor: the oldest message of the group
            // already in the local store (nothing is sent to the group).
            let (chat, anchor, anchor_from_me, anchor_ts) = {
                use baileys_store::ChatStore;
                let (gid, _) = resolve_group(&c, &group).await?;
                let local = store.get_messages(&gid, 100_000, None).await.map_err(e)?;
                let oldest = local
                    .iter()
                    .min_by_key(|m| m.timestamp)
                    .ok_or_else(|| format!("no local messages for {gid} to anchor on"))?;
                (gid, oldest.id.clone(), oldest.from_me, oldest.timestamp)
            };
            let mut rx = c.events();
            c.fetch_message_history(&chat, 10, &anchor, anchor_from_me, anchor_ts)
                .await
                .map_err(e)?;
            let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
            loop {
                let ev = match tokio::time::timeout_at(deadline, rx.recv()).await {
                    Err(_) => return Err("no on-demand history sync within 45s".into()),
                    Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
                    Ok(Err(_)) => return Err("event channel closed".into()),
                    Ok(Ok(ev)) => ev,
                };
                let BaileysEvent::HistorySync {
                    sync_type,
                    messages,
                    contacts,
                    conversations,
                    is_latest,
                    chunk_order,
                    peer_data_request_session_id,
                    ..
                } = ev
                else {
                    continue;
                };
                // ON_DEMAND = 6 in HistorySync.HistorySyncType.
                if sync_type != 6 {
                    continue;
                }
                if is_latest.is_some() {
                    return Err(format!(
                        "is_latest must be None for on-demand, got {is_latest:?}"
                    ));
                }
                if messages.is_empty() {
                    let chats: Vec<_> = conversations
                        .iter()
                        .map(|c| (c.id.clone(), c.messages.len()))
                        .collect();
                    return Err(format!(
                        "on-demand history carried no messages (chats {chats:?}, {} contacts)",
                        contacts.len()
                    ));
                }
                let user = chat.split('@').next().unwrap_or_default().to_string();
                let foreign = messages
                    .iter()
                    .filter(|m| {
                        let chat = m
                            .key
                            .as_ref()
                            .and_then(|k| k.remote_jid.clone())
                            .unwrap_or_default();
                        // Self-chat may be addressed by phone number or by our LID.
                        !(chat.starts_with(&user) || chat.ends_with("@lid"))
                    })
                    .count();
                if foreign > 0 {
                    return Err(format!("{foreign} message(s) from another chat"));
                }
                let newer = messages
                    .iter()
                    .filter(|m| m.message_timestamp.unwrap_or(0) > anchor_ts)
                    .count();
                if newer > 0 {
                    return Err(format!("{newer} message(s) newer than the anchor"));
                }
                return Ok(format!(
                    "{} messages, {} chats, {} contacts, chunk {:?}, session {:?}",
                    messages.len(),
                    conversations.len(),
                    contacts.len(),
                    chunk_order,
                    peer_data_request_session_id
                ));
            }
        },
    )
    .await;
}

async fn run_account(s: &mut Suite, self_jid: &str) {
    println!("== account settings (restored after each step) ==");
    if !s.selected("account") {
        return;
    }
    let me = self_jid.to_string();
    s.step(
        "account",
        "default disappearing mode (set → restore)",
        move |c| async move {
            let current = |c: Arc<BaileysClient>, me: String| async move {
                let rows = c
                    .fetch_disappearing_duration(&[me.as_str()])
                    .await
                    .map_err(e)?;
                Ok::<_, String>(rows.first().map(|r| r.1))
            };
            let before = current(c.clone(), me.clone())
                .await?
                .ok_or("no disappearing_mode row for ourselves")?;
            let wanted = if before == 86_400 { 604_800 } else { 86_400 };
            c.update_default_disappearing_mode(wanted)
                .await
                .map_err(e)?;
            let seen = eventually("new default timer", || {
                let (c, me) = (c.clone(), me.clone());
                async move { Ok(current(c, me).await?.filter(|d| *d == wanted)) }
            })
            .await;
            // Always put the account back, even when the check above failed.
            let restore = c.update_default_disappearing_mode(before).await.map_err(e);
            seen?;
            restore?;
            Ok(format!("{before}s → {wanted}s → {before}s"))
        },
    )
    .await;

    let me = self_jid.to_string();
    s.step(
        "account",
        "placeholder resend request",
        move |c| async move {
            use baileys_core::baileys_proto::proto::MessageKey;
            let key = MessageKey {
                remote_jid: Some(me.clone()),
                from_me: Some(false),
                id: Some("LIVESUITE-NO-SUCH-MESSAGE".to_string()),
                participant: None,
            };
            let (id, st) = send_acked(&c, || c.request_placeholder_resend(&key)).await?;
            Ok(format!("id={id} {st}"))
        },
    )
    .await;
}

async fn run_groups(s: &mut Suite) {
    println!("== groups (self-created) ==");
    if !s.selected("groups") {
        return;
    }
    let group: Arc<tokio::sync::Mutex<Option<String>>> = Arc::new(tokio::sync::Mutex::new(None));

    let slot = group.clone();
    s.step(
        "groups",
        "create → chats.upsert + groups.upsert",
        move |c| async move {
            // Subscribe both receivers before acting so neither event can be missed.
            let mut rx = c.events();
            let mut rx_chat = c.events();
            let subject = format!("live-suite {}", chrono::Local::now().format("%H:%M:%S"));
            let gid = c.group_create(&subject, &[]).await.map_err(e)?;
            *slot.lock().await = Some(gid.clone());
            let seen = expect_event(&mut rx, "GroupsUpsert", |ev| match ev {
                BaileysEvent::GroupsUpsert { groups } if groups.iter().any(|g| g.id == gid) => {
                    Some(groups.len())
                }
                _ => None,
            })
            .await?;
            // Baileys emits chats.upsert right before groups.upsert for a new group.
            let name = expect_event(&mut rx_chat, "ChatsUpsert", |ev| match ev {
                BaileysEvent::ChatsUpsert { chats } => chats
                    .iter()
                    .find(|ch| ch.jid == gid)
                    .map(|ch| ch.name.clone()),
                _ => None,
            })
            .await?;
            if name.as_deref() != Some(subject.as_str()) {
                return Err(format!("chats.upsert name {name:?} != {subject:?}"));
            }
            Ok(format!("{gid} ({seen} upserted, chat named)"))
        },
    )
    .await;

    let Some(gid) = group.lock().await.clone() else {
        for name in [
            "subject → groups.update",
            "description → groups.update",
            "announce → groups.update",
            "picture set → removed",
            "member label",
            "invite code",
            "fetch all participating",
        ] {
            s.skip("groups", name, "group was not created");
        }
        s.skip(
            "groups",
            "participants add/remove",
            "needs a second account",
        );
        return;
    };

    let g = gid.clone();
    s.step("groups", "subject → groups.update", move |c| async move {
        let mut rx = c.events();
        let new_subject = "live-suite renamed";
        c.group_update_subject(&g, new_subject).await.map_err(e)?;
        expect_event(&mut rx, "GroupsUpdate{subject}", |ev| match ev {
            BaileysEvent::GroupsUpdate { update }
                if update.id == g && update.subject.as_deref() == Some(new_subject) =>
            {
                Some(())
            }
            _ => None,
        })
        .await?;
        Ok("subject change observed".into())
    })
    .await;

    let g = gid.clone();
    s.step(
        "groups",
        "description → groups.update",
        move |c| async move {
            let mut rx = c.events();
            let desc = "live-suite description";
            c.group_update_description(&g, desc).await.map_err(e)?;
            expect_event(&mut rx, "GroupsUpdate{description}", |ev| match ev {
                BaileysEvent::GroupsUpdate { update }
                    if update.id == g && update.description.as_deref() == Some(desc) =>
                {
                    Some(())
                }
                _ => None,
            })
            .await?;
            Ok("description change observed".into())
        },
    )
    .await;

    let g = gid.clone();
    s.step("groups", "announce → groups.update", move |c| async move {
        let mut rx = c.events();
        c.group_setting_update(&g, "announcement")
            .await
            .map_err(e)?;
        expect_event(&mut rx, "GroupsUpdate{announce=true}", |ev| match ev {
            BaileysEvent::GroupsUpdate { update }
                if update.id == g && update.announce == Some(true) =>
            {
                Some(())
            }
            _ => None,
        })
        .await?;
        // restore
        c.group_setting_update(&g, "not_announcement")
            .await
            .map_err(e)?;
        Ok("announce on observed, restored".into())
    })
    .await;

    let g = gid.clone();
    s.step("groups", "picture set → removed", move |c| async move {
        // The server answers 404 item-not-found when a group has no picture (Baileys throws on it
        // too), so for polling that means "no picture yet".
        async fn picture(c: &BaileysClient, g: &str) -> Result<Option<String>, String> {
            match c.profile_picture_url(g, "image").await {
                Ok(url) => Ok(url),
                Err(err) if err.to_string().contains("item-not-found") => Ok(None),
                Err(err) => Err(err.to_string()),
            }
        }
        c.update_profile_picture(Some(&g), &TINY_PNG)
            .await
            .map_err(|err| format!("setting the picture failed: {err}"))?;
        eventually("group picture url", || {
            let (c, g) = (c.clone(), g.clone());
            async move { picture(&c, &g).await }
        })
        .await?;
        c.remove_profile_picture(Some(&g))
            .await
            .map_err(|err| format!("removing the picture failed: {err}"))?;
        eventually("group picture removed", || {
            let (c, g) = (c.clone(), g.clone());
            async move { Ok(picture(&c, &g).await?.is_none().then_some(())) }
        })
        .await
        .map(|_| "picture set, seen, removed".to_string())
    })
    .await;

    let g = gid.clone();
    s.step("groups", "member label", move |c| async move {
        let (id, st) = send_acked(&c, || c.update_member_label(&g, "live-suite")).await?;
        Ok(format!("id={id} {st}"))
    })
    .await;

    let g = gid.clone();
    s.step("groups", "invite code", move |c| async move {
        let code = c.group_get_invite_code(&g).await.map_err(e)?;
        if code.is_empty() {
            return Err("empty invite code".into());
        }
        Ok(format!("code len {}", code.len()))
    })
    .await;

    let g = gid.clone();
    s.step("groups", "fetch all participating", move |c| async move {
        let all = c.group_fetch_all_participating().await.map_err(e)?;
        if all.iter().any(|m| m.id == g) {
            Ok(format!("{} groups, ours included", all.len()))
        } else {
            Err(format!(
                "created group {g} missing from {} groups",
                all.len()
            ))
        }
    })
    .await;

    s.skip(
        "groups",
        "participants add/remove",
        "needs a second account",
    );

    // Cleanup always runs, even if earlier steps failed.
    let g = gid.clone();
    s.step("groups", "leave (cleanup)", move |c| async move {
        c.group_leave(&g).await.map_err(e)?;
        Ok("left".into())
    })
    .await;
}

async fn run_privacy(s: &mut Suite, self_jid: &str) {
    println!("== privacy / queries (read-only) ==");
    s.step("privacy", "fetch privacy settings", |c| async move {
        c.fetch_privacy_settings().await.map_err(e)?;
        Ok("ok".into())
    })
    .await;
    s.step("privacy", "fetch blocklist", |c| async move {
        let list = c.fetch_blocklist().await.map_err(e)?;
        Ok(format!("{} blocked", list.len()))
    })
    .await;
    let jid = self_jid.to_string();
    s.step("privacy", "onWhatsApp(self)", move |c| async move {
        let number = jid.split('@').next().unwrap_or("").to_string();
        let res = c.check_on_whatsapp(&[number.as_str()]).await.map_err(e)?;
        if res.is_empty() {
            Err("no result for own number".into())
        } else {
            Ok(format!("{} result(s)", res.len()))
        }
    })
    .await;
}

fn report(results: &[Outcome], json: bool) {
    println!("\n================ RESULTS ================");
    let width = results
        .iter()
        .map(|r| r.area.len() + r.name.len() + 1)
        .max()
        .unwrap_or(10);
    for r in results {
        println!(
            "{:<4}  {:<w$}  {:>6} ms",
            r.status.label(),
            format!("{}.{}", r.area, r.name),
            r.millis,
            w = width
        );
    }
    let count = |st| results.iter().filter(|r| r.status == st).count();
    println!(
        "\n{} passed, {} failed, {} skipped",
        count(Status::Pass),
        count(Status::Fail),
        count(Status::Skip)
    );
    for r in results.iter().filter(|r| r.status == Status::Fail) {
        println!("  FAIL {}.{}: {}", r.area, r.name, r.detail);
    }
    if json {
        let rows: Vec<_> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "area": r.area,
                    "name": r.name,
                    "status": r.status.label(),
                    "detail": r.detail,
                    "millis": r.millis,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&rows).unwrap_or_default()
        );
    }
}
