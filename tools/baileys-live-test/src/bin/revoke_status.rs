use baileys_core::{BaileysClient, BaileysConfigBuilder, BaileysEvent};
use baileys_proto::proto::message::protocol_message::Type as ProtocolType;
use baileys_proto::proto::message::ProtocolMessage;
use baileys_proto::proto::{Message, MessageKey};
use baileys_store::SqliteStore;
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = ".sessions/live_status_session.db".to_string();

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("baileys_live_test=info,baileys_core=debug,baileys_signal=warn,baileys_binary=warn,baileys_store=warn"));

    let subscriber = FmtSubscriber::builder()
        .with_env_filter(filter)
        .with_target(false)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    println!("============================================================");
    println!("   EMERGENCY STATUS REVOKE RUNNER (with edit=\"7\" stanza)   ");
    println!("   Connecting to WhatsApp Gateway to force-revoke stories... ");
    println!("============================================================");

    let store = Arc::new(SqliteStore::open(&db_path)?);

    let config = BaileysConfigBuilder::new()
        .with_browser("Chrome", "Mac OS", "14.4.1")
        .with_dynamic_version(true)
        .build();

    let client = Arc::new(BaileysClient::new(store.clone(), config));
    let mut event_rx = client.events();
    let client_bg = client.clone();

    let runner_handle = tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            match event {
                BaileysEvent::Connected { ref me_jid } => {
                    println!("[+] Connected as: {}", me_jid);
                    println!("[*] Sending WhatsApp Revoke Stanzas (with edit=\"7\")...");

                    let targets = vec![
                        "3EB0816EBF8FF37DD0D1",
                        "3EB072952F106C3ADD8C",
                        "3EB0D1104B0153FC93DC",
                    ];

                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as i64;

                    for id in &targets {
                        // Pass 1: Standard delete with participant = None
                        println!(
                            "[*] [Pass 1] Revoking status message ID: {} (participant = None)...",
                            id
                        );
                        match client_bg.send_revoke_message("status@broadcast", id).await {
                            Ok(ack) => println!("  [✓] Sent Pass 1 revoke! Stanza ID: {}", ack),
                            Err(e) => eprintln!("  [-] Pass 1 failed for {}: {}", id, e),
                        }
                        tokio::time::sleep(Duration::from_millis(500)).await;

                        // Pass 2: Delete with participant = me_jid explicitly
                        println!(
                            "[*] [Pass 2] Revoking status message ID: {} (participant = {})...",
                            id, me_jid
                        );
                        let proto_msg = Message {
                            protocol_message: Some(Box::new(ProtocolMessage {
                                key: Some(MessageKey {
                                    remote_jid: Some("status@broadcast".to_string()),
                                    from_me: Some(true),
                                    id: Some(id.to_string()),
                                    participant: Some(me_jid.clone()),
                                }),
                                r#type: Some(ProtocolType::Revoke as i32),
                                timestamp_ms: Some(now_ms),
                                ..Default::default()
                            })),
                            ..Default::default()
                        };
                        match client_bg.send_message("status@broadcast", proto_msg).await {
                            Ok(ack) => println!("  [✓] Sent Pass 2 revoke! Stanza ID: {}", ack),
                            Err(e) => eprintln!("  [-] Pass 2 failed for {}: {}", id, e),
                        }
                        tokio::time::sleep(Duration::from_millis(500)).await;

                        // Pass 3: Clean phone number without companion suffix (e.g. 6283162498175@s.whatsapp.net)
                        let clean_phone_jid =
                            me_jid.split(':').next().unwrap_or(me_jid).to_string();
                        let clean_phone_jid = if clean_phone_jid.contains('@') {
                            clean_phone_jid
                        } else {
                            format!("{}@s.whatsapp.net", clean_phone_jid)
                        };
                        if clean_phone_jid != *me_jid {
                            println!(
                                "[*] [Pass 3] Revoking status message ID: {} (participant = {})...",
                                id, clean_phone_jid
                            );
                            let proto_msg = Message {
                                protocol_message: Some(Box::new(ProtocolMessage {
                                    key: Some(MessageKey {
                                        remote_jid: Some("status@broadcast".to_string()),
                                        from_me: Some(true),
                                        id: Some(id.to_string()),
                                        participant: Some(clean_phone_jid),
                                    }),
                                    r#type: Some(ProtocolType::Revoke as i32),
                                    timestamp_ms: Some(now_ms),
                                    ..Default::default()
                                })),
                                ..Default::default()
                            };
                            let _ = client_bg.send_message("status@broadcast", proto_msg).await;
                            tokio::time::sleep(Duration::from_millis(500)).await;
                        }
                    }

                    println!("\n[+] All 3-pass revocation stanzas dispatched! Waiting 8 seconds for server delivery...");
                    tokio::time::sleep(Duration::from_secs(8)).await;
                    println!("[+] Emergency revocation routine finished successfully.");
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
                _ => {}
            }
        }
    });

    println!("[*] Connecting client to WhatsApp Web Gateway...");
    client.connect().await?;

    let _ = runner_handle.await;
    Ok(())
}
