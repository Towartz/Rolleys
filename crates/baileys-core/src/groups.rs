//! WhatsApp Advanced Group Administration Protocol.
//!
//! Provides models, IQ builders, and response parsers for:
//! - Detailed group metadata and participant permissions (`GroupMetadata`, `GroupParticipant`)
//! - Batch participating groups synchronization (`groupFetchAllParticipating`)
//! - Membership approval mode and pending join requests (`membership_approval_mode`)
//! - Member add permissions (`admin_add` vs `all_member_add`)
//! - v4 invite revoking and direct message acceptance.

use crate::CoreError;
use baileys_binary::BinaryNode;
use serde::{Deserialize, Serialize};

/// Details of a single participant within a group.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GroupParticipant {
    pub jid: String,
    pub phone_number: Option<String>,
    pub lid: Option<String>,
    pub admin: Option<String>, // "admin" | "superadmin" | None
}

/// Comprehensive metadata for a WhatsApp group.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GroupMetadata {
    pub id: String,
    pub subject: String,
    pub subject_owner: Option<String>,
    pub subject_time: Option<i64>,
    pub size: u32,
    pub creation: i64,
    pub owner: Option<String>,
    pub description: Option<String>,
    pub description_id: Option<String>,
    pub restrict: bool,
    pub announce: bool,
    pub is_community: bool,
    pub is_community_announce: bool,
    pub join_approval_mode: bool,
    pub member_add_mode: String,
    pub ephemeral_duration: Option<u32>,
    pub addressing_mode: Option<String>,
    pub participants: Vec<GroupParticipant>,
}

/// Pending membership approval request for joining a group.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GroupJoinRequest {
    pub jid: String,
    pub requested_time: i64,
    pub request_method: Option<String>,
}

/// Builds an IQ node for fetching all participating groups.
pub fn build_fetch_participating_groups_iq() -> BinaryNode {
    let iq_id = format!("3EB0{:X}", rand::random::<u64>());
    let participating = BinaryNode::new("participating")
        .with_child(BinaryNode::new("participants"))
        .with_child(BinaryNode::new("description"));

    BinaryNode::new("iq")
        .with_attr("to", "@g.us")
        .with_attr("xmlns", "w:g2")
        .with_attr("type", "get")
        .with_attr("id", &iq_id)
        .with_child(participating)
}

/// Builds an IQ node for setting group join approval mode ("on" or "off").
pub fn build_group_join_approval_mode_iq(group_jid: &str, mode: &str) -> BinaryNode {
    let iq_id = format!("3EB0{:X}", rand::random::<u64>());
    let join_node = BinaryNode::new("group_join").with_attr("state", mode);
    let approval_node = BinaryNode::new("membership_approval_mode").with_child(join_node);

    BinaryNode::new("iq")
        .with_attr("to", group_jid)
        .with_attr("xmlns", "w:g2")
        .with_attr("type", "set")
        .with_attr("id", &iq_id)
        .with_child(approval_node)
}

/// Builds an IQ node for querying pending membership approval requests.
pub fn build_group_request_participants_list_iq(group_jid: &str) -> BinaryNode {
    let iq_id = format!("3EB0{:X}", rand::random::<u64>());
    BinaryNode::new("iq")
        .with_attr("to", group_jid)
        .with_attr("xmlns", "w:g2")
        .with_attr("type", "get")
        .with_attr("id", &iq_id)
        .with_child(BinaryNode::new("membership_approval_requests"))
}

/// Builds an IQ node for approving or rejecting membership requests.
pub fn build_group_request_participants_update_iq(
    group_jid: &str,
    participants: &[String],
    action: &str,
) -> BinaryNode {
    let iq_id = format!("3EB0{:X}", rand::random::<u64>());
    let mut action_node = BinaryNode::new(action);
    for p in participants {
        let p_jid = if !p.contains('@') {
            format!("{}@s.whatsapp.net", p.trim_start_matches('+'))
        } else {
            p.clone()
        };
        action_node =
            action_node.with_child(BinaryNode::new("participant").with_attr("jid", &p_jid));
    }

    let req_action = BinaryNode::new("membership_requests_action").with_child(action_node);

    BinaryNode::new("iq")
        .with_attr("to", group_jid)
        .with_attr("xmlns", "w:g2")
        .with_attr("type", "set")
        .with_attr("id", &iq_id)
        .with_child(req_action)
}

/// Builds an IQ node for configuring member add permissions ("admin_add" or "all_member_add").
pub fn build_group_member_add_mode_iq(group_jid: &str, mode: &str) -> BinaryNode {
    let iq_id = format!("3EB0{:X}", rand::random::<u64>());
    let member_add = BinaryNode::new("member_add_mode").with_content(mode.as_bytes().to_vec());

    BinaryNode::new("iq")
        .with_attr("to", group_jid)
        .with_attr("xmlns", "w:g2")
        .with_attr("type", "set")
        .with_attr("id", &iq_id)
        .with_child(member_add)
}

/// Builds an IQ node for revoking a participant's v4 group invite.
pub fn build_group_revoke_invite_v4_iq(group_jid: &str, invited_jid: &str) -> BinaryNode {
    let iq_id = format!("3EB0{:X}", rand::random::<u64>());
    let p_jid = if !invited_jid.contains('@') {
        format!("{}@s.whatsapp.net", invited_jid.trim_start_matches('+'))
    } else {
        invited_jid.to_string()
    };

    let revoke_node = BinaryNode::new("revoke")
        .with_child(BinaryNode::new("participant").with_attr("jid", &p_jid));

    BinaryNode::new("iq")
        .with_attr("to", group_jid)
        .with_attr("xmlns", "w:g2")
        .with_attr("type", "set")
        .with_attr("id", &iq_id)
        .with_child(revoke_node)
}

/// Builds an IQ node for accepting a v4 group invite message.
pub fn build_group_accept_invite_v4_iq(
    group_jid: &str,
    invite_code: &str,
    invite_expiration: i64,
    admin_jid: &str,
) -> BinaryNode {
    let iq_id = format!("3EB0{:X}", rand::random::<u64>());
    let accept_node = BinaryNode::new("accept")
        .with_attr("code", invite_code)
        .with_attr("expiration", invite_expiration.to_string())
        .with_attr("admin", admin_jid);

    BinaryNode::new("iq")
        .with_attr("to", group_jid)
        .with_attr("xmlns", "w:g2")
        .with_attr("type", "set")
        .with_attr("id", &iq_id)
        .with_child(accept_node)
}

/// Parses a `GroupMetadata` structure from a `<group>` XML binary node.
pub fn parse_group_metadata(node: &BinaryNode) -> Result<GroupMetadata, CoreError> {
    let group = if node.tag == "group" {
        node
    } else if let Some(g) = node.get_child("group") {
        g
    } else {
        return Err(CoreError::Protocol(
            "Missing <group> node in response".into(),
        ));
    };

    let id_raw = group
        .get_attr("id")
        .ok_or_else(|| CoreError::Protocol("Group metadata node missing 'id' attribute".into()))?;

    let id = if id_raw.contains('@') {
        id_raw.to_string()
    } else {
        format!("{}@g.us", id_raw)
    };

    let subject = group.get_attr("subject").unwrap_or("").to_string();
    let subject_owner = group.get_attr("s_o").map(String::from);
    let subject_time = group.get_attr("s_t").and_then(|s| s.parse::<i64>().ok());
    let creation = group
        .get_attr("creation")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);
    let owner = group.get_attr("creator").map(String::from);

    let (description, description_id) = if let Some(desc) = group.get_child("description") {
        let did = desc.get_attr("id").map(String::from);
        let body = if let Some(b) = desc.get_child("body") {
            b.content_as_bytes()
                .map(|bytes| String::from_utf8_lossy(bytes).to_string())
        } else {
            None
        };
        (body, did)
    } else {
        (None, None)
    };

    let restrict = group.get_child("locked").is_some();
    let announce = group.get_child("announcement").is_some();
    let is_community = group.get_child("parent").is_some();
    let is_community_announce = group.get_child("default_sub_group").is_some();
    let join_approval_mode = group.get_child("membership_approval_mode").is_some();

    let member_add_mode = group
        .get_child("member_add_mode")
        .and_then(|m| m.content_as_bytes())
        .map(|bytes| String::from_utf8_lossy(bytes).to_string())
        .unwrap_or_else(|| "admin_add".to_string());

    let ephemeral_duration = group
        .get_child("ephemeral")
        .and_then(|e| e.get_attr("expiration"))
        .and_then(|s| s.parse::<u32>().ok());

    let mut participants = Vec::new();
    let participant_nodes = if let Some(p_container) = group.get_child("participants") {
        p_container.get_children()
    } else {
        group.get_children()
    };

    for child in participant_nodes {
        if child.tag == "participant" {
            if let Some(jid) = child.get_attr("jid") {
                participants.push(GroupParticipant {
                    jid: jid.to_string(),
                    phone_number: child.get_attr("phone_number").map(String::from),
                    lid: child.get_attr("lid").map(String::from),
                    admin: child.get_attr("type").map(String::from),
                });
            }
        }
    }

    let size = group
        .get_attr("size")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(participants.len() as u32);

    let addressing_mode = group.get_attr("addressing_mode").map(String::from);
    tracing::info!(
        "[GROUP_METADATA_PARSED] id={}, addressing_mode={:?}, attrs={:?}",
        id,
        addressing_mode,
        group.attrs
    );

    Ok(GroupMetadata {
        id,
        subject,
        subject_owner,
        subject_time,
        size,
        creation,
        owner,
        description,
        description_id,
        restrict,
        announce,
        is_community,
        is_community_announce,
        join_approval_mode,
        member_add_mode,
        ephemeral_duration,
        addressing_mode,
        participants,
    })
}

/// Parses all participating groups from a `<participating>` query response.
pub fn parse_participating_groups(node: &BinaryNode) -> Result<Vec<GroupMetadata>, CoreError> {
    let groups_node = if node.tag == "groups" {
        node
    } else if let Some(g) = node.get_child("groups") {
        g
    } else {
        return Ok(Vec::new());
    };

    let mut list = Vec::new();
    for child in groups_node.get_children() {
        if child.tag == "group" {
            if let Ok(meta) = parse_group_metadata(child) {
                list.push(meta);
            }
        }
    }

    Ok(list)
}

/// Parses membership approval requests from a `<membership_approval_requests>` response.
pub fn parse_group_join_requests(node: &BinaryNode) -> Result<Vec<GroupJoinRequest>, CoreError> {
    let requests_node = if node.tag == "membership_approval_requests" {
        node
    } else if let Some(r) = node.get_child("membership_approval_requests") {
        r
    } else {
        return Ok(Vec::new());
    };

    let mut list = Vec::new();
    for child in requests_node.get_children() {
        if child.tag == "membership_approval_request" {
            if let Some(jid) = child.get_attr("jid") {
                let requested_time = child
                    .get_attr("requested_time")
                    .and_then(|s| s.parse::<i64>().ok())
                    .unwrap_or(0);
                let request_method = child.get_attr("request_method").map(String::from);
                list.push(GroupJoinRequest {
                    jid: jid.to_string(),
                    requested_time,
                    request_method,
                });
            }
        }
    }

    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_group_metadata() {
        let group_node = BinaryNode::new("group")
            .with_attr("id", "120363045678901234")
            .with_attr("subject", "Rust Core Developers")
            .with_attr("creator", "6283162498175@s.whatsapp.net")
            .with_attr("creation", "1700000000")
            .with_attr("size", "2")
            .with_child(
                BinaryNode::new("description")
                    .with_attr("id", "desc123")
                    .with_child(
                        BinaryNode::new("body")
                            .with_content("Official Group Description".as_bytes().to_vec()),
                    ),
            )
            .with_child(BinaryNode::new("announcement"))
            .with_child(BinaryNode::new("membership_approval_mode"))
            .with_child(
                BinaryNode::new("member_add_mode")
                    .with_content("all_member_add".as_bytes().to_vec()),
            )
            .with_child(BinaryNode::new("ephemeral").with_attr("expiration", "86400"))
            .with_child(
                BinaryNode::new("participant")
                    .with_attr("jid", "6283162498175@s.whatsapp.net")
                    .with_attr("type", "superadmin"),
            )
            .with_child(
                BinaryNode::new("participant")
                    .with_attr("jid", "628123456789@s.whatsapp.net")
                    .with_attr("type", "admin"),
            );

        let meta = parse_group_metadata(&group_node).expect("Parsing failed");
        assert_eq!(meta.id, "120363045678901234@g.us");
        assert_eq!(meta.subject, "Rust Core Developers");
        assert_eq!(meta.owner.as_deref(), Some("6283162498175@s.whatsapp.net"));
        assert_eq!(
            meta.description.as_deref(),
            Some("Official Group Description")
        );
        assert_eq!(meta.description_id.as_deref(), Some("desc123"));
        assert!(meta.announce);
        assert!(!meta.restrict);
        assert!(meta.join_approval_mode);
        assert_eq!(meta.member_add_mode, "all_member_add");
        assert_eq!(meta.ephemeral_duration, Some(86400));
        assert_eq!(meta.participants.len(), 2);
        assert_eq!(meta.participants[0].admin.as_deref(), Some("superadmin"));
    }

    #[test]
    fn test_parse_participating_groups() {
        let iq_node = BinaryNode::new("iq").with_child(
            BinaryNode::new("groups")
                .with_child(
                    BinaryNode::new("group")
                        .with_attr("id", "g1@g.us")
                        .with_attr("subject", "Group One")
                        .with_attr("creation", "1700000000"),
                )
                .with_child(
                    BinaryNode::new("group")
                        .with_attr("id", "g2@g.us")
                        .with_attr("subject", "Group Two")
                        .with_attr("creation", "1700000001"),
                ),
        );

        let groups = parse_participating_groups(&iq_node).expect("Parsing failed");
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].id, "g1@g.us");
        assert_eq!(groups[0].subject, "Group One");
        assert_eq!(groups[1].id, "g2@g.us");
        assert_eq!(groups[1].subject, "Group Two");
    }

    #[test]
    fn test_parse_group_join_requests() {
        let requests_node = BinaryNode::new("membership_approval_requests").with_child(
            BinaryNode::new("membership_approval_request")
                .with_attr("jid", "628999999999@s.whatsapp.net")
                .with_attr("requested_time", "1700005000")
                .with_attr("request_method", "invite_link"),
        );

        let requests = parse_group_join_requests(&requests_node).expect("Parsing failed");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].jid, "628999999999@s.whatsapp.net");
        assert_eq!(requests[0].requested_time, 1700005000);
        assert_eq!(requests[0].request_method.as_deref(), Some("invite_link"));
    }

    #[test]
    fn test_group_iq_builders() {
        let join_approval = build_group_join_approval_mode_iq("123@g.us", "on");
        assert_eq!(join_approval.get_attr("to"), Some("123@g.us"));
        let approval = join_approval.get_child("membership_approval_mode").unwrap();
        assert_eq!(
            approval.get_child("group_join").unwrap().get_attr("state"),
            Some("on")
        );

        let req_update = build_group_request_participants_update_iq(
            "123@g.us",
            &["628111@s.whatsapp.net".to_string()],
            "approve",
        );
        let action = req_update
            .get_child("membership_requests_action")
            .unwrap()
            .get_child("approve")
            .unwrap();
        assert_eq!(
            action.get_children()[0].get_attr("jid"),
            Some("628111@s.whatsapp.net")
        );
    }
}
