//! Domain submodule for privacy operations on BaileysClient.

use super::*;

impl BaileysClient {
    /// Fetches the user's current privacy settings from WhatsApp gateway.
    pub async fn fetch_privacy_settings(
        &self,
    ) -> Result<crate::privacy::PrivacySettings, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = crate::privacy::build_privacy_query_iq(&iq_id);
        let res = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        Ok(crate::privacy::parse_privacy_settings_response(&res))
    }
    /// Updates a privacy setting (category: "readreceipts", "profile", "status", "online", "last", "groupadd", "calladd").
    pub async fn update_privacy_setting(
        &self,
        category: &str,
        value: &str,
    ) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = crate::privacy::build_privacy_update_iq(&iq_id, category, value);
        self.send_iq(iq_node, Duration::from_secs(10)).await?;
        Ok(())
    }
    /// Fetches the list of blocked JIDs from WhatsApp server and syncs to local SQLite store.
    pub async fn fetch_blocklist(&self) -> Result<Vec<String>, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = crate::privacy::build_fetch_blocklist_iq(&iq_id);
        let res = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        let list = crate::privacy::parse_blocklist_response(&res);
        let _ = self.store.set_blocked_contacts(&list).await;
        Ok(list)
    }
    /// Retrieves all locally cached blocked contacts from SQLite.
    pub async fn get_blocked_contacts(&self) -> Result<Vec<String>, CoreError> {
        self.store
            .get_blocked_contacts()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }
    /// Blocks or unblocks a contact ("block" or "unblock") and persists to SQLite.
    pub async fn update_block_status(&self, jid: &str, action: &str) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let normalized = if !jid.contains('@') {
            format!("{}@s.whatsapp.net", jid.trim_start_matches('+'))
        } else {
            jid.to_string()
        };

        // If contact has LID/PN stored, pass pn_jid
        let pn_opt = if normalized.ends_with("@lid") {
            if let Ok(Some(c)) = self.store.get_contact(&normalized).await {
                c.lid.clone()
            } else {
                None
            }
        } else {
            None
        };

        let iq_node =
            crate::privacy::build_block_iq(&iq_id, &normalized, action, pn_opt.as_deref());
        self.send_iq(iq_node, Duration::from_secs(10)).await?;

        // Update local SQLite store
        if action == "block" || action == "add" {
            let _ = self.store.save_blocked_contact(&normalized).await;
        } else if action == "unblock" || action == "remove" {
            let _ = self.store.remove_blocked_contact(&normalized).await;
        }

        Ok(())
    }
    /// Updates your WhatsApp About / Bio status string.
    pub async fn update_profile_status(&self, status: &str) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = crate::privacy::build_update_profile_status_iq(&iq_id, status);
        self.send_iq(iq_node, Duration::from_secs(10)).await?;
        Ok(())
    }
    /// Queries the WhatsApp About / Bio status of a user.
    pub async fn fetch_status(
        &self,
        jid: &str,
    ) -> Result<Option<crate::privacy::ProfileStatus>, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let normalized = if !jid.contains('@') {
            format!("{}@s.whatsapp.net", jid.trim_start_matches('+'))
        } else {
            jid.to_string()
        };
        let iq_node = crate::privacy::build_fetch_status_iq(&iq_id, &normalized);
        let res = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        Ok(crate::privacy::parse_profile_status_response(&res))
    }
    /// Queries business profile metadata (address, description, websites, email, category) for a WhatsApp Business account.
    pub async fn get_business_profile(
        &self,
        jid: &str,
    ) -> Result<Option<crate::privacy::BusinessProfile>, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let normalized = if !jid.contains('@') {
            format!("{}@s.whatsapp.net", jid.trim_start_matches('+'))
        } else {
            jid.to_string()
        };
        let iq_node = crate::privacy::build_business_profile_iq(&iq_id, &normalized);
        let res = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        Ok(crate::privacy::parse_business_profile_response(&res))
    }
    /// Fetches the profile picture URL for a user or group.
    /// `picture_type` can be "preview" (low-res thumbnail) or "image" (high-res).
    pub async fn profile_picture_url(
        &self,
        jid: &str,
        picture_type: &str,
    ) -> Result<Option<String>, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        // Like Baileys' jidNormalizedUser: a device jid ("628..:12@s.whatsapp.net") has no picture.
        let normalized = if !jid.contains('@') {
            format!("{}@s.whatsapp.net", jid)
        } else {
            crate::client::normalize_user_jid(jid)
        };
        let ptype = if picture_type == "image" {
            "image"
        } else {
            "preview"
        };
        let picture_node = BinaryNode::new("picture")
            .with_attr("type", ptype)
            .with_attr("query", "url");

        let iq_node = BinaryNode::new("iq")
            .with_attr("target", &normalized)
            .with_attr("to", S_WHATSAPP_NET)
            .with_attr("type", "get")
            .with_attr("xmlns", "w:profile:picture")
            .with_attr("id", &iq_id)
            .with_child(picture_node);

        let res = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        if let Some(pic) = res.get_child("picture") {
            Ok(pic.get_attr("url").map(|s| s.to_string()))
        } else {
            Ok(None)
        }
    }
    /// Updates the profile picture for the logged-in user or a group.
    pub async fn update_profile_picture(
        &self,
        jid_opt: Option<&str>,
        image_bytes: &[u8],
    ) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let mut iq_node = BinaryNode::new("iq")
            .with_attr("to", S_WHATSAPP_NET)
            .with_attr("type", "set")
            .with_attr("xmlns", "w:profile:picture")
            .with_attr("id", &iq_id);

        if let Some(target) = jid_opt {
            let normalized = if !target.contains('@') {
                format!("{}@s.whatsapp.net", target)
            } else {
                target.to_string()
            };
            iq_node = iq_node.with_attr("target", normalized);
        }

        // Like Baileys' generateProfilePicture: centred square, 640 px, JPEG.
        let jpeg = baileys_media::generate_profile_picture(image_bytes, 640)
            .map_err(|e| CoreError::Media(e.to_string()))?;
        let picture_node = BinaryNode::new("picture")
            .with_attr("type", "image")
            .with_bytes_content(jpeg);

        iq_node = iq_node.with_child(picture_node);
        self.send_iq(iq_node, Duration::from_secs(15)).await?;
        Ok(())
    }
    /// Removes the profile picture for self or a group.
    pub async fn remove_profile_picture(&self, jid_opt: Option<&str>) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let mut iq_node = BinaryNode::new("iq")
            .with_attr("to", S_WHATSAPP_NET)
            .with_attr("type", "set")
            .with_attr("xmlns", "w:profile:picture")
            .with_attr("id", &iq_id);

        if let Some(target) = jid_opt {
            let normalized = if !target.contains('@') {
                format!("{}@s.whatsapp.net", target)
            } else {
                target.to_string()
            };
            iq_node = iq_node.with_attr("target", normalized);
        }

        self.send_iq(iq_node, Duration::from_secs(10)).await?;
        Ok(())
    }
    /// Updates the push name / display name for the logged-in user.
    pub async fn update_profile_name(&self, name: &str) -> Result<(), CoreError> {
        let name_trimmed = name.trim();
        // 1. Update stored auth credentials
        if let Ok(Some(mut creds)) = self.store.get_creds().await {
            creds.me_name = Some(name_trimmed.to_string());
            let _ = self.store.save_creds(&creds).await;
        }

        // 2. Also persist in KV store for quick access
        let _ = self.store.set_kv("profile:name", name_trimmed).await;

        // 3. Send presence node with updated name over socket if connected
        let node = BinaryNode::new("presence")
            .with_attr("name", name_trimmed)
            .with_attr("type", "available");

        let socket_opt = {
            let lock = self.socket.lock().await;
            lock.clone()
        };
        if let Some(socket) = socket_opt {
            let _ = socket.send_node(node).await;
        }

        Ok(())
    }
    /// Subscribes to presence updates (online / offline / typing status) for a contact or group.
    pub async fn presence_subscribe(&self, to_jid: &str) -> Result<(), CoreError> {
        let socket_opt = {
            let lock = self.socket.lock().await;
            lock.clone()
        };
        let socket = socket_opt.ok_or_else(|| CoreError::Network("Client not connected".into()))?;

        let normalized = if !to_jid.contains('@') {
            format!("{}@s.whatsapp.net", to_jid)
        } else {
            to_jid.to_string()
        };

        let node = BinaryNode::new("presence")
            .with_attr("to", normalized)
            .with_attr("id", &format!("3EB0{:X}", rand::random::<u64>())[..12])
            .with_attr("type", "subscribe");

        socket.send_node(node).await
    }
    /// Checks whether phone numbers are registered on WhatsApp using the USync contact protocol.
    pub async fn check_on_whatsapp(
        &self,
        numbers: &[&str],
    ) -> Result<Vec<crate::usync::OnWhatsAppResult>, CoreError> {
        let iq_node = crate::usync::build_usync_contact_iq(numbers);
        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        crate::usync::parse_usync_contact_response(&res)
    }
    /// Sets the default disappearing-messages timer applied to new chats (`0` turns it off)
    /// (Baileys `updateDefaultDisappearingMode`).
    pub async fn update_default_disappearing_mode(
        &self,
        duration_secs: u32,
    ) -> Result<(), CoreError> {
        let iq = crate::usync::build_default_disappearing_mode_iq(duration_secs);
        self.send_iq(iq, Duration::from_secs(10)).await?;
        Ok(())
    }
    /// Default disappearing-messages timer of users, as `(jid, duration_secs, set_at)`
    /// (Baileys `fetchDisappearingDuration`).
    pub async fn fetch_disappearing_duration(
        &self,
        jids: &[&str],
    ) -> Result<Vec<(String, u32, u64)>, CoreError> {
        let iq = crate::usync::build_usync_disappearing_mode_iq(jids);
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        crate::usync::parse_usync_disappearing_mode_response(&res)
    }
    /// Configures disappearing messages ephemeral timer for a 1:1 or group chat.
    /// Duration in seconds: 0 (Off), 86400 (24h), 604800 (7d), 7776000 (90d).
    pub async fn set_chat_ephemeral(
        &self,
        chat_jid: &str,
        expiration_seconds: u32,
    ) -> Result<(), CoreError> {
        let normalized_jid = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid)
        } else {
            chat_jid.to_string()
        };
        if normalized_jid.ends_with("@g.us") {
            let iq_node =
                crate::ephemeral::build_group_ephemeral_iq(&normalized_jid, expiration_seconds);
            self.send_iq(iq_node, Duration::from_secs(10)).await?;
        } else {
            let msg = crate::ephemeral::build_ephemeral_setting_message(
                &normalized_jid,
                expiration_seconds,
            );
            self.send_message(&normalized_jid, msg).await?;
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let _ = self
            .store
            .update_chat_ephemeral(&normalized_jid, expiration_seconds, Some(now))
            .await;

        self.events.emit(BaileysEvent::ChatEphemeralUpdate {
            chat_jid: normalized_jid,
            ephemeral_expiration: expiration_seconds,
        });

        Ok(())
    }
    /// Retrieves current ephemeral disappearing timer settings for a chat (seconds, timestamp).
    pub async fn get_chat_ephemeral(
        &self,
        chat_jid: &str,
    ) -> Result<Option<(u32, Option<i64>)>, CoreError> {
        let normalized_jid = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid)
        } else {
            chat_jid.to_string()
        };
        self.store
            .get_chat_ephemeral(&normalized_jid)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }
    /// Sets business account cover photo.
    pub async fn update_cover_photo(
        &self,
        photo_id: &str,
        token: &str,
        ts: u64,
    ) -> Result<(), CoreError> {
        let iq = crate::commerce::build_update_cover_photo_iq(photo_id, token, ts);
        self.send_iq(iq, Duration::from_secs(20)).await?;
        Ok(())
    }
    /// Removes business account cover photo.
    pub async fn remove_cover_photo(&self, photo_id: &str) -> Result<(), CoreError> {
        let iq = crate::commerce::build_remove_cover_photo_iq(photo_id);
        self.send_iq(iq, Duration::from_secs(15)).await?;
        Ok(())
    }
    /// Fetches contact status text and timestamp via USync protocol.
    pub async fn get_user_status(
        &self,
        jids: &[&str],
    ) -> Result<Vec<(String, String, u64)>, CoreError> {
        let iq = crate::usync::build_usync_status_iq(jids);
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        crate::usync::parse_usync_status_response(&res)
    }

    /// Queries the linked recovery email associated with this WhatsApp account.
    pub async fn get_linked_email(&self) -> Result<Option<String>, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq = crate::account::build_get_email_iq(&iq_id);
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        crate::account::parse_email_response(&res)
    }

    /// Sets or updates the recovery email associated with this WhatsApp account.
    pub async fn set_linked_email(&self, email: &str) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq = crate::account::build_set_email_iq(&iq_id, email);
        self.send_iq(iq, Duration::from_secs(15)).await?;
        Ok(())
    }

    /// Requests a 6-digit confirmation code sent to the account's pending email.
    pub async fn request_email_code(&self, lang: &str, locale: &str) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq = crate::account::build_request_email_code_iq(&iq_id, lang, locale);
        self.send_iq(iq, Duration::from_secs(15)).await?;
        Ok(())
    }

    /// Submits a 6-digit confirmation code to verify the pending email.
    pub async fn verify_email_code(&self, code: &str) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq = crate::account::build_verify_email_code_iq(&iq_id, code);
        self.send_iq(iq, Duration::from_secs(15)).await?;
        Ok(())
    }

    /// Queries the acceptance status of specific Terms of Service notices.
    pub async fn query_tos(
        &self,
        notice_ids: &[&str],
    ) -> Result<Vec<crate::account::TosNotice>, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq = crate::account::build_tos_query_iq(&iq_id, notice_ids);
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        crate::account::parse_tos_response(&res)
    }

    /// Accepts or updates the stage for specified Terms of Service notices.
    pub async fn accept_tos(
        &self,
        notice_ids: &[&str],
        stage: Option<&str>,
    ) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq = crate::account::build_tos_update_iq(&iq_id, notice_ids, stage);
        self.send_iq(iq, Duration::from_secs(15)).await?;
        Ok(())
    }
}
