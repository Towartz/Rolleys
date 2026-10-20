//! Domain submodule for community operations on BaileysClient.

use super::*;

impl BaileysClient {
    /// Creates a new WhatsApp Community.
    pub async fn community_create(
        &self,
        subject: &str,
        description: Option<&str>,
    ) -> Result<CommunityMetadata, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let create_node = crate::community::build_create_community_node(subject, description);
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", "@g.us")
            .with_attr("type", "set")
            .with_attr("xmlns", "w:g2")
            .with_attr("id", &iq_id)
            .with_child(create_node);

        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        crate::community::parse_community_metadata(&res)
    }
    /// Creates a new subgroup linked under an existing parent Community.
    pub async fn community_create_subgroup(
        &self,
        parent_community_jid: &str,
        subject: &str,
        participants: &[String],
    ) -> Result<String, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let create_node = crate::community::build_create_community_subgroup_node(
            subject,
            participants,
            parent_community_jid,
        );
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", "@g.us")
            .with_attr("type", "set")
            .with_attr("xmlns", "w:g2")
            .with_attr("id", &iq_id)
            .with_child(create_node);

        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        if let Some(group) = res.get_child("group") {
            let gid = group.get_attr("id").unwrap_or("");
            let group_jid = if gid.contains('@') {
                gid.to_string()
            } else {
                format!("{}@g.us", gid)
            };
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let stored_sub = StoredCommunitySubgroup {
                group_jid: group_jid.clone(),
                subject: subject.to_string(),
                creation: Some(now),
                creator: self.me_jid().await.ok().flatten(),
                size: Some((participants.len() + 1) as u32),
                linked_at: now,
            };
            let _ = self
                .store
                .save_community_subgroup(parent_community_jid, &stored_sub)
                .await;
            Ok(group_jid)
        } else {
            Err(CoreError::Protocol(
                "Missing <group> in create subgroup response".to_string(),
            ))
        }
    }
    /// Links an existing WhatsApp group into a parent Community.
    pub async fn community_link_subgroup(
        &self,
        parent_community_jid: &str,
        group_jid: &str,
    ) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let link_node = crate::community::build_link_subgroup_node(group_jid);
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", parent_community_jid)
            .with_attr("type", "set")
            .with_attr("xmlns", "w:g2")
            .with_attr("id", &iq_id)
            .with_child(link_node);

        self.send_iq(iq_node, Duration::from_secs(15)).await?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let stored_sub = StoredCommunitySubgroup {
            group_jid: group_jid.to_string(),
            subject: String::new(),
            creation: None,
            creator: None,
            size: None,
            linked_at: now,
        };
        let _ = self
            .store
            .save_community_subgroup(parent_community_jid, &stored_sub)
            .await;
        Ok(())
    }
    /// Unlinks a subgroup from a parent Community.
    pub async fn community_unlink_subgroup(
        &self,
        parent_community_jid: &str,
        group_jid: &str,
    ) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let unlink_node = crate::community::build_unlink_subgroup_node(group_jid);
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", parent_community_jid)
            .with_attr("type", "set")
            .with_attr("xmlns", "w:g2")
            .with_attr("id", &iq_id)
            .with_child(unlink_node);

        self.send_iq(iq_node, Duration::from_secs(15)).await?;
        let _ = self
            .store
            .remove_community_subgroup(parent_community_jid, group_jid)
            .await;
        Ok(())
    }
    /// Fetches all linked subgroups under a Community and caches them locally in SQLite.
    pub async fn community_fetch_subgroups(
        &self,
        community_jid: &str,
    ) -> Result<Vec<CommunitySubgroup>, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let query_node = crate::community::build_fetch_subgroups_node();
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", community_jid)
            .with_attr("type", "get")
            .with_attr("xmlns", "w:g2")
            .with_attr("id", &iq_id)
            .with_child(query_node);

        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        let subgroups = crate::community::parse_linked_subgroups(&res)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        for sub in &subgroups {
            let stored = StoredCommunitySubgroup {
                group_jid: sub.id.clone(),
                subject: sub.subject.clone(),
                creation: sub.creation,
                creator: sub.creator.clone(),
                size: sub.size,
                linked_at: now,
            };
            let _ = self
                .store
                .save_community_subgroup(community_jid, &stored)
                .await;
        }
        Ok(subgroups)
    }
    /// Queries interactive metadata for a Community.
    pub async fn community_get_metadata(
        &self,
        community_jid: &str,
    ) -> Result<CommunityMetadata, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let query_node = crate::community::build_community_metadata_query_node();
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", community_jid)
            .with_attr("type", "get")
            .with_attr("xmlns", "w:g2")
            .with_attr("id", &iq_id)
            .with_child(query_node);

        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        crate::community::parse_community_metadata(&res)
    }
    /// Leaves an existing WhatsApp Community.
    pub async fn community_leave(&self, community_jid: &str) -> Result<(), CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let clean_id = community_jid.trim_end_matches("@g.us");
        let leave_node = crate::community::build_community_leave_node(clean_id);
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", "@g.us")
            .with_attr("type", "set")
            .with_attr("xmlns", "w:g2")
            .with_attr("id", &iq_id)
            .with_child(leave_node);

        self.send_iq(iq_node, Duration::from_secs(15)).await?;
        Ok(())
    }
    /// Gets an invite code for a Community.
    pub async fn community_get_invite_code(
        &self,
        community_jid: &str,
    ) -> Result<String, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let invite_node = crate::community::build_community_invite_query_node();
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", community_jid)
            .with_attr("type", "get")
            .with_attr("xmlns", "w:g2")
            .with_attr("id", &iq_id)
            .with_child(invite_node);

        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        if let Some(invite) = res.get_child("invite") {
            if let Some(code) = invite.get_attr("code") {
                return Ok(code.to_string());
            }
        }
        Err(CoreError::Protocol(
            "Missing invite code in response".to_string(),
        ))
    }
    /// Accepts an invite code to join a WhatsApp Community.
    pub async fn community_accept_invite(&self, code: &str) -> Result<String, CoreError> {
        let iq_id = format!("3EB0{:X}", rand::random::<u64>());
        let invite_node = crate::community::build_community_accept_invite_node(code);
        let iq_node = BinaryNode::new("iq")
            .with_attr("to", "@g.us")
            .with_attr("type", "set")
            .with_attr("xmlns", "w:g2")
            .with_attr("id", &iq_id)
            .with_child(invite_node);

        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        if let Some(comm) = res
            .get_child("community")
            .or_else(|| res.get_child("group"))
        {
            if let Some(jid) = comm.get_attr("jid").or_else(|| comm.get_attr("id")) {
                let full_jid = if jid.contains('@') {
                    jid.to_string()
                } else {
                    format!("{}@g.us", jid)
                };
                return Ok(full_jid);
            }
        }
        Ok(code.to_string())
    }
    /// Revokes an existing community invite code and generates a new one.
    pub async fn community_revoke_invite(&self, community_jid: &str) -> Result<String, CoreError> {
        let iq = crate::community::build_community_revoke_invite_iq(community_jid);
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        res.get_child("invitation")
            .and_then(|inv| inv.get_attr("code"))
            .map(|s| s.to_string())
            .ok_or_else(|| CoreError::Protocol("Missing code in revoke invite response".into()))
    }
    /// Gets invite info for a community invite link code.
    pub async fn community_get_invite_info(
        &self,
        invite_code: &str,
    ) -> Result<crate::community::CommunityInviteInfo, CoreError> {
        let iq = crate::community::build_community_get_invite_info_iq(invite_code);
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        let group_node = res
            .get_child("group")
            .ok_or_else(|| CoreError::Protocol("Missing group in invite info response".into()))?;
        let id = group_node.get_attr("id").unwrap_or_default().to_string();
        let subject = group_node
            .get_attr("subject")
            .unwrap_or_default()
            .to_string();
        let creator = group_node.get_attr("creator").map(|s| s.to_string());
        let size = group_node.get_attr("size").and_then(|s| s.parse().ok());
        Ok(crate::community::CommunityInviteInfo {
            jid: format!("{}@g.us", id),
            subject,
            creator,
            size,
        })
    }
    /// Updates ephemeral disappearing messages setting for a community.
    pub async fn community_toggle_ephemeral(
        &self,
        community_jid: &str,
        duration_sec: u32,
    ) -> Result<(), CoreError> {
        let iq = crate::community::build_community_toggle_ephemeral_iq(community_jid, duration_sec);
        self.send_iq(iq, Duration::from_secs(15)).await?;
        Ok(())
    }
    /// Updates announcement or locked setting for a community.
    pub async fn community_setting_update(
        &self,
        community_jid: &str,
        setting: &str,
    ) -> Result<(), CoreError> {
        let iq = crate::community::build_community_setting_update_iq(community_jid, setting);
        self.send_iq(iq, Duration::from_secs(15)).await?;
        Ok(())
    }
    /// Sets member add mode for a community ("admin_add" or "all_member_add").
    pub async fn community_member_add_mode(
        &self,
        community_jid: &str,
        mode: &str,
    ) -> Result<(), CoreError> {
        let iq = crate::community::build_community_member_add_mode_iq(community_jid, mode);
        self.send_iq(iq, Duration::from_secs(15)).await?;
        Ok(())
    }
    /// Sets join approval mode for a community ("on" or "off").
    pub async fn community_join_approval_mode(
        &self,
        community_jid: &str,
        mode: &str,
    ) -> Result<(), CoreError> {
        let iq = crate::community::build_community_join_approval_mode_iq(community_jid, mode);
        self.send_iq(iq, Duration::from_secs(15)).await?;
        Ok(())
    }
    /// Fetches all participating community JIDs.
    pub async fn community_fetch_all_participating(&self) -> Result<Vec<String>, CoreError> {
        let iq = crate::community::build_community_fetch_all_participating_iq();
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        let mut jids = Vec::new();
        if let Some(comm_node) = res.get_child("communities") {
            for sub in comm_node
                .get_children()
                .iter()
                .filter(|n| n.tag == "community")
            {
                if let Some(jid) = sub.get_attr("jid") {
                    jids.push(jid.to_string());
                }
            }
        }
        Ok(jids)
    }
}
