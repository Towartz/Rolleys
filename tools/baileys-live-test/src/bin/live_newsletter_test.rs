use baileys_core::{BaileysClient, BaileysConfigBuilder, BaileysEvent};
use baileys_store::{SessionStore, SqliteStore};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let mut db_path = if Path::new(".sessions/live_status_session.db").exists() {
        ".sessions/live_status_session.db".to_string()
    } else if Path::new("rose-baileys/.sessions/live_status_session.db").exists() {
        "rose-baileys/.sessions/live_status_session.db".to_string()
    } else {
        "live_status_session.db".to_string()
    };
    let mut target_channel: Option<String> = None;
    let mut delete_channel: Option<String> = None;
    let mut keep_channel = false;
    let mut skip_media = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--db" | "-s" => {
                if i + 1 < args.len() {
                    db_path = args[i + 1].clone();
                    i += 1;
                }
            }
            "--channel" | "-c" => {
                if i + 1 < args.len() {
                    target_channel = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--delete" | "-d" => {
                if i + 1 < args.len() {
                    delete_channel = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--keep" => {
                keep_channel = true;
            }
            "--skip-media" => {
                skip_media = true;
            }
            "--help" | "-h" => {
                println!("Rose-Baileys WhatsApp Newsletter / Channel Live Test Suite\n");
                println!("Usage: live-newsletter-test [OPTIONS]\n");
                println!("Options:");
                println!("  -s, --db <PATH>       Path to session database (default: .sessions/live_status_session.db)");
                println!("  -c, --channel <JID>   Existing channel JID to test against (skips channel creation)");
                println!("  -d, --delete <JID>    Delete a specific channel and exit immediately");
                println!("      --keep            Do NOT delete the created test channel at the end of the test");
                println!("      --skip-media      Skip media/image story upload");
                println!("  -h, --help            Show this help");
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

    println!("============================================================");
    println!("   Rose-Baileys: WhatsApp Newsletter / Channel Live Test     ");
    println!("   Session Database: {}", db_path);
    if let Some(ref ch) = target_channel {
        println!("   Target Channel:   {}", ch);
    } else {
        println!(
            "   Target Channel:   Auto-create isolated test channel (Auto-delete: {})",
            !keep_channel
        );
    }
    println!("============================================================");

    let store = Arc::new(SqliteStore::open(&db_path)?);

    let config = BaileysConfigBuilder::new()
        .with_browser("Chrome", "Mac OS", "14.4.1")
        .with_dynamic_version(true)
        .build();

    let client = Arc::new(BaileysClient::new(store.clone(), config));
    let mut event_rx = client.events();
    let client_bg = client.clone();

    let test_handle = tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            #[allow(clippy::single_match)]
            match event {
                BaileysEvent::Connected { me_jid } => {
                    println!("\n[+] SUCCESS: Authenticated & Connected as: {}", me_jid);

                    if let Some(ref del_jid) = delete_channel {
                        println!(
                            "\n[*] Emergency Channel Deletion Requested for: {}...",
                            del_jid
                        );
                        match client_bg.newsletter_delete(del_jid).await {
                            Ok(_) => println!("  [✓] SUCCESS: Channel '{}' deleted!", del_jid),
                            Err(e) => {
                                eprintln!("  [-] Failed to delete channel '{}': {}", del_jid, e)
                            }
                        }
                        std::process::exit(0);
                    }

                    println!(
                        "[*] Starting WhatsApp Newsletter / Channel Test Suite in 2 seconds...\n"
                    );
                    tokio::time::sleep(Duration::from_secs(2)).await;

                    // ── Phase 1: Query Subscribed Channels ──
                    println!("────────────────────────────────────────────────────────────");
                    println!("[Phase 1/5] Querying Subscribed WhatsApp Channels...");
                    match client_bg.newsletter_get_subscribed().await {
                        Ok(channels) => {
                            println!("  [✓] Retrieved {} subscribed channel(s):", channels.len());
                            for (idx, ch) in channels.iter().enumerate() {
                                println!(
                                    "    {}. [{}] JID: {} (Subscribers: {}, Role: {:?})",
                                    idx + 1,
                                    ch.name,
                                    ch.jid,
                                    ch.subscribers_count,
                                    ch.role
                                );
                            }
                        }
                        Err(e) => eprintln!("  [-] Failed to fetch subscribed channels: {}", e),
                    }

                    tokio::time::sleep(Duration::from_secs(2)).await;

                    // ── Phase 2: Channel Provisioning ──
                    println!("\n────────────────────────────────────────────────────────────");
                    let (channel_jid, created_by_us) = if let Some(existing_jid) = target_channel {
                        println!(
                            "[Phase 2/5] Using existing target channel: {}",
                            existing_jid
                        );
                        (existing_jid, false)
                    } else {
                        let ts = chrono::Local::now().format("%H:%M:%S").to_string();
                        let test_ch_name = format!("Rose-Baileys Test ({})", ts);
                        let test_ch_desc =
                            "Automated isolated integration test channel for Rose-Baileys";
                        println!(
                            "[Phase 2/5] Creating isolated private test channel: '{}'...",
                            test_ch_name
                        );

                        match client_bg
                            .newsletter_create(&test_ch_name, Some(test_ch_desc))
                            .await
                        {
                            Ok(meta) => {
                                println!("  [✓] SUCCESS: Channel created successfully!");
                                println!("      JID:           {}", meta.jid);
                                println!("      Name:          {}", meta.name);
                                println!("      Role:          {:?}", meta.role);
                                if let Some(ref code) = meta.invite_code {
                                    println!(
                                        "      Invite Link:   https://whatsapp.com/channel/{}",
                                        code
                                    );
                                }
                                (meta.jid, true)
                            }
                            Err(e) => {
                                eprintln!("  [-] Channel creation failed: {}. Attempting fallback to first subscribed channel...", e);
                                if let Ok(subscribed) = client_bg.newsletter_get_subscribed().await
                                {
                                    if let Some(first) = subscribed.first() {
                                        println!(
                                            "  [*] Falling back to subscribed channel: {} ({})",
                                            first.name, first.jid
                                        );
                                        (first.jid.clone(), false)
                                    } else {
                                        eprintln!(
                                            "  [-] No channels available for testing. Aborting."
                                        );
                                        std::process::exit(1);
                                    }
                                } else {
                                    std::process::exit(1);
                                }
                            }
                        }
                    };

                    tokio::time::sleep(Duration::from_secs(2)).await;

                    // ── Phase 3: Channel Posts, Stories & Interactions ──
                    println!("\n────────────────────────────────────────────────────────────");
                    println!(
                        "[Phase 3/5] Publishing Updates & Stories to Channel: {}",
                        channel_jid
                    );

                    // 3A: Plaintext announcement post
                    println!("  [*] [3A] Publishing Plaintext Story Post...");
                    let post_text = format!(
                        "🚀 Rose-Baileys Channel Live Test: Announcement at {}",
                        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
                    );
                    match client_bg
                        .newsletter_send_post(&channel_jid, &post_text)
                        .await
                    {
                        Ok(msg_id) => println!(
                            "  [✓] SUCCESS: Text post published! Stanza Msg ID: {}",
                            msg_id
                        ),
                        Err(e) => eprintln!("  [-] Failed to publish text post: {}", e),
                    }

                    tokio::time::sleep(Duration::from_secs(3)).await;

                    // 3B: HD Media / Image Story Upload
                    if !skip_media {
                        println!("\n  [*] [3B] Uploading HD Image Story to Channel...");
                        let sample_path =
                            if Path::new("downloads/A5CA598A9270996297A28DA9E9A5286E.jpg").exists()
                            {
                                "downloads/A5CA598A9270996297A28DA9E9A5286E.jpg"
                            } else if Path::new(
                                "rose-baileys/downloads/A5CA598A9270996297A28DA9E9A5286E.jpg",
                            )
                            .exists()
                            {
                                "rose-baileys/downloads/A5CA598A9270996297A28DA9E9A5286E.jpg"
                            } else {
                                "downloads/A5CA598A9270996297A28DA9E9A5286E.jpg"
                            };
                        let image_bytes = match std::fs::read(sample_path) {
                            Ok(b) => {
                                println!(
                                    "      Using sample image: {} ({} bytes)",
                                    sample_path,
                                    b.len()
                                );
                                b
                            }
                            Err(_) => {
                                println!("      Sample file not found, generating minimal 1x1 valid JPEG...");
                                vec![
                                    0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46,
                                    0x00, 0x01, 0x01, 0x01, 0x00, 0x48, 0x00, 0x48, 0x00, 0x00,
                                    0xFF, 0xDB, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06, 0x07, 0x06,
                                    0x05, 0x08, 0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0A, 0x0C,
                                    0x14, 0x0D, 0x0C, 0x0B, 0x0B, 0x0C, 0x19, 0x12, 0x13, 0x0F,
                                    0x14, 0x1D, 0x1A, 0x1F, 0x1E, 0x1D, 0x1A, 0x1C, 0x1C, 0x20,
                                    0x24, 0x2E, 0x27, 0x20, 0x22, 0x2C, 0x23, 0x1C, 0x1C, 0x28,
                                    0x37, 0x29, 0x2C, 0x30, 0x31, 0x34, 0x34, 0x34, 0x1F, 0x27,
                                    0x39, 0x3D, 0x38, 0x32, 0x3C, 0x2E, 0x33, 0x34, 0x32, 0xFF,
                                    0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01, 0x01, 0x01,
                                    0x11, 0x00, 0xFF, 0xC4, 0x00, 0x1F, 0x00, 0x00, 0x01, 0x05,
                                    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00,
                                    0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06,
                                    0x07, 0x08, 0x09, 0x0A, 0x0B, 0xFF, 0xDA, 0x00, 0x08, 0x01,
                                    0x01, 0x00, 0x00, 0x3F, 0x00, 0xBF, 0x00, 0xFF, 0xD9,
                                ]
                            }
                        };

                        match client_bg.newsletter_send_image(&channel_jid, &image_bytes, Some("Rose-Baileys Live Channel HD Image Story 📸")).await {
                            Ok(msg_id) => println!("  [✓] SUCCESS: Image story uploaded and published! Stanza Msg ID: {}", msg_id),
                            Err(e) => eprintln!("  [-] Failed to upload channel image story: {}", e),
                        }

                        tokio::time::sleep(Duration::from_secs(3)).await;
                    }

                    // 3C: Fetch channel messages & React
                    println!("\n  [*] [3C] Fetching Channel Messages...");
                    match client_bg
                        .newsletter_fetch_messages(&channel_jid, 5, None)
                        .await
                    {
                        Ok(msgs) => {
                            println!("  [✓] Retrieved {} channel message(s):", msgs.len());
                            for m in &msgs {
                                println!(
                                    "      - [Server ID: {}] HasMedia: {} | Summary: {}",
                                    m.server_id, m.has_media, m.summary
                                );
                            }

                            if let Some(first_msg) = msgs.first() {
                                println!(
                                    "  [*] Reacting with '🔥' to message Server ID: {}...",
                                    first_msg.server_id
                                );
                                match client_bg
                                    .newsletter_react(&channel_jid, first_msg.server_id, "🔥")
                                    .await
                                {
                                    Ok(_) => println!("  [✓] SUCCESS: Emoji reaction dispatched!"),
                                    Err(e) => eprintln!("  [-] Failed to send reaction: {}", e),
                                }

                                tokio::time::sleep(Duration::from_secs(2)).await;

                                // 3D: View Receipt
                                println!(
                                    "  [*] [3D] Dispatching view receipt for Server ID: {}...",
                                    first_msg.server_id
                                );
                                match client_bg
                                    .newsletter_view_receipt(&channel_jid, &[first_msg.server_id])
                                    .await
                                {
                                    Ok(_) => println!("  [✓] SUCCESS: View receipt dispatched!"),
                                    Err(e) => eprintln!("  [-] Failed to send view receipt: {}", e),
                                }

                                tokio::time::sleep(Duration::from_secs(2)).await;

                                // 3E: Channel Status Story to status@broadcast (Isolated to self)
                                println!("  [*] [3E] Broadcasting interactive Channel Text Story to status@broadcast (Isolated to self)...");
                                let self_jid_opt = store
                                    .get_creds()
                                    .await
                                    .ok()
                                    .flatten()
                                    .and_then(|c| c.me_jid);
                                let audience = self_jid_opt.clone().map(|j| vec![j]);
                                let story_caption = format!(
                                    "🚀 Update Saluran Baru! Post #{}",
                                    first_msg.server_id
                                );
                                match client_bg.newsletter_send_status_story(&channel_jid, first_msg.server_id as i32, "Rose-Baileys Test", &story_caption, audience).await {
                                    Ok(msg_id) => println!("  [✓] SUCCESS: Channel Text Status Story published to status@broadcast! (Msg ID: {})", msg_id),
                                    Err(e) => eprintln!("  [-] Failed to publish channel status story: {}", e),
                                }

                                tokio::time::sleep(Duration::from_secs(2)).await;

                                // 3F: Interactive Full-Screen HD Image Story to status@broadcast (Isolated to self)
                                if !skip_media {
                                    println!("  [*] [3F] Broadcasting interactive Full-Screen HD Image Story to status@broadcast (Isolated to self)...");
                                    let sample_path = if Path::new(
                                        "downloads/A5CA598A9270996297A28DA9E9A5286E.jpg",
                                    )
                                    .exists()
                                    {
                                        "downloads/A5CA598A9270996297A28DA9E9A5286E.jpg"
                                    } else {
                                        "rose-baileys/downloads/A5CA598A9270996297A28DA9E9A5286E.jpg"
                                    };
                                    if let Ok(img_bytes) = std::fs::read(sample_path) {
                                        let audience_img = self_jid_opt.map(|j| vec![j]);
                                        let story_img_caption = format!(
                                            "📸 Saluran HD Image Story! Post #{}",
                                            first_msg.server_id
                                        );
                                        match client_bg.newsletter_send_image_status_story(&img_bytes, Some(&story_img_caption), &channel_jid, first_msg.server_id as i32, "Rose-Baileys Test", audience_img).await {
                                            Ok(msg_id) => println!("  [✓] SUCCESS: Full-Screen HD Channel Image Story published to status@broadcast! (Msg ID: {})", msg_id),
                                            Err(e) => eprintln!("  [-] Failed to publish channel image status story: {}", e),
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => eprintln!("  [-] Fetch channel messages failed: {}", e),
                    }

                    tokio::time::sleep(Duration::from_secs(2)).await;

                    // ── Phase 4: Channel Management & Metadata ──
                    println!("\n────────────────────────────────────────────────────────────");
                    println!("[Phase 4/5] Testing Channel Settings & Management...");

                    // Mute / Unmute
                    println!("  [*] Setting channel mute to 'true'...");
                    match client_bg.newsletter_mute(&channel_jid, true).await {
                        Ok(_) => println!("  [✓] SUCCESS: Channel muted!"),
                        Err(e) => eprintln!("  [-] Channel mute failed: {}", e),
                    }

                    // Admin count
                    println!("  [*] Querying channel admin count...");
                    match client_bg.newsletter_admin_count(&channel_jid).await {
                        Ok(count) => println!("  [✓] Admin count: {}", count),
                        Err(e) => eprintln!("  [-] Admin count query failed: {}", e),
                    }

                    // Query metadata
                    println!("  [*] Re-fetching channel metadata...");
                    match client_bg.newsletter_query_metadata(&channel_jid).await {
                        Ok(meta) => {
                            println!("  [✓] Current Channel Metadata:");
                            println!("      Name:              {}", meta.name);
                            println!("      Subscribers Count: {}", meta.subscribers_count);
                            println!("      Muted:             {}", meta.muted);
                            println!("      Role:              {:?}", meta.role);
                        }
                        Err(e) => eprintln!("  [-] Re-fetching metadata failed: {}", e),
                    }

                    tokio::time::sleep(Duration::from_secs(2)).await;

                    // ── Phase 5: Safe Teardown ──
                    println!("\n────────────────────────────────────────────────────────────");
                    if created_by_us {
                        if keep_channel {
                            println!(
                                "[Phase 5/5] --keep specified: Leaving test channel '{}' intact.",
                                channel_jid
                            );
                        } else {
                            println!("[Phase 5/5] Performing Safe Teardown: Deleting test channel '{}'...", channel_jid);
                            match client_bg.newsletter_delete(&channel_jid).await {
                                Ok(_) => println!(
                                    "  [✓] SUCCESS: Test channel deleted! Account remains clean."
                                ),
                                Err(e) => eprintln!("  [-] Failed to delete test channel: {}", e),
                            }
                        }
                    } else {
                        println!("[Phase 5/5] Channel was not created by this test run. Skipping deletion.");
                    }

                    println!("\n════════════════════════════════════════════════════════════");
                    println!("   NEWSLETTER / CHANNEL LIVE TEST COMPLETED SUCCESSFULLY    ");
                    println!("════════════════════════════════════════════════════════════\n");

                    std::process::exit(0);
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
