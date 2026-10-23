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
    only: Option<String>,
    with_community: bool,
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
    println!("      --only <AREA>     Run one area: core, messaging, media, groups, community, newsletter, privacy");
    println!("      --community       Also run community scenarios (leaves a stub community)");
    println!("      --json            Print the report as JSON after the table");
    println!("  -h, --help");
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut phone = std::env::var("LIVE_PHONE").ok();
    let mut db_path = std::env::var("ROSE_LIVE_SESSION_DB")
        .unwrap_or_else(|_| ".sessions/live_suite.db".to_string());
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
    let client = Arc::new(BaileysClient::new(store, config));

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
        only,
        with_community,
        results: Vec::new(),
    };

    run_core(&mut suite, &phone).await;
    run_messaging(&mut suite, &self_jid).await;
    run_media(&mut suite, &self_jid).await;
    run_groups(&mut suite).await;
    run_community(&mut suite).await;
    run_newsletter(&mut suite).await;
    run_privacy(&mut suite, &self_jid).await;

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

async fn run_groups(s: &mut Suite) {
    println!("== groups (self-created) ==");
    if !s.selected("groups") {
        return;
    }
    let group: Arc<tokio::sync::Mutex<Option<String>>> = Arc::new(tokio::sync::Mutex::new(None));

    let slot = group.clone();
    s.step("groups", "create → groups.upsert", move |c| async move {
        let mut rx = c.events();
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
        Ok(format!("{gid} ({seen} upserted)"))
    })
    .await;

    let Some(gid) = group.lock().await.clone() else {
        for name in [
            "subject → groups.update",
            "description → groups.update",
            "announce → groups.update",
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
