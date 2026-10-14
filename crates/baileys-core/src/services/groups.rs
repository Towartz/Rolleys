//! Domain service for WhatsApp Groups.

use crate::client::BaileysClient;
use crate::groups::{GroupJoinRequest, GroupMetadata};
use crate::CoreError;
use baileys_binary::BinaryNode;

/// Domain service providing all WhatsApp Group operations.
#[derive(Clone)]
pub struct GroupService {
    client: BaileysClient,
}

impl GroupService {
    pub fn new(client: BaileysClient) -> Self {
        Self { client }
    }

    /// Creates a new WhatsApp group with the given subject and initial participant JIDs.
    pub async fn create(
        &self,
        subject: &str,
        participants: &[String],
    ) -> Result<String, CoreError> {
        self.client.group_create(subject, participants).await
    }

    /// Queries raw binary metadata for a group.
    pub async fn get_raw_metadata(&self, group_jid: &str) -> Result<BinaryNode, CoreError> {
        self.client.group_metadata(group_jid).await
    }

    /// Fetches structured high-level metadata for a group.
    pub async fn get_metadata(&self, group_jid: &str) -> Result<GroupMetadata, CoreError> {
        self.client.group_fetch_metadata(group_jid).await
    }

    /// Fetches metadata for all groups the current account is participating in.
    pub async fn get_all_participating(&self) -> Result<Vec<GroupMetadata>, CoreError> {
        self.client.group_fetch_all_participating().await
    }

    /// Updates participants in a group ("add", "remove", "promote", "demote").
    pub async fn update_participants(
        &self,
        group_jid: &str,
        participants: &[String],
        action: &str,
    ) -> Result<Vec<(String, String)>, CoreError> {
        self.client
            .group_participants_update(group_jid, participants, action)
            .await
    }

    /// Adds participants to a group.
    pub async fn add_participants(
        &self,
        group_jid: &str,
        participants: &[String],
    ) -> Result<Vec<(String, String)>, CoreError> {
        self.update_participants(group_jid, participants, "add")
            .await
    }

    /// Removes participants from a group.
    pub async fn remove_participants(
        &self,
        group_jid: &str,
        participants: &[String],
    ) -> Result<Vec<(String, String)>, CoreError> {
        self.update_participants(group_jid, participants, "remove")
            .await
    }

    /// Promotes participants to group admins.
    pub async fn promote_participants(
        &self,
        group_jid: &str,
        participants: &[String],
    ) -> Result<Vec<(String, String)>, CoreError> {
        self.update_participants(group_jid, participants, "promote")
            .await
    }

    /// Demotes group admins back to normal participants.
    pub async fn demote_participants(
        &self,
        group_jid: &str,
        participants: &[String],
    ) -> Result<Vec<(String, String)>, CoreError> {
        self.update_participants(group_jid, participants, "demote")
            .await
    }

    /// Updates the group's subject / title.
    pub async fn update_subject(&self, group_jid: &str, subject: &str) -> Result<(), CoreError> {
        self.client.group_update_subject(group_jid, subject).await
    }

    /// Updates the group's description.
    pub async fn update_description(
        &self,
        group_jid: &str,
        description: &str,
    ) -> Result<(), CoreError> {
        self.client
            .group_update_description(group_jid, description)
            .await
    }

    /// Generates or retrieves an active invite code for the group.
    pub async fn get_invite_code(&self, group_jid: &str) -> Result<String, CoreError> {
        self.client.group_get_invite_code(group_jid).await
    }

    /// Revokes the current invite code and generates a new one.
    pub async fn revoke_invite(&self, group_jid: &str) -> Result<String, CoreError> {
        self.client.group_revoke_invite(group_jid).await
    }

    /// Accepts an invite code to join a group.
    pub async fn accept_invite(&self, code: &str) -> Result<String, CoreError> {
        self.client.group_accept_invite(code).await
    }

    /// Inspects an invite link code without joining.
    pub async fn get_invite_info(&self, code: &str) -> Result<BinaryNode, CoreError> {
        self.client.group_get_invite_info(code).await
    }

    /// Leaves a WhatsApp group.
    pub async fn leave(&self, group_jid: &str) -> Result<(), CoreError> {
        self.client.group_leave(group_jid).await
    }

    /// Updates group settings ("announcement", "not_announcement", "locked", "unlocked").
    pub async fn update_setting(&self, group_jid: &str, setting: &str) -> Result<(), CoreError> {
        self.client.group_setting_update(group_jid, setting).await
    }

    /// Configures join approval mode ("on" or "off").
    pub async fn set_join_approval_mode(
        &self,
        group_jid: &str,
        mode: &str,
    ) -> Result<(), CoreError> {
        self.client.group_join_approval_mode(group_jid, mode).await
    }

    /// Lists pending participant join approval requests.
    pub async fn list_join_requests(
        &self,
        group_jid: &str,
    ) -> Result<Vec<GroupJoinRequest>, CoreError> {
        self.client.group_request_participants_list(group_jid).await
    }

    /// Approves or rejects pending participant join requests ("approve" or "reject").
    pub async fn update_join_requests(
        &self,
        group_jid: &str,
        participants: &[String],
        action: &str,
    ) -> Result<Vec<(String, String)>, CoreError> {
        self.client
            .group_request_participants_update(group_jid, participants, action)
            .await
    }

    /// Sets member add mode ("admin_add" or "all_member_add").
    pub async fn set_member_add_mode(&self, group_jid: &str, mode: &str) -> Result<(), CoreError> {
        self.client.group_member_add_mode(group_jid, mode).await
    }

    /// Revokes a direct v4 group invite.
    pub async fn revoke_invite_v4(
        &self,
        group_jid: &str,
        invited_jid: &str,
    ) -> Result<(), CoreError> {
        self.client
            .group_revoke_invite_v4(group_jid, invited_jid)
            .await
    }

    /// Accepts a direct v4 group invite.
    pub async fn accept_invite_v4(
        &self,
        group_jid: &str,
        invite_code: &str,
        invite_expiration: i64,
        from_jid: &str,
    ) -> Result<String, CoreError> {
        self.client
            .group_accept_invite_v4(group_jid, invite_code, invite_expiration, from_jid)
            .await
    }
}
