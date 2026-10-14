use baileys_core::{BaileysClient, BaileysConfigBuilder, BaileysEvent};
use baileys_store::SqliteStore;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let mut phone_number = "6283162498175".to_string();
    let mut recipient_number: Option<String> = None;
    let mut broadcast_all = false;
    let mut revoke_only = false;

    // Resolve default isolated session path:
    // 1. Check ROSE_LIVE_SESSION_DB environment variable
    // 2. If .sessions/live_status_session.db exists -> use it
    // 3. If live_status_session.db exists in cwd -> use it
    // 4. Default -> .sessions/live_status_session.db
    let default_db_path = if let Ok(env_path) = std::env::var("ROSE_LIVE_SESSION_DB") {
        env_path
    } else if Path::new(".sessions/live_status_session.db").exists() {
        ".sessions/live_status_session.db".to_string()
    } else if Path::new("live_status_session.db").exists() {
        "live_status_session.db".to_string()
    } else {
        ".sessions/live_status_session.db".to_string()
    };

    let mut db_path = default_db_path;
    let mut clean_session = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--phone" | "-p" => {
                if i + 1 < args.len() {
                    phone_number = args[i + 1].clone();
                    i += 1;
                }
            }
            "--recipient" | "-r" => {
                if i + 1 < args.len() {
                    recipient_number = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--broadcast-all" => {
                broadcast_all = true;
            }
            "--revoke" | "--revoke-all" | "--clean-status" => {
                revoke_only = true;
            }
            "--db" | "-s" => {
                if i + 1 < args.len() {
                    db_path = args[i + 1].clone();
                    i += 1;
                }
            }
            "--clean" | "--reset" => {
                clean_session = true;
            }
            "--help" | "-h" => {
                println!("Live Status Broadcast Tester for Rose-Baileys\n");
                println!("Usage: live-status-test [OPTIONS]\n");
                println!("Options:");
                println!(
                    "  -p, --phone <NUMBER>      Phone number to pair (default: 6283162498175)"
                );
                println!(
                    "  -r, --recipient <NUMBER>  Audience recipient contact (default: self only)"
                );
                println!(
                    "      --broadcast-all       EXPLICIT: Broadcast to ALL contacts in phonebook"
                );
                println!("      --revoke              Connect and REVOKE / delete all sent status stories");
                println!("  -s, --db <PATH>           Path to session database (default: .sessions/live_status_session.db)");
                println!("      --clean               Purge existing session before starting");
                println!("  -h, --help                Show this help");
                return Ok(());
            }
            _ => {}
        }
        i += 1;
    }

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("baileys_live_test=info,baileys_core=debug,baileys_signal=warn,baileys_binary=warn,baileys_store=warn"));

    let subscriber = FmtSubscriber::builder()
        .with_env_filter(filter)
        .with_target(false)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    // Ensure session parent directory exists
    if let Some(parent) = Path::new(&db_path).parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }

    if clean_session {
        println!("[*] Cleaning previous session at '{}'...", db_path);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(format!("{}-wal", db_path));
        let _ = std::fs::remove_file(format!("{}-shm", db_path));
    }

    // Safety guardrail: Default to SELF only unless explicitly requested
    let clean_phone = phone_number.replace('+', "").trim().to_string();
    let audience: Option<Vec<String>> = if let Some(r) = recipient_number {
        Some(vec![r])
    } else if broadcast_all {
        None
    } else {
        Some(vec![format!("{}@s.whatsapp.net", clean_phone)])
    };

    println!("============================================================");
    if revoke_only {
        println!("   Rose-Baileys: Emergency Status Story Revoke Mode          ");
    } else {
        println!("   Rose-Baileys: Live WhatsApp Status Broadcast Test Harness  ");
    }
    println!("   Target Phone Number: +{}", phone_number);
    if revoke_only {
        println!("   Action:              Revoking / Deleting all published status stories");
    } else if let Some(ref aud) = audience {
        println!(
            "   Audience Recipients: {:?} (SAFE: Private to Self/Specified)",
            aud
        );
    } else {
        println!("   Audience Recipients: ALL CONTACTS (broadcast-all enabled)");
    }
    println!("   Session Database:    {}", db_path);
    println!("============================================================");

    let store = Arc::new(SqliteStore::open(&db_path)?);

    let config = BaileysConfigBuilder::new()
        .with_browser("Chrome", "Mac OS", "14.4.1")
        .with_phone_pairing(&phone_number)
        .with_dynamic_version(true)
        .build();

    let client = Arc::new(BaileysClient::new(store.clone(), config));
    let mut event_rx = client.events();

    let client_bg = client.clone();
    let _store_bg = store.clone();
    let audience_bg = audience.clone();
    let phone_clone = phone_number.clone();

    // Event monitoring task
    let test_handle = tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            match event {
                BaileysEvent::Connecting => {
                    println!("[*] Connecting to WhatsApp Web Gateway...");
                }
                BaileysEvent::PairingCode { code } => {
                    println!("\n╔══════════════════════════════════════════════════════════════╗");
                    println!(
                        "║               WHATSAPP PAIRING CODE: {:^15}       ║",
                        code
                    );
                    println!("╠══════════════════════════════════════════════════════════════╣");
                    println!("║  1. Open WhatsApp on phone                                   ║");
                    println!("║  2. Go to: Settings > Linked Devices > Link with phone number ║");
                    println!(
                        "║  3. Enter code: {:^15}                              ║",
                        code
                    );
                    println!("╚══════════════════════════════════════════════════════════════╝\n");
                }
                BaileysEvent::Connected { me_jid } => {
                    println!("\n[+] SUCCESS: Authenticated & Connected as: {}", me_jid);

                    if revoke_only {
                        println!("[*] Querying all recent status stories for revocation...");
                        let mut count = 0;
                        if let Ok(stories) = client_bg.get_recent_stories(50, false).await {
                            for s in stories {
                                if s.author_jid.contains(&phone_clone)
                                    || s.author_jid.contains("me")
                                {
                                    println!("[*] Revoking status message ID: {}...", s.msg_id);
                                    let _ = client_bg
                                        .send_revoke_message("status@broadcast", &s.msg_id)
                                        .await;
                                    count += 1;
                                    tokio::time::sleep(Duration::from_millis(1000)).await;
                                }
                            }
                        }
                        println!(
                            "[+] Dispatched revokes for {} status stories. Waiting 5s for ACKs...",
                            count
                        );
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        println!("[+] Revoke operation complete.");
                        std::process::exit(0);
                    }

                    println!("[*] Starting Automated WhatsApp Status Broadcast Test Suite in 3 seconds...\n");
                    tokio::time::sleep(Duration::from_secs(3)).await;

                    // ── Test 1: Plain / Colored Text Status Broadcast ──
                    println!("────────────────────────────────────────────────────────────");
                    println!("[Test 1/4] Publishing Text Status Card to status@broadcast...");
                    println!("  Text: 'Rose-Baileys Status Live Test: Text Card #1 🌟'");
                    println!("  Background: '#1E88E5' (Material Blue)");
                    println!("  Font: Sans-Serif (1)");

                    match client_bg
                        .send_text_status(
                            "Rose-Baileys Status Live Test: Text Card #1 🌟",
                            Some("#1E88E5"),
                            Some(1),
                            audience_bg.clone(),
                        )
                        .await
                    {
                        Ok(msg_id) => {
                            println!("  [✓] SUCCESS: Text Status published! Msg ID: {}", msg_id);
                        }
                        Err(e) => {
                            eprintln!("  [✗] FAILED: Text Status publish failed: {}", e);
                        }
                    }

                    tokio::time::sleep(Duration::from_secs(3)).await;

                    // ── Test 2: Rich URL Link Preview Status Broadcast ──
                    println!("\n────────────────────────────────────────────────────────────");
                    println!(
                        "[Test 2/4] Publishing Rich Link Preview Status to status@broadcast..."
                    );
                    println!("  URL: 'https://github.com/WhiskeySockets/Baileys'");
                    println!("  Title: 'WhiskeySockets/Baileys'");
                    println!("  Desc: 'Lightweight & full-featured WhatsApp Web client'");
                    println!("  Background: '#7B1FA2' (Purple)");

                    match client_bg.send_link_status(
                        "https://github.com/WhiskeySockets/Baileys",
                        Some("WhiskeySockets/Baileys"),
                        Some("Lightweight & full-featured WhatsApp Web client"),
                        Some("Testing Status Link Preview with Rose-Baileys! 🚀 https://github.com/WhiskeySockets/Baileys"),
                        Some("#7B1FA2"),
                        Some(2),
                        None,
                        audience_bg.clone(),
                    ).await {
                        Ok(msg_id) => {
                            println!("  [✓] SUCCESS: Link Status published! Msg ID: {}", msg_id);
                        }
                        Err(e) => {
                            eprintln!("  [✗] FAILED: Link Status publish failed: {}", e);
                        }
                    }

                    tokio::time::sleep(Duration::from_secs(3)).await;

                    // ── Test 3: HD Image Status Broadcast ──
                    println!("\n────────────────────────────────────────────────────────────");
                    println!("[Test 3/4] Publishing HD Image Status to status@broadcast...");

                    // Check for existing image in downloads, or generate fallback JPEG
                    let test_image_path = "downloads/A5CA598A9270996297A28DA9E9A5286E.jpg";
                    let image_bytes = match std::fs::read(test_image_path) {
                        Ok(b) => {
                            println!(
                                "  Using sample image: {} ({} bytes)",
                                test_image_path,
                                b.len()
                            );
                            b
                        }
                        Err(_) => {
                            println!("  Sample file not found, creating test image bytes...");
                            vec![
                                0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00,
                                0x01, 0x01, 0x01, 0x00, 0x48, 0x00, 0x48, 0x00, 0x00, 0xFF, 0xDB,
                                0x00, 0x43, 0x00, 0x08, 0x06, 0x06, 0x07, 0x06, 0x05, 0x08, 0x07,
                                0x07, 0x07, 0x09, 0x09, 0x08, 0x0A, 0x0C, 0x14, 0x0D, 0x0C, 0x0B,
                                0x0B, 0x0C, 0x19, 0x12, 0x13, 0x0F, 0x14, 0x1D, 0x1A, 0x1F, 0x1E,
                                0x1D, 0x1A, 0x1C, 0x1C, 0x20, 0x24, 0x2E, 0x27, 0x20, 0x22, 0x2C,
                                0x23, 0x1C, 0x1C, 0x28, 0x37, 0x29, 0x2C, 0x30, 0x31, 0x34, 0x34,
                                0x34, 0x1F, 0x27, 0x39, 0x3D, 0x38, 0x32, 0x3C, 0x2E, 0x33, 0x34,
                                0x32, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01, 0x01,
                                0x01, 0x11, 0x00, 0xFF, 0xC4, 0x00, 0x1F, 0x00, 0x00, 0x01, 0x05,
                                0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
                                0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
                                0x09, 0x0A, 0x0B, 0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00,
                                0x3F, 0x00, 0xBF, 0x00, 0xFF, 0xD9,
                            ]
                        }
                    };

                    match client_bg
                        .send_status_image(
                            &image_bytes,
                            Some("Rose-Baileys HD Image Story Live Test 📷"),
                            Some("image/jpeg"),
                            audience_bg.clone(),
                        )
                        .await
                    {
                        Ok(msg_id) => {
                            println!("  [✓] SUCCESS: Image Status published! Msg ID: {}", msg_id);
                        }
                        Err(e) => {
                            eprintln!("  [✗] FAILED: Image Status publish failed: {}", e);
                        }
                    }

                    tokio::time::sleep(Duration::from_secs(3)).await;

                    // ── Test 4: Stories Store Cache & Status Read Verification ──
                    println!("\n────────────────────────────────────────────────────────────");
                    println!("[Test 4/4] Verifying Stories Persistence and Cache...");
                    match client_bg.get_recent_stories(10, false).await {
                        Ok(stories) => {
                            println!(
                                "  [✓] Successfully queried stories cache ({} records found):",
                                stories.len()
                            );
                            for (idx, s) in stories.iter().enumerate() {
                                println!(
                                    "    {}. [{}] ID: {} | Author: {} | Viewed: {}",
                                    idx + 1,
                                    s.message_type,
                                    s.msg_id,
                                    s.author_jid,
                                    s.viewed
                                );
                                if let Some(ref c) = s.content {
                                    println!("       Content: {}", c);
                                }
                                if let Some(ref cap) = s.caption {
                                    println!("       Caption: {}", cap);
                                }
                            }
                        }
                        Err(e) => eprintln!("  [✗] Failed to fetch stories from store: {}", e),
                    }

                    println!("\n════════════════════════════════════════════════════════════");
                    println!("   STATUS BROADCAST LIVE TEST SUITE EXECUTION COMPLETE     ");
                    println!("   Monitoring incoming receipts for 10 seconds...          ");
                    println!("════════════════════════════════════════════════════════════\n");

                    tokio::time::sleep(Duration::from_secs(10)).await;
                    println!("[+] Test suite completed successfully.");
                    std::process::exit(0);
                }
                BaileysEvent::Receipt {
                    key_id,
                    remote_jid,
                    status,
                } => {
                    println!(
                        "[RECEIPT] Message: {} | To: {} | Status: {}",
                        key_id, remote_jid, status
                    );
                }
                BaileysEvent::Message { message } => {
                    if let Some(ref key) = message.key {
                        let remote = key.remote_jid.as_deref().unwrap_or("");
                        if remote == "status@broadcast" {
                            println!(
                                "[INCOMING STORY] Story update received from: {:?}",
                                key.participant
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    });

    println!("[*] Connecting client to WhatsApp Web Gateway...");
    client.connect().await?;

    let _ = test_handle.await;
    Ok(())
}
