//! Domain submodule for groups operations on BaileysClient.

use super::*;

impl BaileysClient {
    /// Queries group metadata (participants, subject, owner, creation time) from WhatsApp server.
    pub async fn group_metadata(&self, group_jid: &str) -> Result<BinaryNode, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", group_jid)
            .with_attr("xmlns", "w:g2")
            .with_attr("type", "get")
            .with_attr("id", &iq_id)
            .with_child(BinaryNode::new("query").with_attr("request", "interactive"));

        self.send_iq(iq_node, Duration::from_secs(10)).await
    }
    /// Creates a new WhatsApp group with the given subject and initial participants.
    /// Returns the created group's JID (e.g. "120363xxx@g.us").
    pub async fn group_create(
        &self,
        subject: &str,
        participants: &[String],
    ) -> Result<String, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let key = format!("3EB0{:X}", rand::random::<u64>());
        let mut create_node = BinaryNode::new("create")
            .with_attr("subject", subject)
            .with_attr("key", &key);

        for p in participants {
            let p_norm = baileys_binary::jid::jid_normalized_user(p);
            create_node =
                create_node.with_child(BinaryNode::new("participant").with_attr("jid", &p_norm));
        }

        let iq_node = BinaryNode::new("iq")
            .with_attr("to", "@g.us")
            .with_attr("xmlns", "w:g2")
            .with_attr("type", "set")
            .with_attr("id", &iq_id)
            .with_child(create_node);

        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        let group_node = res.get_child("group").unwrap_or(&res);
        if let Some(id) = group_node.get_attr("id") {
            let jid = if id.contains('@') {
                id.to_string()
            } else {
                format!("{}@g.us", id)
            };
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let stored_chat = baileys_store::StoredChat {
                jid: jid.clone(),
                name: Some(subject.to_string()),
                unread_count: 0,
                last_message_timestamp: now,
                last_message_summary: None,
                pinned: false,
                archived: false,
                mute_until: None,
                ephemeral_expiration: 0,
                ephemeral_setting_timestamp: None,
            };
            let _ = self.store.save_chat(&stored_chat).await;
            return Ok(jid);
        }
        Err(CoreError::Protocol(
            "Group creation failed: no group id returned".into(),
        ))
    }
    /// Updates participants in a group ("add", "remove", "promote", "demote").
    /// Returns a list of (participant_jid, status_code).
    pub async fn group_participants_update(
        &self,
        group_jid: &str,
        participants: &[String],
        action: &str,
    ) -> Result<Vec<(String, String)>, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let mut action_node = BinaryNode::new(action);

        for p in participants {
            let p_jid = if !p.contains('@') {
                format!("{}@s.whatsapp.net", p)
            } else {
                p.clone()
            };
            action_node =
                action_node.with_child(BinaryNode::new("participant").with_attr("jid", &p_jid));
        }

        let iq_node = BinaryNode::new("iq")
            .with_attr("to", group_jid)
            .with_attr("xmlns", "w:g2")
            .with_attr("type", "set")
            .with_attr("id", &iq_id)
            .with_child(action_node);

        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        let mut results = Vec::new();
        if let Some(act_res) = res.get_child(action) {
            for child in act_res.get_children() {
                if child.tag == "participant" {
                    let jid = child.get_attr("jid").unwrap_or("").to_string();
                    let code = child
                        .get_attr("error")
                        .or_else(|| child.get_attr("code"))
                        .unwrap_or("200")
                        .to_string();
                    results.push((jid, code));
                }
            }
        }
        Ok(results)
    }
    /// Changes the subject/title of a group.
    pub async fn group_update_subject(
        &self,
        group_jid: &str,
        subject: &str,
    ) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", group_jid)
            .with_attr("xmlns", "w:g2")
            .with_attr("type", "set")
            .with_attr("id", &iq_id)
            .with_child(BinaryNode::new("subject").with_content(subject.as_bytes().to_vec()));

        let res = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        if res.get_attr("type") == Some("result") {
            Ok(())
        } else {
            Err(CoreError::Protocol(format!(
                "Failed to update group subject: {:?}",
                res
            )))
        }
    }
    /// Changes the description of a group.
    pub async fn group_update_description(
        &self,
        group_jid: &str,
        description: &str,
    ) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let desc_id = format!("3EB0{:X}", rand::random::<u64>());

        let desc_node = BinaryNode::new("description")
            .with_attr("id", &desc_id)
            .with_child(BinaryNode::new("body").with_content(description.as_bytes().to_vec()));

        let iq_node = BinaryNode::new("iq")
            .with_attr("to", group_jid)
            .with_attr("xmlns", "w:g2")
            .with_attr("type", "set")
            .with_attr("id", &iq_id)
            .with_child(desc_node);

        let res = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        if res.get_attr("type") == Some("result") {
            Ok(())
        } else {
            Err(CoreError::Protocol(format!(
                "Failed to update group description: {:?}",
                res
            )))
        }
    }
    /// Fetches the group invite code (used in https://chat.whatsapp.com/<code>).
    pub async fn group_get_invite_code(&self, group_jid: &str) -> Result<String, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", group_jid)
            .with_attr("xmlns", "w:g2")
            .with_attr("type", "get")
            .with_attr("id", &iq_id)
            .with_child(BinaryNode::new("invite"));

        let res = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        if let Some(invite) = res.get_child("invite") {
            if let Some(code) = invite.get_attr("code") {
                return Ok(code.to_string());
            }
        }
        Err(CoreError::Protocol(
            "Failed to retrieve group invite code".into(),
        ))
    }
    /// Accepts a group invite using an invite code, joining the group.
    /// Returns the joined group's JID.
    pub async fn group_accept_invite(&self, code: &str) -> Result<String, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", "@g.us")
            .with_attr("xmlns", "w:g2")
            .with_attr("type", "set")
            .with_attr("id", &iq_id)
            .with_child(BinaryNode::new("invite").with_attr("code", code));

        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        if let Some(group) = res.get_child("group") {
            if let Some(jid) = group.get_attr("jid") {
                return Ok(jid.to_string());
            }
        }
        Err(CoreError::Protocol(
            "Failed to accept group invite code".into(),
        ))
    }
    /// Leaves a WhatsApp group.
    pub async fn group_leave(&self, group_jid: &str) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let leave_node = BinaryNode::new("leave")
            .with_child(BinaryNode::new("group").with_attr("id", group_jid));
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", "@g.us")
            .with_attr("xmlns", "w:g2")
            .with_attr("type", "set")
            .with_attr("id", &iq_id)
            .with_child(leave_node);

        self.send_iq(iq_node, Duration::from_secs(10)).await?;
        Ok(())
    }
    /// Updates group settings ("announcement", "not_announcement", "locked", "unlocked").
    pub async fn group_setting_update(
        &self,
        group_jid: &str,
        setting: &str,
    ) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", group_jid)
            .with_attr("xmlns", "w:g2")
            .with_attr("type", "set")
            .with_attr("id", &iq_id)
            .with_child(BinaryNode::new(setting));

        self.send_iq(iq_node, Duration::from_secs(10)).await?;
        Ok(())
    }
    /// Revokes the current invite code of a group and generates a new one.
    pub async fn group_revoke_invite(&self, group_jid: &str) -> Result<String, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", group_jid)
            .with_attr("xmlns", "w:g2")
            .with_attr("type", "set")
            .with_attr("id", &iq_id)
            .with_child(BinaryNode::new("invite"));

        let res = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        if let Some(invite) = res.get_child("invite") {
            if let Some(code) = invite.get_attr("code") {
                return Ok(code.to_string());
            }
        }
        Err(CoreError::Protocol(
            "Failed to revoke group invite code".into(),
        ))
    }
    /// Queries metadata for a group invite code (before joining).
    pub async fn group_get_invite_info(&self, code: &str) -> Result<BinaryNode, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", "@g.us")
            .with_attr("xmlns", "w:g2")
            .with_attr("type", "get")
            .with_attr("id", &iq_id)
            .with_child(BinaryNode::new("invite").with_attr("code", code));

        self.send_iq(iq_node, Duration::from_secs(10)).await
    }
    /// Fetches parsed typed metadata and participant list for a group.
    pub async fn group_fetch_metadata(
        &self,
        group_jid: &str,
    ) -> Result<crate::groups::GroupMetadata, CoreError> {
        let node = self.group_metadata(group_jid).await?;
        let meta = crate::groups::parse_group_metadata(&node)?;
        let chat = baileys_store::StoredChat {
            jid: meta.id.clone(),
            name: Some(meta.subject.clone()),
            unread_count: 0,
            last_message_timestamp: meta.creation as u64,
            last_message_summary: None,
            pinned: false,
            archived: false,
            mute_until: None,
            ephemeral_expiration: 0,
            ephemeral_setting_timestamp: None,
        };
        let _ = self.store.save_chat(&chat).await;
        Ok(meta)
    }
    /// Fetches all participating groups the authenticated account belongs to.
    /// Also caches their basic conversation records into SQLite.
    pub async fn group_fetch_all_participating(
        &self,
    ) -> Result<Vec<crate::groups::GroupMetadata>, CoreError> {
        let iq_node = crate::groups::build_fetch_participating_groups_iq();
        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        let groups = crate::groups::parse_participating_groups(&res)?;

        // Cache into local database
        for g in &groups {
            let chat = baileys_store::StoredChat {
                jid: g.id.clone(),
                name: Some(g.subject.clone()),
                unread_count: 0,
                last_message_timestamp: g.creation as u64,
                last_message_summary: None,
                pinned: false,
                archived: false,
                mute_until: None,
                ephemeral_expiration: g.ephemeral_duration.unwrap_or(0),
                ephemeral_setting_timestamp: None,
            };
            let _ = self.store.save_chat(&chat).await;
        }

        Ok(groups)
    }
    /// Configures membership approval mode for a group ("on" or "off").
    pub async fn group_join_approval_mode(
        &self,
        group_jid: &str,
        mode: &str,
    ) -> Result<(), CoreError> {
        let iq_node = crate::groups::build_group_join_approval_mode_iq(group_jid, mode);
        self.send_iq(iq_node, Duration::from_secs(10)).await?;
        Ok(())
    }
    /// Queries all pending membership approval requests for a group.
    pub async fn group_request_participants_list(
        &self,
        group_jid: &str,
    ) -> Result<Vec<crate::groups::GroupJoinRequest>, CoreError> {
        let iq_node = crate::groups::build_group_request_participants_list_iq(group_jid);
        let res = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        crate::groups::parse_group_join_requests(&res)
    }
    /// Approves or rejects pending membership approval requests for a group.
    pub async fn group_request_participants_update(
        &self,
        group_jid: &str,
        participants: &[String],
        action: &str,
    ) -> Result<Vec<(String, String)>, CoreError> {
        let iq_node = crate::groups::build_group_request_participants_update_iq(
            group_jid,
            participants,
            action,
        );
        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        let mut results = Vec::new();
        if let Some(act_node) = res
            .get_child("membership_requests_action")
            .and_then(|m| m.get_child(action))
        {
            for child in act_node.get_children() {
                if child.tag == "participant" {
                    let jid = child.get_attr("jid").unwrap_or("").to_string();
                    let status = child
                        .get_attr("error")
                        .or_else(|| child.get_attr("code"))
                        .unwrap_or("200")
                        .to_string();
                    results.push((jid, status));
                }
            }
        }
        Ok(results)
    }
    /// Configures member add mode for a group ("admin_add" or "all_member_add").
    pub async fn group_member_add_mode(
        &self,
        group_jid: &str,
        mode: &str,
    ) -> Result<(), CoreError> {
        let iq_node = crate::groups::build_group_member_add_mode_iq(group_jid, mode);
        self.send_iq(iq_node, Duration::from_secs(10)).await?;
        Ok(())
    }
    /// Revokes a participant's v4 group invite.
    pub async fn group_revoke_invite_v4(
        &self,
        group_jid: &str,
        invited_jid: &str,
    ) -> Result<(), CoreError> {
        let iq_node = crate::groups::build_group_revoke_invite_v4_iq(group_jid, invited_jid);
        self.send_iq(iq_node, Duration::from_secs(10)).await?;
        Ok(())
    }
    /// Accepts a v4 group invite directly via IQ stanza. Returns the joined group JID.
    pub async fn group_accept_invite_v4(
        &self,
        group_jid: &str,
        invite_code: &str,
        invite_expiration: i64,
        admin_jid: &str,
    ) -> Result<String, CoreError> {
        let iq_node = crate::groups::build_group_accept_invite_v4_iq(
            group_jid,
            invite_code,
            invite_expiration,
            admin_jid,
        );
        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        let joined_jid = res.get_attr("from").unwrap_or(group_jid).to_string();
        Ok(joined_jid)
    }
}
