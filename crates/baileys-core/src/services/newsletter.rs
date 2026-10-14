//! Domain service for WhatsApp Channels / Newsletters.

use std::collections::HashMap;
use std::time::Duration;

use crate::client::{BaileysClient, MessageOptions};
use crate::newsletter::{NewsletterMessage, NewsletterMetadata};
use crate::CoreError;
use baileys_store::StoredNewsletter;

/// Domain service providing all WhatsApp Channel (Newsletter) operations.
#[derive(Clone)]
pub struct NewsletterService {
    client: BaileysClient,
}

impl NewsletterService {
    pub fn new(client: BaileysClient) -> Self {
        Self { client }
    }

    /// Queries metadata for a WhatsApp Channel by JID or invite code/link.
    pub async fn query_metadata(
        &self,
        jid_or_invite: &str,
    ) -> Result<NewsletterMetadata, CoreError> {
        let clean = jid_or_invite.trim();
        let (input_key, key_type, fetch_viewer) = if clean.contains("@newsletter") {
            (clean.to_string(), "JID", true)
        } else {
            let code = clean
                .trim_start_matches("https://whatsapp.com/channel/")
                .trim_start_matches("http://whatsapp.com/channel/")
                .trim_start_matches("whatsapp.com/channel/");
            (code.to_string(), "INVITE", false)
        };

        let vars = serde_json::json!({
            "input": {
                "key": input_key,
                "type": key_type
            },
            "fetch_viewer_metadata": fetch_viewer,
            "fetch_full_image": true,
            "fetch_creation_time": true
        });

        let iq = crate::newsletter::build_mex_iq(crate::newsletter::QUERY_FETCH_NEWSLETTER, vars);
        let res = self.client.send_iq(iq, Duration::from_secs(12)).await?;
        let data = crate::newsletter::parse_mex_response(&res)?;

        let nl_obj = data.get("xwa2_newsletter").ok_or_else(|| {
            CoreError::Protocol("Missing xwa2_newsletter in MEX response".to_string())
        })?;

        let meta = crate::newsletter::parse_single_newsletter(nl_obj).ok_or_else(|| {
            CoreError::Protocol("Failed to parse newsletter metadata".to_string())
        })?;

        // Cache in local SQLite store
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let stored = StoredNewsletter {
            jid: meta.jid.clone(),
            name: meta.name.clone(),
            description: meta.description.clone(),
            invite_code: meta.invite_code.clone(),
            picture_url: meta.picture_url.clone(),
            role: meta.role.clone(),
            muted: meta.muted,
            subscribers_count: meta.subscribers_count,
            updated_at: now,
        };
        let _ = self.client.store.save_newsletter(&stored).await;

        Ok(meta)
    }

    /// Retrieves all newsletters/channels the user is currently subscribed to.
    pub async fn get_subscribed(&self) -> Result<Vec<NewsletterMetadata>, CoreError> {
        let vars = serde_json::json!({});
        let iq =
            crate::newsletter::build_mex_iq(crate::newsletter::QUERY_SUBSCRIBED_NEWSLETTERS, vars);
        let res = self.client.send_iq(iq, Duration::from_secs(12)).await?;
        let data = crate::newsletter::parse_mex_response(&res)?;

        let list = crate::newsletter::parse_subscribed_newsletters(&data);

        // Cache in local SQLite store
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        for meta in &list {
            let stored = StoredNewsletter {
                jid: meta.jid.clone(),
                name: meta.name.clone(),
                description: meta.description.clone(),
                invite_code: meta.invite_code.clone(),
                picture_url: meta.picture_url.clone(),
                role: meta.role.clone(),
                muted: meta.muted,
                subscribers_count: meta.subscribers_count,
                updated_at: now,
            };
            let _ = self.client.store.save_newsletter(&stored).await;
        }

        Ok(list)
    }

    /// Follows a WhatsApp Channel.
    pub async fn follow(&self, jid: &str) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };

        let vars = serde_json::json!({
            "newsletter_id": normalized
        });
        let iq =
            crate::newsletter::build_mex_iq(crate::newsletter::MUTATION_FOLLOW_NEWSLETTER, vars);
        let res = self.client.send_iq(iq, Duration::from_secs(10)).await?;
        crate::newsletter::parse_mex_response(&res)?;

        // Update local store role
        if let Ok(Some(mut stored)) = self.client.store.get_newsletter(&normalized).await {
            stored.role = Some("SUBSCRIBER".to_string());
            let _ = self.client.store.save_newsletter(&stored).await;
        }

        Ok(())
    }

    /// Unfollows a WhatsApp Channel.
    pub async fn unfollow(&self, jid: &str) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };

        let vars = serde_json::json!({
            "newsletter_id": normalized
        });
        let iq =
            crate::newsletter::build_mex_iq(crate::newsletter::MUTATION_UNFOLLOW_NEWSLETTER, vars);
        let res = self.client.send_iq(iq, Duration::from_secs(10)).await?;
        crate::newsletter::parse_mex_response(&res)?;

        // Update local store role
        if let Ok(Some(mut stored)) = self.client.store.get_newsletter(&normalized).await {
            stored.role = Some("GUEST".to_string());
            let _ = self.client.store.save_newsletter(&stored).await;
        }

        Ok(())
    }

    /// Toggles mute status for a WhatsApp Channel.
    pub async fn mute(&self, jid: &str, mute: bool) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };

        let query_id = if mute {
            crate::newsletter::MUTATION_MUTE_NEWSLETTER
        } else {
            crate::newsletter::MUTATION_UNMUTE_NEWSLETTER
        };

        let vars = serde_json::json!({
            "newsletter_id": normalized
        });
        let iq = crate::newsletter::build_mex_iq(query_id, vars);
        let res = self.client.send_iq(iq, Duration::from_secs(10)).await?;
        crate::newsletter::parse_mex_response(&res)?;

        // Update local store
        if let Ok(Some(mut stored)) = self.client.store.get_newsletter(&normalized).await {
            stored.muted = mute;
            let _ = self.client.store.save_newsletter(&stored).await;
        }

        Ok(())
    }

    /// Creates a new WhatsApp Channel.
    pub async fn create(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> Result<NewsletterMetadata, CoreError> {
        let mut input_obj = serde_json::json!({
            "name": name
        });
        if let Some(desc) = description {
            input_obj["description"] = serde_json::Value::String(desc.to_string());
        }

        let vars = serde_json::json!({
            "newsletter_input": input_obj
        });

        let iq =
            crate::newsletter::build_mex_iq(crate::newsletter::MUTATION_CREATE_NEWSLETTER, vars);
        let res = self.client.send_iq(iq, Duration::from_secs(15)).await?;
        let data = crate::newsletter::parse_mex_response(&res)?;

        let nl_obj = data.get("xwa2_newsletter_create").ok_or_else(|| {
            CoreError::Protocol("Missing xwa2_newsletter_create in MEX response".to_string())
        })?;

        let meta = crate::newsletter::parse_single_newsletter(nl_obj).ok_or_else(|| {
            CoreError::Protocol("Failed to parse created newsletter metadata".to_string())
        })?;

        // Cache in local SQLite store
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let stored = StoredNewsletter {
            jid: meta.jid.clone(),
            name: meta.name.clone(),
            description: meta.description.clone(),
            invite_code: meta.invite_code.clone(),
            picture_url: meta.picture_url.clone(),
            role: Some("OWNER".to_string()),
            muted: false,
            subscribers_count: 1,
            updated_at: now,
        };
        let _ = self.client.store.save_newsletter(&stored).await;

        Ok(meta)
    }

    /// Fetches message history for a WhatsApp Channel.
    pub async fn fetch_messages(
        &self,
        jid: &str,
        count: u32,
        before: Option<u64>,
    ) -> Result<Vec<NewsletterMessage>, CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };

        let iq = crate::newsletter::build_newsletter_messages_iq(&normalized, count, before);
        let res = self.client.send_iq(iq, Duration::from_secs(15)).await?;

        let mut messages = Vec::new();
        if let Some(msgs_parent) = res.get_child("messages") {
            for child in msgs_parent.get_children() {
                if child.tag == "message" {
                    let server_id = child
                        .get_attr("server_id")
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(0);
                    let ts = child
                        .get_attr("t")
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(0);

                    let views_count = child
                        .get_child("views")
                        .and_then(|v| v.get_attr("count"))
                        .and_then(|c| c.parse::<u64>().ok());

                    let mut reactions = HashMap::new();
                    if let Some(reacts_node) = child.get_child("reactions") {
                        for r in reacts_node.get_children() {
                            if r.tag == "reaction" {
                                if let (Some(code), Some(cnt_str)) =
                                    (r.get_attr("code"), r.get_attr("count"))
                                {
                                    if let Ok(cnt) = cnt_str.parse::<u32>() {
                                        reactions.insert(code.to_string(), cnt);
                                    }
                                }
                            }
                        }
                    }

                    let raw_bytes = child
                        .get_child("plaintext")
                        .and_then(|p| p.content_as_bytes())
                        .map(|b| b.to_vec())
                        .unwrap_or_default();

                    let (summary, has_media) = if !raw_bytes.is_empty() {
                        if let Ok(m) = <baileys_proto::proto::Message as prost::Message>::decode(
                            &raw_bytes[..],
                        ) {
                            let s = crate::messages::extract_message_summary(&m);
                            let h_m = crate::messages::message_has_media(&m);
                            (s, h_m)
                        } else {
                            ("<channel post>".to_string(), false)
                        }
                    } else {
                        ("<channel post>".to_string(), false)
                    };

                    messages.push(NewsletterMessage {
                        server_id,
                        views_count,
                        reactions,
                        timestamp: ts,
                        summary,
                        has_media,
                        raw_proto: raw_bytes,
                    });
                }
            }
        }

        Ok(messages)
    }

    /// Sends an emoji reaction to a message in a WhatsApp Channel.
    pub async fn react(&self, jid: &str, server_id: u64, reaction: &str) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };

        let node = crate::newsletter::build_newsletter_reaction_node(
            &normalized,
            server_id,
            reaction,
            None,
        );
        let socket_opt = self.client.socket.lock().await;
        if let Some(ref socket) = *socket_opt {
            socket.send_node(node).await
        } else {
            Err(CoreError::Network("Socket is disconnected".to_string()))
        }
    }

    /// Sends a view receipt for channel message server IDs.
    pub async fn view_receipt(&self, jid: &str, server_ids: &[u64]) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };

        let node = crate::newsletter::build_newsletter_view_receipt(&normalized, server_ids);
        let socket_opt = self.client.socket.lock().await;
        if let Some(ref socket) = *socket_opt {
            socket.send_node(node).await
        } else {
            Err(CoreError::Network("Socket is disconnected".to_string()))
        }
    }

    /// Sends a plaintext broadcast post to an owned WhatsApp Channel.
    pub async fn send_post(&self, jid: &str, text: &str) -> Result<String, CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };

        let proto_msg = baileys_proto::proto::Message {
            extended_text_message: Some(Box::new(
                baileys_proto::proto::message::ExtendedTextMessage {
                    text: Some(text.to_string()),
                    ..Default::default()
                },
            )),
            ..Default::default()
        };

        self.client.send_message(&normalized, proto_msg).await
    }

    /// Sends an image broadcast post to an owned WhatsApp Channel using unencrypted media CDN flow.
    pub async fn send_image(
        &self,
        jid: &str,
        image_bytes: &[u8],
        caption: Option<&str>,
    ) -> Result<String, CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };

        let file_sha256 = baileys_crypto::sha256(image_bytes);

        let media_conn = self.client.refresh_media_conn(false).await?;
        let upload_res = baileys_media::upload_newsletter_media(
            image_bytes,
            baileys_media::MediaType::Image,
            &media_conn,
        )
        .await
        .map_err(|e| CoreError::Media(e.to_string()))?;

        let proto_msg = baileys_proto::proto::Message {
            image_message: Some(Box::new(baileys_proto::proto::message::ImageMessage {
                url: upload_res.url,
                mimetype: Some("image/jpeg".to_string()),
                caption: caption.map(|s| s.to_string()),
                file_sha256: Some(file_sha256.to_vec()),
                file_length: Some(image_bytes.len() as u64),
                direct_path: upload_res.direct_path,
                ..Default::default()
            })),
            ..Default::default()
        };

        self.client
            .send_message_with_options(
                &normalized,
                proto_msg,
                MessageOptions {
                    media_handle: upload_res.handle,
                    ..Default::default()
                },
            )
            .await
    }

    /// Sends a status story to `status@broadcast` forwarding/referencing a WhatsApp Channel post.
    pub async fn send_status_story(
        &self,
        newsletter_jid: &str,
        server_msg_id: i32,
        newsletter_name: &str,
        caption_or_text: &str,
        status_recipients: Option<Vec<String>>,
    ) -> Result<String, CoreError> {
        self.client
            .newsletter_send_status_story(
                newsletter_jid,
                server_msg_id,
                newsletter_name,
                caption_or_text,
                status_recipients,
            )
            .await
    }

    /// Publishes an interactive HD Image Story to `status@broadcast` that references/forwards a WhatsApp Channel post.
    pub async fn send_image_status_story(
        &self,
        image_bytes: &[u8],
        caption: Option<&str>,
        newsletter_jid: &str,
        server_msg_id: i32,
        newsletter_name: &str,
        status_recipients: Option<Vec<String>>,
    ) -> Result<String, CoreError> {
        self.client
            .newsletter_send_image_status_story(
                image_bytes,
                caption,
                newsletter_jid,
                server_msg_id,
                newsletter_name,
                status_recipients,
            )
            .await
    }

    /// Deletes a WhatsApp Channel.
    pub async fn delete(&self, jid: &str) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };

        let vars = serde_json::json!({
            "newsletter_id": normalized
        });
        let iq =
            crate::newsletter::build_mex_iq(crate::newsletter::MUTATION_DELETE_NEWSLETTER, vars);
        let res = self.client.send_iq(iq, Duration::from_secs(12)).await?;
        crate::newsletter::parse_mex_response(&res)?;

        let _ = self.client.store.delete_chat(&normalized).await;
        Ok(())
    }

    /// Updates a WhatsApp Channel name.
    pub async fn update_name(&self, jid: &str, name: &str) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };

        let vars = serde_json::json!({
            "newsletter_id": normalized,
            "updates": {
                "name": name
            }
        });
        let iq =
            crate::newsletter::build_mex_iq(crate::newsletter::MUTATION_UPDATE_NEWSLETTER, vars);
        let res = self.client.send_iq(iq, Duration::from_secs(12)).await?;
        crate::newsletter::parse_mex_response(&res)?;

        if let Ok(Some(mut stored)) = self.client.store.get_newsletter(&normalized).await {
            stored.name = name.to_string();
            let _ = self.client.store.save_newsletter(&stored).await;
        }

        Ok(())
    }

    /// Demotes an admin in a WhatsApp Channel.
    pub async fn demote(&self, jid: &str, user_jid: &str) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };
        let user_norm = if !user_jid.contains('@') {
            format!("{}@s.whatsapp.net", user_jid)
        } else {
            user_jid.to_string()
        };

        let vars = serde_json::json!({
            "newsletter_id": normalized,
            "user_id": user_norm
        });
        let iq = crate::newsletter::build_mex_iq(
            crate::newsletter::MUTATION_DEMOTE_ADMIN_NEWSLETTER,
            vars,
        );
        let res = self.client.send_iq(iq, Duration::from_secs(12)).await?;
        crate::newsletter::parse_mex_response(&res)?;
        Ok(())
    }

    /// Transfers ownership of a WhatsApp Channel to another user.
    pub async fn change_owner(&self, jid: &str, new_owner_jid: &str) -> Result<(), CoreError> {
        let normalized = if !jid.contains('@') {
            format!("{}@newsletter", jid)
        } else {
            jid.to_string()
        };
        let user_norm = if !new_owner_jid.contains('@') {
            format!("{}@s.whatsapp.net", new_owner_jid)
        } else {
            new_owner_jid.to_string()
        };

        let vars = serde_json::json!({
            "newsletter_id": normalized,
            "new_owner_id": user_norm
        });
        let iq = crate::newsletter::build_mex_iq(
            crate::newsletter::MUTATION_CHANGE_OWNER_NEWSLETTER,
            vars,
        );
        let res = self.client.send_iq(iq, Duration::from_secs(12)).await?;
        crate::newsletter::parse_mex_response(&res)?;

        if let Ok(Some(mut stored)) = self.client.store.get_newsletter(&normalized).await {
            stored.role = Some("ADMIN".to_string());
            let _ = self.client.store.save_newsletter(&stored).await;
        }

        Ok(())
    }

    /// Fetches the live administrator count for a WhatsApp Channel.
    pub async fn admin_count(&self, newsletter_jid: &str) -> Result<u32, CoreError> {
        let vars = serde_json::json!({
            "input": { "key": newsletter_jid }
        });
        let iq = crate::newsletter::build_mex_iq(crate::newsletter::QUERY_ADMIN_COUNT, vars);
        let res = self.client.send_iq(iq, Duration::from_secs(12)).await?;
        let data = crate::newsletter::parse_mex_response(&res)?;

        crate::newsletter::parse_newsletter_admin_count(&data)
            .ok_or_else(|| CoreError::Protocol("Admin count not found in response".into()))
    }

    /// Subscribes to live updates (edits, reactions) for a newsletter.
    pub async fn subscribe_live_updates(
        &self,
        newsletter_jid: &str,
    ) -> Result<Option<String>, CoreError> {
        let iq = crate::newsletter::build_newsletter_live_updates_iq(newsletter_jid);
        let res = self.client.send_iq(iq, Duration::from_secs(15)).await?;
        Ok(crate::newsletter::parse_newsletter_live_updates_response(
            &res,
        ))
    }
}
