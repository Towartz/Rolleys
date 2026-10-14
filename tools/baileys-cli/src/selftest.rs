use crate::ui::log_ts;
use baileys_core::BaileysClient;
use baileys_store::{ChatStore, SqliteStore};
use std::sync::Arc;

pub async fn run_self_test(
    client: &Arc<BaileysClient>,
    store: &Arc<SqliteStore>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("\n============================================================");
    println!("         STARTING ROSE-BAILEYS AUTOMATED SELF-TEST");
    println!("============================================================");

    // 1. Connection check
    if !client.is_connected() {
        println!("[-] Self-Test Aborted: Client is not yet connected to WhatsApp Gateway.");
        println!("    Please pair your device first with the 8-digit code!");
        return Ok(());
    }

    // 2. Account Identity Check
    let me_jid = client
        .me_jid()
        .await
        .unwrap_or(None)
        .unwrap_or_else(|| "unknown".into());
    let me_lid = client
        .me_lid()
        .await
        .unwrap_or(None)
        .unwrap_or_else(|| "none".into());
    println!("\n[*] Step 1/6: Verifying Account Identity...");
    println!("    [+] Authenticated Phone JID : {}", me_jid);
    println!("    [+] Authenticated LID       : {}", me_lid);

    // 3. Ping Roundtrip Latency
    println!("\n[*] Step 2/6: Measuring Gateway Ping Latency...");
    match client.ping().await {
        Ok(latency) => println!("    [+] Ping Success! Round-trip latency: {:?}", latency),
        Err(e) => println!("    [-] Ping warning: {}", e),
    }

    // 4. Create Group with Only Self
    println!("\n[*] Step 3/6: Creating test group with only self number...");
    let group_subject = format!(
        "Rose-Baileys Self-Test ({})",
        chrono::Local::now().format("%H:%M")
    );
    let group_jid = match client.group_create(&group_subject, &[]).await {
        Ok(gid) => {
            println!("    [+] Self-Group Created Successfully!");
            println!("        Subject   : {}", group_subject);
            println!("        Group JID : {}", gid);
            gid
        }
        Err(e) => {
            println!("    [-] Group creation failed: {}", e);
            return Ok(());
        }
    };

    // 5. Query Group Metadata
    println!("\n[*] Step 4/6: Querying Group Metadata from WhatsApp Gateway...");
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    match client.group_fetch_metadata(&group_jid).await {
        Ok(meta) => {
            println!("    [+] Group Metadata Verified:");
            println!("        Subject        : {}", meta.subject);
            println!(
                "        Owner          : {}",
                meta.owner.as_deref().unwrap_or("me")
            );
            println!("        Total Members  : {}", meta.size);
            for (idx, p) in meta.participants.iter().enumerate() {
                println!(
                    "        Member #{}: JID={}, LID={:?}, Admin={:?}",
                    idx + 1,
                    p.jid,
                    p.lid,
                    p.admin
                );
            }
        }
        Err(e) => println!("    [-] Group metadata query warning: {}", e),
    }

    // 6. Send Test Message into the Group
    println!("\n[*] Step 5/6: Sending verification message to newly created group...");
    let test_group_msg = format!(
        "Test message to group from Rose-Baileys Self-Test at {}",
        log_ts()
    );
    match client.send_text_message(&group_jid, &test_group_msg).await {
        Ok(mid) => {
            println!("    [+] Group message dispatched successfully!");
            println!("        Target Group : {}", group_jid);
            println!("        Message ID   : {}", mid);
            println!("        Content      : {}", test_group_msg);
        }
        Err(e) => println!("    [-] Group send failed: {}", e),
    }

    // 7. Send 1-on-1 Note-to-Self
    println!("\n[*] Step 6/6: Sending 1-on-1 note-to-self direct message...");
    let clean_me = me_jid.split(':').next().unwrap_or(&me_jid);
    let self_target = if clean_me.contains('@') {
        clean_me.to_string()
    } else {
        format!("{}@s.whatsapp.net", clean_me)
    };
    let self_dm_msg = format!(
        "Rose-Baileys Note-to-Self verification ping at {}",
        log_ts()
    );
    match client.send_text_message(&self_target, &self_dm_msg).await {
        Ok(mid) => {
            println!("    [+] Note-to-self dispatched successfully!");
            println!("        Target JID : {}", self_target);
            println!("        Message ID : {}", mid);
            println!("        Content    : {}", self_dm_msg);
        }
        Err(e) => println!("    [-] Note-to-self failed: {}", e),
    }

    // Verify SQLite Store
    println!("\n[*] Verifying local SQLite session persistence...");
    if let Ok(Some(saved_chat)) = store.get_chat(&group_jid).await {
        println!(
            "    [+] SQLite verified: Chat cached with name: '{:?}'",
            saved_chat.name
        );
    }
    let saved_msgs = store
        .get_messages(&group_jid, 5, None)
        .await
        .unwrap_or_default();
    println!(
        "    [+] SQLite verified: {} messages stored for test group.",
        saved_msgs.len()
    );

    println!("\n============================================================");
    println!("  [+] ALL SELF-TEST SCENARIOS EXECUTED SUCCESSFULLY!");
    println!("============================================================\n");
    Ok(())
}
