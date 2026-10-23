//! SQLite store benchmark on synthetic data: `cargo bench -p baileys-store --bench sqlite`.
//!
//! A plain harness (no extra dependencies) that reports the median time per operation over
//! several rounds. Numbers are meant for before/after comparison on the same machine.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use baileys_store::*;

const SESSIONS: usize = 10_000;
const LID_MAPPINGS: usize = 2_000;
const CONTACTS: usize = 10_000;
const CHATS: usize = 2_000;
const MESSAGES: usize = 50_000;
const MESSAGE_CHATS: usize = 100;

fn bench_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("sqlite-bench");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn message(i: usize) -> StoredMessage {
    StoredMessage {
        id: format!("MSG{i:08}"),
        chat_jid: format!("62800{:06}@s.whatsapp.net", i % MESSAGE_CHATS),
        sender_jid: format!("62800{:06}@s.whatsapp.net", i % MESSAGE_CHATS),
        from_me: i.is_multiple_of(3),
        timestamp: 1_700_000_000 + i as u64,
        summary: format!("message number {i} with some text"),
        status: "received".into(),
        has_media: false,
        proto_bytes: vec![(i % 251) as u8; 256],
        is_starred: i.is_multiple_of(500),
        is_revoked: false,
        is_edited: false,
    }
}

fn chat(i: usize) -> StoredChat {
    StoredChat {
        jid: format!("62811{i:06}@s.whatsapp.net"),
        name: Some(format!("Chat {i}")),
        unread_count: (i % 5) as u32,
        last_message_timestamp: 1_700_000_000 + i as u64,
        last_message_summary: Some("hello".into()),
        pinned: i.is_multiple_of(100),
        archived: false,
        mute_until: None,
        ephemeral_expiration: 0,
        ephemeral_setting_timestamp: None,
    }
}

fn contact(i: usize) -> StoredContact {
    StoredContact {
        jid: format!("62822{i:06}@s.whatsapp.net"),
        name: Some(format!("Contact {i}")),
        push_name: Some(format!("Push {i}")),
        lid: Some(format!("{}@lid", 100_000_000_000u64 + i as u64)),
    }
}

fn key(key_type: &str, i: usize, len: usize) -> KeyEntry {
    KeyEntry {
        key_type: key_type.into(),
        id: format!("62833{i:06}:{}@s.whatsapp.net", i % 4),
        data: vec![(i % 251) as u8; len],
    }
}

/// Runs `op` `iters` times per round and prints the median time per call.
async fn measure<F, Fut>(name: &str, rounds: usize, iters: usize, mut op: F)
where
    F: FnMut(usize) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let mut samples = Vec::with_capacity(rounds);
    let mut n = 0;
    for _ in 0..rounds {
        let start = Instant::now();
        for _ in 0..iters {
            op(n).await;
            n += 1;
        }
        samples.push(start.elapsed() / iters as u32);
    }
    samples.sort();
    println!("{name:<44} {:>12}", fmt(samples[samples.len() / 2]));
}

fn fmt(d: Duration) -> String {
    let ns = d.as_nanos();
    if ns >= 1_000_000 {
        format!("{:.2} ms", ns as f64 / 1e6)
    } else {
        format!("{:.2} µs", ns as f64 / 1e3)
    }
}

async fn populate(store: &SqliteStore) {
    store
        .set_keys((0..SESSIONS).map(|i| key("session", i, 1200)).collect())
        .await
        .unwrap();
    store
        .set_keys(
            (0..LID_MAPPINGS)
                .map(|i| key("lid-mapping", i, 24))
                .collect(),
        )
        .await
        .unwrap();
    store
        .set_keys((0..812).map(|i| key("pre-key", i, 140)).collect())
        .await
        .unwrap();
    for i in 0..CONTACTS {
        store.save_contact(&contact(i)).await.unwrap();
    }
    for i in 0..CHATS {
        store.save_chat(&chat(i)).await.unwrap();
    }
    let all: Vec<_> = (0..MESSAGES).map(message).collect();
    for chunk in all.chunks(5_000) {
        store.save_messages_batch(chunk).await.unwrap();
    }
}

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(run());
}

async fn run() {
    let dir = bench_dir();
    let path = dir.join("bench.db");

    let start = Instant::now();
    let store = SqliteStore::open(&path).unwrap();
    populate(&store).await;
    println!("{:<44} {:>12}", "populate (setup)", fmt(start.elapsed()));
    drop(store);

    let start = Instant::now();
    // SQLITE_BENCH_READERS=0 compares against a single shared connection.
    let mut options = SqliteOptions::default();
    if let Some(n) = std::env::var("SQLITE_BENCH_READERS")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        options.read_connections = n;
    }
    if let Some(n) = std::env::var("SQLITE_BENCH_MMAP")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        options.mmap_bytes = n;
    }
    let store = Arc::new(SqliteStore::open_with(&path, options).unwrap());
    println!("{:<44} {:>12}", "open existing db", fmt(start.elapsed()));

    let s = &store;
    measure("get_key session (hit)", 7, 2_000, |n| async move {
        let k = key("session", (n * 7919) % SESSIONS, 0);
        assert!(s.get_key("session", &k.id).await.unwrap().is_some());
    })
    .await;
    measure("get_key session (miss)", 7, 2_000, |n| async move {
        let id = format!("missing-{n}");
        assert!(s.get_key("session", &id).await.unwrap().is_none());
    })
    .await;
    measure("set_keys 1 session", 7, 500, |n| async move {
        s.set_keys(vec![key("session", n % SESSIONS, 1200)])
            .await
            .unwrap();
    })
    .await;
    measure("get_all_keys lid-mapping (2000)", 7, 20, |_| async move {
        assert_eq!(
            s.get_all_keys("lid-mapping").await.unwrap().len(),
            LID_MAPPINGS
        );
    })
    .await;
    measure("get_key_count pre-key", 7, 500, |_| async move {
        s.get_key_count("pre-key").await.unwrap();
    })
    .await;
    measure("get_contact by jid", 7, 1_000, |n| async move {
        let c = contact((n * 31) % CONTACTS);
        assert!(s.get_contact(&c.jid).await.unwrap().is_some());
    })
    .await;
    measure("get_contact by lid", 7, 200, |n| async move {
        let c = contact((n * 31) % CONTACTS);
        assert!(s
            .get_contact(c.lid.as_deref().unwrap())
            .await
            .unwrap()
            .is_some());
    })
    .await;
    measure("get_chats (2000)", 7, 10, |_| async move {
        assert_eq!(s.get_chats().await.unwrap().len(), CHATS);
    })
    .await;
    measure("get_messages page 50", 7, 200, |n| async move {
        let jid = message(n % MESSAGE_CHATS).chat_jid;
        assert_eq!(s.get_messages(&jid, 50, None).await.unwrap().len(), 50);
    })
    .await;
    measure("get_starred_messages 50", 7, 50, |_| async move {
        s.get_starred_messages(50).await.unwrap();
    })
    .await;
    measure("save_messages_batch 1000", 5, 3, |n| async move {
        let batch: Vec<_> = (0..1_000)
            .map(|i| message(MESSAGES + n * 1_000 + i))
            .collect();
        s.save_messages_batch(&batch).await.unwrap();
    })
    .await;
    measure(
        "history chunk (500 push+200 chat+1000 lid)",
        5,
        1,
        |n| async move { history_chunk(s, n).await },
    )
    .await;

    // Four readers hammer get_key while one writer stores message batches.
    let readers_only = concurrent_reads(&store, false).await;
    let with_writer = concurrent_reads(&store, true).await;
    println!(
        "{:<44} {:>12}",
        "4 parallel get_key (per call)",
        fmt(readers_only)
    );
    println!(
        "{:<44} {:>12}",
        "4 parallel get_key + writer (per call)",
        fmt(with_writer)
    );

    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

/// What one history sync chunk writes besides messages, the way the client stores it.
async fn history_chunk(s: &SqliteStore, n: usize) {
    let base = CONTACTS + n * 2_000;
    let pushnames: Vec<_> = (0..500)
        .map(|i| StoredContact {
            lid: None,
            ..contact(base + i)
        })
        .collect();
    s.save_contacts_batch(&pushnames).await.unwrap();
    let chats: Vec<_> = (0..200).map(|i| chat(CHATS + n * 200 + i)).collect();
    s.save_chats_batch(&chats).await.unwrap();
    let lids: Vec<_> = (0..1_000).map(|i| contact(base + 500 + i)).collect();
    s.save_contacts_batch(&lids).await.unwrap();
}

async fn concurrent_reads(store: &Arc<SqliteStore>, with_writer: bool) -> Duration {
    const CALLS: usize = 5_000;
    let writer = with_writer.then(|| {
        let s = store.clone();
        tokio::spawn(async move {
            for n in 0..20 {
                let batch: Vec<_> = (0..1_000)
                    .map(|i| message(2 * MESSAGES + n * 1_000 + i))
                    .collect();
                s.save_messages_batch(&batch).await.unwrap();
            }
        })
    });
    let start = Instant::now();
    let tasks: Vec<_> = (0..4)
        .map(|t| {
            let s = store.clone();
            tokio::spawn(async move {
                for n in 0..CALLS {
                    let k = key("session", (t * CALLS + n) * 7919 % SESSIONS, 0);
                    s.get_key("session", &k.id).await.unwrap();
                }
            })
        })
        .collect();
    for t in tasks {
        t.await.unwrap();
    }
    let elapsed = start.elapsed() / (4 * CALLS) as u32;
    if let Some(w) = writer {
        w.await.unwrap();
    }
    elapsed
}
