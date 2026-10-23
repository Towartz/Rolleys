//! Domain submodule for chat_actions operations on BaileysClient.

use super::*;

impl BaileysClient {
    /// Synchronizes multi-device App State collections with WhatsApp Web Gateway.
    ///
    /// Requests snapshots or patches for collections (e.g. critical_block, regular_high, regular_low, regular),
    /// decrypts mutations, persists changes to the SQLite store (ChatStore, ContactStore, MessageStore),
    /// updates stored LTHash state, and emits real-time events.
    pub async fn resync_app_state(
        &self,
        collections: &[&str],
        force_snapshot: bool,
    ) -> Result<AppStateSyncSummary, CoreError> {
        if collections.is_empty() {
            return Ok(AppStateSyncSummary::default());
        }

        // 1. Load current LTHash state per collection
        let mut states: HashMap<String, crate::syncd::LTHashState> = HashMap::new();
        for col in collections {
            if let Ok(Some(v_bytes)) = self.store.get_key("app-state-sync-version", col).await {
                if let Ok(stored) =
                    serde_json::from_slice::<crate::syncd::StoredLTHashState>(&v_bytes)
                {
                    states.insert(col.to_string(), stored.into());
                } else if v_bytes.len() >= 8 {
                    let ver = u64::from_be_bytes(v_bytes[0..8].try_into().unwrap_or([0; 8]));
                    let h = if v_bytes.len() >= 136 {
                        v_bytes[8..136].to_vec()
                    } else {
                        vec![0u8; 128]
                    };
                    states.insert(
                        col.to_string(),
                        crate::syncd::LTHashState {
                            version: ver,
                            hash: h,
                            index_value_map: HashMap::new(),
                        },
                    );
                }
            }
        }

        // 2. Build wire resync IQ
        let iq = crate::syncd::build_resync_iq(collections, &states, force_snapshot);
        debug!(
            "Dispatching AppState resync IQ for {:?} (force_snapshot={})",
            collections, force_snapshot
        );

        let response = match self.send_iq(iq, Duration::from_secs(20)).await {
            Ok(resp) => resp,
            Err(e) => {
                warn!("AppState resync IQ failed: {}", e);
                return Err(e);
            }
        };

        // 3. Extract collections data from IQ response
        let collections_data = crate::syncd::extract_syncd_response(&response)?;

        // 4. Preload all app state keys into in-memory resolver map
        let mut key_map: HashMap<String, Vec<u8>> = HashMap::new();
        if let Ok(all_keys) = self.store.get_all_keys("app-state-sync-key").await {
            for (id, data) in all_keys {
                key_map.insert(id, data);
            }
        }
        if let Ok(Some(primary)) = self.store.get_key("app-state-sync-key", "primary").await {
            key_map.insert("primary".to_string(), primary);
        }

        let key_resolver = |key_id: &[u8]| -> Option<Vec<u8>> {
            let b64 = base64::engine::general_purpose::STANDARD.encode(key_id);
            if let Some(k) = key_map.get(&b64) {
                return Some(k.clone());
            }
            let hx = hex::encode(key_id);
            if let Some(k) = key_map.get(&hx) {
                return Some(k.clone());
            }
            if let Some(k) = key_map.get("primary") {
                return Some(k.clone());
            }
            None
        };

        let mut summary = AppStateSyncSummary::default();

        // 5. Process each collection
        for (col_name, col_data) in collections_data {
            let mut curr_state = states.remove(&col_name).unwrap_or_default();
            let mut all_mutations = Vec::new();

            // A. Decode snapshot if present
            if let Some(ref snapshot) = col_data.snapshot {
                match crate::syncd::decode_syncd_snapshot(snapshot, &col_name, key_resolver, false)
                {
                    Ok((new_state, mut muts)) => {
                        curr_state = new_state;
                        all_mutations.append(&mut muts);
                    }
                    Err(e) => {
                        warn!("Failed to decode Syncd snapshot for '{}': {}", col_name, e);
                    }
                }
            }

            // B. Decode patches
            for patch in &col_data.patches {
                match crate::syncd::decode_syncd_patch(
                    patch,
                    &col_name,
                    &curr_state,
                    key_resolver,
                    false,
                ) {
                    Ok((new_state, mut muts)) => {
                        curr_state = new_state;
                        all_mutations.append(&mut muts);
                    }
                    Err(e) => {
                        warn!("Failed to decode Syncd patch for '{}': {}", col_name, e);
                    }
                }
            }

            // C. Apply decoded mutations to SQLite stores
            for m in &all_mutations {
                let action = m.parse_action();
                match action {
                    crate::syncd::SyncdAction::Mute {
                        jid,
                        muted,
                        mute_end_timestamp,
                    } => {
                        let mute_until = if muted {
                            mute_end_timestamp.map(|t| t as u64).or(Some(u64::MAX))
                        } else {
                            None
                        };
                        let _ = self.store.update_chat_mute(&jid, mute_until).await;
                        self.events.emit(BaileysEvent::ChatUpdate {
                            jid,
                            pinned: None,
                            archived: None,
                            mute_until: Some(mute_until),
                            unread_count: None,
                        });
                        summary.mutations_applied += 1;
                    }
                    crate::syncd::SyncdAction::Pin { jid, pinned } => {
                        let _ = self.store.update_chat_pin(&jid, pinned).await;
                        self.events.emit(BaileysEvent::ChatUpdate {
                            jid,
                            pinned: Some(pinned),
                            archived: None,
                            mute_until: None,
                            unread_count: None,
                        });
                        summary.mutations_applied += 1;
                    }
                    crate::syncd::SyncdAction::Archive { jid, archived } => {
                        let _ = self.store.update_chat_archive(&jid, archived).await;
                        self.events.emit(BaileysEvent::ChatUpdate {
                            jid,
                            pinned: None,
                            archived: Some(archived),
                            mute_until: None,
                            unread_count: None,
                        });
                        summary.mutations_applied += 1;
                    }
                    crate::syncd::SyncdAction::Star {
                        jid,
                        msg_id,
                        starred,
                        ..
                    } => {
                        let _ = self.store.update_message_star(&jid, &msg_id, starred).await;
                        summary.mutations_applied += 1;
                    }
                    crate::syncd::SyncdAction::Contact { id, name, lid, .. } => {
                        let contact = baileys_store::StoredContact {
                            jid: id.clone(),
                            name: name.clone(),
                            push_name: name.clone(),
                            lid,
                        };
                        let _ = self.store.save_contact(&contact).await;
                        self.events.emit(BaileysEvent::ContactUpdate { contact });
                        summary.mutations_applied += 1;
                    }
                    crate::syncd::SyncdAction::MarkChatAsRead { jid, read } => {
                        let _ = self.store.update_chat_unread(&jid, !read).await;
                        let unread_count = if read { 0 } else { 1 };
                        self.events.emit(BaileysEvent::ChatUpdate {
                            jid,
                            pinned: None,
                            archived: None,
                            mute_until: None,
                            unread_count: Some(unread_count),
                        });
                        summary.mutations_applied += 1;
                    }
                    crate::syncd::SyncdAction::ClearChat { jid } => {
                        let _ = self.store.clear_chat_messages(&jid, true).await;
                        summary.mutations_applied += 1;
                    }
                    crate::syncd::SyncdAction::DeleteChat { jid } => {
                        let _ = self.store.delete_chat(&jid).await;
                        summary.mutations_applied += 1;
                    }
                    crate::syncd::SyncdAction::Other { .. } => {}
                }
            }

            // D. Persist updated LTHashState for this collection
            let stored = crate::syncd::StoredLTHashState::from(&curr_state);
            if let Ok(json_bytes) = serde_json::to_vec(&stored) {
                let _ = self
                    .store
                    .set_keys(vec![baileys_store::KeyEntry {
                        key_type: "app-state-sync-version".to_string(),
                        id: col_name.clone(),
                        data: json_bytes,
                    }])
                    .await;
            }

            summary.collections_synced += 1;
        }

        // 6. Emit AppStateSynced event
        self.events.emit(BaileysEvent::AppStateSynced {
            collections_synced: summary.collections_synced,
            mutations_applied: summary.mutations_applied,
        });

        info!(
            "AppState sync complete: {} collections synced, {} mutations applied",
            summary.collections_synced, summary.mutations_applied
        );

        Ok(summary)
    }
    /// Helper to dispatch AppState Syncd mutation patch to WhatsApp server.
    ///
    /// Fails when the primary device has not shared an app-state key yet, when the patch cannot be
    /// built, or when the server rejects or does not answer the IQ. Callers update the local store
    /// first, so on error the local state is ahead of the server.
    pub(crate) async fn dispatch_syncd_patch<F>(
        &self,
        collection: &str,
        patch_fn: F,
    ) -> Result<(), CoreError>
    where
        F: FnOnce(&[u8], &[u8], &[u8], u64) -> Result<crate::syncd::SyncdPatchResult, CoreError>,
    {
        let version_state = self
            .store
            .get_key("app-state-sync-version", collection)
            .await
            .unwrap_or(None);
        let (current_version, current_hash) = if let Some(v_bytes) = version_state {
            if let Ok(stored) = serde_json::from_slice::<crate::syncd::StoredLTHashState>(&v_bytes)
            {
                let state: crate::syncd::LTHashState = stored.into();
                (state.version, state.hash)
            } else if v_bytes.len() >= 8 {
                let ver = u64::from_be_bytes(v_bytes[0..8].try_into().unwrap_or([0; 8]));
                let h = if v_bytes.len() >= 136 {
                    v_bytes[8..136].to_vec()
                } else {
                    vec![0u8; 128]
                };
                (ver, h)
            } else {
                (0, vec![0u8; 128])
            }
        } else {
            (0, vec![0u8; 128])
        };

        let key_data_opt = self
            .store
            .get_key("app-state-sync-key", "primary")
            .await
            .unwrap_or(None);
        let key_id_opt = self
            .store
            .get_key("app-state-sync-key", "primary_id")
            .await
            .unwrap_or(None);

        let (Some(key_data), Some(key_id)) = (key_data_opt, key_id_opt) else {
            return Err(CoreError::Protocol(format!(
                "no AppStateSyncKey for '{collection}': the primary device has not shared app-state keys yet"
            )));
        };

        let res = patch_fn(&key_id, &key_data, &current_hash, current_version)?;
        let iq = crate::syncd::build_syncd_iq(collection, current_version, &res.patch);
        self.send_iq(iq, Duration::from_secs(10)).await?;
        debug!(
            "Syncd mutation for collection '{}' acknowledged by WhatsApp gateway",
            collection
        );
        let updated_state = crate::syncd::LTHashState {
            version: res.next_version,
            hash: res.new_hash,
            index_value_map: HashMap::new(),
        };
        let stored = crate::syncd::StoredLTHashState::from(&updated_state);
        if let Ok(json_bytes) = serde_json::to_vec(&stored) {
            let _ = self
                .store
                .set_keys(vec![baileys_store::KeyEntry {
                    key_type: "app-state-sync-version".to_string(),
                    id: collection.to_string(),
                    data: json_bytes,
                }])
                .await;
        }
        Ok(())
    }
    /// Modifies chat mute status (None = unmuted, Some(duration_seconds) = muted until now + duration, or Some(u64::MAX) for indefinite mute).
    pub async fn chat_modify_mute(
        &self,
        jid: &str,
        duration_secs: Option<u64>,
    ) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@s.whatsapp.net", jid.trim_start_matches('+'))
        } else {
            jid.to_string()
        };
        let mute_until = duration_secs.map(|dur| {
            if dur == u64::MAX {
                u64::MAX
            } else {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    + dur
            }
        });

        // 1. Update local database immediately
        self.store
            .update_chat_mute(&normalized, mute_until)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Dispatch wire-level Syncd patch
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_HIGH,
            |key_id, key_data, state_hash, ver| {
                crate::syncd::create_mute_patch(
                    &normalized,
                    mute_until,
                    key_id,
                    key_data,
                    state_hash,
                    ver,
                )
            },
        )
        .await?;

        Ok(())
    }
    /// Pins or unpins a chat.
    pub async fn chat_modify_pin(&self, jid: &str, pin: bool) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@s.whatsapp.net", jid.trim_start_matches('+'))
        } else {
            jid.to_string()
        };

        // 1. Update local database immediately
        self.store
            .update_chat_pin(&normalized, pin)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Dispatch wire-level Syncd patch
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_LOW,
            |key_id, key_data, state_hash, ver| {
                crate::syncd::create_pin_patch(&normalized, pin, key_id, key_data, state_hash, ver)
            },
        )
        .await?;

        Ok(())
    }
    /// Archives or unarchives a chat.
    pub async fn chat_modify_archive(&self, jid: &str, archive: bool) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@s.whatsapp.net", jid.trim_start_matches('+'))
        } else {
            jid.to_string()
        };

        // 1. Update local database immediately
        self.store
            .update_chat_archive(&normalized, archive)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Dispatch wire-level Syncd patch
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_LOW,
            |key_id, key_data, state_hash, ver| {
                crate::syncd::create_archive_patch(
                    &normalized,
                    archive,
                    key_id,
                    key_data,
                    state_hash,
                    ver,
                )
            },
        )
        .await?;

        Ok(())
    }
    /// Stars or unstars an individual message.
    pub async fn chat_modify_star(
        &self,
        jid: &str,
        msg_id: &str,
        from_me: bool,
        star: bool,
    ) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@s.whatsapp.net", jid.trim_start_matches('+'))
        } else {
            jid.to_string()
        };

        // 1. Update local database immediately
        self.store
            .update_message_star(&normalized, msg_id, star)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Dispatch wire-level Syncd patch
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_LOW,
            |key_id, key_data, state_hash, ver| {
                crate::syncd::create_star_patch(
                    &normalized,
                    msg_id,
                    from_me,
                    star,
                    key_id,
                    key_data,
                    state_hash,
                    ver,
                )
            },
        )
        .await?;

        Ok(())
    }
    /// Rejects an incoming WhatsApp voice or video call.
    pub async fn reject_call(&self, call_id: &str, call_from: &str) -> Result<(), CoreError> {
        let me = self
            .me_jid()
            .await?
            .unwrap_or_else(|| S_WHATSAPP_NET.to_string());
        let reject_node = crate::call::build_reject_call_node(&me, call_id, call_from);
        let socket_opt = {
            let lock = self.socket.lock().await;
            lock.clone()
        };
        let socket = socket_opt.ok_or_else(|| CoreError::Network("Client not connected".into()))?;
        socket.send_node(reject_node).await
    }
    /// Fetches past call logs stored in SQLite.
    pub async fn get_call_history(
        &self,
        limit: u32,
    ) -> Result<Vec<baileys_store::StoredCall>, CoreError> {
        self.store
            .get_call_history(limit)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }
    /// Creates a WhatsApp call link for an audio or video call.
    pub async fn create_call_link(&self, is_video: bool) -> Result<Option<String>, CoreError> {
        let media_type = if is_video { "video" } else { "audio" };
        let iq = crate::call::build_create_call_link_node(media_type, None);
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        Ok(crate::call::parse_call_link_response(&res))
    }
    /// Requests on-demand message history for a chat via w:sync:history IQ.
    pub async fn fetch_message_history(
        &self,
        chat_jid: &str,
        count: u32,
        oldest_msg_id: &str,
        oldest_from_me: bool,
        oldest_timestamp: u64,
    ) -> Result<String, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let req_node = BinaryNode::new("history_sync_request")
            .with_attr("chat_jid", chat_jid)
            .with_attr("count", count.to_string())
            .with_attr("oldest_msg_id", oldest_msg_id)
            .with_attr(
                "oldest_msg_from_me",
                if oldest_from_me { "true" } else { "false" },
            )
            .with_attr("oldest_msg_timestamp", oldest_timestamp.to_string());

        let iq_node = BinaryNode::new("iq")
            .with_attr("to", S_WHATSAPP_NET)
            .with_attr("type", "set")
            .with_attr("xmlns", "w:sync:history")
            .with_attr("id", &iq_id)
            .with_child(req_node);

        self.send_iq(iq_node, Duration::from_secs(15)).await?;
        Ok(iq_id)
    }
    /// Clears message history for a chat while optionally keeping starred messages.
    pub async fn chat_modify_clear(
        &self,
        chat_jid: &str,
        keep_starred: bool,
    ) -> Result<(), CoreError> {
        let normalized = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid.trim_start_matches('+'))
        } else {
            chat_jid.to_string()
        };

        // 1. Update local database immediately
        self.store
            .clear_chat_messages(&normalized, keep_starred)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Dispatch wire-level Syncd patch
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_HIGH,
            |key_id, key_data, state_hash, ver| {
                crate::syncd::create_clear_chat_patch(
                    &normalized,
                    key_id,
                    key_data,
                    state_hash,
                    ver,
                )
            },
        )
        .await?;

        Ok(())
    }
    /// Deletes a chat conversation thread from WhatsApp and local SQLite store.
    pub async fn chat_modify_delete(&self, chat_jid: &str) -> Result<(), CoreError> {
        let normalized = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid.trim_start_matches('+'))
        } else {
            chat_jid.to_string()
        };

        // 1. Update local database immediately
        self.store
            .delete_chat(&normalized)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Dispatch wire-level Syncd patch
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_HIGH,
            |key_id, key_data, state_hash, ver| {
                crate::syncd::create_delete_chat_patch(
                    &normalized,
                    key_id,
                    key_data,
                    state_hash,
                    ver,
                )
            },
        )
        .await?;

        Ok(())
    }
    /// Marks a chat as read or unread across devices.
    pub async fn chat_modify_mark_unread(
        &self,
        chat_jid: &str,
        unread: bool,
    ) -> Result<(), CoreError> {
        let normalized = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid.trim_start_matches('+'))
        } else {
            chat_jid.to_string()
        };

        // 1. Update local database immediately
        self.store
            .update_chat_unread(&normalized, unread)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Dispatch wire-level Syncd patch
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_LOW,
            |key_id, key_data, state_hash, ver| {
                crate::syncd::create_mark_chat_unread_patch(
                    &normalized,
                    unread,
                    key_id,
                    key_data,
                    state_hash,
                    ver,
                )
            },
        )
        .await?;

        Ok(())
    }
}
