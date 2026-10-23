//! Domain submodule for connection operations on BaileysClient.

use super::*;

impl BaileysClient {
    /// Connects to the WhatsApp Web gateway, managing initial pairing, automatic post-pairing
    /// reconnect, session authentication, and background keepalive.
    pub async fn connect(&self) -> Result<(), CoreError> {
        if self
            .is_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            debug!(
                "Connection supervisor is already running; skipping redundant supervisor spawn."
            );
            if self.is_connected.load(Ordering::SeqCst) {
                if let Ok(Some(creds)) = self.store.get_creds().await {
                    if let Some(ref me) = creds.me_jid {
                        self.events
                            .emit(BaileysEvent::Connected { me_jid: me.clone() });
                    }
                }
            } else {
                self.events.emit(BaileysEvent::Connecting);
            }
            return Ok(());
        }

        self.events.emit(BaileysEvent::Connecting);
        self.set_connection_state(crate::state::ConnectionState::Connecting);

        let creds = match self
            .store
            .get_creds()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?
        {
            Some(c) => c,
            None => {
                let fresh = AuthCreds::init();
                self.store
                    .save_creds(&fresh)
                    .await
                    .map_err(|e| CoreError::Store(e.to_string()))?;
                fresh
            }
        };

        // Support mock URL for offline unit test suites
        if self.config.wa_websocket_url.starts_with("mock://") {
            info!("Mock URL detected, running simulated offline handshake...");
            if creds.me_jid.is_none() {
                if let Some(ref _phone) = self.config.phone_number_pairing {
                    self.events.emit(BaileysEvent::PairingCode {
                        code: "1234-ABCD".into(),
                    });
                } else {
                    let qr = format!(
                        "1@MOCK_REF,{},{},{}",
                        BASE64_STANDARD.encode(&creds.noise_key.public),
                        BASE64_STANDARD.encode(&creds.signed_identity_key.public),
                        BASE64_STANDARD.encode(&creds.adv_secret_key),
                    );
                    self.events.emit(BaileysEvent::QrCode { code: qr });
                }
            } else if let Some(me) = &creds.me_jid {
                self.is_connected.store(true, Ordering::SeqCst);
                self.events
                    .emit(BaileysEvent::Connected { me_jid: me.clone() });
            }
            return Ok(());
        }

        let client = self.clone();

        // Spawn background connection supervisor
        tokio::spawn(async move {
            let store_clone = &client.store;
            let config_clone = client.config.clone();
            let events_clone = &client.events;
            let is_connected_clone = &client.is_connected;
            let is_running_clone = &client.is_running;
            let socket_holder = &client.socket;
            let signal_clone = &client.signal;
            let pending_iq_clone = &client.pending_iq;
            let recent_messages_clone = &client.recent_messages;
            let auto_download_clone = &client.auto_download;
            let active_calls_clone = &client.active_calls;
            let companion_state_holder = &client.companion_state;
            let last_qr_holder = &client.last_qr;

            let mut is_first_connection = true;
            let mut consecutive_connect_failures: u32 = 0;

            while is_running_clone.load(Ordering::SeqCst) {
                // messaging-history.status state lives per socket, as in Baileys.
                let history_status = Arc::new(tokio::sync::Mutex::new(
                    crate::history::HistorySyncStatusTracker::default(),
                ));
                let mut creds = match store_clone.get_creds().await {
                    Ok(Some(c)) => c,
                    Ok(None) => {
                        let fresh = AuthCreds::init();
                        let _ = store_clone.save_creds(&fresh).await;
                        fresh
                    }
                    Err(e) => {
                        error!("Failed to read credentials from store: {}", e);
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        continue;
                    }
                };

                if !is_first_connection {
                    events_clone.emit(BaileysEvent::Connecting);
                    client.set_connection_state(crate::state::ConnectionState::Connecting);
                }

                // 1. Resolve dynamic WhatsApp Web version if enabled
                let mut active_config = config_clone.clone();
                if active_config.dynamic_version
                    && !active_config.wa_websocket_url.starts_with("mock://")
                {
                    let resolved_version =
                        crate::version::resolve_wa_version(store_clone.as_ref(), &active_config)
                            .await;
                    active_config.app_version = resolved_version;
                }

                client.set_connection_state(crate::state::ConnectionState::Handshaking);

                // Establish live WebSocket connection and complete Noise_XX handshake
                let (raw_socket, mut incoming_rx) =
                    match WhatsAppSocket::connect(&creds, &active_config).await {
                        Ok(s) => {
                            consecutive_connect_failures = 0;
                            s
                        }
                        Err(e) => {
                            consecutive_connect_failures += 1;
                            let backoff =
                                (3u64 * (1 << (consecutive_connect_failures - 1).min(3))).min(30);
                            let jitter = rand::random::<u64>() % 1000;
                            let delay = Duration::from_millis(backoff * 1000 + jitter);
                            client.set_connection_state(
                                crate::state::ConnectionState::Reconnecting {
                                    attempt: consecutive_connect_failures,
                                    next_retry_millis: delay.as_millis() as u64,
                                },
                            );
                            client.metrics().record_reconnect();
                            error!(
                            "Failed to connect to WhatsApp Web Gateway: {}. Retrying in {:.1}s...",
                            e,
                            delay.as_secs_f32()
                        );
                            if !is_running_clone.load(Ordering::SeqCst) {
                                break;
                            }
                            tokio::time::sleep(delay).await;
                            continue;
                        }
                    };

                let socket = Arc::new(raw_socket);
                {
                    let mut lock = socket_holder.lock().await;
                    *lock = Some(Arc::clone(&socket));
                }

                // 2. Stanza processing loop for the current connection
                let mut ping_interval = tokio::time::interval(Duration::from_secs(15));
                ping_interval.tick().await;

                let mut session_terminated = false;
                let mut me_lid: Option<String> = creds.me_lid.clone();
                let mut reconnect_delay = Duration::from_secs(3);
                let mut msg_retries: HashMap<String, u32> = HashMap::new();
                let mut last_ping_sent_at: Option<tokio::time::Instant> = None;
                let mut ticks_since_prekey_check: u32 = 0;
                let mut ticks_since_wam_flush: u32 = 0;
                let mut pairing_code_sent = false;

                while is_running_clone.load(Ordering::SeqCst) && !session_terminated {
                    tokio::select! {
                        _ = ping_interval.tick() => {
                            // Dead socket check: only abort if ping has gone unanswered for >= 45s
                            if let Some(sent_at) = last_ping_sent_at {
                                if sent_at.elapsed() >= Duration::from_secs(45) {
                                    warn!(
                                        "Dead socket detected: WhatsApp server failed to respond within {:.1}s to keepalive ping. Aborting zombie TCP connection...",
                                        sent_at.elapsed().as_secs_f32()
                                    );
                                    session_terminated = true;
                                    continue;
                                }
                            }

                            // Auto-replenish pre-keys if low (checked every ~60s)
                            ticks_since_prekey_check += 1;
                            if ticks_since_prekey_check >= 4 {
                                ticks_since_prekey_check = 0;
                                if is_connected_clone.load(Ordering::SeqCst) {
                                    if let Ok(count) = store_clone.get_key_count("pre-key").await {
                                        if crate::registration::needs_prekey_upload(count) {
                                            info!(
                                                "Available one-time prekeys low (count: {} < {}). Automatically uploading 50 new prekeys to ensure uninterrupted message delivery...",
                                                count,
                                                crate::registration::MIN_UNUSED_PREKEYS
                                            );
                                            if let Err(e) = upload_pre_keys(&mut creds, store_clone, &socket, 50).await {
                                                warn!("Failed to auto-replenish prekeys: {}", e);
                                            } else {
                                                info!("Successfully auto-replenished and uploaded 50 prekeys to WhatsApp server.");
                                            }
                                        }
                                    }
                                }
                            }

                            // Periodic WAM (w:stats) anti-fingerprinting telemetry batch dispatch (every ~5 min)
                            ticks_since_wam_flush += 1;
                            if ticks_since_wam_flush >= 20 {
                                ticks_since_wam_flush = 0;
                                if is_connected_clone.load(Ordering::SeqCst) {
                                    let stream_id = (rand::random::<u8>() % 100) + 1;
                                    let mut batch = crate::wam::WamBatch::new(stream_id, 1, 0);
                                    let now = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_secs();
                                    batch.write_event(
                                        now,
                                        472, // WebcSocketConnect event
                                        1,
                                        &[
                                            crate::wam::WamField::new(1, 0i64),
                                            crate::wam::WamField::new(2, client.metrics().messages_in.load(Ordering::Relaxed) as i64),
                                            crate::wam::WamField::new(3, client.metrics().messages_out.load(Ordering::Relaxed) as i64),
                                        ],
                                    );
                                    let wam_iq = batch.build_iq_node(now);
                                    let _ = socket.send_node(wam_iq).await;
                                    debug!("Dispatched background WAM anti-fingerprinting telemetry batch");
                                }
                            }

                            // Send keepalive ping (using standard WhatsApp IQ message tag format)
                            let ping_id = format!("{}.{}-1", rand::random::<u16>(), rand::random::<u16>());
                            let ping = BinaryNode::new("iq")
                                .with_attr("to", S_WHATSAPP_NET)
                                .with_attr("type", "get")
                                .with_attr("id", &ping_id)
                                .with_attr("xmlns", "w:p")
                                .with_child(BinaryNode::new("ping"));

                            if let Err(e) = socket.send_node(ping).await {
                                warn!("Failed to send keepalive ping: {}", e);
                                session_terminated = true;
                            } else {
                                last_ping_sent_at = Some(tokio::time::Instant::now());
                            }
                        }

                        maybe_node = incoming_rx.recv() => {
                            match maybe_node {
                                Some(node) => {
                                    // Received traffic confirms connection is actively alive
                                    if let Some(sent_at) = last_ping_sent_at.take() {
                                        client.metrics().record_ping_latency(sent_at.elapsed());
                                    }
                                    client.metrics().record_msg_in();

                                    debug!("Processing incoming stanza: <{}> (id={:?}, type={:?})",
                                        node.tag, node.get_attr("id"), node.get_attr("type"));

                                    match node.tag.as_str() {
                                        "notification" => {
                                            // Send ACK to WhatsApp server for notification stanza (Baileys parity)
                                            let ack_node = crate::registration::build_ack_node(&node, None, None);
                                            let _ = socket.send_node(ack_node).await;

                                            let active_companion = {
                                                let lock = companion_state_holder.lock().await;
                                                lock.clone()
                                            };
                                            if let Some(ref companion) = active_companion {
                                                if let Some(link_reg) = node.get_child("link_code_companion_reg") {
                                                    let stage = link_reg.get_attr("stage");
                                                    if stage == Some("refresh_code") {
                                                        warn!("Server sent stage='refresh_code' (phone requested pairing code refresh). Automatically generating fresh code...");
                                                        let new_state = Arc::new(CompanionPairingState::new(&companion.phone_number));
                                                        match new_state.build_companion_hello_iq(&creds, &config_clone) {
                                                            Ok(hello_node) => {
                                                                let _ = socket.send_node(hello_node).await;
                                                                let display_code = if new_state.pairing_code.len() == 8 {
                                                                    format!("{}-{}", &new_state.pairing_code[0..4], &new_state.pairing_code[4..8])
                                                                } else {
                                                                    new_state.pairing_code.clone()
                                                                };
                                                                info!("WhatsApp Pairing Code refreshed: {}", display_code);
                                                                events_clone.emit(BaileysEvent::PairingCode {
                                                                    code: display_code,
                                                                });
                                                                {
                                                                    let mut lock = companion_state_holder.lock().await;
                                                                    *lock = Some(Arc::clone(&new_state));
                                                                }
                                                            }
                                                            Err(e) => {
                                                                error!("Failed to rebuild companion_hello on refresh_code: {}", e);
                                                            }
                                                        }
                                                    }

                                                    if link_reg.get_child("primary_identity_pub").is_some() {
                                                        debug!("Received <notification> with primary_identity_pub, computing key bundle reply...");
                                                        match companion.handle_companion_finish_iq(&node, &mut creds) {
                                                            Ok(reply) => {
                                                                debug!("Sending companion_finish IQ reply to server!");
                                                                let _ = socket.send_node(reply).await;
                                                                if store_clone.save_creds(&creds).await.is_ok() {
                                                                    events_clone.emit(BaileysEvent::CredsUpdate);
                                                                }
                                                            }
                                                            Err(e) => {
                                                                error!("Failed to handle companion notification: {}", e);
                                                            }
                                                        }
                                                    } else if stage != Some("refresh_code") {
                                                        debug!("Received <notification> with link_code_companion_reg stage={:?}", stage);
                                                    }
                                                }
                                            }

                                            // Group / newsletter push notifications (Baileys parity)
                                            match node.get_attr("type").unwrap_or("") {
                                                "w:gp2" => {
                                                    for ev in crate::notifications::parse_group_notification(&node) {
                                                        events_clone.emit(ev);
                                                    }
                                                }
                                                "newsletter" => {
                                                    for ev in crate::notifications::parse_newsletter_notification(&node) {
                                                        events_clone.emit(ev);
                                                    }
                                                }
                                                "mex" => {
                                                    for ev in crate::notifications::parse_mex_notification(&node) {
                                                        events_clone.emit(ev);
                                                    }
                                                }
                                                _ => {}
                                            }

                                            // Check blocklist push notification
                                            let notif_type = node.get_attr("type").unwrap_or("");
                                            if notif_type == "blocklist" || node.get_child("blocklist").is_some() {
                                                let (added, removed) = crate::privacy::parse_blocklist_notification(&node);
                                                if !added.is_empty() || !removed.is_empty() {
                                                    let store_bl = Arc::clone(store_clone);
                                                    let added_clone = added.clone();
                                                    let removed_clone = removed.clone();
                                                    tokio::spawn(async move {
                                                        for jid in &added_clone {
                                                            let _ = store_bl.save_blocked_contact(jid).await;
                                                        }
                                                        for jid in &removed_clone {
                                                            let _ = store_bl.remove_blocked_contact(jid).await;
                                                        }
                                                    });
                                                    events_clone.emit(BaileysEvent::BlocklistUpdate { added, removed });
                                                }
                                            }

                                            // Check ephemeral timer update push notification
                                            if notif_type == "ephemeral" || node.get_child("ephemeral").is_some() || node.get_child("not_ephemeral").is_some() {
                                                let from_chat = node.get_attr("from").unwrap_or("").to_string();
                                                if !from_chat.is_empty() {
                                                    let expiration = if node.get_child("not_ephemeral").is_some() {
                                                        0
                                                    } else {
                                                        node.get_child("ephemeral")
                                                            .and_then(|e| e.get_attr("expiration"))
                                                            .or_else(|| node.get_attr("expiration"))
                                                            .and_then(|exp| exp.parse::<u32>().ok())
                                                            .unwrap_or(0)
                                                    };
                                                    let store_eph = Arc::clone(store_clone);
                                                    let chat_eph = from_chat.clone();
                                                    tokio::spawn(async move {
                                                        let _ = store_eph.update_chat_ephemeral(&chat_eph, expiration, None).await;
                                                    });
                                                    events_clone.emit(BaileysEvent::ChatEphemeralUpdate {
                                                        chat_jid: from_chat,
                                                        ephemeral_expiration: expiration,
                                                    });
                                                }
                                            }

                                            // Check devices / md companion push notification
                                            if notif_type == "devices" || notif_type == "md" {
                                                let from_user = node.get_attr("from").unwrap_or("").to_string();
                                                let mut updated_devs = Vec::new();
                                                let mut removed_devs = Vec::new();

                                                if let Some(update_node) = node.get_child("update") {
                                                    for dev in update_node.get_children().iter().filter(|c| c.tag == "device") {
                                                        if let Some(id_str) = dev.get_attr("id") {
                                                            if let Ok(id) = id_str.parse::<u32>() {
                                                                updated_devs.push(id);
                                                            }
                                                        }
                                                    }
                                                }

                                                if let Some(remove_node) = node.get_child("remove") {
                                                    for dev in remove_node.get_children().iter().filter(|c| c.tag == "device") {
                                                        if let Some(id_str) = dev.get_attr("id") {
                                                            if let Ok(id) = id_str.parse::<u32>() {
                                                                removed_devs.push(id);
                                                            }
                                                        }
                                                    }
                                                }

                                                // Check if our own companion was removed
                                                let our_jid = creds.me_jid.as_deref().unwrap_or("");
                                                let our_device_id = our_jid.split_once(':')
                                                    .and_then(|(_, rest)| rest.split_once('@'))
                                                    .and_then(|(d, _)| d.parse::<u32>().ok());

                                                if let Some(our_id) = our_device_id {
                                                    if removed_devs.contains(&our_id) {
                                                        warn!("Our companion device ID {} was revoked by primary. Disconnecting...", our_id);
                                                        events_clone.emit(BaileysEvent::Disconnected {
                                                            reason: "Logged out by primary device".to_string(),
                                                        });
                                                    }
                                                }

                                                if !updated_devs.is_empty() || !removed_devs.is_empty() {
                                                    let store_dev = Arc::clone(store_clone);
                                                    let user_target = from_user.clone();
                                                    let u_devs = updated_devs.clone();
                                                    let r_devs = removed_devs.clone();
                                                    tokio::spawn(async move {
                                                        if !u_devs.is_empty() {
                                                            let _ = store_dev.save_user_devices(&user_target, &u_devs).await;
                                                        }
                                                        for r in r_devs {
                                                            let _ = store_dev.remove_user_device(&user_target, r).await;
                                                        }
                                                    });
                                                    events_clone.emit(BaileysEvent::DeviceUpdate {
                                                        user_jid: from_user,
                                                        devices: updated_devs,
                                                    });
                                                }
                                            }

                                            // Check identity change notification
                                            if notif_type == "identity" || node.get_child("identity").is_some() {
                                                let from_user = node.get_attr("from").unwrap_or("").to_string();
                                                let timestamp = node.get_attr("t").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
                                                let is_companion = from_user.contains(':') && !from_user.contains(":0@");
                                                let our_jid = creds.me_jid.as_deref().unwrap_or("");
                                                let is_self = !from_user.is_empty() && (from_user == our_jid || our_jid.starts_with(&format!("{}:", from_user.split('@').next().unwrap_or(""))));
                                                if !is_companion && !is_self && !from_user.is_empty() {
                                                    info!("Received identity change notification for JID: {}", from_user);
                                                    let store_id = Arc::clone(store_clone);
                                                    let target_id = from_user.clone();
                                                    tokio::spawn(async move {
                                                        let _ = store_id.remove_keys("session", std::slice::from_ref(&target_id)).await;
                                                        let _ = store_id.remove_keys("tctoken", std::slice::from_ref(&target_id)).await;
                                                    });
                                                    events_clone.emit(BaileysEvent::IdentityChanged {
                                                        jid: from_user,
                                                        timestamp,
                                                    });
                                                }
                                            }

                                            // Check mediaretry push notification
                                            if notif_type == "mediaretry" {
                                                let msg_id = node.get_attr("id").unwrap_or("").to_string();
                                                debug!("Received mediaretry notification for message ID: {}", msg_id);
                                                if let Some(encrypt_node) = node.get_child("encrypt") {
                                                    let enc_p = encrypt_node.get_child("enc_p").and_then(|c| c.content_as_bytes());
                                                    let enc_iv = encrypt_node.get_child("enc_iv").and_then(|c| c.content_as_bytes());
                                                    if let (Some(ciphertext), Some(iv)) = (enc_p, enc_iv) {
                                                        let store_m = Arc::clone(store_clone);
                                                        let events_m = events_clone.clone();
                                                        let ct_vec = ciphertext.to_vec();
                                                        let iv_vec = iv.to_vec();
                                                        let target_id = msg_id.clone();
                                                        tokio::spawn(async move {
                                                            if let Ok(Some(stored_msg)) = store_m.get_message(&target_id).await {
                                                                if let Ok(proto_info) = baileys_proto::proto::WebMessageInfo::decode(&stored_msg.proto_bytes[..]) {
                                                                    if let Some(ref m) = proto_info.message {
                                                                        let media_key = m.image_message.as_ref().and_then(|img| img.media_key.clone())
                                                                            .or_else(|| m.video_message.as_ref().and_then(|v| v.media_key.clone()))
                                                                            .or_else(|| m.audio_message.as_ref().and_then(|a| a.media_key.clone()))
                                                                            .or_else(|| m.document_message.as_ref().and_then(|d| d.media_key.clone()))
                                                                            .or_else(|| m.sticker_message.as_ref().and_then(|s| s.media_key.clone()));

                                                                        if let Some(m_key) = media_key {
                                                                            match baileys_media::decrypt_media_retry_notification(&ct_vec, &iv_vec, &m_key, &target_id) {
                                                                                Ok(retry_notif) => {
                                                                                    if let Some(ref new_path) = retry_notif.direct_path {
                                                                                        info!("Media retry succeeded for {}, new direct_path: {}", target_id, new_path);
                                                                                        let mut updated_info = proto_info.clone();
                                                                                        if let Some(ref mut up_m) = updated_info.message {
                                                                                            if let Some(ref mut img) = up_m.image_message { img.direct_path = Some(new_path.clone()); }
                                                                                            if let Some(ref mut v) = up_m.video_message { v.direct_path = Some(new_path.clone()); }
                                                                                            if let Some(ref mut a) = up_m.audio_message { a.direct_path = Some(new_path.clone()); }
                                                                                            if let Some(ref mut d) = up_m.document_message { d.direct_path = Some(new_path.clone()); }
                                                                                            if let Some(ref mut s) = up_m.sticker_message { s.direct_path = Some(new_path.clone()); }
                                                                                        }
                                                                                        let mut updated_bytes = Vec::new();
                                                                                        let _ = updated_info.encode(&mut updated_bytes);
                                                                                        let _ = store_m.update_message_content(&stored_msg.chat_jid, &target_id, &stored_msg.summary, Some(&updated_bytes)).await;
                                                                                        events_m.emit(BaileysEvent::MediaUpdated {
                                                                                            message_id: target_id,
                                                                                            direct_path: new_path.clone(),
                                                                                        });
                                                                                    }
                                                                                }
                                                                                Err(e) => {
                                                                                    warn!("Failed to decrypt media retry notification for {}: {}", target_id, e);
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        });
                                                    }
                                                }
                                            }

                                            // Check server_sync notification for app state
                                            if notif_type == "server_sync" || node.get_child("collection").is_some() {
                                                debug!("Received server_sync notification, scheduling AppState resync...");
                                                let client_sync = client.clone();
                                                tokio::spawn(async move {
                                                    if let Err(e) = client_sync.resync_app_state(crate::syncd::ALL_COLLECTIONS, false).await {
                                                        debug!("Server sync notification AppState resync note: {}", e);
                                                    }
                                                });
                                            }

                                            // Send ACK after processing notification
                                            let ack = build_ack_node(&node, None, creds.me_jid.as_deref());
                                            let _ = socket.send_node(ack).await;
                                        }

                                        "iq" => {
                                            let iq_type = node.get_attr("type").unwrap_or("");
                                            let msg_id = node.get_attr("id").unwrap_or("").to_string();

                                            // Fulfill any registered oneshot sender awaiting this IQ response
                                            if iq_type == "result" || iq_type == "error" {
                                                let mut pending = pending_iq_clone.lock().await;
                                                if let Some(tx) = pending.remove(&msg_id) {
                                                    let _ = tx.send(node.clone());
                                                }
                                            }

                                            if iq_type == "error" {
                                                warn!("WhatsApp server returned IQ error: {:?}", node);
                                            }

                                            // A. QR tokens or phone pairing
                                            if let Some(pair_device_node) = node.get_child("pair-device") {
                                                debug!("Received server <pair-device> stanza!");
                                                let ack = BinaryNode::new("iq")
                                                    .with_attr("to", S_WHATSAPP_NET)
                                                    .with_attr("type", "result")
                                                    .with_attr("id", &msg_id);
                                                let _ = socket.send_node(ack).await;

                                                if creds.me_jid.is_none() {
                                                    if let Some(ref phone) = config_clone.phone_number_pairing {
                                                        if !pairing_code_sent {
                                                            pairing_code_sent = true;
                                                            info!("Initiating WhatsApp companion pairing for phone: {}", phone);
                                                            let state = Arc::new(CompanionPairingState::new(phone));
                                                            match state.build_companion_hello_iq(&creds, &config_clone) {
                                                                Ok(hello_node) => {
                                                                    let _ = socket.send_node(hello_node).await;
                                                                    let display_code = if state.pairing_code.len() == 8 {
                                                                        format!("{}-{}", &state.pairing_code[0..4], &state.pairing_code[4..8])
                                                                    } else {
                                                                        state.pairing_code.clone()
                                                                    };
                                                                    info!("WhatsApp Pairing Code generated: {}", display_code);
                                                                    events_clone.emit(BaileysEvent::PairingCode {
                                                                        code: display_code,
                                                                    });
                                                                    {
                                                                        let mut lock = companion_state_holder.lock().await;
                                                                        *lock = Some(Arc::clone(&state));
                                                                    }
                                                                }
                                                                Err(e) => {
                                                                    error!("Failed to build companion_hello: {}", e);
                                                                }
                                                            }
                                                        }
                                                    } else {
                                                        let ref_children = pair_device_node.get_children();
                                                        for ref_child in ref_children {
                                                            if ref_child.tag == "ref" {
                                                                if let Some(ref_str) = ref_child.content_as_str() {
                                                                    let noise_b64 = BASE64_STANDARD.encode(&creds.noise_key.public);
                                                                    let ident_b64 = BASE64_STANDARD.encode(&creds.signed_identity_key.public);
                                                                    let adv_b64 = BASE64_STANDARD.encode(&creds.adv_secret_key);
                                                                    let qr = if ref_str.contains('@') {
                                                                        format!("{},{},{},{}", ref_str, noise_b64, ident_b64, adv_b64)
                                                                    } else {
                                                                        format!("1@{},{},{},{}", ref_str, noise_b64, ident_b64, adv_b64)
                                                                    };
                                                                    debug!("WhatsApp QR Code generated (ref_len={})", ref_str.len());
                                                                    {
                                                                        let mut lock = last_qr_holder.lock().await;
                                                                        *lock = Some(qr.clone());
                                                                    }
                                                                    events_clone.emit(BaileysEvent::QrCode { code: qr });
                                                                    break;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            // B. Device paired successfully! (<pair-success>)
                                            if node.get_child("pair-success").is_some() {
                                                info!("Device paired successfully! Validating server signatures...");
                                                match configure_successful_pairing(&node, &creds) {
                                                    Ok((reply_node, updated)) => {
                                                        debug!("Pairing validated! Sending <pair-device-sign> confirmation...");
                                                        if let Err(e) = socket.send_node(reply_node).await {
                                                            error!("Failed to send pair-device-sign ACK: {}", e);
                                                        }

                                                        creds.me_jid = Some(updated.me_jid.clone());
                                                        // A new pairing is a new identity, so anything left from a previous one
                                                        // (sessions, pre-keys, app-state versions) is wrong for it.
                                                        match crate::registration::clear_identity_bound_keys(store_clone.as_ref()).await {
                                                            Ok(0) => {}
                                                            Ok(n) => info!("Cleared {} keys left over from a previous identity.", n),
                                                            Err(e) => warn!("Failed to clear keys of the previous identity: {}", e),
                                                        }
                                                        if let Some(ref lid) = updated.lid {
                                                            creds.me_lid = Some(lid.clone());
                                                            me_lid = Some(lid.clone());
                                                        }
                                                        if let Some(ref acc) = updated.account {
                                                            creds.account = Some(acc.encode_to_vec());
                                                        }
                                                        if let Err(e) = store_clone.save_creds(&creds).await {
                                                            error!("Failed to save updated credentials: {}", e);
                                                        } else {
                                                            events_clone.emit(BaileysEvent::CredsUpdate);
                                                        }

                                                        {
                                                            let mut lock = last_qr_holder.lock().await;
                                                            *lock = None;
                                                        }

                                                        info!("WhatsApp pairing authenticated for '{}'! Server will now restart connection...", updated.me_jid);
                                                    }
                                                    Err(e) => {
                                                        error!("Failed to configure successful pairing: {}", e);
                                                    }
                                                }
                                            }

                                            // C. Respond to incoming ping
                                            if iq_type == "get" && node.get_child("ping").is_some() {
                                                let pong = BinaryNode::new("iq")
                                                    .with_attr("to", S_WHATSAPP_NET)
                                                    .with_attr("type", "result")
                                                    .with_attr("id", &msg_id);
                                                let _ = socket.send_node(pong).await;
                                            }
                                        }

                                        "success" => {
                                            debug!("Server sent <success> node! Reconnect authentication complete.");
                                            reconnect_delay = Duration::from_secs(3);
                                            if let Some(lid) = node.get_attr("lid") {
                                                debug!("Associated LID: {}", lid);
                                                me_lid = Some(lid.to_string());
                                                creds.me_lid = Some(lid.to_string());
                                                if store_clone.save_creds(&creds).await.is_ok() {
                                                    events_clone.emit(BaileysEvent::CredsUpdate);
                                                }
                                            }

                                            // Top up pre-keys only when the stock is low (always true right after
                                            // pairing). Uploading on every connection leaked 30 keys each time.
                                            let unused_prekeys = store_clone.get_key_count("pre-key").await.unwrap_or(0);
                                            if crate::registration::needs_prekey_upload(unused_prekeys) {
                                                debug!("Uploading PreKey bundle to WhatsApp server ({} unused in store)...", unused_prekeys);
                                                if let Err(e) = upload_pre_keys(&mut creds, store_clone, &socket, 30).await {
                                                    warn!("Failed to upload PreKeys: {}", e);
                                                }
                                            } else {
                                                debug!("{} unused PreKeys in store; skipping upload.", unused_prekeys);
                                            }

                                            // Send passive active IQ
                                            debug!("Activating real-time stream via passive IQ...");
                                            let passive = build_passive_iq("active");
                                            if let Err(e) = socket.send_node(passive).await {
                                                warn!("Failed to send passive active IQ: {}", e);
                                            }

                                            // Send unified session telemetry
                                            let now_ms = std::time::SystemTime::now()
                                                .duration_since(std::time::UNIX_EPOCH)
                                                .map(|d| d.as_millis() as u64)
                                                .unwrap_or(0);
                                            let unified_id = ((now_ms + 3 * 86_400_000) % (7 * 86_400_000)).to_string();
                                            let unified = BinaryNode::new("ib")
                                                .with_child(BinaryNode::new("unified_session").with_attr("id", unified_id));
                                            let _ = socket.send_node(unified).await;

                                            if let Some(ref me) = creds.me_jid {
                                                is_connected_clone.store(true, Ordering::SeqCst);
                                                client.set_connection_state(crate::state::ConnectionState::Connected);
                                                debug!("WhatsApp authenticated and connected as '{}'!", me);
                                                events_clone.emit(BaileysEvent::Connected { me_jid: me.clone() });

                                                // Trigger automatic multi-device App State resync
                                                let client_sync = client.clone();
                                                tokio::spawn(async move {
                                                    tokio::time::sleep(Duration::from_millis(500)).await;
                                                    if let Err(e) = client_sync.resync_app_state(crate::syncd::ALL_COLLECTIONS, false).await {
                                                        debug!("Initial AppState auto-resync note: {}", e);
                                                    }
                                                });
                                            }
                                        }

                                        "message" => {
                                            let from = node.get_attr("from").unwrap_or("unknown").to_string();
                                            let msg_id = node.get_attr("id").unwrap_or("").to_string();
                                            let participant_opt = node.get_attr("participant").map(|s| s.to_string());
                                            let recipient_opt = node.get_attr("recipient").map(|s| s.to_string());
                                            let notify_opt = node.get_attr("notify").map(|s| s.to_string());
                                            let sender_pn_opt = node.get_attr("sender_pn").or_else(|| node.get_attr("participant_pn")).map(|s| s.to_string());
                                            let sender_lid_opt = node.get_attr("sender_lid").or_else(|| node.get_attr("participant_lid")).map(|s| s.to_string());

                                            if let Some(ref pn) = sender_pn_opt {
                                                let c = baileys_store::StoredContact {
                                                    jid: pn.clone(),
                                                    name: None,
                                                    push_name: notify_opt.clone(),
                                                    lid: sender_lid_opt.clone(),
                                                };
                                                let _ = store_clone.save_contact(&c).await;
                                            }

                                            let is_peer_category = node.get_attr("category") == Some("peer");
                                            let node_ts: u64 = node.get_attr("t")
                                                .and_then(|t| t.parse::<u64>().ok())
                                                .unwrap_or_else(|| std::time::SystemTime::now()
                                                    .duration_since(std::time::UNIX_EPOCH)
                                                    .map(|d| d.as_secs())
                                                    .unwrap_or(0));

                                            // 1. Encrypted messages are acknowledged with a <receipt> after decryption
                                            //    (delivery / peer_msg / hist_sync). Only unencrypted stanzas get a plain ACK here.
                                            let has_enc = node.get_children().iter().any(|c| c.tag == "enc");
                                            if !has_enc {
                                                let ack = build_ack_node(&node, None, creds.me_jid.as_deref());
                                                let _ = socket.send_node(ack).await;
                                            }

                                            // 2. Identify author and chat (participant takes precedence for groups and broadcast/status)
                                            let author_jid = if let Some(ref p) = participant_opt {
                                                p.clone()
                                            } else {
                                                from.clone()
                                            };
                                            let is_from_me = {
                                                let author_user = jid_user_part(&author_jid);
                                                creds.me_jid.as_deref().map(jid_user_part) == Some(author_user)
                                                    || me_lid.as_deref().map(jid_user_part) == Some(author_user)
                                            };
                                            // For 1:1 messages sent by us from another device, the chat is the `recipient`
                                            let chat_jid_raw = if is_from_me && participant_opt.is_none() {
                                                recipient_opt.clone().unwrap_or_else(|| from.clone())
                                            } else {
                                                from.clone()
                                            };
                                            let chat_jid_resolved = baileys_binary::jid::jid_normalized_user(&chat_jid_raw);

                                            // Ephemeral disappearing timer sync from XML attributes
                                            if let Some(eph_child) = node.get_child("ephemeral") {
                                                if let Some(exp_str) = eph_child.get_attr("expiration") {
                                                    if let Ok(expiration) = exp_str.parse::<u32>() {
                                                        let store_eph = Arc::clone(store_clone);
                                                        let chat_eph = chat_jid_resolved.clone();
                                                        tokio::spawn(async move {
                                                            let _ = store_eph.update_chat_ephemeral(&chat_eph, expiration, None).await;
                                                        });
                                                        events_clone.emit(BaileysEvent::ChatEphemeralUpdate {
                                                            chat_jid: chat_jid_resolved.clone(),
                                                            ephemeral_expiration: expiration,
                                                        });
                                                    }
                                                }
                                            } else if node.get_child("not_ephemeral").is_some() {
                                                let store_eph = Arc::clone(store_clone);
                                                let chat_eph = chat_jid_resolved.clone();
                                                tokio::spawn(async move {
                                                    let _ = store_eph.update_chat_ephemeral(&chat_eph, 0, None).await;
                                                });
                                                events_clone.emit(BaileysEvent::ChatEphemeralUpdate {
                                                    chat_jid: chat_jid_resolved.clone(),
                                                    ephemeral_expiration: 0,
                                                });
                                            }

                                            // 3. Scan children for <enc> nodes
                                            let enc_children: Vec<&BinaryNode> = node.get_children().iter()
                                                .filter(|c| c.tag == "enc")
                                                .collect();

                                            if enc_children.is_empty() {
                                                // Support unencrypted channel broadcasts (<plaintext> node)
                                                if let Some(plain_node) = node.get_child("plaintext") {
                                                    if let Some(bytes) = plain_node.content_as_bytes() {
                                                        if let Ok(proto) = <Message as ProstMessage>::decode(bytes) {
                                                            let msg_info = WebMessageInfo {
                                                                key: Some(MessageKey {
                                                                    remote_jid: Some(from.clone()),
                                                                    from_me: Some(false),
                                                                    id: Some(msg_id.clone()),
                                                                    participant: participant_opt,
                                                                }),
                                                                message: Some(proto),
                                                                message_timestamp: Some(
                                                                    std::time::SystemTime::now()
                                                                        .duration_since(std::time::UNIX_EPOCH)
                                                                        .unwrap()
                                                                        .as_secs(),
                                                                ),
                                                                ..Default::default()
                                                            };

                                                            // Cache in recent_messages
                                                            {
                                                                let mut cache = recent_messages_clone.lock().await;
                                                                if cache.len() > 500 {
                                                                    cache.clear();
                                                                }
                                                                cache.insert(msg_id.clone(), msg_info.clone());
                                                            }

                                                            events_clone.emit(BaileysEvent::Message {
                                                                message: Box::new(msg_info),
                                                            });
                                                        }
                                                    }
                                                } else if let Some(body_node) = node.get_child("body") {
                                                    if let Some(text) = body_node.content_as_string() {
                                                        let msg_info = WebMessageInfo {
                                                            key: Some(MessageKey {
                                                                remote_jid: Some(from.clone()),
                                                                from_me: Some(false),
                                                                id: Some(msg_id.clone()),
                                                                participant: participant_opt,
                                                            }),
                                                            message: Some(Message {
                                                                conversation: Some(text.to_string()),
                                                                ..Default::default()
                                                            }),
                                                            message_timestamp: Some(
                                                                std::time::SystemTime::now()
                                                                    .duration_since(std::time::UNIX_EPOCH)
                                                                    .unwrap()
                                                                    .as_secs(),
                                                            ),
                                                            ..Default::default()
                                                        };

                                                        // Cache in recent_messages
                                                        {
                                                            let mut cache = recent_messages_clone.lock().await;
                                                            if cache.len() > 500 {
                                                                cache.clear();
                                                            }
                                                            cache.insert(msg_id.clone(), msg_info.clone());
                                                        }

                                                        events_clone.emit(BaileysEvent::Message {
                                                            message: Box::new(msg_info),
                                                        });
                                                    }
                                                }
                                            } else {
                                                // Decrypt enc children. Prioritize pkmsg/msg before skmsg to ingest SenderKeyDistributionMessage
                                                let mut sorted_encs = enc_children.clone();
                                                sorted_encs.sort_by_key(|c| {
                                                    match c.get_attr("type").unwrap_or("") {
                                                        "pkmsg" => 0,
                                                        "msg" => 1,
                                                        "skmsg" => 2,
                                                        _ => 3,
                                                    }
                                                });

                                                let mut decrypted_message_proto: Option<Message> = None;
                                                let mut decryption_error: Option<String> = None;

                                                for enc in sorted_encs {
                                                    let enc_type = enc.get_attr("type").unwrap_or("");
                                                    if let Some(ciphertext) = enc.content_as_bytes() {
                                                        match enc_type {
                                                            "pkmsg" | "msg" => {
                                                                let our_id = KeyPair::try_from(&creds.signed_identity_key).ok();
                                                                let our_spk = KeyPair::try_from(&creds.signed_pre_key.key_pair).ok();
                                                                if let (Some(id), Some(spk)) = (our_id, our_spk) {
                                                                    match signal_clone.decrypt_message(&author_jid, enc_type, ciphertext, &id, &spk).await {
                                                                        Ok(plaintext) => {
                                                                            client.metrics().record_signal_hit();
                                                                            let unpadded = unpad_random_max16(&plaintext);
                                                                            if let Ok(proto) = Message::decode(unpadded) {
                                                                                if let Some(ref skdm) = proto.sender_key_distribution_message {
                                                                                    if let (Some(ref grp), Some(ref axolotl_bytes)) = (&skdm.group_id, &skdm.axolotl_sender_key_distribution_message) {
                                                                                        debug!("Processing incoming SenderKeyDistributionMessage for group {} from {}", grp, author_jid);
                                                                                        match signal_clone.process_sender_key_distribution_message(grp, &author_jid, axolotl_bytes).await {
                                                                                            Ok(_) => debug!("Successfully saved SenderKey for group {} from {}", grp, author_jid),
                                                                                            Err(e) => warn!("Failed to process SenderKeyDistributionMessage for group {} from {}: {}", grp, author_jid, e),
                                                                                        }
                                                                                    }
                                                                                }
                                                                                if crate::messages::has_real_message_content(&proto) {
                                                                                    decrypted_message_proto = Some(proto);
                                                                                }
                                                                            }
                                                                        }
                                                                        Err(e) => {
                                                                            client.metrics().record_signal_miss();
                                                                            warn!("Failed to decrypt 1-on-1 message ({}) from {}: {}", enc_type, author_jid, e);
                                                                            decryption_error = Some(e.to_string());
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            "skmsg" => {
                                                                match signal_clone.decrypt_group_message(&from, &author_jid, ciphertext).await {
                                                                    Ok(plaintext) => {
                                                                        client.metrics().record_signal_hit();
                                                                        let unpadded = unpad_random_max16(&plaintext);
                                                                        if let Ok(proto) = Message::decode(unpadded) {
                                                                            decrypted_message_proto = Some(proto);
                                                                        }
                                                                    }
                                                                    Err(e) => {
                                                                        client.metrics().record_signal_miss();
                                                                        let err_str = e.to_string();
                                                                        if err_str.contains("No established Signal session found") {
                                                                            debug!("Group sender {} in {} has no active Signal session yet; sending retry receipt for key exchange", author_jid, from);
                                                                        } else {
                                                                            warn!("Failed to decrypt group skmsg from {} in {}: {}", author_jid, from, err_str);
                                                                        }
                                                                        decryption_error = Some(err_str);
                                                                    }
                                                                }
                                                            }
                                                            _ => {}
                                                        }
                                                    }
                                                }

                                                // If decryption failed, send a retry receipt to request key re-transmission
                                                if decrypted_message_proto.is_none() && decryption_error.is_some() {
                                                    let retry_key = format!("{}:{}", msg_id, author_jid);
                                                    let count = msg_retries.entry(retry_key).or_insert(0);
                                                    *count += 1;
                                                    let current_retry = *count;

                                                    if current_retry <= 5 {
                                                        debug!("Key negotiation for msg {} from {} (attempt {}/5). Sending retry receipt...", msg_id, author_jid, current_retry);
                                                        match create_retry_keys_node(&mut creds, store_clone).await {
                                                            Ok(keys_node) => {
                                                                let retry = build_retry_receipt(&node, creds.registration_id, current_retry, Some(keys_node));
                                                                let _ = socket.send_node(retry).await;
                                                            }
                                                            Err(e) => {
                                                                warn!("Failed to create retry keys node: {}. Sending basic retry receipt...", e);
                                                                let retry = build_retry_receipt(&node, creds.registration_id, current_retry, None);
                                                                let _ = socket.send_node(retry).await;
                                                            }
                                                        }
                                                    } else {
                                                        warn!("Max retry attempts (5) reached for msg {} from {}. Dropping retry.", msg_id, author_jid);
                                                    }

                                                    // Acknowledge the stanza so the server does not redeliver it endlessly
                                                    let ack = build_ack_node(&node, None, creds.me_jid.as_deref());
                                                    let _ = socket.send_node(ack).await;
                                                } else {
                                                    // Decryption succeeded: send the delivery receipt (Baileys parity)
                                                    //  - category="peer" (messages from our own primary, e.g. history sync) => peer_msg
                                                    //  - sent by us from another device => sender
                                                    //  - otherwise plain delivery receipt (no type)
                                                    let mut receipt = BinaryNode::new("receipt").with_attr("id", &msg_id);
                                                    if is_peer_category {
                                                        receipt = receipt.with_attr("to", &from).with_attr("type", "peer_msg");
                                                    } else if is_from_me {
                                                        receipt = receipt.with_attr("type", "sender");
                                                        if participant_opt.is_none() && recipient_opt.is_some() {
                                                            receipt = receipt
                                                                .with_attr("to", &from)
                                                                .with_attr("recipient", recipient_opt.as_deref().unwrap_or_default());
                                                        } else {
                                                            receipt = receipt.with_attr("to", &from);
                                                            if let Some(ref p) = participant_opt {
                                                                receipt = receipt.with_attr("participant", p);
                                                            }
                                                        }
                                                    } else {
                                                        receipt = receipt.with_attr("to", &from);
                                                        if let Some(ref p) = participant_opt {
                                                            receipt = receipt.with_attr("participant", p);
                                                        }
                                                    }
                                                    let _ = socket.send_node(receipt).await;

                                                    // History sync notifications additionally require a `hist_sync` receipt;
                                                    // without it the primary phone stops after the bootstrap chunk and never
                                                    // streams RECENT / FULL history.
                                                    let is_history_msg = decrypted_message_proto.as_ref()
                                                        .and_then(|m| m.protocol_message.as_ref())
                                                        .map(|p| p.history_sync_notification.is_some())
                                                        .unwrap_or(false);
                                                    if is_history_msg {
                                                        let hist_receipt = BinaryNode::new("receipt")
                                                            .with_attr("id", &msg_id)
                                                            .with_attr("to", normalize_user_jid(&chat_jid_resolved))
                                                            .with_attr("type", "hist_sync");
                                                        debug!("Sending hist_sync receipt for history notification {}", msg_id);
                                                        let _ = socket.send_node(hist_receipt).await;
                                                    }
                                                }

                                                // If message successfully decrypted, emit Message event
                                                if let Some(proto) = decrypted_message_proto {
                                                    let retry_key = format!("{}:{}", msg_id, author_jid);
                                                    msg_retries.remove(&retry_key);
                                                    let msg_info = WebMessageInfo {
                                                        key: Some(MessageKey {
                                                            remote_jid: Some(chat_jid_resolved.clone()),
                                                            from_me: Some(is_from_me),
                                                            id: Some(msg_id.clone()),
                                                            participant: participant_opt,
                                                        }),
                                                        message: Some(proto.clone()),
                                                        message_timestamp: Some(node_ts),
                                                        push_name: notify_opt.clone(),
                                                        ..Default::default()
                                                    };

                                                    // Cache in recent_messages
                                                    {
                                                        let mut cache = recent_messages_clone.lock().await;
                                                        if cache.len() > 500 {
                                                            cache.clear();
                                                        }
                                                        cache.insert(msg_id.clone(), msg_info.clone());
                                                    }

                                                    // Auto-download media if enabled or anti-view-once active
                                                    let is_auto_dl = auto_download_clone.load(Ordering::SeqCst);
                                                    let is_auto_save_vo = config_clone.auto_save_view_once;
                                                    if is_auto_dl || is_auto_save_vo {
                                                        if let Some(media_attach) = baileys_media::extract_media_from_message(&proto) {
                                                            let should_download = is_auto_dl || (is_auto_save_vo && media_attach.is_view_once);
                                                            if should_download {
                                                                let msg_id_dl = msg_id.clone();
                                                                tokio::spawn(async move {
                                                                    let dest = if media_attach.is_view_once {
                                                                        std::path::Path::new("downloads/view_once")
                                                                    } else {
                                                                        std::path::Path::new("downloads")
                                                                    };
                                                                    let _ = std::fs::create_dir_all(dest);
                                                                    match baileys_media::download_media_to_dir(&media_attach, dest, &msg_id_dl).await {
                                                                        Ok(path) => {
                                                                            if media_attach.is_view_once {
                                                                                info!("[Anti View-Once] Intercepted and saved view-once media for {} -> {:?}", msg_id_dl, path);
                                                                            } else {
                                                                                debug!("Auto-downloaded media for {} -> {:?}", msg_id_dl, path);
                                                                            }
                                                                        }
                                                                        Err(e) => warn!("Auto-download failed for {}: {}", msg_id_dl, e),
                                                                    }
                                                                });
                                                            }
                                                        }
                                                    }

                                                    // Process protocol_message payloads (HistorySync, AppStateSyncKeyShare, etc.)
                                                    if let Some(ref protocol) = proto.protocol_message {
                                                        if let Some(ref key_share) = protocol.app_state_sync_key_share {
                                                            debug!("Received AppStateSyncKeyShare with {} keys", key_share.keys.len());
                                                            use base64::Engine;
                                                            let mut key_entries = Vec::new();
                                                            for k in &key_share.keys {
                                                                if let (Some(id), Some(data)) = (&k.key_id, &k.key_data) {
                                                                    if let (Some(ref id_bytes), Some(ref data_bytes)) = (&id.key_id, &data.key_data) {
                                                                        let id_b64 = base64::engine::general_purpose::STANDARD.encode(id_bytes);
                                                                        key_entries.push(baileys_store::KeyEntry {
                                                                            key_type: "app-state-sync-key".to_string(),
                                                                            id: id_b64,
                                                                            data: data_bytes.clone(),
                                                                        });
                                                                        key_entries.push(baileys_store::KeyEntry {
                                                                            key_type: "app-state-sync-key".to_string(),
                                                                            id: "primary".to_string(),
                                                                            data: data_bytes.clone(),
                                                                        });
                                                                        key_entries.push(baileys_store::KeyEntry {
                                                                            key_type: "app-state-sync-key".to_string(),
                                                                            id: "primary_id".to_string(),
                                                                            data: id_bytes.clone(),
                                                                        });
                                                                    }
                                                                }
                                                            }
                                                            if !key_entries.is_empty() {
                                                                let store_keys = Arc::clone(store_clone);
                                                                let client_sync = client.clone();
                                                                tokio::spawn(async move {
                                                                    let _ = store_keys.set_keys(key_entries).await;
                                                                    tokio::time::sleep(Duration::from_millis(300)).await;
                                                                    if let Err(e) = client_sync.resync_app_state(crate::syncd::ALL_COLLECTIONS, false).await {
                                                                        debug!("AppStateSyncKeyShare auto-resync note: {}", e);
                                                                    }
                                                                });
                                                            }
                                                        }

                                                        if let Some(ref history_notif) = protocol.history_sync_notification {
                                                            debug!("Received HistorySyncNotification (sync_type={:?}, chunk={:?}), downloading archive...",
                                                                history_notif.sync_type, history_notif.chunk_order);

                                                            // messaging-history.status (Baileys historySyncStatus)
                                                            {
                                                                let sync_type = history_notif.sync_type.unwrap_or(-1);
                                                                let (statuses, arm) = history_status
                                                                    .lock()
                                                                    .await
                                                                    .on_chunk(sync_type, history_notif.progress);
                                                                for (sync_type, status, explicit) in statuses {
                                                                    events_clone.emit(BaileysEvent::MessagingHistoryStatus {
                                                                        sync_type,
                                                                        status: status.to_string(),
                                                                        explicit,
                                                                    });
                                                                }
                                                                if let Some(generation) = arm {
                                                                    let tracker = Arc::clone(&history_status);
                                                                    let events_t = events_clone.clone();
                                                                    tokio::spawn(async move {
                                                                        tokio::time::sleep(crate::history::HISTORY_SYNC_PAUSED_TIMEOUT).await;
                                                                        if let Some((sync_type, status, explicit)) =
                                                                            tracker.lock().await.on_pause_timeout(generation)
                                                                        {
                                                                            events_t.emit(BaileysEvent::MessagingHistoryStatus {
                                                                                sync_type,
                                                                                status: status.to_string(),
                                                                                explicit,
                                                                            });
                                                                        }
                                                                    });
                                                                }
                                                            }

                                                            let notif_clone = history_notif.clone();
                                                            let hist_msg_id = msg_info.key.as_ref().and_then(|k| k.id.clone()).unwrap_or_default();
                                                            let events_h = events_clone.clone();
                                                            let store_h = Arc::clone(store_clone);
                                                            let signal_h = Arc::clone(signal_clone);

                                                            tokio::spawn(async move {
                                                                // Baileys' isLatest: no non-on-demand history notification was
                                                                // processed before this one. Recorded before downloading, as Baileys does.
                                                                let on_demand = notif_clone.sync_type
                                                                    == Some(baileys_proto::proto::message::HistorySyncType::OnDemand as i32);
                                                                let is_latest = if on_demand {
                                                                    None
                                                                } else {
                                                                    let seen_before = store_h
                                                                        .get_all_keys("history-processed")
                                                                        .await
                                                                        .map(|k| !k.is_empty())
                                                                        .unwrap_or(false);
                                                                    let _ = store_h.set_keys(vec![baileys_store::KeyEntry {
                                                                        key_type: "history-processed".to_string(),
                                                                        id: hist_msg_id,
                                                                        data: Vec::new(),
                                                                    }]).await;
                                                                    Some(!seen_before)
                                                                };
                                                                match baileys_media::download_history_sync(&notif_clone).await {
                                                                    Ok(mut history) => {
                                                                        let sync_type = history.sync_type.unwrap_or(0);
                                                                        let progress = history.progress.unwrap_or(0);
                                                                        let conv_count = history.conversations.len();
                                                                        let status_count = history.status_v3_messages.len();
                                                                        debug!("Successfully decrypted HistorySync: {} conversations, {} status updates (type={}, progress={})",
                                                                            conv_count, status_count, sync_type, progress);

                                                                        // Persist Pushnames and build lookup map for chat naming. Each kind of row in
                                                                        // the chunk is written in one batch (one transaction) rather than row by row.
                                                                        let mut pushname_map: HashMap<String, String> = HashMap::new();
                                                                        let mut pushname_contacts = Vec::new();
                                                                        for pn in &history.pushnames {
                                                                            if let (Some(ref id), Some(ref name)) = (&pn.id, &pn.pushname) {
                                                                                pushname_map.insert(id.clone(), name.clone());
                                                                                pushname_contacts.push(StoredContact {
                                                                                    jid: id.clone(),
                                                                                    name: None,
                                                                                    push_name: Some(name.clone()),
                                                                                    lid: None,
                                                                                });
                                                                            }
                                                                        }
                                                                        if !pushname_contacts.is_empty() {
                                                                            let _ = store_h.save_contacts_batch(&pushname_contacts).await;
                                                                        }

                                                                        // Persist Conversations & Historical Messages
                                                                        let mut stored_msgs = Vec::new();
                                                                        let mut stored_chats = Vec::new();
                                                                        for conv in &history.conversations {
                                                                            let chat_jid = conv.id.as_deref().unwrap_or_default();
                                                                            if chat_jid.is_empty() {
                                                                                continue;
                                                                            }

                                                                            let mut latest_ts = conv.last_msg_timestamp.or(conv.conversation_timestamp).unwrap_or(0);
                                                                            let mut latest_summary = None;

                                                                            for hist_msg in &conv.messages {
                                                                                if let Some(ref wmi) = hist_msg.message {
                                                                                    let msg_id = wmi.key.as_ref().and_then(|k| k.id.clone()).unwrap_or_default();
                                                                                    let sender = wmi.key.as_ref().and_then(|k| k.participant.clone())
                                                                                        .or_else(|| wmi.key.as_ref().and_then(|k| k.remote_jid.clone()))
                                                                                        .unwrap_or_else(|| chat_jid.to_string());
                                                                                    let from_me = wmi.key.as_ref().and_then(|k| k.from_me).unwrap_or(false);
                                                                                    let ts = wmi.message_timestamp.unwrap_or(0);
                                                                                    if ts > latest_ts {
                                                                                        latest_ts = ts;
                                                                                    }

                                                                                    let (summary, has_media) = if let Some(ref m) = wmi.message {
                                                                                        let s = crate::messages::extract_message_summary(m);
                                                                                        let h_m = crate::messages::message_has_media(m);
                                                                                        (s, h_m)
                                                                                    } else {
                                                                                        ("<empty>".to_string(), false)
                                                                                    };

                                                                                    if latest_summary.is_none() {
                                                                                        latest_summary = Some(summary.clone());
                                                                                    }

                                                                                    let mut raw_bytes = Vec::new();
                                                                                    if let Some(ref m) = wmi.message {
                                                                                        let _ = m.encode(&mut raw_bytes);
                                                                                    }

                                                                                    let is_starred = wmi.starred.unwrap_or(false);
                                                                                    stored_msgs.push(StoredMessage {
                                                                                        id: msg_id,
                                                                                        chat_jid: chat_jid.to_string(),
                                                                                        sender_jid: sender,
                                                                                        from_me,
                                                                                        timestamp: ts,
                                                                                        summary,
                                                                                        status: "delivered".to_string(),
                                                                                        has_media,
                                                                                        proto_bytes: raw_bytes,
                                                                                        is_starred,
                                                                                        is_revoked: false,
                                                                                        is_edited: false,
                                                                                    });
                                                                                }
                                                                            }

                                                                            let is_pinned = conv.pinned.map(|p| p > 0).unwrap_or(false);
                                                                            let is_archived = conv.archived.unwrap_or(false);
                                                                            let mute_until = conv.mute_end_time;

                                                                            let chat_name = conv.name.clone()
                                                                                .or_else(|| conv.display_name.clone())
                                                                                .or_else(|| conv.username.clone())
                                                                                .or_else(|| pushname_map.get(chat_jid).cloned());

                                                                            let stored_chat = StoredChat {
                                                                                jid: chat_jid.to_string(),
                                                                                name: chat_name,
                                                                                unread_count: conv.unread_count.unwrap_or(0),
                                                                                last_message_timestamp: latest_ts,
                                                                                last_message_summary: latest_summary,
                                                                                pinned: is_pinned,
                                                                                archived: is_archived,
                                                                                mute_until,
                                                                                ephemeral_expiration: conv.ephemeral_expiration.unwrap_or(0),
                                                                                ephemeral_setting_timestamp: conv.ephemeral_setting_timestamp,
                                                                            };
                                                                            stored_chats.push(stored_chat);
                                                                        }
                                                                        if !stored_msgs.is_empty() {
                                                                            let _ = store_h.save_messages_batch(&stored_msgs).await;
                                                                        }
                                                                        if !stored_chats.is_empty() {
                                                                            let _ = store_h.save_chats_batch(&stored_chats).await;
                                                                        }

                                                                        // Messages are persisted above; now build the Baileys payload, which
                                                                        // also trims each chat to its latest message.
                                                                        let processed = crate::history::process_history_sync(&mut history);

                                                                        // Persist LID-PN mappings (explicit ones plus those derived from chats)
                                                                        let mut entries = Vec::new();
                                                                        let mut lid_contacts = Vec::new();
                                                                        for m in &processed.lid_pn_mappings {
                                                                            signal_h.lid_mappings().store_mapping(&m.pn, &m.lid);
                                                                            entries.push(baileys_store::KeyEntry {
                                                                                key_type: "lid-mapping".to_string(),
                                                                                id: m.pn.clone(),
                                                                                data: m.lid.as_bytes().to_vec(),
                                                                            });
                                                                            lid_contacts.push(StoredContact {
                                                                                jid: m.pn.clone(),
                                                                                name: None,
                                                                                push_name: None,
                                                                                lid: Some(m.lid.clone()),
                                                                            });
                                                                        }
                                                                        if !lid_contacts.is_empty() {
                                                                            let _ = store_h.save_contacts_batch(&lid_contacts).await;
                                                                        }
                                                                        if !entries.is_empty() {
                                                                            let _ = store_h.set_keys(entries).await;
                                                                        }

                                                                        events_h.emit(BaileysEvent::HistorySync {
                                                                            sync_type,
                                                                            progress,
                                                                            conversations: history.conversations,
                                                                            status_v3_messages: history.status_v3_messages,
                                                                            contacts: processed.contacts,
                                                                            messages: processed.messages,
                                                                            lid_pn_mappings: processed.lid_pn_mappings,
                                                                            past_participants: history.past_participants,
                                                                            is_latest,
                                                                            chunk_order: notif_clone.chunk_order,
                                                                            peer_data_request_session_id: notif_clone.peer_data_request_session_id.clone(),
                                                                        });
                                                                    }
                                                                    Err(e) => {
                                                                        warn!("Failed to download/decrypt HistorySync archive: {}", e);
                                                                    }
                                                                }
                                                            });
                                                        }
                                                    }

                                                    // Persist live inbound message in SQLite
                                                    let store_live = Arc::clone(store_clone);
                                                    let events_live = events_clone.clone();
                                                    let msg_for_db = msg_info.clone();
                                                    let anti_revoke_live = config_clone.anti_revoke;
                                                    tokio::spawn(async move {
                                                        let msg_id = msg_for_db.key.as_ref().and_then(|k| k.id.clone()).unwrap_or_default();
                                                        let chat_jid = msg_for_db.key.as_ref().and_then(|k| k.remote_jid.clone()).unwrap_or_default();
                                                        let author = msg_for_db.key.as_ref().and_then(|k| k.participant.clone()).unwrap_or_else(|| chat_jid.clone());
                                                        let from_me = msg_for_db.key.as_ref().and_then(|k| k.from_me).unwrap_or(false);
                                                        let ts = msg_for_db.message_timestamp.unwrap_or(0);

                                                        // Reactions are also stored and emitted as messages, like Baileys.
                                                        if let Some(ev) = crate::notifications::reaction_event(&msg_for_db) {
                                                            events_live.emit(ev);
                                                        }
                                                        if let Some(ev) = crate::notifications::member_tag_event(&msg_for_db) {
                                                            events_live.emit(ev);
                                                        }

                                                        if let Some(ref m) = msg_for_db.message {
                                                            if let Some(parsed_pm) = crate::ephemeral::parse_protocol_message(m, &chat_jid) {
                                                                match parsed_pm {
                                                                    crate::ephemeral::ParsedProtocolMessage::MessageEdit {
                                                                        target_chat_jid,
                                                                        target_msg_id,
                                                                        new_text,
                                                                        edited_message,
                                                                    } => {
                                                                        let mut edited_bytes = Vec::new();
                                                                        let _ = edited_message.encode(&mut edited_bytes);
                                                                        let _ = store_live.update_message_content(&target_chat_jid, &target_msg_id, &new_text, Some(&edited_bytes)).await;
                                                                        events_live.emit(BaileysEvent::MessageEdited {
                                                                            chat_jid: target_chat_jid,
                                                                            message_id: target_msg_id,
                                                                            new_text,
                                                                        });
                                                                        return;
                                                                    }
                                                                    crate::ephemeral::ParsedProtocolMessage::Revoke {
                                                                        target_chat_jid,
                                                                        target_msg_id,
                                                                        participant,
                                                                    } => {
                                                                        let _ = store_live.revoke_message_ex(&target_chat_jid, &target_msg_id, anti_revoke_live).await;
                                                                        if anti_revoke_live {
                                                                            info!("[Anti-Revoke] Message {} deleted by {:?} in {} - Original preserved in SQLite", target_msg_id, participant, target_chat_jid);
                                                                        }
                                                                        events_live.emit(BaileysEvent::MessageRevoked {
                                                                            chat_jid: target_chat_jid,
                                                                            message_id: target_msg_id,
                                                                            participant,
                                                                        });
                                                                        return;
                                                                    }
                                                                    crate::ephemeral::ParsedProtocolMessage::EphemeralSetting {
                                                                        target_chat_jid,
                                                                        expiration,
                                                                        timestamp,
                                                                    } => {
                                                                        let _ = store_live.update_chat_ephemeral(&target_chat_jid, expiration, timestamp).await;
                                                                        events_live.emit(BaileysEvent::ChatEphemeralUpdate {
                                                                            chat_jid: target_chat_jid,
                                                                            ephemeral_expiration: expiration,
                                                                        });
                                                                        return;
                                                                    }
                                                                    crate::ephemeral::ParsedProtocolMessage::Other => {}
                                                                }
                                                            }
                                                        }

                                                        let (summary, has_media) = if let Some(ref m) = msg_for_db.message {
                                                            let s = crate::messages::extract_message_summary(m);
                                                            let h_m = crate::messages::message_has_media(m);
                                                            (s, h_m)
                                                        } else {
                                                            ("<empty>".to_string(), false)
                                                        };

                                                        let mut raw_bytes = Vec::new();
                                                        if let Some(ref m) = msg_for_db.message {
                                                            let _ = m.encode(&mut raw_bytes);
                                                        }

                                                        let stored_m = StoredMessage {
                                                            id: msg_id,
                                                            chat_jid: chat_jid.clone(),
                                                            sender_jid: author,
                                                            from_me,
                                                            timestamp: ts,
                                                            summary: summary.clone(),
                                                            status: "received".to_string(),
                                                            has_media,
                                                            proto_bytes: raw_bytes,
                                                            is_starred: false,
                                                            is_revoked: false,
                                                            is_edited: false,
                                                        };
                                                        let _ = store_live.save_message(&stored_m).await;

                                                        let stored_c = StoredChat {
                                                            jid: chat_jid,
                                                            name: None,
                                                            unread_count: if from_me { 0 } else { 1 },
                                                            last_message_timestamp: ts,
                                                            last_message_summary: Some(summary),
                                                            pinned: false,
                                                            archived: false,
                                                            mute_until: None,
                                                            ephemeral_expiration: 0,
                                                            ephemeral_setting_timestamp: None,
                                                        };
                                                        let _ = store_live.save_chat(&stored_c).await;
                                                    });

                                                    // Process incoming PollCreationMessage or PollUpdateMessage
                                                    let store_poll = Arc::clone(store_clone);
                                                    let events_poll = events_clone.clone();
                                                    let proto_for_poll = proto.clone();
                                                    let msg_id_poll = msg_id.clone();
                                                    let from_poll = from.clone();
                                                    let author_poll = author_jid.clone();

                                                    tokio::spawn(async move {
                                                        if let Some(poll) = proto_for_poll.poll_creation_message.as_ref()
                                                            .or(proto_for_poll.poll_creation_message_v2.as_ref())
                                                            .or(proto_for_poll.poll_creation_message_v3.as_ref()) {
                                                            let q = poll.name.clone().unwrap_or_else(|| "Poll".to_string());
                                                            let opts: Vec<String> = poll.options.iter().filter_map(|o| o.option_name.clone()).collect();
                                                            let enc_key = poll.enc_key.clone().unwrap_or_default();
                                                            let sel = poll.selectable_options_count.unwrap_or(1);
                                                            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                                                            let stored_poll = StoredPoll {
                                                                id: msg_id_poll.clone(),
                                                                chat_jid: from_poll.clone(),
                                                                sender_jid: author_poll.clone(),
                                                                question: q,
                                                                options: opts,
                                                                enc_key,
                                                                selectable_count: sel,
                                                                created_at: now,
                                                            };
                                                            let _ = store_poll.save_poll(&stored_poll).await;
                                                        }

                                                        if let Some(ref update) = proto_for_poll.poll_update_message {
                                                            if let (Some(ref key), Some(ref vote)) = (&update.poll_creation_message_key, &update.vote) {
                                                                if let Some(ref poll_id) = key.id {
                                                                    if let Ok(Some(stored_poll)) = store_poll.get_poll(poll_id).await {
                                                                        if let Ok(selected_hashes) = crate::polls::decrypt_poll_vote(
                                                                            vote,
                                                                            &stored_poll.sender_jid,
                                                                            poll_id,
                                                                            &stored_poll.enc_key,
                                                                            &author_poll,
                                                                        ) {
                                                                            let mut selected_options = Vec::new();
                                                                            for opt in &stored_poll.options {
                                                                                let h = crate::polls::hash_poll_option(opt);
                                                                                if selected_hashes.iter().any(|sh| sh.as_slice() == &h[..]) {
                                                                                    selected_options.push(opt.clone());
                                                                                }
                                                                            }
                                                                            let _ = store_poll.save_poll_vote(poll_id, &author_poll, &selected_options).await;
                                                                            if let Ok(Some(tally)) = store_poll.get_poll_tally(poll_id).await {
                                                                                events_poll.emit(BaileysEvent::PollVote {
                                                                                    poll_id: poll_id.clone(),
                                                                                    voter_jid: author_poll.clone(),
                                                                                    selected_options,
                                                                                    tally,
                                                                                });
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }

                                                        if let Some(ref ord) = proto_for_poll.order_message {
                                                            let order_id = ord.order_id.clone().unwrap_or_else(|| msg_id_poll.clone());
                                                            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                                                            let stored_order = StoredOrder {
                                                                order_id,
                                                                chat_jid: from_poll.clone(),
                                                                seller_jid: ord.seller_jid.clone().unwrap_or_else(|| author_poll.clone()),
                                                                item_count: ord.item_count.unwrap_or(1),
                                                                status: "INQUIRY".to_string(),
                                                                surface: ord.catalog_type.clone().or_else(|| Some("CATALOG".to_string())),
                                                                order_title: ord.order_title.clone(),
                                                                message: ord.message.clone(),
                                                                total_amount_1000: ord.total_amount1000.unwrap_or(0),
                                                                total_currency_code: ord.total_currency_code.clone().unwrap_or_else(|| "USD".to_string()),
                                                                token: ord.token.clone(),
                                                                created_at: now,
                                                            };
                                                            let _ = store_poll.save_order(&stored_order).await;
                                                        }
                                                    });

                                                    // Process incoming Status / Broadcast Stories
                                                    if from == "status@broadcast" {
                                                        let store_story = Arc::clone(store_clone);
                                                        let events_story = events_clone.clone();
                                                        let proto_for_story = proto.clone();
                                                        let msg_id_story = msg_id.clone();
                                                        let author_story = author_jid.clone();
                                                        let now_story = std::time::SystemTime::now()
                                                            .duration_since(std::time::UNIX_EPOCH)
                                                            .unwrap()
                                                            .as_secs();

                                                        tokio::spawn(async move {
                                                            let story = crate::status::parse_story_from_proto(
                                                                &msg_id_story,
                                                                &author_story,
                                                                None,
                                                                now_story,
                                                                &proto_for_story,
                                                            );
                                                            let _ = store_story.save_story(&story).await;
                                                            events_story.emit(BaileysEvent::StoryReceived { story });
                                                        });
                                                    }

                                                    events_clone.emit(BaileysEvent::Message {
                                                        message: Box::new(msg_info),
                                                    });

                                                }
                                            }
                                        }

                                        "receipt" => {
                                            let id = node.get_attr("id").unwrap_or("").to_string();
                                            let from = node.get_attr("from").unwrap_or("unknown").to_string();
                                            let receipt_type = node.get_attr("type").unwrap_or("delivery").to_string();

                                            // Send ACK for the receipt
                                            let ack = build_ack_node(&node, None, creds.me_jid.as_deref());
                                            let _ = socket.send_node(ack).await;

                                            // If recipient could not decrypt our sent message, re-encrypt & resend with updated keys
                                            if receipt_type == "retry" {
                                                let retry_client = client.clone();
                                                let creds_snap = creds.clone();
                                                let retry_node = node.clone();

                                                tokio::spawn(async move {
                                                    if let Err(e) = crate::retry::handle_outbound_retry_receipt(
                                                        &retry_node,
                                                        &creds_snap,
                                                        retry_client.store.as_ref(),
                                                        &retry_client.signal,
                                                        &retry_client.socket,
                                                        &retry_client.pending_iq,
                                                        &retry_client.recent_messages,
                                                        &retry_client.outbound_retries,
                                                    ).await {
                                                        tracing::warn!("Failed to process outbound retry receipt: {}", e);
                                                    }
                                                });
                                            }

                                            events_clone.emit(BaileysEvent::Receipt {
                                                key_id: id,
                                                remote_jid: from,
                                                status: receipt_type,
                                            });
                                        }

                                        "chatstate" => {
                                            let from = node.get_attr("from").unwrap_or("unknown").to_string();
                                            let participant = node.get_attr("participant").map(|s| s.to_string());
                                            let jid = participant.unwrap_or(from);

                                            let state = if node.get_child("composing").is_some() {
                                                "composing"
                                            } else if node.get_child("paused").is_some() {
                                                "paused"
                                            } else {
                                                "active"
                                            };

                                            let ack = build_ack_node(&node, None, creds.me_jid.as_deref());
                                            let _ = socket.send_node(ack).await;

                                            events_clone.emit(BaileysEvent::Presence {
                                                jid,
                                                presence: state.to_string(),
                                            });
                                        }

                                        "call" => {
                                            // 1. Send ACK immediately for the call node
                                            let ack = build_ack_node(&node, None, creds.me_jid.as_deref());
                                            let _ = socket.send_node(ack).await;

                                            // 2. Parse call stanza into CallEvent
                                            match crate::call::parse_call_stanza(&node) {
                                                Ok(mut call_event) => {
                                                    let call_id = call_event.id.clone();
                                                    let status = call_event.status.clone();

                                                    // 3. Correlate with active_calls cache
                                                    {
                                                        let mut active = active_calls_clone.lock().await;
                                                        if status == "offer" {
                                                            active.insert(call_id.clone(), call_event.clone());
                                                        } else if let Some(existing) = active.get(&call_id) {
                                                            call_event.is_video = existing.is_video;
                                                            call_event.is_group = existing.is_group;
                                                            if call_event.caller_pn.is_none() {
                                                                call_event.caller_pn = existing.caller_pn.clone();
                                                            }
                                                            if call_event.group_jid.is_none() {
                                                                call_event.group_jid = existing.group_jid.clone();
                                                            }
                                                        }

                                                        if status == "reject"
                                                            || status == "accept"
                                                            || status == "timeout"
                                                            || status == "terminate"
                                                        {
                                                            active.remove(&call_id);
                                                        }
                                                    }

                                                    // 4. Save call record to SQLite
                                                    let store_call = Arc::clone(store_clone);
                                                    let stored = StoredCall {
                                                        call_id: call_event.id.clone(),
                                                        chat_jid: call_event.chat_jid.clone(),
                                                        caller_jid: call_event.from.clone(),
                                                        is_group: call_event.is_group,
                                                        group_jid: call_event.group_jid.clone(),
                                                        is_video: call_event.is_video,
                                                        status: call_event.status.clone(),
                                                        timestamp: call_event.timestamp,
                                                        duration: None,
                                                    };
                                                    tokio::spawn(async move {
                                                        let _ = store_call.save_call(&stored).await;
                                                    });

                                                    // 5. Emit Call event to client
                                                    events_clone.emit(BaileysEvent::Call { call: call_event });
                                                }
                                                Err(e) => {
                                                    warn!("Failed to parse incoming call stanza: {}", e);
                                                }
                                            }
                                        }

                                        "ib" => {
                                            if node.get_child("offline_preview").is_some() {
                                                debug!("Received <ib><offline_preview/>, acknowledging offline batch count=100...");
                                                let reply = BinaryNode::new("ib")
                                                    .with_child(BinaryNode::new("offline_batch").with_attr("count", "100"));
                                                let _ = socket.send_node(reply).await;
                                            }

                                            if let Some(dirty_node) = node.get_child("dirty") {
                                                let dirty_type = dirty_node.get_attr("type").unwrap_or("account_sync").to_string();
                                                debug!("Received <ib><dirty type='{}'/>, cleaning dirty bit and scheduling AppState resync...", dirty_type);
                                                let client_sync = client.clone();
                                                let dirty_type_clone = dirty_type.clone();
                                                tokio::spawn(async move {
                                                    let _ = client_sync.clean_dirty_bits(&dirty_type_clone, None).await;
                                                    if let Err(e) = client_sync.resync_app_state(crate::syncd::ALL_COLLECTIONS, false).await {
                                                        debug!("Dirty bit AppState resync note: {}", e);
                                                    }
                                                });
                                            }
                                        }

                                        "stream:error" => {
                                            let code = node.get_attr("code");
                                            let is_conflict = node.get_child("conflict").is_some() || code == Some("440");
                                            if code == Some("405") {
                                                warn!("WhatsApp server returned 405 (Method Not Allowed / Outdated App Version). Invalidating cached version to force fresh resolution...");
                                                crate::version::invalidate_cached_version(store_clone.as_ref()).await;
                                                reconnect_delay = Duration::from_millis(500);
                                            } else if is_conflict {
                                                warn!("WhatsApp server sent connection conflict (replaced): {:?}. Backing off reconnect for 15s to allow old socket release...", node);
                                                reconnect_delay = Duration::from_secs(15);
                                            } else if code == Some("515") {
                                                info!("WhatsApp server sent restart required (515) after pairing. Reconnecting in 2.0s...");
                                                reconnect_delay = Duration::from_secs(2);
                                            } else {
                                                warn!("WhatsApp server sent <stream:error>: {:?}", node);
                                                reconnect_delay = Duration::from_millis(1500);
                                            }
                                            session_terminated = true;
                                        }

                                        "failure" => {
                                            let reason = node.get_attr("reason").unwrap_or("unknown");
                                            warn!("WhatsApp server sent <failure reason='{}'>", reason);
                                            if reason == "405" {
                                                warn!("WhatsApp login failed with 405 (Outdated client version). Invalidating cached version...");
                                                crate::version::invalidate_cached_version(store_clone.as_ref()).await;
                                            } else if reason == "401" {
                                                warn!("Authentication failed (401 Unauthorized): resetting invalid stored session.");
                                                let reset_creds = AuthCreds::init();
                                                let _ = store_clone.save_creds(&reset_creds).await;
                                                // The next pairing is a new identity: drop what belonged to the old one.
                                                match crate::registration::clear_identity_bound_keys(store_clone.as_ref()).await {
                                                    Ok(n) => debug!("Cleared {} keys bound to the rejected session.", n),
                                                    Err(e) => warn!("Failed to clear keys of the rejected session: {}", e),
                                                }
                                                events_clone.emit(BaileysEvent::Disconnected {
                                                    reason: "Logged out: invalid session credentials (401)".into(),
                                                });
                                            }
                                            session_terminated = true;
                                        }

                                        "ack" => {
                                            let id = node.get_attr("id").unwrap_or_default().to_string();
                                            let class = node.get_attr("class").unwrap_or_default();
                                            let from = node.get_attr("from").unwrap_or_default().to_string();
                                            let participant = node.get_attr("participant").unwrap_or_default().to_string();
                                            let err_attr = node.get_attr("error")
                                                .or_else(|| node.get_child("error").and_then(|e| e.get_attr("code")));
                                            let remote_jid = if !participant.is_empty() {
                                                participant
                                            } else if !from.is_empty() {
                                                from.clone()
                                            } else {
                                                node.get_attr("to").unwrap_or_default().to_string()
                                            };

                                            if let Some(err_code) = err_attr {
                                                warn!("Server returned NACK for stanza id='{}', class='{}', error={}: {:?}", id, class, err_code, node);

                                                // Automatic recovery for Group Sender Key Desynchronization (Error 421)
                                                if err_code == "421" && (from.ends_with("@g.us") || remote_jid.ends_with("@g.us")) {
                                                    let grp_jid = if from.ends_with("@g.us") { from.clone() } else { remote_jid.clone() };
                                                    info!("Error 421 detected for group {}: invalidating desynchronized sender key and clearing memory cache to force fresh session creation", grp_jid);
                                                    {
                                                        let mut mem = client.sender_key_memory.lock().await;
                                                        mem.remove(&grp_jid);
                                                    }
                                                    let store_ack = Arc::clone(store_clone);
                                                    let grp_target = grp_jid.clone();
                                                    let me_jid_opt = creds.me_jid.clone();
                                                    let me_lid_opt = creds.me_lid.clone();
                                                    tokio::spawn(async move {
                                                        let mut keys_to_remove = Vec::new();
                                                        if let Some(ref jid) = me_jid_opt {
                                                            let (u, _) = jid.split_once('@').unwrap_or((jid.as_str(), ""));
                                                            let (u_clean, d) = u.split_once(':').unwrap_or((u, "0"));
                                                            keys_to_remove.push(format!("{}::{}:{}", grp_target, u_clean, d));
                                                            keys_to_remove.push(format!("{}::{}::{}", grp_target, u_clean, d));
                                                        }
                                                        if let Some(ref lid) = me_lid_opt {
                                                            let (u, _) = lid.split_once('@').unwrap_or((lid.as_str(), ""));
                                                            let (u_clean, d) = u.split_once(':').unwrap_or((u, "0"));
                                                            keys_to_remove.push(format!("{}::{}:{}", grp_target, u_clean, d));
                                                            keys_to_remove.push(format!("{}::{}::{}", grp_target, u_clean, d));
                                                        }
                                                        let _ = store_ack.remove_keys("sender-key", &keys_to_remove).await;
                                                    });
                                                }

                                                let store_ack = Arc::clone(store_clone);
                                                let id_clone = id.clone();
                                                let err_status = format!("error_{}", err_code);
                                                let err_status_clone = err_status.clone();
                                                tokio::spawn(async move {
                                                    let _ = store_ack.update_message_status(&id_clone, &err_status_clone).await;
                                                });
                                                events_clone.emit(BaileysEvent::Receipt {
                                                    key_id: id,
                                                    remote_jid,
                                                    status: err_status,
                                                });
                                            } else {
                                                info!("Server returned ACK for stanza id='{}', class='{}'", id, class);
                                                let store_ack = Arc::clone(store_clone);
                                                let id_clone = id.clone();
                                                tokio::spawn(async move {
                                                    let _ = store_ack.update_message_status(&id_clone, "server_ack").await;
                                                });
                                                events_clone.emit(BaileysEvent::Receipt {
                                                    key_id: id,
                                                    remote_jid,
                                                    status: "server_ack".to_string(),
                                                });
                                            }
                                        }

                                        _ => {
                                            debug!("Unhandled stanza node: tag='{}', attrs={:?}", node.tag, node.attrs);
                                        }
                                    }
                                }
                                None => {
                                    warn!("WhatsApp gateway connection closed by server.");
                                    session_terminated = true;
                                }
                            }
                        }
                    }
                }

                is_connected_clone.store(false, Ordering::SeqCst);
                client.set_connection_state(crate::state::ConnectionState::Disconnected);
                {
                    let mut lock = socket_holder.lock().await;
                    if let Some(s) = lock.take() {
                        s.close().await;
                    }
                }
                socket.close().await;

                if !is_running_clone.load(Ordering::SeqCst) {
                    debug!("Client connection supervisor stopped.");
                    break;
                }

                is_first_connection = false;
                events_clone.emit(BaileysEvent::Disconnected {
                    reason: "Connection reset by server, reconnecting...".into(),
                });

                let jitter = Duration::from_millis(rand::random::<u64>() % 1500);
                let effective_delay = reconnect_delay.max(Duration::from_secs(3)) + jitter;
                reconnect_delay = (reconnect_delay * 2).min(Duration::from_secs(30));
                client.set_connection_state(crate::state::ConnectionState::Reconnecting {
                    attempt: 1,
                    next_retry_millis: effective_delay.as_millis() as u64,
                });
                client.metrics().record_reconnect();
                warn!(
                    "Reconnecting to WhatsApp Web Gateway in {:.1}s...",
                    effective_delay.as_secs_f32()
                );
                tokio::time::sleep(effective_delay).await;
            }
        });

        Ok(())
    }
    /// Dynamically requests an 8-character phone number pairing code if not authenticated.
    pub async fn request_pairing_code(&self, phone: &str) -> Result<String, CoreError> {
        let creds = match self
            .store
            .get_creds()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?
        {
            Some(c) => c,
            None => {
                let fresh = AuthCreds::init();
                self.store
                    .save_creds(&fresh)
                    .await
                    .map_err(|e| CoreError::Store(e.to_string()))?;
                fresh
            }
        };
        if creds.me_jid.is_some() {
            return Err(CoreError::Protocol(
                "Client is already authenticated".into(),
            ));
        }

        let clean_phone = phone.replace(['+', ' ', '-'], "");
        let state = Arc::new(CompanionPairingState::new(&clean_phone));
        let hello_node = state.build_companion_hello_iq(&creds, &self.config)?;

        // Ensure connection is running
        if !self.is_running.load(Ordering::SeqCst) {
            let _ = self.connect().await;
        }

        // Wait up to 10 seconds for the socket to become ready
        let mut socket_opt = {
            let lock = self.socket.lock().await;
            lock.clone()
        };
        if socket_opt.is_none() {
            let start = std::time::Instant::now();
            while socket_opt.is_none() && start.elapsed() < Duration::from_secs(10) {
                tokio::time::sleep(Duration::from_millis(250)).await;
                let lock = self.socket.lock().await;
                socket_opt = lock.clone();
            }
        }
        let socket = socket_opt
            .ok_or_else(|| CoreError::Network("Client not connected to WhatsApp gateway".into()))?;
        socket.send_node(hello_node).await?;

        let display_code = if state.pairing_code.len() == 8 {
            format!(
                "{}-{}",
                &state.pairing_code[0..4],
                &state.pairing_code[4..8]
            )
        } else {
            state.pairing_code.clone()
        };

        {
            let mut lock = self.companion_state.lock().await;
            *lock = Some(Arc::clone(&state));
        }

        self.events.emit(BaileysEvent::PairingCode {
            code: display_code.clone(),
        });

        Ok(display_code)
    }
    /// Cleans dirty bits on WhatsApp server (e.g. "account_sync" or "groups").
    pub async fn clean_dirty_bits(
        &self,
        dirty_type: &str,
        from_timestamp: Option<u64>,
    ) -> Result<(), CoreError> {
        let msg_id = format!("clean_{:X}", rand::random::<u64>());
        let mut clean_node = BinaryNode::new("clean").with_attr("type", dirty_type);
        if let Some(ts) = from_timestamp {
            clean_node = clean_node.with_attr("timestamp", ts.to_string());
        }
        let iq = BinaryNode::new("iq")
            .with_attr("id", msg_id)
            .with_attr("to", S_WHATSAPP_NET)
            .with_attr("type", "set")
            .with_attr("xmlns", "urn:xmpp:whatsapp:dirty")
            .with_child(clean_node);
        self.send_iq(iq, Duration::from_secs(10)).await?;
        Ok(())
    }
    /// Fetches the account reachout timelock timestamp via WhatsApp GraphQL MEX query.
    pub async fn fetch_account_reachout_timelock(
        &self,
    ) -> Result<Option<(bool, Option<String>, Option<u64>)>, CoreError> {
        let vars = serde_json::json!({});
        let iq = crate::newsletter::build_mex_iq(crate::newsletter::QUERY_REACHOUT_TIMELOCK, vars);
        let res = self.send_iq(iq, Duration::from_secs(12)).await?;
        let data = crate::newsletter::parse_mex_response(&res)?;
        Ok(crate::newsletter::parse_reachout_timelock_response(&data))
    }
    /// Fetches the new chat message cap limit via WhatsApp GraphQL MEX query.
    pub async fn fetch_new_chat_message_cap(&self) -> Result<Option<serde_json::Value>, CoreError> {
        let vars = serde_json::json!({});
        let iq =
            crate::newsletter::build_mex_iq(crate::newsletter::QUERY_MESSAGE_CAPPING_INFO, vars);
        let res = self.send_iq(iq, Duration::from_secs(12)).await?;
        let data = crate::newsletter::parse_mex_response(&res)?;
        Ok(crate::newsletter::parse_message_capping_response(&data))
    }
    /// Fetches the WhatsApp AI Bot list v2.
    pub async fn get_bot_list_v2(&self) -> Result<Vec<(String, String)>, CoreError> {
        let msg_id = format!("bot_{:X}", rand::random::<u64>());
        let iq = BinaryNode::new("iq")
            .with_attr("id", msg_id)
            .with_attr("to", S_WHATSAPP_NET)
            .with_attr("type", "get")
            .with_attr("xmlns", "bot")
            .with_child(BinaryNode::new("bot").with_attr("v", "2"));
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        let mut bots = Vec::new();
        if let Some(bot_node) = res.get_child("bot") {
            for section in bot_node
                .get_children()
                .iter()
                .filter(|n| n.tag == "section")
            {
                for bot in section.get_children().iter().filter(|n| n.tag == "bot") {
                    if let (Some(jid), Some(persona_id)) =
                        (bot.get_attr("jid"), bot.get_attr("persona_id"))
                    {
                        bots.push((jid.to_string(), persona_id.to_string()));
                    }
                }
            }
        }
        Ok(bots)
    }
}
