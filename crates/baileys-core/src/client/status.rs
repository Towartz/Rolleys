//! Domain submodule for status operations on BaileysClient.

use super::*;

impl BaileysClient {
    /// Publishes a text status story to `status@broadcast`.
    pub async fn send_text_status(
        &self,
        text: &str,
        background_color: Option<&str>,
        font: Option<i32>,
        audience: Option<Vec<String>>,
    ) -> Result<String, CoreError> {
        let msg = crate::status::build_text_status_message(text, background_color, font);
        let options = MessageOptions {
            status_jid_list: audience,
            ..Default::default()
        };
        let msg_id = self
            .send_message_with_options("status@broadcast", msg, options)
            .await?;

        // Save our own story in SQLite store
        let creds = self.store.get_creds().await.ok().flatten();
        let me_jid = creds
            .and_then(|c| c.me_jid)
            .unwrap_or_else(|| "me".to_string());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let story = baileys_store::StoredStory {
            msg_id: msg_id.clone(),
            author_jid: me_jid,
            author_name: Some("Me".to_string()),
            timestamp: now,
            message_type: "text".to_string(),
            content: Some(text.to_string()),
            caption: None,
            has_media: false,
            viewed: true,
        };
        let _ = self.store.save_story(&story).await;

        Ok(msg_id)
    }
    /// Publishes an HD Image status story to `status@broadcast` without heavy client compression.
    pub async fn send_status_image(
        &self,
        image_bytes: &[u8],
        caption: Option<&str>,
        mimetype: Option<&str>,
        audience: Option<Vec<String>>,
    ) -> Result<String, CoreError> {
        let processed_bytes = baileys_media::scale_image_for_status(image_bytes, true)
            .unwrap_or_else(|_| image_bytes.to_vec());

        let encrypted = baileys_media::encrypt_media_bytes(
            &processed_bytes,
            None,
            baileys_media::MediaType::Image,
        )
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let media_conn = self.refresh_media_conn(false).await?;

        let upload_res = baileys_media::upload_encrypted_media(
            &encrypted.encrypted_bytes,
            &encrypted.file_enc_sha256,
            baileys_media::MediaType::Image,
            &media_conn,
        )
        .await
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let dims = baileys_media::extract_image_dimensions(&processed_bytes);
        let micro_thumb = baileys_media::generate_micro_thumbnail(&processed_bytes, 48).ok();
        let now_sec = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let is_hd = dims.as_ref().map(|d| d.is_hd()).unwrap_or(false);

        let attrs = crate::status::StatusMediaAttributes {
            width: dims.as_ref().map(|d| d.width),
            height: dims.as_ref().map(|d| d.height),
            jpeg_thumbnail: micro_thumb,
            is_hd,
            media_key_timestamp: Some(now_sec as i64),
        };

        let msg = crate::status::build_image_status_message(
            upload_res.url,
            upload_res.direct_path,
            encrypted.media_key.to_vec(),
            encrypted.file_sha256.to_vec(),
            encrypted.file_enc_sha256.to_vec(),
            image_bytes.len() as u64,
            mimetype,
            caption,
            Some(attrs),
        );

        let options = MessageOptions {
            status_jid_list: audience,
            ..Default::default()
        };
        let msg_id = self
            .send_message_with_options("status@broadcast", msg, options)
            .await?;

        let creds = self.store.get_creds().await.ok().flatten();
        let me_jid = creds
            .and_then(|c| c.me_jid)
            .unwrap_or_else(|| "me".to_string());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let story = baileys_store::StoredStory {
            msg_id: msg_id.clone(),
            author_jid: me_jid,
            author_name: Some("Me".to_string()),
            timestamp: now,
            message_type: "image".to_string(),
            content: None,
            caption: caption.map(|s| s.to_string()),
            has_media: true,
            viewed: true,
        };
        let _ = self.store.save_story(&story).await;

        Ok(msg_id)
    }
    /// Publishes an HD Video status story to `status@broadcast` without heavy client compression.
    pub async fn send_status_video(
        &self,
        video_bytes: &[u8],
        caption: Option<&str>,
        mimetype: Option<&str>,
        audience: Option<Vec<String>>,
    ) -> Result<String, CoreError> {
        let encrypted =
            baileys_media::encrypt_media_bytes(video_bytes, None, baileys_media::MediaType::Video)
                .map_err(|e| CoreError::Media(e.to_string()))?;

        let media_conn = self.refresh_media_conn(false).await?;

        let upload_res = baileys_media::upload_encrypted_media(
            &encrypted.encrypted_bytes,
            &encrypted.file_enc_sha256,
            baileys_media::MediaType::Video,
            &media_conn,
        )
        .await
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let now_sec = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let attrs = crate::status::StatusMediaAttributes {
            width: None,
            height: None,
            jpeg_thumbnail: None,
            is_hd: true,
            media_key_timestamp: Some(now_sec as i64),
        };

        let msg = crate::status::build_video_status_message(
            upload_res.url,
            upload_res.direct_path,
            encrypted.media_key.to_vec(),
            encrypted.file_sha256.to_vec(),
            encrypted.file_enc_sha256.to_vec(),
            video_bytes.len() as u64,
            30,
            mimetype,
            caption,
            Some(attrs),
        );

        let options = MessageOptions {
            status_jid_list: audience,
            ..Default::default()
        };
        let msg_id = self
            .send_message_with_options("status@broadcast", msg, options)
            .await?;

        let creds = self.store.get_creds().await.ok().flatten();
        let me_jid = creds
            .and_then(|c| c.me_jid)
            .unwrap_or_else(|| "me".to_string());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let story = baileys_store::StoredStory {
            msg_id: msg_id.clone(),
            author_jid: me_jid,
            author_name: Some("Me".to_string()),
            timestamp: now,
            message_type: "video".to_string(),
            content: None,
            caption: caption.map(|s| s.to_string()),
            has_media: true,
            viewed: true,
        };
        let _ = self.store.save_story(&story).await;

        Ok(msg_id)
    }
    /// Publishes a rich URL link preview status story to `status@broadcast`.
    pub async fn send_link_status(
        &self,
        url: &str,
        title: Option<&str>,
        description: Option<&str>,
        custom_text: Option<&str>,
        background_color: Option<&str>,
        font: Option<i32>,
        thumbnail_bytes: Option<Vec<u8>>,
        audience: Option<Vec<String>>,
    ) -> Result<String, CoreError> {
        let msg = crate::status::build_link_status_message(
            url,
            title,
            description,
            custom_text,
            background_color,
            font,
            thumbnail_bytes,
        );
        let options = MessageOptions {
            status_jid_list: audience,
            ..Default::default()
        };
        let msg_id = self
            .send_message_with_options("status@broadcast", msg, options)
            .await?;

        // Save our own story in SQLite store
        let creds = self.store.get_creds().await.ok().flatten();
        let me_jid = creds
            .and_then(|c| c.me_jid)
            .unwrap_or_else(|| "me".to_string());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let story = baileys_store::StoredStory {
            msg_id: msg_id.clone(),
            author_jid: me_jid,
            author_name: Some("Me".to_string()),
            timestamp: now,
            message_type: "text".to_string(),
            content: Some(custom_text.unwrap_or(url).to_string()),
            caption: None,
            has_media: false,
            viewed: true,
        };
        let _ = self.store.save_story(&story).await;

        Ok(msg_id)
    }
    /// Publishes an HD Audio / Voice Note status story to `status@broadcast`.
    pub async fn send_status_audio(
        &self,
        audio_bytes: &[u8],
        seconds: u32,
        mimetype: Option<&str>,
        audience: Option<Vec<String>>,
    ) -> Result<String, CoreError> {
        let encrypted =
            baileys_media::encrypt_media_bytes(audio_bytes, None, baileys_media::MediaType::Audio)
                .map_err(|e| CoreError::Media(e.to_string()))?;

        let media_conn = self.refresh_media_conn(false).await?;

        let upload_res = baileys_media::upload_encrypted_media(
            &encrypted.encrypted_bytes,
            &encrypted.file_enc_sha256,
            baileys_media::MediaType::Audio,
            &media_conn,
        )
        .await
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let msg = crate::status::build_audio_status_message(
            upload_res.url,
            upload_res.direct_path,
            encrypted.media_key.to_vec(),
            encrypted.file_sha256.to_vec(),
            encrypted.file_enc_sha256.to_vec(),
            audio_bytes.len() as u64,
            seconds,
            mimetype,
            None,
        );

        let options = MessageOptions {
            status_jid_list: audience,
            ..Default::default()
        };
        let msg_id = self
            .send_message_with_options("status@broadcast", msg, options)
            .await?;

        let creds = self.store.get_creds().await.ok().flatten();
        let me_jid = creds
            .and_then(|c| c.me_jid)
            .unwrap_or_else(|| "me".to_string());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let story = baileys_store::StoredStory {
            msg_id: msg_id.clone(),
            author_jid: me_jid,
            author_name: Some("Me".to_string()),
            timestamp: now,
            message_type: "audio".to_string(),
            content: None,
            caption: None,
            has_media: true,
            viewed: true,
        };
        let _ = self.store.save_story(&story).await;

        Ok(msg_id)
    }
    /// Sends a read receipt for a status story, marking it as viewed both on WhatsApp and locally.
    pub async fn send_status_read_receipt(
        &self,
        msg_id: &str,
        author_jid: &str,
    ) -> Result<(), CoreError> {
        if self.is_ghost_mode() {
            debug!(
                "[Ghost Mode] Suppressed status read receipt for {} to {}",
                msg_id, author_jid
            );
            self.store
                .mark_story_viewed(msg_id)
                .await
                .map_err(|e| CoreError::Store(e.to_string()))?;
            return Ok(());
        }

        let _creds = self
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

        let receipt_node = crate::status::build_status_read_receipt(msg_id, author_jid);
        if let Some(socket) = socket_opt {
            socket.send_node(receipt_node).await?;
        }

        // Mark as viewed in local SQLite store
        self.store
            .mark_story_viewed(msg_id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;
        Ok(())
    }
    /// Fetches recent status updates from the SQLite cache.
    pub async fn get_recent_stories(
        &self,
        limit: u32,
        unviewed_only: bool,
    ) -> Result<Vec<baileys_store::StoredStory>, CoreError> {
        self.store
            .get_stories(limit, unviewed_only)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }
    /// Fetches all active status stories posted by a specific contact.
    pub async fn get_stories_by_author(
        &self,
        author_jid: &str,
    ) -> Result<Vec<baileys_store::StoredStory>, CoreError> {
        self.store
            .get_stories_by_author(author_jid)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }
}
