//! Domain submodule for messaging operations on BaileysClient.

use super::*;

impl BaileysClient {
    /// Sends an arbitrary Protobuf `Message` (text, media, reaction, etc.) to a recipient or group.
    pub async fn send_message(
        &self,
        to_jid: &str,
        proto_msg: Message,
    ) -> Result<String, CoreError> {
        self.send_message_with_options(to_jid, proto_msg, MessageOptions::default())
            .await
    }
    /// Relays an arbitrary Protobuf `Message` with options (alias for `send_message_with_options`).
    pub async fn relay_message(
        &self,
        to_jid: &str,
        proto_msg: Message,
        options: MessageOptions,
    ) -> Result<String, CoreError> {
        self.send_message_with_options(to_jid, proto_msg, options)
            .await
    }
    /// Sends an arbitrary Protobuf `Message` with relay options (such as custom status broadcast audience).
    pub async fn send_message_with_options(
        &self,
        to_jid: &str,
        proto_msg: Message,
        options: MessageOptions,
    ) -> Result<String, CoreError> {
        let creds = self
            .store
            .get_creds()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?
            .ok_or_else(|| CoreError::Auth("No active session credentials found".into()))?;

        let socket_opt = {
            let lock = self.socket.lock().await;
            lock.clone()
        };

        if socket_opt.is_none() && !self.config.wa_websocket_url.starts_with("mock://") {
            return Err(CoreError::Network("Client not connected".into()));
        }

        let normalized_jid = if !to_jid.contains('@') {
            format!("{}@s.whatsapp.net", to_jid)
        } else {
            to_jid.to_string()
        };

        let rand_bytes: [u8; 9] = rand::random();
        let msg_id = format!("3EB0{}", hex::encode(rand_bytes).to_uppercase());

        // If chat has an active disappearing timer and this is not already an ephemeral or protocol message
        let mut proto_msg = if proto_msg.protocol_message.is_none()
            && proto_msg.ephemeral_message.is_none()
            && !normalized_jid.ends_with("@newsletter")
        {
            if let Ok(Some((exp, _))) = self.store.get_chat_ephemeral(&normalized_jid).await {
                if exp > 0 {
                    crate::ephemeral::wrap_ephemeral_message(proto_msg)
                } else {
                    proto_msg
                }
            } else {
                proto_msg
            }
        } else {
            proto_msg
        };

        // If message contains an unwrapped interactive_message, wrap it into view_once_message
        // with MessageContextInfo v2 as required by official WhatsApp clients.
        if proto_msg.interactive_message.is_some() {
            if let Some(inter) = proto_msg.interactive_message.take() {
                proto_msg = crate::interactive::wrap_interactive_view_once(*inter);
            }
        }

        // Ensure message_context_info has a message_secret (WhatsApp Web / Baileys parity)
        if proto_msg.reaction_message.is_none()
            && proto_msg.poll_update_message.is_none()
            && !normalized_jid.ends_with("@newsletter")
        {
            let mut ctx = proto_msg.message_context_info.unwrap_or_default();
            if ctx.message_secret.as_ref().is_none_or(|s| s.is_empty()) {
                let mut secret = vec![0u8; 32];
                rand::thread_rng().fill_bytes(&mut secret);
                ctx.message_secret = Some(secret);
            }
            proto_msg.message_context_info = Some(ctx);
        }

        // 1. Serialize Protobuf Message
        let mut msg_bytes = Vec::new();
        proto_msg.encode(&mut msg_bytes).map_err(|e| {
            CoreError::Protocol(format!("Failed to encode Message protobuf: {}", e))
        })?;

        let padded = pad_random_max16(&msg_bytes);

        // 2. Encrypt based on target type
        let msg_node = if normalized_jid.ends_with("@newsletter") {
            let (msg_type, media_type) = if proto_msg.image_message.is_some() {
                ("media", Some("image"))
            } else if proto_msg.video_message.is_some() {
                ("media", Some("video"))
            } else if proto_msg.audio_message.is_some() {
                ("media", Some("audio"))
            } else if proto_msg.document_message.is_some() {
                ("media", Some("document"))
            } else if proto_msg.sticker_message.is_some() {
                ("media", Some("image"))
            } else {
                ("text", None)
            };

            let mut pt_node = BinaryNode::new("plaintext").with_bytes_content(msg_bytes.clone());
            if let Some(mt) = media_type {
                pt_node = pt_node.with_attr("mediatype", mt);
            }

            let mut node = BinaryNode::new("message")
                .with_attr("to", &normalized_jid)
                .with_attr("id", &msg_id)
                .with_attr("type", msg_type);
            if let Some(ref handle) = options.media_handle {
                node = node.with_attr("media_id", handle);
            }
            node.with_child(pt_node)
        } else if normalized_jid.ends_with("@g.us") {
            // ── Baileys relayMessage parity for groups ──
            // 1. Resolve participants + addressing mode
            let meta = self.group_fetch_metadata(&normalized_jid).await.ok();
            let raw_participants: Vec<String> = meta
                .as_ref()
                .map(|m| m.participants.iter().map(|p| p.jid.clone()).collect())
                .unwrap_or_default();

            let is_lid_group = meta
                .as_ref()
                .and_then(|m| m.addressing_mode.as_deref())
                .map(|m| m == "lid")
                .unwrap_or_else(|| raw_participants.iter().any(|j| j.ends_with("@lid")));
            let addressing_mode = if is_lid_group { "lid" } else { "pn" };

            // 2. Sender identity must match group addressing
            let me_pn = creds
                .me_jid
                .clone()
                .unwrap_or_else(|| S_WHATSAPP_NET.to_string());
            let me_lid = creds.me_lid.clone();
            let group_sender_identity = if is_lid_group {
                creds.me_lid.clone().unwrap_or_else(|| me_pn.clone())
            } else {
                me_pn.clone()
            };

            // 3. Encrypt group payload -> ciphertext + root SKDM
            let (ciphertext, skdm_opt) = self
                .signal
                .encrypt_group_message(&normalized_jid, &group_sender_identity, &padded)
                .await
                .map_err(|e| CoreError::Crypto(e.to_string()))?;

            // 4. Target participants must ONLY use the domain of the group (no mixed lid/pn!)
            let mut target_users: Vec<String> = if let Some(ref m) = meta {
                m.participants
                    .iter()
                    .map(|p| {
                        if is_lid_group {
                            if p.jid.ends_with("@lid") {
                                baileys_binary::jid::jid_normalized_user(&p.jid)
                            } else if let Some(ref lid) = p.lid {
                                baileys_binary::jid::jid_normalized_user(lid)
                            } else {
                                baileys_binary::jid::jid_normalized_user(&p.jid)
                            }
                        } else {
                            if !p.jid.ends_with("@lid") {
                                baileys_binary::jid::jid_normalized_user(&p.jid)
                            } else if let Some(ref pn) = p.phone_number {
                                baileys_binary::jid::jid_normalized_user(pn)
                            } else {
                                baileys_binary::jid::jid_normalized_user(&p.jid)
                            }
                        }
                    })
                    .collect()
            } else {
                raw_participants
                    .into_iter()
                    .filter(|j| {
                        if is_lid_group {
                            j.ends_with("@lid")
                        } else {
                            !j.ends_with("@lid")
                        }
                    })
                    .map(|j| baileys_binary::jid::jid_normalized_user(&j))
                    .collect()
            };

            let own_target_user = baileys_binary::jid::jid_normalized_user(&group_sender_identity);
            if !target_users.contains(&own_target_user) {
                target_users.push(own_target_user);
            }

            // 5. Expand devices for each user (excluding hosted devices)
            let mut all_target_devices: Vec<String> = Vec::new();
            let mut all_group_devices: Vec<String> = Vec::new();
            for user in &target_users {
                let (u, server) = user.split_once('@').unwrap_or((
                    user.as_str(),
                    if is_lid_group {
                        "lid"
                    } else {
                        "s.whatsapp.net"
                    },
                ));
                let full_user_jid = format!("{}@{}", u, server);
                let devs = self
                    .get_user_devices(&full_user_jid)
                    .await
                    .unwrap_or_default();
                let devs = if devs.is_empty() { vec![0u32] } else { devs };
                for d in devs {
                    if d == 99 {
                        continue; // Hosted devices (device 99) never receive SKDM and not in phash
                    }
                    let dj = if d == 0 {
                        format!("{}@{}", u, server)
                    } else {
                        format!("{}:{}@{}", u, d, server)
                    };
                    if !all_group_devices.contains(&dj) {
                        all_group_devices.push(dj.clone());
                    }
                    // Exclude this exact sending device from SKDM distribution
                    let me_pn_exact = me_pn.as_str();
                    let me_lid_exact = me_lid.as_deref().unwrap_or("");
                    if dj.as_str() == me_pn_exact || dj.as_str() == me_lid_exact {
                        continue;
                    }
                    if !all_target_devices.contains(&dj) {
                        all_target_devices.push(dj);
                    }
                }
            }

            // 6. Check sender-key-memory: only send SKDM to devices that haven't received it yet
            let mut sender_key_recipients = Vec::new();
            {
                let mut mem = self.sender_key_memory.lock().await;
                let group_mem = mem.entry(normalized_jid.clone()).or_default();
                for dj in &all_target_devices {
                    if !group_mem.contains(dj) {
                        sender_key_recipients.push(dj.clone());
                    }
                }
            }

            let mut participant_nodes = Vec::new();
            let mut include_device_identity = false;

            if !sender_key_recipients.is_empty() {
                if let Some(skdm) = skdm_opt {
                    let skdm_msg = Message {
                        sender_key_distribution_message: Some(
                            baileys_proto::proto::message::SenderKeyDistributionMessage {
                                group_id: Some(normalized_jid.clone()),
                                axolotl_sender_key_distribution_message: Some(
                                    skdm.serialize().to_vec(),
                                ),
                            },
                        ),
                        ..Default::default()
                    };
                    let mut skdm_bytes = Vec::new();
                    skdm_msg.encode(&mut skdm_bytes).map_err(|e| {
                        CoreError::Protocol(format!("Failed to encode SKDM: {}", e))
                    })?;
                    let skdm_padded = pad_random_max16(&skdm_bytes);
                    let our_identity = KeyPair::try_from(&creds.signed_identity_key)
                        .map_err(|e| CoreError::Crypto(e.to_string()))?;

                    for dj in &sender_key_recipients {
                        if let Err(e) = self.assert_session(dj, &creds).await {
                            warn!("[SKDM] No session for {} ({}), skipping", dj, e);
                            continue;
                        }
                        match self
                            .signal
                            .encrypt_message(dj, &skdm_padded, &our_identity)
                            .await
                        {
                            Ok((enc_type, ct)) => {
                                self.metrics().record_signal_hit();
                                if enc_type == "pkmsg" {
                                    include_device_identity = true;
                                }
                                participant_nodes.push(
                                    BinaryNode::new("to").with_attr("jid", dj).with_child(
                                        BinaryNode::new("enc")
                                            .with_attr("v", "2")
                                            .with_attr("type", enc_type)
                                            .with_bytes_content(ct),
                                    ),
                                );
                            }
                            Err(e) => warn!("[SKDM] Encrypt failed for {}: {}", dj, e),
                        }
                    }

                    // Mark successfully encrypted devices in sender_key_memory
                    {
                        let mut mem = self.sender_key_memory.lock().await;
                        let group_mem = mem.entry(normalized_jid.clone()).or_default();
                        for node in &participant_nodes {
                            if let Some(j) = node.get_attr("jid") {
                                group_mem.insert(j.to_string());
                            }
                        }
                    }
                    debug!(
                        "[GROUP-SEND] {} -> SKDM distributed to {} device(s), addressing={}",
                        normalized_jid,
                        participant_nodes.len(),
                        addressing_mode
                    );
                }
            }

            // 7. Assemble message stanza (Whatsmeow / WA Web parity):
            // - phash attribute on <message>
            // - participants FIRST, device-identity SECOND, enc THIRD
            let phash = Self::generate_participant_hash_v2(&all_group_devices);
            let mut node = BinaryNode::new("message")
                .with_attr("to", &normalized_jid)
                .with_attr("id", &msg_id)
                .with_attr("type", "text")
                .with_attr("addressing_mode", addressing_mode)
                .with_attr("phash", phash);
            if let Some(exp) = meta
                .as_ref()
                .and_then(|m| m.ephemeral_duration)
                .filter(|e| *e > 0)
            {
                node = node.with_attr("expiration", exp.to_string());
            }
            if !participant_nodes.is_empty() {
                node = node
                    .with_child(BinaryNode::new("participants").with_children(participant_nodes));
            }
            if include_device_identity {
                if let Some(ref acc) = creds.account {
                    node = node.with_child(
                        BinaryNode::new("device-identity").with_bytes_content(acc.clone()),
                    );
                }
            }
            node = node.with_child(
                BinaryNode::new("enc")
                    .with_attr("v", "2")
                    .with_attr("type", "skmsg")
                    .with_bytes_content(ciphertext),
            );
            node
        } else if normalized_jid == "status@broadcast" {
            // ── Full Signal Protocol Parity for status@broadcast Stories ──
            let our_identity = KeyPair::try_from(&creds.signed_identity_key)
                .map_err(|e| CoreError::Crypto(e.to_string()))?;

            let me_pn = creds
                .me_jid
                .clone()
                .unwrap_or_else(|| S_WHATSAPP_NET.to_string());
            let sender_identity = me_pn.clone();

            // 1. Determine message type & media type (Baileys Parity: getMessageType & getMediaType)
            let (msg_type, media_type) = if proto_msg.image_message.is_some() {
                ("media", Some("image"))
            } else if let Some(ref vid) = proto_msg.video_message {
                let mt = if vid.gif_playback.unwrap_or(false) {
                    "gif"
                } else {
                    "video"
                };
                ("media", Some(mt))
            } else if let Some(ref aud) = proto_msg.audio_message {
                let mt = if aud.ptt.unwrap_or(false) {
                    "ptt"
                } else {
                    "audio"
                };
                ("media", Some(mt))
            } else {
                ("text", None)
            };

            // 2. Encrypt group payload -> ciphertext + root SKDM
            let (ciphertext, skdm_opt) = self
                .signal
                .encrypt_group_message(&normalized_jid, &sender_identity, &padded)
                .await
                .map_err(|e| CoreError::Crypto(e.to_string()))?;

            // 3. Resolve target recipients (our own phone :0 & companion devices + status audience)
            let mut target_users: Vec<String> = Vec::new();
            let own_pn_user = baileys_binary::jid::jid_normalized_user(&sender_identity);
            target_users.push(own_pn_user);
            // CRITICAL: DO NOT add own_lid_user! Status broadcast is strictly @s.whatsapp.net domain.

            // Populate external contacts from options.status_jid_list or local store
            if let Some(ref custom_audience) = options.status_jid_list {
                for jid in custom_audience {
                    let norm = if !jid.contains('@') {
                        format!("{}@s.whatsapp.net", jid)
                    } else {
                        jid.clone()
                    };
                    let norm_user = baileys_binary::jid::jid_normalized_user(&norm);
                    if !target_users.contains(&norm_user) {
                        target_users.push(norm_user);
                    }
                }
            } else {
                // No explicit audience: load stored contacts/chats (capped to 50 for bounded stanza size)
                if let Ok(chats) = self.store.get_chats().await {
                    for c in chats {
                        if c.jid.ends_with("@s.whatsapp.net") {
                            let norm = baileys_binary::jid::jid_normalized_user(&c.jid);
                            if !target_users.contains(&norm) {
                                target_users.push(norm);
                                if target_users.len() >= 51 {
                                    break;
                                }
                            }
                        }
                    }
                }
                if target_users.len() < 2 {
                    if let Ok(contacts) = self.store.get_contacts().await {
                        for c in contacts {
                            if c.jid.ends_with("@s.whatsapp.net") {
                                let norm = baileys_binary::jid::jid_normalized_user(&c.jid);
                                if !target_users.contains(&norm) {
                                    target_users.push(norm);
                                    if target_users.len() >= 51 {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 4. Expand devices for each user (excluding this exact sending companion session)
            let mut all_target_devices: Vec<String> = Vec::new();
            for user in &target_users {
                let (u, server) = user
                    .split_once('@')
                    .unwrap_or((user.as_str(), "s.whatsapp.net"));
                let devs = self.get_user_devices(user).await.unwrap_or_default();
                let devs = if devs.is_empty() { vec![0u32] } else { devs };
                for d in devs {
                    let dj = if d == 0 {
                        format!("{}@{}", u, server)
                    } else {
                        format!("{}:{}@{}", u, d, server)
                    };
                    if dj == me_pn {
                        continue;
                    }
                    if !all_target_devices.contains(&dj) {
                        all_target_devices.push(dj);
                    }
                }
            }

            // 5. Baileys Parity: status@broadcast NEVER caches in sender_key_memory!
            // In Baileys (messages-send.ts: if (!participant && !isStatus)), status broadcasts
            // must distribute the SKDM to all target devices on EVERY update so every story
            // (text, link preview, HD media) can be decrypted by all devices.
            let sender_key_recipients = all_target_devices.clone();

            let mut participant_nodes = Vec::new();
            let mut include_device_identity = false;

            if !sender_key_recipients.is_empty() {
                if let Some(skdm) = skdm_opt {
                    let skdm_msg = Message {
                        sender_key_distribution_message: Some(
                            baileys_proto::proto::message::SenderKeyDistributionMessage {
                                group_id: Some(normalized_jid.clone()),
                                axolotl_sender_key_distribution_message: Some(
                                    skdm.serialize().to_vec(),
                                ),
                            },
                        ),
                        ..Default::default()
                    };
                    let mut skdm_bytes = Vec::new();
                    skdm_msg.encode(&mut skdm_bytes).map_err(|e| {
                        CoreError::Protocol(format!("Failed to encode status SKDM: {}", e))
                    })?;
                    let skdm_padded = pad_random_max16(&skdm_bytes);

                    for dj in &sender_key_recipients {
                        if let Err(e) = self.assert_session(dj, &creds).await {
                            warn!("[STATUS-SKDM] No session for {} ({}), skipping", dj, e);
                            continue;
                        }
                        match self
                            .signal
                            .encrypt_message(dj, &skdm_padded, &our_identity)
                            .await
                        {
                            Ok((enc_type, ct)) => {
                                if enc_type == "pkmsg" {
                                    include_device_identity = true;
                                }
                                participant_nodes.push(
                                    BinaryNode::new("to").with_attr("jid", dj).with_child(
                                        BinaryNode::new("enc")
                                            .with_attr("v", "2")
                                            .with_attr("type", enc_type)
                                            .with_bytes_content(ct),
                                    ),
                                );
                            }
                            Err(e) => warn!("[STATUS-SKDM] Encrypt failed for {}: {}", dj, e),
                        }
                    }

                    debug!(
                        "[STATUS-SEND] status@broadcast -> SKDM distributed to {} device(s)",
                        participant_nodes.len()
                    );
                }
            }

            // 6. Assemble status message stanza (Baileys Parity: NO addressing_mode, dynamic type and mediatype)
            let mut enc_node = BinaryNode::new("enc")
                .with_attr("v", "2")
                .with_attr("type", "skmsg")
                .with_bytes_content(ciphertext);
            if let Some(mt) = media_type {
                enc_node = enc_node.with_attr("mediatype", mt);
            }

            let mut node = BinaryNode::new("message")
                .with_attr("to", &normalized_jid)
                .with_attr("id", &msg_id)
                .with_attr("type", msg_type);
            node = node.with_child(enc_node);

            if !participant_nodes.is_empty() {
                node = node
                    .with_child(BinaryNode::new("participants").with_children(participant_nodes));
            }
            if include_device_identity {
                if let Some(ref acc) = creds.account {
                    node = node.with_child(
                        BinaryNode::new("device-identity").with_bytes_content(acc.clone()),
                    );
                }
            }
            node
        } else {
            // ── Baileys relayMessage parity for 1-on-1 chats ──
            let our_identity = KeyPair::try_from(&creds.signed_identity_key)
                .map_err(|e| CoreError::Crypto(e.to_string()))?;

            let me_pn = creds
                .me_jid
                .clone()
                .unwrap_or_else(|| S_WHATSAPP_NET.to_string());
            let me_lid = creds.me_lid.clone();

            let target_user_norm_init = baileys_binary::jid::jid_normalized_user(&normalized_jid);
            let own_pn_norm = baileys_binary::jid::jid_normalized_user(&me_pn);
            let own_lid_norm = me_lid
                .as_deref()
                .map(baileys_binary::jid::jid_normalized_user);

            let is_note_to_self = target_user_norm_init == own_pn_norm
                || (own_lid_norm.is_some()
                    && Some(&target_user_norm_init) == own_lid_norm.as_ref());

            // If note-to-self and LID is available, normalize conversation to LID for WhatsApp Web sync parity!
            let normalized_jid = if is_note_to_self {
                own_lid_norm.as_ref().unwrap_or(&normalized_jid).clone()
            } else {
                normalized_jid.clone()
            };

            let is_lid = normalized_jid.ends_with("@lid");
            let sender_identity = if is_lid {
                me_lid.as_ref().cloned().unwrap_or_else(|| me_pn.clone())
            } else {
                me_pn.clone()
            };

            let own_user_norm = baileys_binary::jid::jid_normalized_user(&sender_identity);
            let target_user_norm = baileys_binary::jid::jid_normalized_user(&normalized_jid);

            // 1. Enumerate own companion devices (excluding this exact sending device)
            let (own_u, own_server) = own_user_norm.split_once('@').unwrap_or((
                own_user_norm.as_str(),
                if is_lid { "lid" } else { "s.whatsapp.net" },
            ));
            let own_devs = self
                .get_user_devices(&own_user_norm)
                .await
                .unwrap_or_default();
            let own_devs = if own_devs.is_empty() {
                vec![0u32]
            } else {
                own_devs
            };

            let mut me_recipients: Vec<String> = Vec::new();
            for d in own_devs {
                let dj = if d == 0 {
                    format!("{}@{}", own_u, own_server)
                } else {
                    format!("{}:{}@{}", own_u, d, own_server)
                };
                if dj == me_pn || me_lid.as_deref() == Some(dj.as_str()) {
                    continue; // Skip exact sending device
                }
                if !me_recipients.contains(&dj) {
                    me_recipients.push(dj);
                }
            }

            // 2. Enumerate other recipient devices (empty if note-to-self)
            let mut other_recipients: Vec<String> = Vec::new();
            if !is_note_to_self {
                let (tgt_u, tgt_server) = target_user_norm.split_once('@').unwrap_or((
                    target_user_norm.as_str(),
                    if is_lid { "lid" } else { "s.whatsapp.net" },
                ));
                let tgt_devs = self
                    .get_user_devices(&target_user_norm)
                    .await
                    .unwrap_or_default();
                let tgt_devs = if tgt_devs.is_empty() {
                    vec![0u32]
                } else {
                    tgt_devs
                };
                for d in tgt_devs {
                    let dj = if d == 0 {
                        format!("{}@{}", tgt_u, tgt_server)
                    } else {
                        format!("{}:{}@{}", tgt_u, d, tgt_server)
                    };
                    if !other_recipients.contains(&dj) {
                        other_recipients.push(dj);
                    }
                }
            }

            let mut all_recipients = me_recipients.clone();
            for r in &other_recipients {
                if !all_recipients.contains(r) {
                    all_recipients.push(r.clone());
                }
            }

            let phash = Self::generate_participant_hash_v2(&all_recipients);

            // Determine mediatype attribute for Baileys/WhatsApp parity
            let media_type_attr = if proto_msg.image_message.is_some() {
                Some("image")
            } else if proto_msg.sticker_message.is_some() {
                Some("sticker")
            } else if proto_msg.document_message.is_some() {
                Some("document")
            } else if proto_msg.video_message.is_some() {
                Some("video")
            } else if proto_msg.audio_message.is_some() {
                Some("audio")
            } else {
                None
            };

            // 3. Encrypt for own devices with DeviceSentMessage
            let mut participant_nodes = Vec::new();
            let mut include_device_identity = false;

            if !me_recipients.is_empty() {
                let dsm_proto = Message {
                    device_sent_message: Some(Box::new(
                        baileys_proto::proto::message::DeviceSentMessage {
                            destination_jid: Some(normalized_jid.clone()),
                            message: Some(Box::new(proto_msg.clone())),
                            phash: Some(phash.clone()),
                        },
                    )),
                    message_context_info: proto_msg.message_context_info.clone(),
                    ..Default::default()
                };

                let mut dsm_bytes = Vec::new();
                if dsm_proto.encode(&mut dsm_bytes).is_ok() {
                    let dsm_padded = pad_random_max16(&dsm_bytes);
                    for r_jid in &me_recipients {
                        if self.assert_session(r_jid, &creds).await.is_ok() {
                            match self
                                .signal
                                .encrypt_message(r_jid, &dsm_padded, &our_identity)
                                .await
                            {
                                Ok((enc_type, ct)) => {
                                    self.metrics().record_signal_hit();
                                    if enc_type == "pkmsg" {
                                        include_device_identity = true;
                                    }
                                    let mut enc_node = BinaryNode::new("enc")
                                        .with_attr("v", "2")
                                        .with_attr("type", enc_type);
                                    if let Some(mt) = media_type_attr {
                                        enc_node = enc_node.with_attr("mediatype", mt);
                                    }
                                    enc_node = enc_node.with_bytes_content(ct);
                                    participant_nodes.push(
                                        BinaryNode::new("to")
                                            .with_attr("jid", r_jid)
                                            .with_child(enc_node),
                                    );
                                }
                                Err(_) => {
                                    self.metrics().record_signal_miss();
                                }
                            }
                        }
                    }
                }
            }

            // 4. Encrypt for other recipients with original message
            for r_jid in &other_recipients {
                if self.assert_session(r_jid, &creds).await.is_ok() {
                    match self
                        .signal
                        .encrypt_message(r_jid, &padded, &our_identity)
                        .await
                    {
                        Ok((enc_type, ct)) => {
                            self.metrics().record_signal_hit();
                            if enc_type == "pkmsg" {
                                include_device_identity = true;
                            }
                            let mut enc_node = BinaryNode::new("enc")
                                .with_attr("v", "2")
                                .with_attr("type", enc_type);
                            if let Some(mt) = media_type_attr {
                                enc_node = enc_node.with_attr("mediatype", mt);
                            }
                            enc_node = enc_node.with_bytes_content(ct);
                            participant_nodes.push(
                                BinaryNode::new("to")
                                    .with_attr("jid", r_jid)
                                    .with_child(enc_node),
                            );
                        }
                        Err(_) => {
                            self.metrics().record_signal_miss();
                        }
                    }
                }
            }

            if !participant_nodes.is_empty() {
                let mut node = BinaryNode::new("message")
                    .with_attr("to", &normalized_jid)
                    .with_attr("id", &msg_id)
                    .with_attr("type", "text")
                    .with_child(BinaryNode::new("participants").with_children(participant_nodes));
                if include_device_identity {
                    if let Some(ref acc) = creds.account {
                        node = node.with_child(
                            BinaryNode::new("device-identity").with_bytes_content(acc.clone()),
                        );
                    }
                }
                node
            } else {
                // Direct 1-to-1 fallback if no multi-device targets found
                self.assert_session(&normalized_jid, &creds).await?;
                let (enc_type, ciphertext) = match self
                    .signal
                    .encrypt_message(&normalized_jid, &padded, &our_identity)
                    .await
                {
                    Ok(res) => {
                        self.metrics().record_signal_hit();
                        res
                    }
                    Err(e) => {
                        self.metrics().record_signal_miss();
                        return Err(CoreError::Crypto(e.to_string()));
                    }
                };

                let mut enc_node = BinaryNode::new("enc")
                    .with_attr("type", enc_type)
                    .with_attr("v", "2");
                if let Some(mt) = media_type_attr {
                    enc_node = enc_node.with_attr("mediatype", mt);
                }
                enc_node = enc_node.with_bytes_content(ciphertext);

                BinaryNode::new("message")
                    .with_attr("to", &normalized_jid)
                    .with_attr("id", &msg_id)
                    .with_attr("type", "text")
                    .with_child(enc_node)
            }
        };

        // Attach reporting token (v2) if message_secret is present
        let mut final_msg_node = msg_node;

        // WhatsApp Protocol parity: Set edit attribute for REVOKE / EDIT messages
        if let Some(ref proto) = proto_msg.protocol_message {
            if proto.r#type
                == Some(baileys_proto::proto::message::protocol_message::Type::Revoke as i32)
            {
                let is_group_admin_delete = normalized_jid.ends_with("@g.us")
                    && proto.key.as_ref().and_then(|k| k.from_me) == Some(false);
                let edit_val = if is_group_admin_delete { "8" } else { "7" };
                final_msg_node = final_msg_node.with_attr("edit", edit_val);
            } else if proto.r#type
                == Some(baileys_proto::proto::message::protocol_message::Type::MessageEdit as i32)
            {
                final_msg_node = final_msg_node.with_attr("edit", "1");
            }
        }
        if let Some(ref ctx) = proto_msg.message_context_info {
            if let Some(ref sec) = ctx.message_secret {
                if !sec.is_empty()
                    && !normalized_jid.ends_with("@newsletter")
                    && normalized_jid != "status@broadcast"
                {
                    let is_lid_dest = normalized_jid.ends_with("@lid")
                        || (normalized_jid.ends_with("@g.us")
                            && final_msg_node.get_attr("addressing_mode") == Some("lid"));
                    let sender_jid = if is_lid_dest && creds.me_lid.is_some() {
                        baileys_binary::jid::jid_normalized_user(creds.me_lid.as_deref().unwrap())
                    } else if let Some(ref me) = creds.me_jid {
                        baileys_binary::jid::jid_normalized_user(me)
                    } else if let Some(ref lid) = creds.me_lid {
                        baileys_binary::jid::jid_normalized_user(lid)
                    } else {
                        String::new()
                    };

                    if !sender_jid.is_empty() {
                        if let Ok(Some(rep_node)) = crate::reporting::get_message_reporting_token(
                            &msg_bytes,
                            &msg_id,
                            &sender_jid,
                            &normalized_jid,
                            sec,
                        ) {
                            debug!(
                                "[REPORTING] Attached reporting token for msg {} (from={}, to={})",
                                msg_id, sender_jid, normalized_jid
                            );
                            final_msg_node = final_msg_node.with_child(rep_node);
                        }
                    }
                }
            }
        }

        // WhatsApp Protocol parity: Attach caller-specified additional_nodes
        if let Some(ref extra_nodes) = options.additional_nodes {
            for node in extra_nodes {
                final_msg_node = final_msg_node.with_child(node.clone());
            }
        }

        // WhatsApp Protocol parity: Inject <biz> and <bot> XML nodes for interactive messages
        let already_has_biz = final_msg_node.get_child("biz").is_some();
        if !already_has_biz {
            if let Some(biz_kind) = crate::biz::detect_biz_kind(&proto_msg) {
                let is_group = normalized_jid.ends_with("@g.us");
                let is_broadcast = normalized_jid.ends_with("@broadcast");
                let is_newsletter = normalized_jid.ends_with("@newsletter");

                if is_group || (!is_broadcast && !is_newsletter) {
                    final_msg_node =
                        final_msg_node.with_child(crate::biz::build_biz_node(&biz_kind));

                    if !is_group && !is_broadcast && !is_newsletter {
                        let has_bot = final_msg_node
                            .get_children()
                            .iter()
                            .any(|c| c.tag == "bot" && c.get_attr("biz_bot") == Some("1"));
                        if !has_bot {
                            final_msg_node =
                                final_msg_node.with_child(crate::biz::build_bot_node());
                        }
                    }
                }
            }
        }

        // 3. Send over socket if connected
        if let Some(socket) = socket_opt {
            info!(
                "Sending outbound message node to '{}' (id: {}): type={}, children={}",
                normalized_jid,
                msg_id,
                final_msg_node.get_attr("type").unwrap_or(""),
                final_msg_node.get_children().len()
            );
            debug!("Outbound message stanza: {:?}", final_msg_node);
            self.metrics().record_msg_out();
            socket.send_node(final_msg_node).await?;
        }

        // 4. Cache & emit Message event
        let msg_info = WebMessageInfo {
            key: Some(MessageKey {
                remote_jid: Some(normalized_jid.clone()),
                from_me: Some(true),
                id: Some(msg_id.clone()),
                participant: None,
            }),
            message: Some(proto_msg),
            message_timestamp: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            ),
            status: Some(baileys_proto::proto::web_message_info::Status::Pending as i32),
            ..Default::default()
        };

        {
            let mut cache = self.recent_messages.lock().await;
            if cache.len() > 500 {
                cache.clear();
            }
            cache.insert(msg_id.clone(), msg_info.clone());
        }

        // Persist sent message & update chat in SQLite
        let (summary, has_media) = if let Some(ref m) = msg_info.message {
            let s = crate::messages::extract_message_summary(m);
            let h_m = crate::messages::message_has_media(m);
            (s, h_m)
        } else {
            ("<empty>".to_string(), false)
        };

        let stored_m = StoredMessage {
            id: msg_id.clone(),
            chat_jid: normalized_jid.clone(),
            sender_jid: creds.me_jid.clone().unwrap_or_else(|| "me".to_string()),
            from_me: true,
            timestamp: msg_info.message_timestamp.unwrap_or(0),
            summary: summary.clone(),
            status: "pending".to_string(),
            has_media,
            proto_bytes: msg_bytes,
            is_starred: false,
            is_revoked: false,
            is_edited: false,
        };
        let _ = self.store.save_message(&stored_m).await;

        let stored_c = StoredChat {
            jid: normalized_jid.clone(),
            name: None,
            unread_count: 0,
            last_message_timestamp: msg_info.message_timestamp.unwrap_or(0),
            last_message_summary: Some(summary),
            pinned: false,
            archived: false,
            mute_until: None,
            ephemeral_expiration: 0,
            ephemeral_setting_timestamp: None,
        };
        let _ = self.store.save_chat(&stored_c).await;

        self.events.emit(BaileysEvent::Message {
            message: Box::new(msg_info),
        });

        Ok(msg_id)
    }
    /// Sends a plain text message to a user or group over the encrypted WhatsApp connection.
    pub async fn send_text_message(&self, to_jid: &str, text: &str) -> Result<String, CoreError> {
        let proto_msg = Message {
            conversation: Some(text.to_string()),
            ..Default::default()
        };
        self.send_message(to_jid, proto_msg).await
    }
    /// Sends a message constructed from dynamic JSON (e.g. from frontend builders or plugins).
    pub async fn send_message_json(
        &self,
        to_jid: &str,
        message_json: &serde_json::Value,
    ) -> Result<String, CoreError> {
        let proto_msg =
            crate::messages::parse_message_from_json(message_json).map_err(CoreError::Protocol)?;
        self.send_message(to_jid, proto_msg).await
    }
    /// Forwards an existing protobuf message to a recipient, setting is_forwarded and forwarding_score.
    pub async fn forward_message(
        &self,
        to_jid: &str,
        mut msg: Message,
        is_forwarded: bool,
        score: u32,
    ) -> Result<String, CoreError> {
        if let Some(ref mut ext) = msg.extended_text_message {
            let ctx = ext.context_info.get_or_insert_with(Default::default);
            ctx.is_forwarded = Some(is_forwarded);
            ctx.forwarding_score = Some(score);
        } else if let Some(ref mut img) = msg.image_message {
            let ctx = img.context_info.get_or_insert_with(Default::default);
            ctx.is_forwarded = Some(is_forwarded);
            ctx.forwarding_score = Some(score);
        } else if let Some(ref mut vid) = msg.video_message {
            let ctx = vid.context_info.get_or_insert_with(Default::default);
            ctx.is_forwarded = Some(is_forwarded);
            ctx.forwarding_score = Some(score);
        } else if let Some(ref mut aud) = msg.audio_message {
            let ctx = aud.context_info.get_or_insert_with(Default::default);
            ctx.is_forwarded = Some(is_forwarded);
            ctx.forwarding_score = Some(score);
        } else if let Some(ref mut doc) = msg.document_message {
            let ctx = doc.context_info.get_or_insert_with(Default::default);
            ctx.is_forwarded = Some(is_forwarded);
            ctx.forwarding_score = Some(score);
        } else if let Some(ref mut stk) = msg.sticker_message {
            let ctx = stk.context_info.get_or_insert_with(Default::default);
            ctx.is_forwarded = Some(is_forwarded);
            ctx.forwarding_score = Some(score);
        } else if let Some(conv) = msg.conversation.take() {
            let ctx = ContextInfo {
                is_forwarded: Some(is_forwarded),
                forwarding_score: Some(score),
                ..Default::default()
            };
            msg.extended_text_message = Some(Box::new(ExtendedTextMessage {
                text: Some(conv),
                context_info: Some(Box::new(ctx)),
                ..Default::default()
            }));
        }

        self.send_message(to_jid, msg).await
    }
    /// Pins or unpins a message in a chat using WhatsApp's PinInChatMessage protocol.
    pub async fn pin_message(
        &self,
        chat_jid: &str,
        msg_id: &str,
        pin: bool,
        _duration_secs: u32,
    ) -> Result<String, CoreError> {
        let normalized_jid = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid)
        } else {
            chat_jid.to_string()
        };

        let from_me = if let Ok(Some(m)) = self.store.get_message(msg_id).await {
            m.from_me
        } else {
            false
        };

        let pin_type = if pin { 1 } else { 2 };
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let proto_msg = Message {
            pin_in_chat_message: Some(baileys_proto::proto::message::PinInChatMessage {
                key: Some(MessageKey {
                    remote_jid: Some(normalized_jid.clone()),
                    from_me: Some(from_me),
                    id: Some(msg_id.to_string()),
                    participant: None,
                }),
                r#type: Some(pin_type),
                sender_timestamp_ms: Some(now_ms),
            }),
            ..Default::default()
        };

        self.send_message(&normalized_jid, proto_msg).await
    }
    /// Sends an emoji reaction to a specific message.
    pub async fn send_reaction(
        &self,
        to_jid: &str,
        target_msg_id: &str,
        emoji: &str,
    ) -> Result<String, CoreError> {
        let normalized_jid = if !to_jid.contains('@') {
            format!("{}@s.whatsapp.net", to_jid)
        } else {
            to_jid.to_string()
        };

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let proto_msg = Message {
            reaction_message: Some(baileys_proto::proto::message::ReactionMessage {
                key: Some(MessageKey {
                    remote_jid: Some(normalized_jid.clone()),
                    from_me: None,
                    id: Some(target_msg_id.to_string()),
                    participant: None,
                }),
                text: Some(emoji.to_string()),
                sender_timestamp_ms: Some(now_ms),
                ..Default::default()
            }),
            ..Default::default()
        };

        self.send_message(&normalized_jid, proto_msg).await
    }
    /// Sends a presence update (e.g. "available", "unavailable", "composing", "paused").
    pub async fn send_presence_update(
        &self,
        presence: &str,
        to_jid_opt: Option<&str>,
    ) -> Result<(), CoreError> {
        let socket_opt = {
            let lock = self.socket.lock().await;
            lock.clone()
        };
        let socket = socket_opt.ok_or_else(|| CoreError::Network("Client not connected".into()))?;

        let mut node = BinaryNode::new("presence").with_attr("type", presence);

        if let Some(to) = to_jid_opt {
            let normalized = if !to.contains('@') {
                format!("{}@s.whatsapp.net", to)
            } else {
                to.to_string()
            };
            node = node.with_attr("to", &normalized);
        }

        self.metrics().record_msg_out();
        socket.send_node(node).await
    }
    /// Returns a recently received or sent message by ID.
    pub async fn get_recent_message(&self, msg_id: &str) -> Option<WebMessageInfo> {
        let cache = self.recent_messages.lock().await;
        cache.get(msg_id).cloned()
    }
    /// Revokes (deletes for everyone) a previously sent message.
    pub async fn send_revoke_message(
        &self,
        to_jid: &str,
        target_msg_id: &str,
    ) -> Result<String, CoreError> {
        let normalized_jid = if !to_jid.contains('@') {
            format!("{}@s.whatsapp.net", to_jid)
        } else {
            to_jid.to_string()
        };

        let proto_msg = Message {
            protocol_message: Some(Box::new(baileys_proto::proto::message::ProtocolMessage {
                key: Some(MessageKey {
                    remote_jid: Some(normalized_jid.clone()),
                    from_me: Some(true),
                    id: Some(target_msg_id.to_string()),
                    participant: None,
                }),
                r#type: Some(baileys_proto::proto::message::protocol_message::Type::Revoke as i32),
                ..Default::default()
            })),
            ..Default::default()
        };

        self.send_message(&normalized_jid, proto_msg).await
    }
    /// Sends a text message with a forged quoted context from any sender.
    pub async fn send_fake_quoted_message(
        &self,
        to_jid: &str,
        reply_text: &str,
        fake_sender_jid: &str,
        fake_quoted_text: &str,
    ) -> Result<String, CoreError> {
        let normalized_jid = if !to_jid.contains('@') {
            format!("{}@s.whatsapp.net", to_jid)
        } else {
            to_jid.to_string()
        };
        let normalized_fake_sender = if !fake_sender_jid.contains('@') {
            format!("{}@s.whatsapp.net", fake_sender_jid)
        } else {
            fake_sender_jid.to_string()
        };
        let chat_context = if normalized_jid.ends_with("@g.us") {
            Some(normalized_jid.as_str())
        } else {
            None
        };

        let proto_msg = crate::proto_builders::build_fake_quoted_message(
            reply_text,
            &normalized_fake_sender,
            fake_quoted_text,
            chat_context,
        );

        self.send_message(&normalized_jid, proto_msg).await
    }
    /// Sends a read receipt for a received message.
    pub async fn send_read_receipt(
        &self,
        remote_jid: &str,
        msg_id: &str,
        participant: Option<&str>,
    ) -> Result<(), CoreError> {
        if self.is_ghost_mode() {
            debug!(
                "[Ghost Mode] Suppressed read receipt for message {} in {}",
                msg_id, remote_jid
            );
            return Ok(());
        }

        let socket_opt = {
            let lock = self.socket.lock().await;
            lock.clone()
        };
        let socket = socket_opt.ok_or_else(|| CoreError::Network("Client not connected".into()))?;

        let mut node = BinaryNode::new("receipt")
            .with_attr("id", msg_id)
            .with_attr("to", remote_jid)
            .with_attr("type", "read");

        if let Some(part) = participant {
            node = node.with_attr("participant", part);
        }

        self.metrics().record_msg_out();
        socket.send_node(node).await
    }
    /// Sends a text message quoting/replying to a previous message.
    pub async fn send_text_message_with_quote(
        &self,
        to_jid: &str,
        text: &str,
        quoted_id: &str,
        quoted_participant: Option<&str>,
        quoted_text: Option<&str>,
    ) -> Result<String, CoreError> {
        let normalized_jid = if !to_jid.contains('@') {
            format!("{}@s.whatsapp.net", to_jid)
        } else {
            to_jid.to_string()
        };

        let quoted_msg = quoted_text.map(|qt| {
            Box::new(Message {
                conversation: Some(qt.to_string()),
                ..Default::default()
            })
        });

        let context_info = Box::new(ContextInfo {
            stanza_id: Some(quoted_id.to_string()),
            participant: quoted_participant.map(|p| p.to_string()),
            quoted_message: quoted_msg,
            remote_jid: Some(normalized_jid.clone()),
            ..Default::default()
        });

        let ext_msg = ExtendedTextMessage {
            text: Some(text.to_string()),
            context_info: Some(context_info),
            ..Default::default()
        };

        let proto_msg = Message {
            extended_text_message: Some(Box::new(ext_msg)),
            ..Default::default()
        };

        self.send_message(&normalized_jid, proto_msg).await
    }
    /// Sends a poll creation message with options.
    pub async fn send_poll(
        &self,
        to_jid: &str,
        question: &str,
        options: &[String],
        selectable_count: u32,
    ) -> Result<String, CoreError> {
        let mut enc_key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut enc_key);

        let proto_msg = crate::polls::build_poll_creation_message(
            question,
            options,
            selectable_count,
            &enc_key,
        );

        let normalized_jid = if !to_jid.contains('@') {
            format!("{}@s.whatsapp.net", to_jid)
        } else {
            to_jid.to_string()
        };

        let me = self.me_jid().await?.unwrap_or_else(|| "me".to_string());
        let msg_id = self.send_message(&normalized_jid, proto_msg).await?;

        // Cache poll in local SQLite store
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let stored_poll = StoredPoll {
            id: msg_id.clone(),
            chat_jid: normalized_jid,
            sender_jid: me,
            question: question.to_string(),
            options: options.to_vec(),
            enc_key: enc_key.to_vec(),
            selectable_count: if selectable_count == 0 {
                1
            } else {
                selectable_count
            },
            created_at: now,
        };
        let _ = self.store.save_poll(&stored_poll).await;

        Ok(msg_id)
    }
    /// Casts an encrypted vote on a poll.
    pub async fn send_poll_vote(
        &self,
        to_jid: &str,
        poll_id: &str,
        selected_options: &[String],
    ) -> Result<String, CoreError> {
        let stored_poll = self
            .store
            .get_poll(poll_id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?
            .ok_or_else(|| CoreError::Protocol(format!("Poll '{}' not found in store", poll_id)))?;

        let me = self.me_jid().await?.unwrap_or_else(|| "me".to_string());
        let selected_hashes: Vec<Vec<u8>> = selected_options
            .iter()
            .map(|opt| crate::polls::hash_poll_option(opt).to_vec())
            .collect();

        let vote_enc = crate::polls::encrypt_poll_vote(
            &selected_hashes,
            &stored_poll.sender_jid,
            poll_id,
            &stored_poll.enc_key,
            &me,
        )?;

        let normalized_jid = if !to_jid.contains('@') {
            format!("{}@s.whatsapp.net", to_jid)
        } else {
            to_jid.to_string()
        };

        let poll_creation_key = MessageKey {
            remote_jid: Some(normalized_jid.clone()),
            from_me: Some(stored_poll.sender_jid == me),
            id: Some(poll_id.to_string()),
            participant: None,
        };

        let proto_msg = crate::polls::build_poll_update_message(poll_creation_key, vote_enc);
        let msg_id = self.send_message(&normalized_jid, proto_msg).await?;

        // Save own vote locally
        let _ = self
            .store
            .save_poll_vote(poll_id, &me, selected_options)
            .await;

        Ok(msg_id)
    }
    /// Fetches live vote tally for a poll from the SQLite store.
    pub async fn get_poll_tally(&self, poll_id: &str) -> Result<Option<PollTally>, CoreError> {
        self.store
            .get_poll_tally(poll_id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }
    /// Sends an interactive message with action buttons (URL, call, copy code, quick reply).
    pub async fn send_interactive_buttons(
        &self,
        to_jid: &str,
        body: &str,
        buttons: &[InteractiveButton],
        header: Option<&str>,
        footer: Option<&str>,
    ) -> Result<String, CoreError> {
        let proto_msg =
            crate::polls::build_interactive_button_message(body, buttons, header, footer);
        self.send_message(to_jid, proto_msg).await
    }
    /// Sends an interactive list message.
    pub async fn send_list_message(
        &self,
        to_jid: &str,
        title: &str,
        description: &str,
        button_text: &str,
        sections: &[ListSection],
        footer: Option<&str>,
    ) -> Result<String, CoreError> {
        let proto_msg =
            crate::polls::build_list_message(title, description, button_text, sections, footer);
        self.send_message(to_jid, proto_msg).await
    }
    /// Edits a previously sent message by sending a ProtocolMessage of type MESSAGE_EDIT.
    pub async fn edit_message(
        &self,
        chat_jid: &str,
        target_message_id: &str,
        new_text: &str,
    ) -> Result<String, CoreError> {
        let normalized_jid = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid)
        } else {
            chat_jid.to_string()
        };
        let edit_proto = crate::ephemeral::build_edit_message(
            &normalized_jid,
            target_message_id,
            true,
            new_text,
        );

        let env_id = self
            .send_message(&normalized_jid, edit_proto.clone())
            .await?;

        // Update local database & emit MessageEdited
        let mut raw_bytes = Vec::new();
        let _ = edit_proto.encode(&mut raw_bytes);
        let _ = self
            .store
            .update_message_content(
                &normalized_jid,
                target_message_id,
                new_text,
                Some(&raw_bytes),
            )
            .await;

        self.events.emit(BaileysEvent::MessageEdited {
            chat_jid: normalized_jid,
            message_id: target_message_id.to_string(),
            new_text: new_text.to_string(),
        });

        Ok(env_id)
    }
    /// Revokes (deletes for everyone) a previously sent message by sending a ProtocolMessage of type REVOKE.
    pub async fn revoke_message(
        &self,
        chat_jid: &str,
        target_message_id: &str,
    ) -> Result<String, CoreError> {
        let normalized_jid = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid)
        } else {
            chat_jid.to_string()
        };
        let creds = self
            .store
            .get_creds()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?
            .unwrap_or_else(AuthCreds::init);

        let participant = if normalized_jid.ends_with("@g.us") {
            creds.me_jid.clone()
        } else {
            None
        };

        let revoke_proto = crate::ephemeral::build_revoke_message(
            &normalized_jid,
            target_message_id,
            true,
            participant.as_deref(),
        );

        let env_id = self.send_message(&normalized_jid, revoke_proto).await?;

        // Update local database & emit MessageRevoked
        let _ = self
            .store
            .revoke_message(&normalized_jid, target_message_id)
            .await;

        self.events.emit(BaileysEvent::MessageRevoked {
            chat_jid: normalized_jid,
            message_id: target_message_id.to_string(),
            participant,
        });

        Ok(env_id)
    }
    /// Sends a static location message to a contact or group.
    pub async fn send_location(
        &self,
        to_jid: &str,
        latitude: f64,
        longitude: f64,
        name: Option<&str>,
        address: Option<&str>,
        url: Option<&str>,
    ) -> Result<String, CoreError> {
        let proto = crate::rich_messages::build_location_message(
            latitude, longitude, name, address, url, None,
        );
        self.send_message(to_jid, proto).await
    }
    /// Sends a real-time live location message to a contact or group.
    pub async fn send_live_location(
        &self,
        to_jid: &str,
        latitude: f64,
        longitude: f64,
        accuracy_in_meters: Option<u32>,
        caption: Option<&str>,
    ) -> Result<String, CoreError> {
        let proto = crate::rich_messages::build_live_location_message(
            latitude,
            longitude,
            accuracy_in_meters,
            None,
            None,
            caption,
            Some(1),
            Some(0),
            None,
        );
        self.send_message(to_jid, proto).await
    }
    /// Sends a single contact card (vCard 3.0) to a contact or group.
    pub async fn send_contact(
        &self,
        to_jid: &str,
        display_name: &str,
        phone_number: &str,
    ) -> Result<String, CoreError> {
        let vcard = crate::rich_messages::build_vcard_string(display_name, phone_number, None);
        let proto = crate::rich_messages::build_contact_message(display_name, &vcard);
        self.send_message(to_jid, proto).await
    }
    /// Sends a contact card with custom raw vCard text to a contact or group.
    pub async fn send_vcard(
        &self,
        to_jid: &str,
        display_name: &str,
        vcard: &str,
    ) -> Result<String, CoreError> {
        let proto = crate::rich_messages::build_contact_message(display_name, vcard);
        self.send_message(to_jid, proto).await
    }
    /// Sends an interactive message (Buttons, List menus, Native Flows, Carousel cards) to a contact or group.
    pub async fn send_interactive_message(
        &self,
        to_jid: &str,
        builder: crate::interactive::InteractiveMessageBuilder,
    ) -> Result<String, CoreError> {
        let msg = builder.build();
        self.send_message(to_jid, msg).await
    }
    /// Sends a multi-contact card to a contact or group.
    pub async fn send_contacts_array(
        &self,
        to_jid: &str,
        display_name: &str,
        contacts: Vec<(String, String)>,
    ) -> Result<String, CoreError> {
        let proto_contacts: Vec<baileys_proto::proto::message::ContactMessage> = contacts
            .into_iter()
            .map(|(name, phone)| {
                let vcard = crate::rich_messages::build_vcard_string(&name, &phone, None);
                baileys_proto::proto::message::ContactMessage {
                    display_name: Some(name),
                    vcard: Some(vcard),
                    ..Default::default()
                }
            })
            .collect();
        let proto =
            crate::rich_messages::build_contacts_array_message(display_name, proto_contacts);
        self.send_message(to_jid, proto).await
    }
    /// Sends a group invitation preview message to a contact or group.
    pub async fn send_group_invite(
        &self,
        to_jid: &str,
        group_jid: &str,
        invite_code: &str,
        invite_expiration: i64,
        group_name: &str,
        caption: Option<&str>,
    ) -> Result<String, CoreError> {
        let proto = crate::rich_messages::build_group_invite_message(
            group_jid,
            invite_code,
            invite_expiration,
            group_name,
            caption,
            None,
        );
        self.send_message(to_jid, proto).await
    }
    /// Sends a view-once wrapped message (e.g. image, video) to a contact or group.
    pub async fn send_view_once(
        &self,
        to_jid: &str,
        inner_proto: Message,
        v2: bool,
    ) -> Result<String, CoreError> {
        let proto = crate::rich_messages::wrap_view_once(inner_proto, v2);
        self.send_message(to_jid, proto).await
    }
    /// Sends a calendar event message (`EventMessage`) to a contact or group.
    pub async fn send_event(
        &self,
        to_jid: &str,
        name: &str,
        description: Option<&str>,
        start_time: i64,
        end_time: Option<i64>,
        location_name: Option<&str>,
    ) -> Result<String, CoreError> {
        let proto = crate::proto_builders::build_event_message(
            name,
            description,
            start_time,
            end_time,
            location_name,
            None,
            false,
        );
        self.send_message(to_jid, proto).await
    }
    /// Sends an album metadata message for grouping multiple media items.
    pub async fn send_album(
        &self,
        to_jid: &str,
        expected_image_count: u32,
        expected_video_count: u32,
    ) -> Result<String, CoreError> {
        let proto =
            crate::proto_builders::build_album_message(expected_image_count, expected_video_count);
        self.send_message(to_jid, proto).await
    }
    /// Fetches rich OpenGraph metadata and thumbnail for a URL string.
    pub async fn fetch_url_preview(
        &self,
        text: &str,
    ) -> Result<Option<baileys_media::UrlPreview>, CoreError> {
        baileys_media::fetch_url_preview(text, None)
            .await
            .map_err(|e| CoreError::Protocol(format!("Link preview error: {}", e)))
    }
    /// Sends a text message with an automatically generated OpenGraph preview card if a URL is present.
    pub async fn send_text_with_preview(
        &self,
        to_jid: &str,
        text: &str,
    ) -> Result<String, CoreError> {
        if let Ok(Some(preview)) = baileys_media::fetch_url_preview(text, None).await {
            let ext = baileys_proto::proto::message::ExtendedTextMessage {
                text: Some(text.to_string()),
                matched_text: Some(preview.matched_text),
                title: preview.title,
                description: preview.description,
                jpeg_thumbnail: preview.jpeg_thumbnail,
                ..Default::default()
            };
            let proto_msg = Message {
                extended_text_message: Some(Box::new(ext)),
                ..Default::default()
            };
            self.send_message(to_jid, proto_msg).await
        } else {
            let proto_msg = Message {
                conversation: Some(text.to_string()),
                ..Default::default()
            };
            self.send_message(to_jid, proto_msg).await
        }
    }

    /// Sends a prompt to Meta AI or a WhatsApp bot with bot context info.
    pub async fn send_bot_prompt(
        &self,
        prompt: &str,
        target_chat: Option<&str>,
    ) -> Result<String, CoreError> {
        let destination = target_chat.unwrap_or(crate::bot::META_AI_PN);
        let ext = baileys_proto::proto::message::ExtendedTextMessage {
            text: Some(prompt.to_string()),
            context_info: Some(Box::new(baileys_proto::proto::ContextInfo {
                mentioned_jid: vec![crate::bot::META_AI_PN.to_string()],
                ..Default::default()
            })),
            ..Default::default()
        };
        let proto_msg = Message {
            extended_text_message: Some(Box::new(ext)),
            ..Default::default()
        };
        self.send_message(destination, proto_msg).await
    }

    /// Sends an in-thread comment message replying directly to a target message.
    pub async fn send_comment(
        &self,
        to_jid: &str,
        target_key: &MessageKey,
        text: &str,
    ) -> Result<String, CoreError> {
        let msg = crate::proto_builders::build_comment_message(target_key, text);
        self.send_message(to_jid, msg).await
    }
}
