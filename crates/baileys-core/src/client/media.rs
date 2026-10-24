//! Domain submodule for media operations on BaileysClient.

use super::*;

impl BaileysClient {
    /// Toggles auto-download for incoming media.
    pub fn set_auto_download(&self, enabled: bool) {
        self.auto_download.store(enabled, Ordering::SeqCst);
    }
    /// Checks if auto-download is enabled.
    pub fn is_auto_download(&self) -> bool {
        self.auto_download.load(Ordering::SeqCst)
    }
    /// Downloads and decrypts the media attachment for a cached or database message ID, saving to dest_dir.
    pub async fn download_media_for_message(
        &self,
        msg_id: &str,
        dest_dir: &std::path::Path,
    ) -> Result<std::path::PathBuf, CoreError> {
        let proto = {
            let cache = self.recent_messages.lock().await;
            cache.get(msg_id).and_then(|info| info.message.clone())
        };

        let proto = match proto {
            Some(p) => p,
            None => {
                // Persistent store fallback: search SQLite database
                match self.store.get_message(msg_id).await {
                    Ok(Some(stored)) => {
                        if !stored.proto_bytes.is_empty() {
                            Message::decode(&stored.proto_bytes[..]).map_err(|e| {
                                CoreError::Protocol(format!(
                                    "Failed to decode protobuf for message {}: {}",
                                    msg_id, e
                                ))
                            })?
                        } else {
                            return Err(CoreError::Protocol(format!(
                                "Message '{}' in database has empty protobuf body",
                                msg_id
                            )));
                        }
                    }
                    _ => {
                        return Err(CoreError::Protocol(format!(
                            "Message ID '{}' not found in recent cache or database",
                            msg_id
                        )));
                    }
                }
            }
        };

        let attachment = baileys_media::extract_media_from_message(&proto).ok_or_else(|| {
            CoreError::Media("Message does not contain any media attachment".into())
        })?;

        baileys_media::download_media_to_dir(&attachment, dest_dir, msg_id)
            .await
            .map_err(|e| CoreError::Media(e.to_string()))
    }
    /// Downloads and decrypts the media attachment for a message using the streaming pipeline,
    /// writing incrementally to disk with ~64KB memory usage and emitting real-time progress events.
    pub async fn download_message_media_to_path(
        &self,
        msg_id: &str,
        target_path: Option<&std::path::Path>,
    ) -> Result<std::path::PathBuf, CoreError> {
        let proto = {
            let cache = self.recent_messages.lock().await;
            cache.get(msg_id).and_then(|info| info.message.clone())
        };

        let proto = match proto {
            Some(p) => p,
            None => match self.store.get_message(msg_id).await {
                Ok(Some(stored)) => {
                    if !stored.proto_bytes.is_empty() {
                        Message::decode(&stored.proto_bytes[..]).map_err(|e| {
                            CoreError::Protocol(format!(
                                "Failed to decode protobuf for message {}: {}",
                                msg_id, e
                            ))
                        })?
                    } else {
                        return Err(CoreError::Protocol(format!(
                            "Message '{}' in database has empty protobuf body",
                            msg_id
                        )));
                    }
                }
                _ => {
                    return Err(CoreError::Protocol(format!(
                        "Message ID '{}' not found in recent cache or database",
                        msg_id
                    )));
                }
            },
        };

        let attachment = baileys_media::extract_media_from_message(&proto).ok_or_else(|| {
            CoreError::Media("Message does not contain any media attachment".into())
        })?;

        let dest_file_path = if let Some(p) = target_path {
            p.to_path_buf()
        } else {
            let ext = attachment
                .media_type
                .default_extension(attachment.mimetype.as_deref());
            let filename = if let Some(ref name) = attachment.filename {
                let clean = baileys_media::sanitize_filename(name);
                format!("{}_{}", msg_id, clean)
            } else {
                format!("{}.{}", msg_id, ext)
            };
            std::path::PathBuf::from("downloads").join(filename)
        };

        let download_url = attachment
            .download_url()
            .map_err(|e| CoreError::Media(e.to_string()))?;

        let events = self.events.clone();
        let task_id = msg_id.to_string();
        let progress_cb: baileys_media::MediaProgressCallback =
            std::sync::Arc::new(move |bytes_transferred, total_bytes, percentage| {
                events.emit(BaileysEvent::MediaProgress {
                    task_id: task_id.clone(),
                    direction: "download".to_string(),
                    bytes_transferred,
                    total_bytes,
                    percentage,
                });
            });

        match baileys_media::download_media_streaming(
            &download_url,
            &attachment.media_key,
            attachment.media_type,
            &dest_file_path,
            Some(progress_cb.clone()),
        )
        .await
        {
            Ok(p) => Ok(p),
            Err(err) => {
                // Fallback to DEF_MEDIA_HOST if direct_path exists
                if let Some(ref dp) = attachment.direct_path {
                    let fallback_url = if dp.starts_with('/') {
                        format!("https://{}{}", baileys_media::DEF_MEDIA_HOST, dp)
                    } else {
                        format!("https://{}/{}", baileys_media::DEF_MEDIA_HOST, dp)
                    };
                    if fallback_url != download_url {
                        baileys_media::download_media_streaming(
                            &fallback_url,
                            &attachment.media_key,
                            attachment.media_type,
                            &dest_file_path,
                            Some(progress_cb),
                        )
                        .await
                        .map_err(|e| CoreError::Media(e.to_string()))
                    } else {
                        Err(CoreError::Media(err.to_string()))
                    }
                } else {
                    Err(CoreError::Media(err.to_string()))
                }
            }
        }
    }
    /// Fetches or returns cached WhatsApp media CDN connection info (`media_conn`).
    pub async fn refresh_media_conn(
        &self,
        force: bool,
    ) -> Result<baileys_media::MediaConnInfo, CoreError> {
        if !force {
            let lock = self.media_conn.lock().await;
            if let Some(ref conn) = *lock {
                if !conn.is_expired() {
                    return Ok(conn.clone());
                }
            }
        }

        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", S_WHATSAPP_NET)
            .with_attr("xmlns", "w:m")
            .with_attr("type", "set")
            .with_attr("id", &iq_id)
            .with_child(BinaryNode::new("media_conn"));

        let resp = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        let media_conn_node = resp.get_child("media_conn").ok_or_else(|| {
            CoreError::Protocol("Server response missing <media_conn> node".into())
        })?;

        let auth = media_conn_node
            .get_attr("auth")
            .ok_or_else(|| CoreError::Protocol("Missing auth attribute in media_conn".into()))?
            .to_string();

        let ttl = media_conn_node
            .get_attr("ttl")
            .and_then(|t| t.parse::<u64>().ok())
            .unwrap_or(43200);

        let hosts = media_conn_node
            .get_children()
            .iter()
            .filter(|c| c.tag == "host")
            .filter_map(|h| {
                let hostname = h.get_attr("hostname")?.to_string();
                let max_len = h
                    .get_attr("maxContentLengthBytes")
                    .and_then(|l| l.parse::<u64>().ok());
                Some(baileys_media::MediaConnHost {
                    hostname,
                    max_content_length_bytes: max_len,
                })
            })
            .collect();

        let conn_info = baileys_media::MediaConnInfo {
            auth,
            ttl,
            hosts,
            fetched_at: std::time::SystemTime::now(),
        };

        {
            let mut lock = self.media_conn.lock().await;
            *lock = Some(conn_info.clone());
        }

        Ok(conn_info)
    }
    /// Encrypts, uploads, and sends an image message to a user or group.
    pub async fn send_image_message(
        &self,
        to_jid: &str,
        image_bytes: &[u8],
        caption: Option<&str>,
    ) -> Result<String, CoreError> {
        let encrypted =
            baileys_media::encrypt_media_bytes(image_bytes, None, baileys_media::MediaType::Image)
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

        let proto_msg = Message {
            image_message: Some(Box::new(baileys_proto::proto::message::ImageMessage {
                url: upload_res.url,
                mimetype: Some("image/jpeg".to_string()),
                caption: caption.map(|s| s.to_string()),
                file_sha256: Some(encrypted.file_sha256.to_vec()),
                file_length: Some(image_bytes.len() as u64),
                media_key: Some(encrypted.media_key.to_vec()),
                file_enc_sha256: Some(encrypted.file_enc_sha256.to_vec()),
                direct_path: upload_res.direct_path,
                ..Default::default()
            })),
            ..Default::default()
        };

        self.send_message(to_jid, proto_msg).await
    }
    /// Encrypts, uploads, and sends an audio buffer as a native WhatsApp Voice Note (PTT) with waveform.
    pub async fn send_voice_note(
        &self,
        to_jid: &str,
        audio_bytes: &[u8],
        seconds: u32,
        waveform: Option<&[u8]>,
    ) -> Result<String, CoreError> {
        let normalized_jid = if !to_jid.contains('@') {
            format!("{}@s.whatsapp.net", to_jid)
        } else {
            to_jid.to_string()
        };

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

        let proto_msg = crate::proto_builders::build_voice_note_message(
            upload_res.url,
            upload_res.direct_path,
            encrypted.media_key.to_vec(),
            encrypted.file_sha256.to_vec(),
            encrypted.file_enc_sha256.to_vec(),
            audio_bytes.len() as u64,
            seconds,
            waveform,
            Some("audio/ogg; codecs=opus"),
        );

        self.send_message(&normalized_jid, proto_msg).await
    }
    /// Encrypts, uploads, and sends a video message to a user or group.
    pub async fn send_video_message(
        &self,
        to_jid: &str,
        video_bytes: &[u8],
        caption: Option<&str>,
        seconds: Option<u32>,
        gif_playback: bool,
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

        let proto_msg = Message {
            video_message: Some(Box::new(VideoMessage {
                url: upload_res.url,
                mimetype: Some("video/mp4".to_string()),
                caption: caption.map(|s| s.to_string()),
                file_sha256: Some(encrypted.file_sha256.to_vec()),
                file_length: Some(video_bytes.len() as u64),
                seconds,
                gif_playback: Some(gif_playback),
                media_key: Some(encrypted.media_key.to_vec()),
                file_enc_sha256: Some(encrypted.file_enc_sha256.to_vec()),
                direct_path: upload_res.direct_path,
                ..Default::default()
            })),
            ..Default::default()
        };

        self.send_message(to_jid, proto_msg).await
    }
    /// Encrypts, uploads, and sends an audio message or voice note (PTT) to a user or group.
    pub async fn send_audio_message(
        &self,
        to_jid: &str,
        audio_bytes: &[u8],
        ptt: bool,
        seconds: Option<u32>,
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

        let proto_msg = Message {
            audio_message: Some(Box::new(AudioMessage {
                url: upload_res.url,
                mimetype: Some("audio/ogg; codecs=opus".to_string()),
                file_sha256: Some(encrypted.file_sha256.to_vec()),
                file_length: Some(audio_bytes.len() as u64),
                seconds,
                ptt: Some(ptt),
                media_key: Some(encrypted.media_key.to_vec()),
                file_enc_sha256: Some(encrypted.file_enc_sha256.to_vec()),
                direct_path: upload_res.direct_path,
                ..Default::default()
            })),
            ..Default::default()
        };

        self.send_message(to_jid, proto_msg).await
    }
    /// Encrypts, uploads, and sends a document/file message to a user or group.
    pub async fn send_document_message(
        &self,
        to_jid: &str,
        document_bytes: &[u8],
        file_name: &str,
        mimetype: &str,
        title: Option<&str>,
    ) -> Result<String, CoreError> {
        let encrypted = baileys_media::encrypt_media_bytes(
            document_bytes,
            None,
            baileys_media::MediaType::Document,
        )
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let media_conn = self.refresh_media_conn(false).await?;

        let upload_res = baileys_media::upload_encrypted_media(
            &encrypted.encrypted_bytes,
            &encrypted.file_enc_sha256,
            baileys_media::MediaType::Document,
            &media_conn,
        )
        .await
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let proto_msg = Message {
            document_message: Some(Box::new(DocumentMessage {
                url: upload_res.url,
                mimetype: Some(mimetype.to_string()),
                title: title.map(|s| s.to_string()),
                file_sha256: Some(encrypted.file_sha256.to_vec()),
                file_length: Some(document_bytes.len() as u64),
                media_key: Some(encrypted.media_key.to_vec()),
                file_name: Some(file_name.to_string()),
                file_enc_sha256: Some(encrypted.file_enc_sha256.to_vec()),
                direct_path: upload_res.direct_path,
                ..Default::default()
            })),
            ..Default::default()
        };

        self.send_message(to_jid, proto_msg).await
    }
    /// Encrypts, uploads, and sends a sticker message to a user or group.
    pub async fn send_sticker_message(
        &self,
        to_jid: &str,
        sticker_bytes: &[u8],
        is_animated: bool,
    ) -> Result<String, CoreError> {
        let encrypted = baileys_media::encrypt_media_bytes(
            sticker_bytes,
            None,
            baileys_media::MediaType::Sticker,
        )
        .map_err(|e| CoreError::Media(e.to_string()))?;

        // A cached media connection can expire between sends; retry once with a fresh one.
        let media_conn = self.refresh_media_conn(false).await?;
        let upload_res = match baileys_media::upload_encrypted_media(
            &encrypted.encrypted_bytes,
            &encrypted.file_enc_sha256,
            baileys_media::MediaType::Sticker,
            &media_conn,
        )
        .await
        {
            Ok(res) => res,
            Err(first) => {
                tracing::warn!(
                    "sticker upload failed, retrying with a fresh media connection: {}",
                    first
                );
                let fresh_conn = self.refresh_media_conn(true).await?;
                baileys_media::upload_encrypted_media(
                    &encrypted.encrypted_bytes,
                    &encrypted.file_enc_sha256,
                    baileys_media::MediaType::Sticker,
                    &fresh_conn,
                )
                .await
                .map_err(|e| CoreError::Media(e.to_string()))?
            }
        };

        let proto_msg = Message {
            sticker_message: Some(Box::new(sticker_message_fields(
                sticker_bytes,
                upload_res.url,
                upload_res.direct_path,
                encrypted.media_key.to_vec(),
                encrypted.file_sha256.to_vec(),
                encrypted.file_enc_sha256.to_vec(),
                sticker_bytes.len() as u64,
                is_animated,
            ))),
            ..Default::default()
        };

        self.send_message(to_jid, proto_msg).await
    }
    /// Sends a local media file using the streaming upload pipeline (zero-OOM spooling for >= 10MB)
    /// and emits real-time BaileysEvent::MediaProgress events.
    pub async fn send_media_file_streaming(
        &self,
        chat_jid: &str,
        file_path: &std::path::Path,
        media_type: baileys_media::MediaType,
        caption: Option<&str>,
    ) -> Result<String, CoreError> {
        let normalized_jid = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid.trim_start_matches('+'))
        } else {
            chat_jid.to_string()
        };

        let media_conn = self.refresh_media_conn(false).await?;
        let task_id = format!("upload_{:x}", rand::random::<u64>());

        let events = self.events.clone();
        let tid = task_id.clone();
        let progress_cb: baileys_media::MediaProgressCallback =
            std::sync::Arc::new(move |bytes_transferred, total_bytes, percentage| {
                events.emit(BaileysEvent::MediaProgress {
                    task_id: tid.clone(),
                    direction: "upload".to_string(),
                    bytes_transferred,
                    total_bytes,
                    percentage,
                });
            });

        let stream_res = baileys_media::upload_media_file_streaming(
            file_path,
            media_type,
            &media_conn,
            Some(progress_cb),
        )
        .await
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let file_name = file_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".to_string());

        let proto_msg = match media_type {
            baileys_media::MediaType::Image => Message {
                image_message: Some(Box::new(baileys_proto::proto::message::ImageMessage {
                    url: stream_res.upload_res.url,
                    mimetype: Some("image/jpeg".to_string()),
                    caption: caption.map(|s| s.to_string()),
                    file_sha256: Some(stream_res.file_sha256.to_vec()),
                    file_length: Some(stream_res.file_length),
                    media_key: Some(stream_res.media_key.to_vec()),
                    file_enc_sha256: Some(stream_res.file_enc_sha256.to_vec()),
                    direct_path: stream_res.upload_res.direct_path,
                    ..Default::default()
                })),
                ..Default::default()
            },
            baileys_media::MediaType::Video => Message {
                video_message: Some(Box::new(baileys_proto::proto::message::VideoMessage {
                    url: stream_res.upload_res.url,
                    mimetype: Some("video/mp4".to_string()),
                    caption: caption.map(|s| s.to_string()),
                    file_sha256: Some(stream_res.file_sha256.to_vec()),
                    file_length: Some(stream_res.file_length),
                    media_key: Some(stream_res.media_key.to_vec()),
                    file_enc_sha256: Some(stream_res.file_enc_sha256.to_vec()),
                    direct_path: stream_res.upload_res.direct_path,
                    ..Default::default()
                })),
                ..Default::default()
            },
            baileys_media::MediaType::Audio => Message {
                audio_message: Some(Box::new(baileys_proto::proto::message::AudioMessage {
                    url: stream_res.upload_res.url,
                    mimetype: Some("audio/ogg; codecs=opus".to_string()),
                    file_sha256: Some(stream_res.file_sha256.to_vec()),
                    file_length: Some(stream_res.file_length),
                    media_key: Some(stream_res.media_key.to_vec()),
                    file_enc_sha256: Some(stream_res.file_enc_sha256.to_vec()),
                    direct_path: stream_res.upload_res.direct_path,
                    ..Default::default()
                })),
                ..Default::default()
            },
            baileys_media::MediaType::Document
            | baileys_media::MediaType::History
            | baileys_media::MediaType::AppState => Message {
                document_message: Some(Box::new(baileys_proto::proto::message::DocumentMessage {
                    url: stream_res.upload_res.url,
                    mimetype: Some("application/octet-stream".to_string()),
                    title: caption.map(|s| s.to_string()),
                    file_sha256: Some(stream_res.file_sha256.to_vec()),
                    file_length: Some(stream_res.file_length),
                    media_key: Some(stream_res.media_key.to_vec()),
                    file_name: Some(file_name),
                    file_enc_sha256: Some(stream_res.file_enc_sha256.to_vec()),
                    direct_path: stream_res.upload_res.direct_path,
                    ..Default::default()
                })),
                ..Default::default()
            },
            baileys_media::MediaType::Sticker => {
                let header = std::fs::read(file_path).unwrap_or_default();
                let is_animated = header.windows(4).any(|w| w == b"ANIM");
                Message {
                    sticker_message: Some(Box::new(sticker_message_fields(
                        &header,
                        stream_res.upload_res.url,
                        stream_res.upload_res.direct_path,
                        stream_res.media_key.to_vec(),
                        stream_res.file_sha256.to_vec(),
                        stream_res.file_enc_sha256.to_vec(),
                        stream_res.file_length,
                        is_animated,
                    ))),
                    ..Default::default()
                }
            }
        };

        self.send_message(&normalized_jid, proto_msg).await
    }
    /// Sends a document file attachment message.
    pub async fn send_document(
        &self,
        to_jid: &str,
        doc_bytes: &[u8],
        filename: &str,
        mimetype: &str,
        title: Option<&str>,
        page_count: Option<u32>,
        caption: Option<&str>,
    ) -> Result<String, CoreError> {
        let enc =
            baileys_media::encrypt_media_bytes(doc_bytes, None, baileys_media::MediaType::Document)
                .map_err(|e| CoreError::Crypto(e.to_string()))?;

        let msg = crate::rich_messages::build_document_message(
            None,
            None,
            Some(&enc.media_key),
            Some(&enc.file_sha256),
            Some(&enc.file_enc_sha256),
            doc_bytes.len() as u64,
            filename,
            mimetype,
            title,
            page_count,
            caption,
            None,
        );
        self.send_message(to_jid, msg).await
    }
    /// Sends a video message (or GIF animation).
    pub async fn send_video(
        &self,
        to_jid: &str,
        video_bytes: &[u8],
        caption: Option<&str>,
        is_gif: bool,
        seconds: Option<u32>,
    ) -> Result<String, CoreError> {
        let enc =
            baileys_media::encrypt_media_bytes(video_bytes, None, baileys_media::MediaType::Video)
                .map_err(|e| CoreError::Crypto(e.to_string()))?;

        let msg = crate::rich_messages::build_video_message(
            None,
            None,
            Some(&enc.media_key),
            Some(&enc.file_sha256),
            Some(&enc.file_enc_sha256),
            video_bytes.len() as u64,
            caption,
            is_gif,
            seconds,
            None,
        );
        self.send_message(to_jid, msg).await
    }
    /// Sends a sticker message (WebP format).
    pub async fn send_sticker(
        &self,
        to_jid: &str,
        sticker_bytes: &[u8],
        is_animated: bool,
    ) -> Result<String, CoreError> {
        let enc = baileys_media::encrypt_media_bytes(
            sticker_bytes,
            None,
            baileys_media::MediaType::Sticker,
        )
        .map_err(|e| CoreError::Crypto(e.to_string()))?;

        let msg = crate::rich_messages::build_sticker_message(
            None,
            None,
            Some(&enc.media_key),
            Some(&enc.file_sha256),
            Some(&enc.file_enc_sha256),
            sticker_bytes.len() as u64,
            is_animated,
            false,
        );
        self.send_message(to_jid, msg).await
    }
    /// Requests the sender phone to re-upload expired media and update the direct_path.
    /// Matches Baileys `updateMediaMessage(message)`.
    pub async fn update_media_message(&self, message_id: &str) -> Result<(), CoreError> {
        let msg = self
            .store
            .get_message(message_id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?
            .ok_or_else(|| CoreError::Protocol(format!("Message not found: {}", message_id)))?;

        let proto_info = baileys_proto::proto::WebMessageInfo::decode(&msg.proto_bytes[..])
            .map_err(|e| CoreError::Protocol(format!("Failed to decode message proto: {}", e)))?;

        let m = proto_info
            .message
            .ok_or_else(|| CoreError::Protocol("Message has no content".into()))?;

        let media_key = m
            .image_message
            .as_ref()
            .and_then(|img| img.media_key.clone())
            .or_else(|| m.video_message.as_ref().and_then(|v| v.media_key.clone()))
            .or_else(|| m.audio_message.as_ref().and_then(|a| a.media_key.clone()))
            .or_else(|| {
                m.document_message
                    .as_ref()
                    .and_then(|d| d.media_key.clone())
            })
            .or_else(|| m.sticker_message.as_ref().and_then(|s| s.media_key.clone()))
            .ok_or_else(|| CoreError::Protocol("No mediaKey in message".into()))?;

        let (ciphertext, iv) = baileys_media::encrypt_media_retry_request(message_id, &media_key)
            .map_err(|e| CoreError::Crypto(e.to_string()))?;

        let me_jid = self.me_jid().await?.unwrap_or_default();
        let mut receipt = BinaryNode::new("receipt")
            .with_attr("id", message_id)
            .with_attr("to", me_jid)
            .with_attr("type", "server-error");

        let enc_p = BinaryNode::new("enc_p").with_bytes_content(ciphertext);
        let enc_iv = BinaryNode::new("enc_iv").with_bytes_content(iv.to_vec());
        let encrypt_node = BinaryNode::new("encrypt")
            .with_child(enc_p)
            .with_child(enc_iv);

        let mut rmr_node = BinaryNode::new("rmr")
            .with_attr("jid", &msg.chat_jid)
            .with_attr("from_me", if msg.from_me { "true" } else { "false" });

        if !msg.sender_jid.is_empty() && msg.sender_jid != msg.chat_jid {
            rmr_node = rmr_node.with_attr("participant", &msg.sender_jid);
        }

        receipt = receipt.with_child(encrypt_node).with_child(rmr_node);
        let socket = {
            let lock = self.socket.lock().await;
            lock.clone()
                .ok_or_else(|| CoreError::Network("Not connected".into()))?
        };
        socket.send_node(receipt).await
    }
}

/// Sticker fields receivers need to lay out and decrypt the sticker. Width and height come from
/// the WebP header (512 when unreadable); without them some clients render an empty frame.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sticker_message_fields(
    webp_bytes: &[u8],
    url: Option<String>,
    direct_path: Option<String>,
    media_key: Vec<u8>,
    file_sha256: Vec<u8>,
    file_enc_sha256: Vec<u8>,
    file_length: u64,
    is_animated: bool,
) -> StickerMessage {
    let dims = baileys_media::webp_dimensions(webp_bytes)
        .unwrap_or(baileys_media::MediaDimensions::new(512, 512));
    StickerMessage {
        url,
        mimetype: Some("image/webp".to_string()),
        file_sha256: Some(file_sha256),
        file_length: Some(file_length),
        is_animated: Some(is_animated),
        media_key: Some(media_key),
        file_enc_sha256: Some(file_enc_sha256),
        direct_path,
        width: Some(dims.width),
        height: Some(dims.height),
        media_key_timestamp: Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or_default(),
        ),
        ..Default::default()
    }
}

#[cfg(test)]
mod sticker_fields_tests {
    use super::*;

    #[test]
    fn sticker_fields_fall_back_to_512_and_set_timestamp() {
        let msg = sticker_message_fields(
            b"not a webp",
            None,
            Some("/p".into()),
            vec![1],
            vec![2],
            vec![3],
            10,
            false,
        );
        assert_eq!((msg.width, msg.height), (Some(512), Some(512)));
        assert!(msg.media_key_timestamp.unwrap_or_default() > 0);
        assert_eq!(msg.mimetype.as_deref(), Some("image/webp"));
        assert_eq!(msg.direct_path.as_deref(), Some("/p"));
    }
}
